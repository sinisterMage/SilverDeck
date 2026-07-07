//! The SilverDeck store: a curated allowlist of Flathub apps baked into the
//! image, enriched with Flathub metadata, installed via flatpak.
//!
//! flatpak only renders progress on a TTY, so installs run under a PTY and
//! percentages are scraped from the raw output. All functions are blocking;
//! the app runs them on background threads/executor.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::Deserialize;
use silverdeck_system::CommandRunner;

pub const DEFAULT_STORE_FILE: &str = "/usr/share/silverdeck/store.json";
pub const FLATHUB_REMOTE: &str = "flathub";

// --- Curated allowlist ------------------------------------------------------

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StoreCategory {
    pub name: String,
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StoreCatalog {
    pub categories: Vec<StoreCategory>,
}

impl StoreCatalog {
    pub fn load_default() -> anyhow::Result<Self> {
        let path = std::env::var_os("SILVERDECK_STORE_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| DEFAULT_STORE_FILE.into());
        Self::load(&path)
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("missing store catalog: {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("invalid store catalog: {}", path.display()))
    }

    pub fn all_apps(&self) -> impl Iterator<Item = &str> {
        self.categories
            .iter()
            .flat_map(|c| c.apps.iter().map(String::as_str))
    }
}

// --- Flathub metadata -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMeta {
    pub app_id: String,
    pub name: String,
    pub summary: String,
    pub icon: Option<PathBuf>,
}

pub struct FlathubClient {
    cache_dir: PathBuf,
    agent: ureq::Agent,
}

impl FlathubClient {
    pub fn new() -> Self {
        let cache_dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| PathBuf::from(h).join(".cache"))
                    .unwrap_or_else(|| "/tmp".into())
            })
            .join("silverdeck");
        FlathubClient {
            cache_dir,
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(15)))
                .build()
                .into(),
        }
    }

    /// Fetch (and disk-cache) name/summary/icon for an app id. Metadata is
    /// cached forever — store entries are curated, a stale summary is fine.
    pub fn meta(&self, app_id: &str) -> anyhow::Result<AppMeta> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let meta_path = self.cache_dir.join(format!("{app_id}.meta.json"));
        let json: serde_json::Value = if meta_path.is_file() {
            serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?
        } else {
            let url = format!("https://flathub.org/api/v2/appstream/{app_id}");
            let body = self
                .agent
                .get(&url)
                .call()
                .with_context(|| format!("flathub request failed: {url}"))?
                .body_mut()
                .read_to_string()?;
            let json: serde_json::Value = serde_json::from_str(&body)?;
            std::fs::write(&meta_path, &body)?;
            json
        };

        let name = json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(app_id)
            .to_owned();
        let summary = json
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let icon = json
            .get("icon")
            .and_then(|v| v.as_str())
            .and_then(|url| self.cached_icon(app_id, url));
        Ok(AppMeta {
            app_id: app_id.to_owned(),
            name,
            summary,
            icon,
        })
    }

    fn cached_icon(&self, app_id: &str, url: &str) -> Option<PathBuf> {
        let path = self.cache_dir.join(format!("{app_id}.icon.png"));
        if path.is_file() {
            return Some(path);
        }
        let mut response = self.agent.get(url).call().ok()?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut bytes)
            .ok()?;
        std::fs::write(&path, bytes).ok()?;
        Some(path)
    }
}

impl Default for FlathubClient {
    fn default() -> Self {
        Self::new()
    }
}

// --- Installed state / uninstall (plain subprocess) ---------------------------

pub fn installed_apps(runner: &dyn CommandRunner) -> anyhow::Result<HashSet<String>> {
    let out = runner.run(&["flatpak", "list", "--app", "--columns=application"])?;
    Ok(out.lines().map(|l| l.trim().to_owned()).collect())
}

pub fn uninstall(runner: &dyn CommandRunner, app_id: &str) -> anyhow::Result<()> {
    runner.run(&[
        "flatpak",
        "uninstall",
        "--system",
        "--noninteractive",
        "-y",
        app_id,
    ])?;
    Ok(())
}

