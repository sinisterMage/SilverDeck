//! System control backends for the Settings tab: Wi-Fi (NetworkManager),
//! volume (PipeWire/wpctl), power and OS updates (systemd).
//!
//! Everything shells out through the `CommandRunner` trait so unit tests (and
//! `make ui-run` on a dev box) can substitute canned output. All functions
//! here are synchronous; the app calls them on the background executor.

use anyhow::{bail, Context as _};

pub trait CommandRunner: Send + Sync {
    /// Run to completion, capture stdout. Non-zero exit is an Err carrying
    /// stderr.
    fn run(&self, argv: &[&str]) -> anyhow::Result<String>;
}

/// Executes commands for real.
pub struct HostRunner;

impl CommandRunner for HostRunner {
    fn run(&self, argv: &[&str]) -> anyhow::Result<String> {
        let output = std::process::Command::new(argv[0])
            .args(&argv[1..])
            .output()
            .with_context(|| format!("failed to run {argv:?}"))?;
        if !output.status.success() {
            bail!(
                "{} failed: {}",
                argv[0],
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

// --- Wi-Fi (NetworkManager via nmcli) -------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// 0..=100
    pub signal: u8,
    pub secured: bool,
    pub connected: bool,
}

pub struct Network<'a>(pub &'a dyn CommandRunner);

impl Network<'_> {
    pub fn wifi_enabled(&self) -> anyhow::Result<bool> {
        Ok(self.0.run(&["nmcli", "radio", "wifi"])?.trim() == "enabled")
    }

    pub fn set_wifi_enabled(&self, on: bool) -> anyhow::Result<()> {
        self.0
            .run(&["nmcli", "radio", "wifi", if on { "on" } else { "off" }])?;
        Ok(())
    }

    /// Scan and list visible networks, strongest first, deduped by SSID.
    pub fn scan(&self) -> anyhow::Result<Vec<WifiNetwork>> {
        let out = self.0.run(&[
            "nmcli",
            "-t",
            "-f",
            "ACTIVE,SIGNAL,SECURITY,SSID",
            "device",
            "wifi",
            "list",
            "--rescan",
            "auto",
        ])?;
        Ok(parse_wifi_list(&out))
    }

    /// Join a network. `password: None` for open/known networks.
    pub fn connect(&self, ssid: &str, password: Option<&str>) -> anyhow::Result<()> {
        match password {
            Some(pw) => self.0.run(&[
                "nmcli", "device", "wifi", "connect", ssid, "password", pw,
            ])?,
            None => self.0.run(&["nmcli", "device", "wifi", "connect", ssid])?,
        };
        Ok(())
    }

    /// One-line status for the settings header, e.g. "wifi: MyNet (connected)".
    pub fn status(&self) -> anyhow::Result<String> {
        let out = self
            .0
            .run(&["nmcli", "-t", "-f", "TYPE,STATE,CONNECTION", "device"])?;
        for line in out.lines() {
            let mut parts = line.splitn(3, ':');
            let (ty, state, conn) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            if state == "connected" && (ty == "ethernet" || ty == "wifi") {
                return Ok(format!("{ty}: {conn}"));
            }
        }
        Ok("disconnected".to_owned())
    }
}

/// Parse `nmcli -t -f ACTIVE,SIGNAL,SECURITY,SSID device wifi list` output.
/// Terse mode is colon-separated with backslash escaping; SSID is last so
/// escaped colons only matter there.
fn parse_wifi_list(out: &str) -> Vec<WifiNetwork> {
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for line in out.lines() {
        let fields = split_terse(line, 4);
        let [active, signal, security, ssid] = match fields.as_slice() {
            [a, b, c, d] => [a, b, c, d],
            _ => continue,
        };
        if ssid.is_empty() {
            continue; // hidden network
        }
        let network = WifiNetwork {
            ssid: ssid.clone(),
            signal: signal.parse().unwrap_or(0),
            secured: !security.is_empty() && security != "--",
            connected: active == "yes",
        };
        match networks.iter_mut().find(|n| n.ssid == network.ssid) {
            Some(existing) => {
                // Keep the strongest AP per SSID; never lose the connected flag.
                existing.connected |= network.connected;
                if network.signal > existing.signal {
                    existing.signal = network.signal;
                    existing.secured = network.secured;
                }
            }
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.signal.cmp(&a.signal))
    });
    networks
}

/// Split one nmcli terse line into at most `n` fields, honoring "\:" escapes.
fn split_terse(line: &str, n: usize) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ':' if fields.len() < n - 1 => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

// --- Volume (wpctl) --------------------------------------------------------

pub struct Audio<'a>(pub &'a dyn CommandRunner);

impl Audio<'_> {
    /// Current default-sink volume as 0..=100 (None while muted).
    pub fn volume(&self) -> anyhow::Result<(u8, bool)> {
        let out = self.0.run(&["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"])?;
        parse_wpctl_volume(&out).context("unparseable wpctl output")
    }

    pub fn set_volume(&self, percent: u8) -> anyhow::Result<()> {
        let value = format!("{:.2}", f32::from(percent.min(100)) / 100.0);
        self.0
            .run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", &value])?;
        Ok(())
    }

    pub fn set_muted(&self, muted: bool) -> anyhow::Result<()> {
        self.0.run(&[
            "wpctl",
            "set-mute",
            "@DEFAULT_AUDIO_SINK@",
            if muted { "1" } else { "0" },
        ])?;
        Ok(())
    }
}

/// Parse "Volume: 0.55" / "Volume: 0.55 [MUTED]".
fn parse_wpctl_volume(out: &str) -> Option<(u8, bool)> {
    let rest = out.trim().strip_prefix("Volume:")?.trim();
    let muted = rest.contains("[MUTED]");
    let value: f32 = rest.split_whitespace().next()?.parse().ok()?;
    Some(((value * 100.0).round().clamp(0.0, 100.0) as u8, muted))
}

