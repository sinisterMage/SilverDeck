//! Game session lifecycle: turn a `LaunchSpec` into a process (optionally
//! wrapped in gamescope), track it until the game is really gone, then hand
//! focus back to the console UI via sway IPC.
//!
//! "Really gone" needs two signals because launchers daemonize (Steam's URL
//! handler returns immediately): the spawned child must have exited AND no
//! foreign (non-silverdeck-ui) sway windows may remain. Outside a sway
//! session (plain `cargo run` on a dev desktop) only child exit is used.

use anyhow::Context as _;
use futures_lite::StreamExt as _;
use silverdeck_core::LaunchSpec;

pub const UI_APP_ID: &str = "silverdeck-ui";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub gamescope: Option<Gamescope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gamescope {
    pub width: u32,
    pub height: u32,
    pub fps_limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    Started,
    Exited { success: bool },
}

/// Build the argv for a launch spec. `heroic_native` selects between a host
/// heroic binary and the flatpak-installed launcher.
pub fn build_command(spec: &LaunchSpec, opts: &LaunchOptions, heroic_native: bool) -> Vec<String> {
    let base: Vec<String> = match spec {
        LaunchSpec::Command(argv) => argv.clone(),
        LaunchSpec::SteamAppId(id) => vec!["steam".into(), format!("steam://rungameid/{id}")],
        LaunchSpec::FlatpakRef(app) => vec!["flatpak".into(), "run".into(), app.clone()],
        LaunchSpec::Heroic { runner, id } => {
            let url = format!("heroic://launch/{runner}/{id}");
            if heroic_native {
                vec!["heroic".into(), "--no-gui".into(), url]
            } else {
                vec![
                    "flatpak".into(),
                    "run".into(),
                    "com.heroicgameslauncher.hgl".into(),
                    "--no-gui".into(),
                    url,
                ]
            }
        }
    };
    match (&opts.gamescope, spec) {
        // Steam URL invocations only signal the running Steam daemon; wrapping
        // them in gamescope would wrap the messenger, not the game.
        (Some(_), LaunchSpec::SteamAppId(_)) => base,
        (Some(g), _) => {
            let mut argv = vec![
                "gamescope".to_owned(),
                "-W".to_owned(),
                g.width.to_string(),
                "-H".to_owned(),
                g.height.to_string(),
            ];
            if let Some(fps) = g.fps_limit {
                argv.push("-r".to_owned());
                argv.push(fps.to_string());
            }
            argv.push("--fullscreen".to_owned());
            argv.push("--".to_owned());
            argv.extend(base);
            argv
        }
        (None, _) => base,
    }
}

pub fn heroic_is_native() -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join("heroic").is_file()))
        .unwrap_or(false)
}

/// Run a game session to completion. Emits `Started` after spawn and
/// `Exited` once the game is gone; focus is returned to the UI window.
pub async fn run_session(
    spec: LaunchSpec,
    opts: LaunchOptions,
    events: async_channel::Sender<SessionEvent>,
) -> anyhow::Result<()> {
    let argv = build_command(&spec, &opts, heroic_is_native());
    log::info!("launching: {argv:?}");
    let mut child = async_process::Command::new(&argv[0])
        .args(&argv[1..])
        .spawn()
        .with_context(|| format!("failed to spawn {argv:?}"))?;
    let _ = events.send(SessionEvent::Started).await;

    let status = child.status().await?;
    let mut success = status.success();

    if in_sway() {
        // Wait for launcher-daemonized games (Steam) to fully close.
        if let Err(err) = wait_for_foreign_windows_gone(&spec).await {
            log::warn!("sway window tracking failed: {err}");
        }
        if let Err(err) = focus_ui().await {
            log::warn!("failed to refocus UI: {err}");
        }
        // For URL-triggered launches the child status is meaningless.
        if matches!(spec, LaunchSpec::SteamAppId(_)) {
            success = true;
        }
    }

    let _ = events.send(SessionEvent::Exited { success }).await;
    Ok(())
}

fn in_sway() -> bool {
    std::env::var_os("SWAYSOCK").is_some()
}

