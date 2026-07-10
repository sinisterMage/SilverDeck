//! Steam library discovery via steamlocate (libraryfolders.vdf + *.acf).

use anyhow::Context as _;
use silverdeck_core::{Game, GameSource, LaunchSpec, SourceKind};

pub struct SteamSource {
    /// Explicit Steam root (tests / SILVERDECK_STEAM_ROOT); None = autodetect.
    root: Option<std::path::PathBuf>,
}

impl SteamSource {
    pub fn from_env() -> Self {
        SteamSource {
            root: std::env::var_os("SILVERDECK_STEAM_ROOT").map(Into::into),
        }
    }

    pub fn at(root: impl Into<std::path::PathBuf>) -> Self {
        SteamSource {
            root: Some(root.into()),
        }
    }

    fn steam_dir(&self) -> anyhow::Result<steamlocate::SteamDir> {
        match &self.root {
            Some(root) => steamlocate::SteamDir::from_dir(root)
                .with_context(|| format!("not a Steam dir: {}", root.display())),
            None => steamlocate::SteamDir::locate().context("Steam installation not found"),
        }
    }
}

/// Steam's own runtime/tool apps that show up as installed apps but are not games.
const NON_GAME_APPIDS: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime 1.0 (scout)
    1391110, // Steam Linux Runtime 2.0 (soldier)
    1628350, // Steam Linux Runtime 3.0 (sniper)
    961940,  // Proton BattlEye Runtime
    1826330, // Proton EasyAntiCheat Runtime
];

impl GameSource for SteamSource {
    fn id(&self) -> &'static str {
        "steam"
    }

    fn scan(&self) -> anyhow::Result<Vec<Game>> {
        let steam_dir = self.steam_dir()?;
        let mut games = vec![
            // Always offer Big Picture: controller-friendly entry point for
            // anything our launcher integration doesn't cover (login, store).
            Game {
                id: "steam:bigpicture".into(),
                title: "Steam Big Picture".into(),
                source: SourceKind::Steam,
                artwork: None,
                launch: LaunchSpec::Command(vec!["steam".into(), "-gamepadui".into()]),
            },
        ];
        for library in steam_dir.libraries()? {
            let library = match library {
                Ok(l) => l,
                Err(err) => {
                    log::warn!("skipping unreadable Steam library: {err}");
                    continue;
                }
            };
            for app in library.apps() {
                let app = match app {
                    Ok(a) => a,
                    Err(err) => {
                        log::warn!("skipping unreadable Steam app manifest: {err}");
                        continue;
                    }
                };
                if NON_GAME_APPIDS.contains(&app.app_id)
                    || app.name.as_deref().is_none_or(|n| n.starts_with("Proton"))
                {
                    continue;
                }
                let mut game = Game::steam(app.app_id, app.name.clone().unwrap_or_default());
                game.artwork = artwork_for(steam_dir.path(), app.app_id);
                games.push(game);
            }
        }
        Ok(games)
    }
}

/// Steam caches grid/cover art under appcache/librarycache. Try the modern
/// per-app directory layout first, then the legacy flat files.
fn artwork_for(steam_root: &std::path::Path, app_id: u32) -> Option<std::path::PathBuf> {
    let cache = steam_root.join("appcache/librarycache");
    let candidates = [
        cache.join(app_id.to_string()).join("library_600x900.jpg"),
        cache.join(app_id.to_string()).join("header.jpg"),
        cache.join(format!("{app_id}_library_600x900.jpg")),
        cache.join(format!("{app_id}_header.jpg")),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_steam_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let steamapps = dir.path().join("steamapps");
        fs::create_dir_all(&steamapps).unwrap();
        fs::write(
            steamapps.join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                dir.path().display()
            ),
        )
        .unwrap();
        fs::write(
            steamapps.join("appmanifest_620.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"620\"\n\t\"name\"\t\t\"Portal 2\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Portal 2\"\n}\n",
        )
        .unwrap();
        fs::write(
            steamapps.join("appmanifest_228980.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"228980\"\n\t\"name\"\t\t\"Steamworks Common Redistributables\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"redist\"\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn scans_games_and_filters_runtimes() {
        let root = fake_steam_root();
        let games = SteamSource::at(root.path()).scan().unwrap();
        let titles: Vec<_> = games.iter().map(|g| g.title.as_str()).collect();
        assert!(titles.contains(&"Portal 2"), "got {titles:?}");
        assert!(titles.contains(&"Steam Big Picture"));
        assert!(!titles.iter().any(|t| t.contains("Redistributables")));
        let portal = games.iter().find(|g| g.title == "Portal 2").unwrap();
        assert_eq!(portal.launch, LaunchSpec::SteamAppId(620));
        assert_eq!(portal.id, "steam:620");
    }

    #[test]
    fn missing_root_is_an_error_not_a_panic() {
        assert!(SteamSource::at("/nonexistent/steam").scan().is_err());
    }
}
