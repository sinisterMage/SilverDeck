//! Core domain model for the SilverDeck console UI: games, launch specs and
//! the launcher-agnostic `GameSource` abstraction.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a game was discovered. Ordering doubles as dedup priority:
/// earlier variants win when two sources report the same game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    Steam,
    Flatpak,
    Heroic,
    Desktop,
}

impl SourceKind {
    pub fn label(&self) -> &'static str {
        match self {
            SourceKind::Steam => "Steam",
            SourceKind::Flatpak => "Flatpak",
            SourceKind::Heroic => "Heroic",
            SourceKind::Desktop => "Desktop",
        }
    }
}

/// How to start a game. Kept declarative so the launch crate owns process
/// mechanics (gamescope wrapping, session tracking) in one place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchSpec {
    /// argv, already split (never shell-interpreted).
    Command(Vec<String>),
    SteamAppId(u32),
    FlatpakRef(String),
    Heroic { runner: String, id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    /// Stable dedup key, e.g. "steam:620", "flatpak:org.example.Game".
    pub id: String,
    pub title: String,
    pub source: SourceKind,
    /// Path to cover/icon art if the source provides one.
    pub artwork: Option<PathBuf>,
    pub launch: LaunchSpec,
}

impl Game {
    pub fn steam(app_id: u32, title: impl Into<String>) -> Self {
        Game {
            id: format!("steam:{app_id}"),
            title: title.into(),
            source: SourceKind::Steam,
            artwork: None,
            launch: LaunchSpec::SteamAppId(app_id),
        }
    }

    pub fn flatpak(app_id: &str, title: impl Into<String>) -> Self {
        Game {
            id: format!("flatpak:{app_id}"),
            title: title.into(),
            source: SourceKind::Flatpak,
            artwork: None,
            launch: LaunchSpec::FlatpakRef(app_id.to_owned()),
        }
    }
}

/// A launcher backend the library can enumerate games from. Implementations
/// are synchronous; the app runs scans on the background executor.
pub trait GameSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn scan(&self) -> anyhow::Result<Vec<Game>>;
}

/// Scan every source, drop per-key duplicates (highest-priority `SourceKind`
/// wins, then first occurrence), and sort by title for a stable grid order.
/// A failing source is skipped (reported via the returned errors) rather than
/// failing the whole library.
pub fn scan_all(sources: &[Box<dyn GameSource>]) -> (Vec<Game>, Vec<(String, anyhow::Error)>) {
    let mut games = Vec::new();
    let mut errors = Vec::new();
    for source in sources {
        match source.scan() {
            Ok(mut found) => games.append(&mut found),
            Err(err) => errors.push((source.id().to_owned(), err)),
        }
    }
    (dedup_games(games), errors)
}

pub fn dedup_games(mut games: Vec<Game>) -> Vec<Game> {
    // Stable sort so that for equal ids the preferred source comes first,
    // then keep the first of each id.
    games.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.source.cmp(&b.source))
    });
    let mut seen = std::collections::HashSet::new();
    games.retain(|g| seen.insert(g.id.clone()));
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_prefers_higher_priority_source() {
        let dup_desktop = Game {
            id: "flatpak:org.example.Game".into(),
            title: "Example Game".into(),
            source: SourceKind::Desktop,
            artwork: None,
            launch: LaunchSpec::Command(vec!["example".into()]),
        };
        let games = dedup_games(vec![
            dup_desktop,
            Game::flatpak("org.example.Game", "Example Game"),
            Game::steam(620, "Portal 2"),
        ]);
        assert_eq!(games.len(), 2);
        let example = games.iter().find(|g| g.id.contains("example")).unwrap();
        assert_eq!(example.source, SourceKind::Flatpak);
    }

    #[test]
    fn scan_all_collects_source_errors_without_failing() {
        struct Ok1;
        impl GameSource for Ok1 {
            fn id(&self) -> &'static str {
                "ok"
            }
            fn scan(&self) -> anyhow::Result<Vec<Game>> {
                Ok(vec![Game::steam(400, "Portal")])
            }
        }
        struct Broken;
        impl GameSource for Broken {
            fn id(&self) -> &'static str {
                "broken"
            }
            fn scan(&self) -> anyhow::Result<Vec<Game>> {
                anyhow::bail!("no config dir")
            }
        }
        let (games, errors) = scan_all(&[Box::new(Ok1), Box::new(Broken)]);
        assert_eq!(games.len(), 1);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "broken");
    }

    #[test]
    fn games_sorted_by_title_case_insensitive() {
        let games = dedup_games(vec![
            Game::steam(1, "zelda-like"),
            Game::steam(2, "Aperture"),
        ]);
        assert_eq!(games[0].title, "Aperture");
    }
}