/// Best-effort fallback if the firstboot unit hasn't added flathub yet.
pub fn ensure_flathub_remote(runner: &dyn CommandRunner) -> anyhow::Result<()> {
    runner.run(&[
        "flatpak",
        "remote-add",
        "--if-not-exists",
        "--system",
        FLATHUB_REMOTE,
        "https://dl.flathub.org/repo/flathub.flatpakrepo",
    ])?;
    Ok(())
}

// --- Install with progress (PTY) ---------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallEvent {
    /// 0..=100, monotone per install.
    Progress(u8),
    Done { success: bool, message: String },
}

/// Install an app from flathub, reporting progress. Blocking — run on a
/// dedicated thread. Events are best-effort (send failures ignored: the UI
/// may have navigated away).
pub fn install(app_id: &str, on_event: impl Fn(InstallEvent)) -> anyhow::Result<()> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("failed to open pty")?;
    let mut cmd = CommandBuilder::new("flatpak");
    cmd.args([
        "install",
        "--system",
        "--noninteractive",
        "-y",
        FLATHUB_REMOTE,
        app_id,
    ]);
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn flatpak install")?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut tail = String::new();
    let mut last_progress = 0u8;
    let mut buf = [0u8; 4096];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break, // EIO when child closes the pty — normal EOF
        };
        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
        tail.push_str(&text);
        if tail.len() > 8192 {
            let cut = tail.len() - 4096;
            tail.drain(..cut);
        }
        if let Some(p) = latest_percent(&text) {
            if p > last_progress {
                last_progress = p;
                on_event(InstallEvent::Progress(p));
            }
        }
    }
    let status = child.wait()?;
    let success = status.success();
    on_event(InstallEvent::Done {
        success,
        message: if success {
            format!("{app_id} installed")
        } else {
            last_error_line(&tail).unwrap_or_else(|| format!("{app_id}: install failed"))
        },
    });
    Ok(())
}

/// Last "NN%" in a chunk of flatpak progress output.
fn latest_percent(text: &str) -> Option<u8> {
    let re = regex::Regex::new(r"(\d{1,3})%").unwrap();
    re.captures_iter(text)
        .filter_map(|c| c[1].parse::<u8>().ok())
        .filter(|p| *p <= 100)
        .last()
}

fn last_error_line(tail: &str) -> Option<String> {
    tail.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.to_lowercase().starts_with("error"))
        .map(|l| l.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use silverdeck_system::FakeRunner;

    #[test]
    fn loads_catalog_and_lists_apps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.json");
        std::fs::write(
            &path,
            r#"{"categories": [
                {"name": "Games", "apps": ["org.supertuxproject.SuperTux"]},
                {"name": "Emulators", "apps": ["org.DolphinEmu.dolphin-emu"]}
            ]}"#,
        )
        .unwrap();
        let catalog = StoreCatalog::load(&path).unwrap();
        assert_eq!(catalog.categories.len(), 2);
        assert_eq!(catalog.all_apps().count(), 2);
    }

    #[test]
    fn invalid_catalog_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(StoreCatalog::load(&path).is_err());
    }

    #[test]
    fn parses_progress_percentages() {
        assert_eq!(latest_percent("Installing… 12%"), Some(12));
        assert_eq!(
            latest_percent("app 1/3  10.2 MB / 80 MB  13%\rapp 1/3  40 MB / 80 MB  50%"),
            Some(50)
        );
        assert_eq!(latest_percent("no numbers here"), None);
        assert_eq!(latest_percent("weird 400%"), None);
    }

    #[test]
    fn installed_apps_parses_list() {
        let runner = FakeRunner::new().expect(
            &["flatpak", "list"],
            Ok("org.supertuxproject.SuperTux\ncom.heroicgameslauncher.hgl\n".into()),
        );
        let installed = installed_apps(&runner).unwrap();
        assert!(installed.contains("org.supertuxproject.SuperTux"));
        assert_eq!(installed.len(), 2);
    }

    #[test]
    fn error_line_extraction() {
        assert_eq!(
            last_error_line("progress\nerror: No remote refs found\n"),
            Some("error: No remote refs found".into())
        );
        assert_eq!(last_error_line("all fine"), None);
    }
}
