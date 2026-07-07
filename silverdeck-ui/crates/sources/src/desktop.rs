//! Desktop-entry discovery: any `.desktop` with Categories containing Game.
//!
//! This is also how installed flatpak games are found — flatpak exports a
//! desktop file per app under <installation>/exports/share/applications, so
//! one scanner covers both, and entries from exports dirs are reported as
//! `SourceKind::Flatpak` with a `flatpak run` launch spec.

use std::path::{Path, PathBuf};

use silverdeck_core::{Game, GameSource, LaunchSpec, SourceKind};

pub struct DesktopSource {
    dirs: Vec<PathBuf>,
}

impl DesktopSource {
    pub fn from_env() -> Self {
        let dirs = crate::env_paths("SILVERDECK_APPLICATIONS_DIRS").unwrap_or_else(|| {
            let home = crate::home_dir();
            vec![
                "/usr/share/applications".into(),
                home.join(".local/share/applications"),
                "/var/lib/flatpak/exports/share/applications".into(),
                home.join(".local/share/flatpak/exports/share/applications"),
            ]
        });
        DesktopSource { dirs }
    }

    pub fn at(dirs: Vec<PathBuf>) -> Self {
        DesktopSource { dirs }
    }
}

impl GameSource for DesktopSource {
    fn id(&self) -> &'static str {
        "desktop"
    }

    fn scan(&self) -> anyhow::Result<Vec<Game>> {
        let mut games = Vec::new();
        for dir in &self.dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(game) = parse_game_entry(&path, &text) {
                    games.push(game);
                }
            }
        }
        Ok(games)
    }
}

/// Parse one desktop file; None when it is not a launchable game entry.
fn parse_game_entry(path: &Path, text: &str) -> Option<Game> {
    let entry = DesktopEntry::parse(text)?;
    if entry.entry_type.as_deref() != Some("Application")
        || entry.bool_true("NoDisplay")
        || entry.bool_true("Hidden")
    {
        return None;
    }
    let categories = entry.get("Categories").unwrap_or_default();
    if !categories.split(';').any(|c| c == "Game") {
        return None;
    }
    let name = entry.get("Name")?;
    let exec = entry.get("Exec")?;
    // Steam installs a per-game desktop shortcut; the Steam source already
    // reports those with richer metadata.
    if exec.contains("steam://rungameid") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_owned();

    if is_flatpak_export(path, &exec) {
        let mut game = Game::flatpak(&stem, name);
        game.artwork = flatpak_icon(path, &stem);
        return Some(game);
    }

    let argv = parse_exec(&exec);
    if argv.is_empty() {
        return None;
    }
    Some(Game {
        id: format!("desktop:{stem}"),
        title: name,
        source: SourceKind::Desktop,
        artwork: entry
            .get("Icon")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file()),
        launch: LaunchSpec::Command(argv),
    })
}

fn is_flatpak_export(path: &Path, exec: &str) -> bool {
    path.components()
        .any(|c| c.as_os_str() == "flatpak")
        && exec.contains("flatpak run")
}

/// Icons for exported flatpaks live next to the applications dir:
/// <exports>/share/icons/hicolor/<size>/apps/<appid>.<ext>
fn flatpak_icon(desktop_path: &Path, app_id: &str) -> Option<PathBuf> {
    let share = desktop_path.parent()?.parent()?; // .../exports/share
    for size in ["512x512", "256x256", "128x128", "64x64"] {
        let png = share.join(format!("icons/hicolor/{size}/apps/{app_id}.png"));
        if png.is_file() {
            return Some(png);
        }
    }
    let svg = share.join(format!("icons/hicolor/scalable/apps/{app_id}.svg"));
    svg.is_file().then_some(svg)
}

