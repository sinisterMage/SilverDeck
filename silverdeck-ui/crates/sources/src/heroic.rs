//! Heroic Games Launcher (Epic via legendary, GOG, sideloaded) discovery.
//!
//! Heroic keeps per-store installed lists as JSON under its config dir:
//!   legendaryConfig/legendary/installed.json   {app: {title, ...}}
//!   gog_store/installed.json                   {installed: [{appName, ...}]}
//!   store_cache/gog_library.json               {games: [{app_name, title}]}
//!   sideload_apps/library.json                 {games: [{app_name, title}]}
//! Both the native (~/.config/heroic) and flatpak
//! (~/.var/app/com.heroicgameslauncher.hgl/config/heroic) roots are scanned.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use silverdeck_core::{Game, GameSource, LaunchSpec, SourceKind};

pub struct HeroicSource {
    roots: Vec<PathBuf>,
}

impl HeroicSource {
    pub fn from_env() -> Self {
        let roots = crate::env_paths("SILVERDECK_HEROIC_DIRS").unwrap_or_else(|| {
            let home = crate::home_dir();
            vec![
                home.join(".config/heroic"),
                home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"),
            ]
        });
        HeroicSource { roots }
    }

    pub fn at(roots: Vec<PathBuf>) -> Self {
        HeroicSource { roots }
    }
}

impl GameSource for HeroicSource {
    fn id(&self) -> &'static str {
        "heroic"
    }

    fn scan(&self) -> anyhow::Result<Vec<Game>> {
        let mut games = Vec::new();
        for root in &self.roots {
            if !root.is_dir() {
                continue;
            }
            games.extend(scan_legendary(root));
            games.extend(scan_gog(root));
            games.extend(scan_sideload(root));
        }
        Ok(games)
    }
}

fn heroic_game(runner: &str, id: &str, title: String) -> Game {
    Game {
        id: format!("heroic:{runner}:{id}"),
        title,
        source: SourceKind::Heroic,
        artwork: None,
        launch: LaunchSpec::Heroic {
            runner: runner.to_owned(),
            id: id.to_owned(),
        },
    }
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(err) => {
            log::warn!("unparseable heroic file {}: {err}", path.display());
            None
        }
    }
}

fn scan_legendary(root: &Path) -> Vec<Game> {
    let Some(map) = read_json(&root.join("legendaryConfig/legendary/installed.json")) else {
        return Vec::new();
    };
    let Some(obj) = map.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .map(|(app, entry)| {
            let title = entry
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or(app)
                .to_owned();
            heroic_game("legendary", app, title)
        })
        .collect()
}

fn scan_gog(root: &Path) -> Vec<Game> {
    let Some(installed) = read_json(&root.join("gog_store/installed.json")) else {
        return Vec::new();
    };
    // Titles live in the library cache, not the install list.
    let titles: HashMap<String, String> = read_json(&root.join("store_cache/gog_library.json"))
        .and_then(|lib| {
            let games = lib.get("games")?.as_array()?.clone();
            Some(
                games
                    .iter()
                    .filter_map(|g| {
                        Some((
                            g.get("app_name")?.as_str()?.to_owned(),
                            g.get("title")?.as_str()?.to_owned(),
                        ))
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();

    let Some(list) = installed.get("installed").and_then(|i| i.as_array()) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let app = entry.get("appName")?.as_str()?;
            let title = titles.get(app).cloned().unwrap_or_else(|| app.to_owned());
            Some(heroic_game("gog", app, title))
        })
        .collect()
}

fn scan_sideload(root: &Path) -> Vec<Game> {
    let Some(lib) = read_json(&root.join("sideload_apps/library.json")) else {
        return Vec::new();
    };
    let Some(list) = lib.get("games").and_then(|g| g.as_array()) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let app = entry.get("app_name")?.as_str()?;
            let title = entry
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or(app)
                .to_owned();
            Some(heroic_game("sideload", app, title))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_all_three_runners() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("legendaryConfig/legendary")).unwrap();
        fs::write(
            root.join("legendaryConfig/legendary/installed.json"),
            r#"{"Fortnite": {"title": "Fortnite", "install_path": "/games/fortnite"}}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("gog_store")).unwrap();
        fs::write(
            root.join("gog_store/installed.json"),
            r#"{"installed": [{"appName": "1207658924", "install_path": "/games/witcher"}]}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("store_cache")).unwrap();
        fs::write(
            root.join("store_cache/gog_library.json"),
            r#"{"games": [{"app_name": "1207658924", "title": "The Witcher"}]}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("sideload_apps")).unwrap();
        fs::write(
            root.join("sideload_apps/library.json"),
            r#"{"games": [{"app_name": "abc", "title": "Local Build"}]}"#,
        )
        .unwrap();

        let games = HeroicSource::at(vec![root.to_path_buf()]).scan().unwrap();
        let mut ids: Vec<_> = games.iter().map(|g| g.id.as_str()).collect();
        ids.sort();
        assert_eq!(
            ids,
            [
                "heroic:gog:1207658924",
                "heroic:legendary:Fortnite",
                "heroic:sideload:abc"
            ]
        );
        let witcher = games.iter().find(|g| g.id.contains("gog")).unwrap();
        assert_eq!(witcher.title, "The Witcher");
    }

    #[test]
    fn gog_title_falls_back_to_app_name_without_library_cache() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("gog_store")).unwrap();
        fs::write(
            root.join("gog_store/installed.json"),
            r#"{"installed": [{"appName": "42"}]}"#,
        )
        .unwrap();
        let games = HeroicSource::at(vec![root.to_path_buf()]).scan().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "42");
    }

    #[test]
    fn missing_root_yields_empty() {
        let games = HeroicSource::at(vec!["/nonexistent".into()]).scan().unwrap();
        assert!(games.is_empty());
    }
}
