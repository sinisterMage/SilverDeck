//! The only process-facing module: disk listing, the connectivity probe, and
//! driving the bash install engine (`silverdeck-install`) non-interactively.
//!
//! The engine owns all install logic (partitioning, pacstrap, bootloader —
//! see Arch-silverblue/src/installer/); this module just launches it with the
//! `SB_INST_*` unattended contract and turns its `SILVERBLUE-INSTALL-*`
//! markers into typed events. Every output line is teed into the install log
//! so the Failed screen has something concrete to show.
//!
//! `SILVERDECK_FAKE_INSTALL=1` replays a canned run for host development
//! (`=fail` ends it with a failure); `SILVERDECK_FAKE_OFFLINE=1` additionally
//! forces the network screen.

use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context as _;

/// A drive the installer may erase, as reported by `silverdeck-install --list-disks`
/// (the live USB is already excluded there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub path: String,
    pub size: String,
    pub model: String,
}

impl Disk {
    /// lsblk's "931.5G" as the friendlier "931.5 GB".
    pub fn friendly_size(&self) -> String {
        let size = self.size.trim();
        match size.chars().last() {
            Some(unit @ ('K' | 'M' | 'G' | 'T' | 'P')) => {
                format!("{} {}B", &size[..size.len() - 1], unit)
            }
            _ => size.to_owned(),
        }
    }

    /// "931.5 GB — Samsung SSD 980" (model may be missing on VMs).
    pub fn title(&self) -> String {
        let model = self.model.trim();
        if model.is_empty() {
            format!("{} drive", self.friendly_size())
        } else {
            format!("{} — {}", self.friendly_size(), model)
        }
    }
}

pub fn fake_mode() -> bool {
    std::env::var_os("SILVERDECK_FAKE_INSTALL").is_some()
}

fn install_cmd() -> String {
    std::env::var("SILVERDECK_INSTALL_CMD").unwrap_or_else(|_| "silverdeck-install".into())
}

pub fn log_path() -> PathBuf {
    if fake_mode() {
        std::env::temp_dir().join("silverdeck-install.log")
    } else {
        PathBuf::from("/var/log/silverdeck-install.log")
    }
}

pub fn list_disks() -> anyhow::Result<Vec<Disk>> {
    if fake_mode() {
        return Ok(vec![
            Disk {
                path: "/dev/nvme0n1".into(),
                size: "931.5G".into(),
                model: "Samsung SSD 980".into(),
            },
            Disk {
                path: "/dev/sda".into(),
                size: "232.9G".into(),
                model: "KINGSTON SA400S37".into(),
            },
        ]);
    }
    let output = Command::new(install_cmd())
        .arg("--list-disks")
        .output()
        .with_context(|| format!("failed to run {} --list-disks", install_cmd()))?;
    if !output.status.success() {
        anyhow::bail!(
            "--list-disks failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parse_disk_list(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_disk_list(out: &str) -> Vec<Disk> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let path = parts.next()?.trim();
            if path.is_empty() {
                return None;
            }
            Some(Disk {
                path: path.to_owned(),
                size: parts.next().unwrap_or("").trim().to_owned(),
                model: parts.next().unwrap_or("").trim().to_owned(),
            })
        })
        .collect()
}

/// Definitive connectivity probe — the same one the engine's preflight makes
/// (reaching the package mirrors is what actually matters).
pub fn network_online() -> bool {
    if fake_mode() {
        return std::env::var_os("SILVERDECK_FAKE_OFFLINE").is_none();
    }
    Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            "5",
            "https://geo.mirror.pkgbuild.com/",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn running_as_root() -> bool {
    if fake_mode() {
        return true;
    }
    Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

/// Install stages in order, as reported by the engine's STEP markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Partition,
    Filesystems,
    Pacstrap,
    Configure,
    Bootloader,
}

impl Stage {
    fn from_marker(name: &str) -> Option<Stage> {
        Some(match name {
            "partition" => Stage::Partition,
            "filesystems" => Stage::Filesystems,
            "pacstrap" => Stage::Pacstrap,
            "configure" => Stage::Configure,
            "bootloader" => Stage::Bootloader,
            _ => return None,
        })
    }

    /// Jargon-free stage headline.
    pub fn label(&self) -> &'static str {
        match self {
            Stage::Partition => "Preparing your drive",
            Stage::Filesystems => "Setting up storage",
            Stage::Pacstrap => "Installing SilverDeck",
            Stage::Configure => "Getting things ready",
            Stage::Bootloader => "Almost done",
        }
    }

    /// Progress-bar anchors: jump to `base` when the stage starts, creep
    /// toward `ceiling` as output lines arrive (pacstrap dominates wall time).
    pub fn base_percent(&self) -> f32 {
        match self {
            Stage::Partition => 5.,
            Stage::Filesystems => 10.,
            Stage::Pacstrap => 15.,
            Stage::Configure => 90.,
            Stage::Bootloader => 97.,
        }
    }

    pub fn ceiling_percent(&self) -> f32 {
        match self {
            Stage::Partition => 10.,
            Stage::Filesystems => 15.,
            Stage::Pacstrap => 85.,
            Stage::Configure => 97.,
            Stage::Bootloader => 99.,
        }
    }
}

pub enum InstallEvent {
    /// One non-marker output line (already logged to the install log).
    Line(String),
    Stage(Stage),
    Finished(Result<(), String>),
}