/// Split an Exec= line per the desktop-entry spec (double-quote quoting,
/// backslash escapes) and drop %-field codes.
fn parse_exec(exec: &str) -> Vec<String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut in_quotes = false;
    let mut has_token = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            '\\' if in_quotes => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ' ' | '\t' if !in_quotes => {
                if has_token || !current.is_empty() {
                    argv.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            '%' if !in_quotes => {
                // Field code: swallow the next char, keep a literal for "%%".
                if let Some('%') = chars.next() {
                    current.push('%');
                }
            }
            _ => current.push(c),
        }
    }
    if has_token || !current.is_empty() {
        argv.push(current);
    }
    argv
}

/// Minimal [Desktop Entry] main-group parser (key=value; localized keys and
/// other groups ignored).
struct DesktopEntry {
    entry_type: Option<String>,
    pairs: Vec<(String, String)>,
}

impl DesktopEntry {
    fn parse(text: &str) -> Option<Self> {
        let mut in_main = false;
        let mut seen_main = false;
        let mut pairs = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_main = group == "Desktop Entry";
                seen_main |= in_main;
                continue;
            }
            if !in_main {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if !key.contains('[') {
                    pairs.push((key.to_owned(), value.trim().to_owned()));
                }
            }
        }
        seen_main.then(|| {
            let entry_type = pairs
                .iter()
                .find(|(k, _)| k == "Type")
                .map(|(_, v)| v.clone());
            DesktopEntry { entry_type, pairs }
        })
    }

    fn get(&self, key: &str) -> Option<String> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    fn bool_true(&self, key: &str) -> bool {
        self.get(key).as_deref() == Some("true")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_entry(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn finds_native_game_and_skips_non_games() {
        let dir = tempfile::tempdir().unwrap();
        write_entry(
            dir.path(),
            "supertux2.desktop",
            "[Desktop Entry]\nType=Application\nName=SuperTux 2\nExec=supertux2 %U\nCategories=Game;ArcadeGame;\n",
        );
        write_entry(
            dir.path(),
            "editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\nCategories=Utility;\n",
        );
        write_entry(
            dir.path(),
            "hidden-game.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nCategories=Game;\nNoDisplay=true\n",
        );
        let games = DesktopSource::at(vec![dir.path().to_path_buf()])
            .scan()
            .unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "SuperTux 2");
        assert_eq!(games[0].source, SourceKind::Desktop);
        assert_eq!(
            games[0].launch,
            LaunchSpec::Command(vec!["supertux2".into()])
        );
    }

    #[test]
    fn flatpak_export_becomes_flatpak_game() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("flatpak/exports/share/applications");
        fs::create_dir_all(&apps).unwrap();
        write_entry(
            &apps,
            "org.supertuxproject.SuperTux.desktop",
            "[Desktop Entry]\nType=Application\nName=SuperTux\nExec=/usr/bin/flatpak run --branch=stable org.supertuxproject.SuperTux\nCategories=Game;\n",
        );
        let games = DesktopSource::at(vec![apps.clone()]).scan().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].source, SourceKind::Flatpak);
        assert_eq!(games[0].id, "flatpak:org.supertuxproject.SuperTux");
        assert_eq!(
            games[0].launch,
            LaunchSpec::FlatpakRef("org.supertuxproject.SuperTux".into())
        );
    }

    #[test]
    fn steam_shortcuts_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_entry(
            dir.path(),
            "Portal 2.desktop",
            "[Desktop Entry]\nType=Application\nName=Portal 2\nExec=steam steam://rungameid/620\nCategories=Game;\n",
        );
        let games = DesktopSource::at(vec![dir.path().to_path_buf()])
            .scan()
            .unwrap();
        assert!(games.is_empty());
    }

    #[test]
    fn exec_parsing_handles_quotes_and_field_codes() {
        assert_eq!(
            parse_exec(r#""/opt/My Game/run" --fullscreen %f"#),
            vec!["/opt/My Game/run".to_owned(), "--fullscreen".to_owned()]
        );
        assert_eq!(parse_exec("game %%stuff"), vec!["game", "%stuff"]);
    }
}