/// Subscribe to window events and return once no non-UI application windows
/// remain. Steam needs a grace period for the game window to appear at all.
async fn wait_for_foreign_windows_gone(spec: &LaunchSpec) -> anyhow::Result<()> {
    use swayipc_async::{Connection, Event, EventType};

    // For direct child processes the exit already proves the game is gone;
    // only daemonizing launchers need window-based tracking.
    if !matches!(spec, LaunchSpec::SteamAppId(_)) {
        return Ok(());
    }

    let mut query = Connection::new().await?;
    let subscription = Connection::new().await?;
    let mut events = subscription.subscribe([EventType::Window]).await?;

    // Give the game up to 120s to show a window (Proton first runs compile
    // shaders); if nothing ever appears, don't block the UI forever.
    let mut saw_foreign = false;
    let started = std::time::Instant::now();
    loop {
        if count_foreign_windows(&mut query).await? > 0 {
            saw_foreign = true;
        } else if saw_foreign || started.elapsed().as_secs() > 120 {
            return Ok(());
        }
        match events.next().await {
            Some(Ok(Event::Window(_))) => {}
            Some(Ok(_)) => {}
            Some(Err(err)) => return Err(err.into()),
            None => return Ok(()),
        }
    }
}

async fn count_foreign_windows(conn: &mut swayipc_async::Connection) -> anyhow::Result<usize> {
    let tree = conn.get_tree().await?;
    let mut count = 0;
    let mut stack = vec![&tree];
    while let Some(node) = stack.pop() {
        let app_id = node.app_id.as_deref();
        let class = node
            .window_properties
            .as_ref()
            .and_then(|p| p.class.as_deref());
        let is_window = node.pid.is_some() && (app_id.is_some() || class.is_some());
        if is_window && app_id != Some(UI_APP_ID) {
            count += 1;
        }
        stack.extend(node.nodes.iter());
        stack.extend(node.floating_nodes.iter());
    }
    Ok(count)
}

async fn focus_ui() -> anyhow::Result<()> {
    let mut conn = swayipc_async::Connection::new().await?;
    conn.run_command(format!(
        "[app_id=\"{UI_APP_ID}\"] focus; [app_id=\"{UI_APP_ID}\"] fullscreen enable"
    ))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_launch_is_a_url_invocation() {
        let argv = build_command(
            &LaunchSpec::SteamAppId(620),
            &LaunchOptions::default(),
            false,
        );
        assert_eq!(argv, ["steam", "steam://rungameid/620"]);
    }

    #[test]
    fn gamescope_wraps_direct_commands() {
        let opts = LaunchOptions {
            gamescope: Some(Gamescope {
                width: 1920,
                height: 1080,
                fps_limit: Some(60),
            }),
        };
        let argv = build_command(
            &LaunchSpec::FlatpakRef("org.example.Game".into()),
            &opts,
            false,
        );
        assert_eq!(
            argv,
            [
                "gamescope",
                "-W",
                "1920",
                "-H",
                "1080",
                "-r",
                "60",
                "--fullscreen",
                "--",
                "flatpak",
                "run",
                "org.example.Game"
            ]
        );
    }

    #[test]
    fn gamescope_never_wraps_steam_urls() {
        let opts = LaunchOptions {
            gamescope: Some(Gamescope {
                width: 1280,
                height: 800,
                fps_limit: None,
            }),
        };
        let argv = build_command(&LaunchSpec::SteamAppId(620), &opts, false);
        assert_eq!(argv[0], "steam");
    }

    #[test]
    fn heroic_uses_flatpak_when_not_native() {
        let argv = build_command(
            &LaunchSpec::Heroic {
                runner: "legendary".into(),
                id: "Fortnite".into(),
            },
            &LaunchOptions::default(),
            false,
        );
        assert_eq!(argv[0], "flatpak");
        assert!(argv
            .last()
            .unwrap()
            .starts_with("heroic://launch/legendary/"));
        let native = build_command(
            &LaunchSpec::Heroic {
                runner: "gog".into(),
                id: "42".into(),
            },
            &LaunchOptions::default(),
            true,
        );
        assert_eq!(native[0], "heroic");
    }
}