/// Launch the install and stream events until Finished. Never blocks the
/// caller: process IO runs on plain threads (same shape as the console
/// shell's journal tail).
pub fn spawn_install(disk: String, tx: async_channel::Sender<InstallEvent>) {
    if fake_mode() {
        spawn_fake(tx);
        return;
    }
    std::thread::Builder::new()
        .name("silverdeck-install".into())
        .spawn(move || {
            let result = run_install(&disk, &tx);
            let outcome = match result {
                Ok(true) => Ok(()),
                Ok(false) => Err(
                    "The install script stopped before finishing. No changes may be complete."
                        .to_owned(),
                ),
                Err(err) => Err(format!("{err:#}")),
            };
            let _ = tx.send_blocking(InstallEvent::Finished(outcome));
        })
        .ok();
}

/// Run the engine to completion; Ok(true) only when it printed its OK marker
/// and exited cleanly.
fn run_install(disk: &str, tx: &async_channel::Sender<InstallEvent>) -> anyhow::Result<bool> {
    let log = Arc::new(Mutex::new(
        std::fs::File::create(log_path())
            .with_context(|| format!("cannot write {}", log_path().display()))?,
    ));
    let mut child = Command::new(install_cmd())
        .env("SB_INSTALL_MARKERS", "1")
        .env("SB_INST_UNATTENDED", "1")
        .env("SB_INST_DISK", disk)
        .env("SB_INST_CONFIRM", "ERASE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run {}", install_cmd()))?;

    let saw_ok = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    for stream in [
        Box::new(stdout) as Box<dyn std::io::Read + Send>,
        Box::new(stderr),
    ] {
        let tx = tx.clone();
        let log = log.clone();
        let saw_ok = saw_ok.clone();
        readers.push(std::thread::spawn(move || {
            for line in std::io::BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                if let Ok(mut file) = log.lock() {
                    let _ = writeln!(file, "{line}");
                }
                if let Some(rest) = line.strip_prefix("SILVERBLUE-INSTALL-STEP name=") {
                    if let Some(stage) = Stage::from_marker(rest.trim()) {
                        let _ = tx.send_blocking(InstallEvent::Stage(stage));
                    }
                } else if line.starts_with("SILVERBLUE-INSTALL-OK") {
                    saw_ok.store(true, Ordering::SeqCst);
                } else if !line.starts_with("SILVERBLUE-INSTALL-") {
                    let _ = tx.send_blocking(InstallEvent::Line(line));
                }
            }
        }));
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child.wait().context("waiting for the install script")?;
    Ok(status.success() && saw_ok.load(Ordering::SeqCst))
}

/// Canned run for host development: same event sequence and pacing shape as a
/// real install, compressed to ~15 seconds.
fn spawn_fake(tx: async_channel::Sender<InstallEvent>) {
    let fail = std::env::var("SILVERDECK_FAKE_INSTALL")
        .map(|v| v == "fail")
        .unwrap_or(false);
    std::thread::spawn(move || {
        let send = |ev| tx.send_blocking(ev).is_ok();
        let _ = std::fs::write(log_path(), "fake install run\n");
        let quick: &[(Stage, &[&str])] = &[
            (
                Stage::Partition,
                &["==> wiping /dev/nvme0n1", "==> creating ESP + Btrfs"],
            ),
            (
                Stage::Filesystems,
                &["==> mkfs.btrfs", "==> mounting subvolumes"],
            ),
        ];
        for (stage, lines) in quick {
            if !send(InstallEvent::Stage(*stage)) {
                return;
            }
            for line in *lines {
                std::thread::sleep(Duration::from_millis(350));
                if !send(InstallEvent::Line((*line).to_owned())) {
                    return;
                }
            }
        }
        if !send(InstallEvent::Stage(Stage::Pacstrap)) {
            return;
        }
        for i in 1..=40 {
            std::thread::sleep(Duration::from_millis(200));
            if fail && i == 25 {
                let _ = send(InstallEvent::Line(
                    "error: failed retrieving file 'mesa' from mirror".to_owned(),
                ));
                let _ = send(InstallEvent::Finished(Err(
                    "The install script stopped before finishing. No changes may be complete."
                        .to_owned(),
                )));
                return;
            }
            if !send(InstallEvent::Line(format!(
                "( {i:2}/40) installing package-{i}..."
            ))) {
                return;
            }
        }
        for stage in [Stage::Configure, Stage::Bootloader] {
            if !send(InstallEvent::Stage(stage)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(900));
        }
        let _ = send(InstallEvent::Finished(Ok(())));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_disk_list_lines() {
        let disks = parse_disk_list("/dev/vda|12G|\n/dev/nvme0n1|931.5G|Samsung SSD 980\n\n");
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].path, "/dev/vda");
        assert_eq!(disks[0].model, "");
        assert_eq!(disks[1].title(), "931.5 GB — Samsung SSD 980");
    }

    #[test]
    fn friendly_sizes() {
        let disk = |size: &str| Disk {
            path: String::new(),
            size: size.into(),
            model: String::new(),
        };
        assert_eq!(disk("931.5G").friendly_size(), "931.5 GB");
        assert_eq!(disk("2T").friendly_size(), "2 TB");
        assert_eq!(disk("").friendly_size(), "");
    }

    #[test]
    fn stage_markers_map() {
        assert_eq!(Stage::from_marker("pacstrap"), Some(Stage::Pacstrap));
        assert_eq!(Stage::from_marker("bogus"), None);
    }
}