// --- Power & OS updates (systemd) ------------------------------------------

pub const UPDATE_UNIT: &str = "silverdeck-update.service";

pub struct Power<'a>(pub &'a dyn CommandRunner);

impl Power<'_> {
    pub fn poweroff(&self) -> anyhow::Result<()> {
        self.0.run(&["systemctl", "poweroff"])?;
        Ok(())
    }

    pub fn reboot(&self) -> anyhow::Result<()> {
        self.0.run(&["systemctl", "reboot"])?;
        Ok(())
    }
}

pub struct Updates<'a>(pub &'a dyn CommandRunner);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Running,
    Failed,
}

impl Updates<'_> {
    /// Kick off the atomic OS update (polkit rule allows the deck user to
    /// start exactly this unit). Non-blocking; progress is streamed from the
    /// journal.
    pub fn start(&self) -> anyhow::Result<()> {
        self.0
            .run(&["systemctl", "start", "--no-block", UPDATE_UNIT])?;
        Ok(())
    }

    pub fn state(&self) -> UpdateState {
        match self.0.run(&["systemctl", "is-active", UPDATE_UNIT]) {
            Ok(out) if out.trim() == "activating" || out.trim() == "active" => {
                UpdateState::Running
            }
            Ok(_) => UpdateState::Idle,
            // is-active exits non-zero for inactive AND failed; disambiguate.
            Err(_) => match self.0.run(&["systemctl", "is-failed", UPDATE_UNIT]) {
                Ok(out) if out.trim() == "failed" => UpdateState::Failed,
                _ => UpdateState::Idle,
            },
        }
    }
}

/// Spawn `journalctl -f` for the update unit, streaming lines to a callback
/// until the process is killed via the returned handle. Runs on its own
/// thread (journalctl blocks).
pub fn tail_update_log(
    mut on_line: impl FnMut(String) + Send + 'static,
) -> anyhow::Result<JournalTail> {
    use std::io::BufRead as _;
    let mut child = std::process::Command::new("journalctl")
        .args(["-f", "-n", "50", "-o", "cat", "-u", UPDATE_UNIT])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to run journalctl")?;
    let stdout = child.stdout.take().expect("piped stdout");
    std::thread::Builder::new()
        .name("silverdeck-journal".into())
        .spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => on_line(line),
                    Err(_) => break,
                }
            }
        })?;
    Ok(JournalTail { child })
}

pub struct JournalTail {
    child: std::process::Child,
}

impl Drop for JournalTail {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// --- Test support -----------------------------------------------------------

/// Canned-output runner for tests and host development.
pub struct FakeRunner {
    pub responses: std::sync::Mutex<Vec<(Vec<String>, anyhow::Result<String>)>>,
    pub calls: std::sync::Mutex<Vec<Vec<String>>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        FakeRunner {
            responses: std::sync::Mutex::new(Vec::new()),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn expect(self, argv_prefix: &[&str], response: anyhow::Result<String>) -> Self {
        self.responses.lock().unwrap().push((
            argv_prefix.iter().map(|s| s.to_string()).collect(),
            response,
        ));
        self
    }
}

impl Default for FakeRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, argv: &[&str]) -> anyhow::Result<String> {
        self.calls
            .lock()
            .unwrap()
            .push(argv.iter().map(|s| s.to_string()).collect());
        let mut responses = self.responses.lock().unwrap();
        let index = responses
            .iter()
            .position(|(prefix, _)| {
                argv.len() >= prefix.len() && argv[..prefix.len()] == prefix[..]
            })
            .with_context(|| format!("unexpected command: {argv:?}"))?;
        responses.remove(index).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_ranks_wifi_list() {
        let out = "yes:87:WPA2:HomeNet\nno:92:WPA2:HomeNet\nno:55:--:CoffeeShop\nno:70:WPA1 WPA2:Neighbor\nno:40::\n";
        let networks = parse_wifi_list(out);
        assert_eq!(networks.len(), 3);
        // Connected network first even though another AP is stronger.
        assert_eq!(networks[0].ssid, "HomeNet");
        assert!(networks[0].connected);
        assert_eq!(networks[0].signal, 92);
        assert_eq!(networks[1].ssid, "Neighbor");
        assert!(networks[1].secured);
        assert!(!networks[2].secured);
    }

    #[test]
    fn terse_split_honors_escaped_colons() {
        let fields = split_terse(r"no:50:WPA2:my\:net", 4);
        assert_eq!(fields[3], "my:net");
    }

    #[test]
    fn parses_wpctl_volume() {
        assert_eq!(parse_wpctl_volume("Volume: 0.55\n"), Some((55, false)));
        assert_eq!(parse_wpctl_volume("Volume: 1.00 [MUTED]\n"), Some((100, true)));
        assert_eq!(parse_wpctl_volume("nonsense"), None);
    }

    #[test]
    fn wifi_connect_passes_password() {
        let runner = FakeRunner::new().expect(&["nmcli", "device", "wifi", "connect"], Ok(String::new()));
        Network(&runner).connect("HomeNet", Some("hunter2")).unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            ["nmcli", "device", "wifi", "connect", "HomeNet", "password", "hunter2"]
        );
    }

    #[test]
    fn update_state_disambiguates_failed() {
        let runner = FakeRunner::new()
            .expect(&["systemctl", "is-active"], Err(anyhow::anyhow!("inactive")))
            .expect(&["systemctl", "is-failed"], Ok("failed\n".into()));
        assert_eq!(Updates(&runner).state(), UpdateState::Failed);
    }
}
