//! Launcher-agnostic game discovery: one `GameSource` impl per launcher.
//!
//! Every source honors an environment override for its search root(s) so unit
//! tests (and `make ui-run` against fixtures) never touch the real home dir:
//!   SILVERDECK_STEAM_ROOT         Steam install dir (contains steamapps/)
//!   SILVERDECK_HEROIC_DIRS        colon-separated heroic config roots
//!   SILVERDECK_APPLICATIONS_DIRS  colon-separated .desktop dirs

mod desktop;
mod heroic;
mod steam;

pub use desktop::DesktopSource;
pub use heroic::HeroicSource;
pub use steam::SteamSource;

use silverdeck_core::GameSource;

/// The full launcher-agnostic source set, in dedup-priority order.
pub fn default_sources() -> Vec<Box<dyn GameSource>> {
    vec![
        Box::new(SteamSource::from_env()),
        Box::new(HeroicSource::from_env()),
        Box::new(DesktopSource::from_env()),
    ]
}

pub(crate) fn env_paths(var: &str) -> Option<Vec<std::path::PathBuf>> {
    let raw = std::env::var(var).ok()?;
    Some(std::env::split_paths(&raw).collect())
}

pub(crate) fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/home/deck".into())
}
