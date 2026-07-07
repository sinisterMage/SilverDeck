//! Library tab: launcher-agnostic game grid + game session lifecycle.

use gpui::{div, img, prelude::*, px, Context, SharedString};
use silverdeck_core::{Game, SourceKind};
use silverdeck_input::NavEvent;
use silverdeck_launch::{Gamescope, LaunchOptions, SessionEvent};

use crate::root::RootView;
use crate::theme;

const COLUMNS: usize = 4;
const VISIBLE_ROWS: usize = 3;

#[derive(Default)]
pub struct LibraryState {
    pub games: Vec<Game>,
    pub selected: usize,
    pub scanning: bool,
    /// Title of the game currently owning the screen.
    pub session: Option<SharedString>,
    /// Wrap the next launch in gamescope (toggled with Menu/F1).
    pub gamescope: bool,
}

pub fn rescan(root: &mut RootView, cx: &mut Context<RootView>) {
    if root.library.scanning {
        return;
    }
    root.library.scanning = true;
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let (games, errors) = background.spawn(async move { scan_games() }).await;
        this.update(cx, |root, cx| {
            root.library.scanning = false;
            root.library.games = games;
            root.library.selected = root
                .library
                .selected
                .min(root.library.games.len().saturating_sub(1));
            for error in errors {
                log::warn!("game source failed: {error}");
            }
            cx.notify();
        })
        .ok();
    })
    .detach();
}

fn scan_games() -> (Vec<Game>, Vec<String>) {
    if std::env::var_os("SILVERDECK_FAKE_GAMES").is_some() {
        return (fake_games(), Vec::new());
    }
    let sources = silverdeck_sources::default_sources();
    let (games, errors) = silverdeck_core::scan_all(&sources);
    (
        games,
        errors
            .into_iter()
            .map(|(id, err)| format!("{id}: {err:#}"))
            .collect(),
    )
}

fn fake_games() -> Vec<Game> {
    let mut games: Vec<Game> = (1..=9)
        .map(|i| Game {
            id: format!("fake:{i}"),
            title: format!("Demo Game {i}"),
            source: SourceKind::Desktop,
            artwork: None,
            launch: silverdeck_core::LaunchSpec::Command(vec!["sleep".into(), "2".into()]),
        })
        .collect();
    games.insert(0, Game::steam(620, "Portal 2 (fake)"));
    games
}

pub fn handle_nav(root: &mut RootView, event: NavEvent, cx: &mut Context<RootView>) {
    let count = root.library.games.len();
    match event {
        NavEvent::Left => move_selection(root, -1, count),
        NavEvent::Right => move_selection(root, 1, count),
        NavEvent::Up => move_selection(root, -(COLUMNS as isize), count),
        NavEvent::Down => move_selection(root, COLUMNS as isize, count),
        NavEvent::Confirm => launch_selected(root, cx),
        NavEvent::Menu => {
            root.library.gamescope = !root.library.gamescope;
            let state = if root.library.gamescope { "on" } else { "off" };
            root.toast(format!("gamescope wrapping {state}"), false, cx);
        }
        NavEvent::Back => {}
        NavEvent::TabNext | NavEvent::TabPrev => unreachable!("handled by root"),
    }
}

fn move_selection(root: &mut RootView, delta: isize, count: usize) {
    if count == 0 {
        return;
    }
    let next = root.library.selected as isize + delta;
    root.library.selected = next.clamp(0, count as isize - 1) as usize;
}

pub fn launch_selected(root: &mut RootView, cx: &mut Context<RootView>) {
    if root.library.session.is_some() {
        return;
    }
    let Some(game) = root.library.games.get(root.library.selected) else {
        return;
    };
    let spec = game.launch.clone();
    let title: SharedString = game.title.clone().into();
    let opts = LaunchOptions {
        gamescope: root.library.gamescope.then_some(Gamescope {
            width: 1920,
            height: 1080,
            fps_limit: None,
        }),
    };
    root.library.session = Some(title.clone());

    let (tx, rx) = async_channel::unbounded();
    let failure_tx = tx.clone();
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = silverdeck_launch::run_session(spec, opts, tx).await {
                log::error!("launch failed: {err:#}");
                let _ = failure_tx
                    .send(SessionEvent::Exited { success: false })
                    .await;
            }
        })
        .detach();

    cx.spawn(async move |this, cx| {
        while let Ok(event) = rx.recv().await {
            let done = matches!(event, SessionEvent::Exited { .. });
            if this
                .update(cx, |root, cx| on_session_event(root, event, cx))
                .is_err()
                || done
            {
                break;
            }
        }
    })
    .detach();
}

fn on_session_event(root: &mut RootView, event: SessionEvent, cx: &mut Context<RootView>) {
    match event {
        SessionEvent::Started => {}
        SessionEvent::Exited { success } => {
            let title = root.library.session.take();
            if !success {
                let name = title.map(|t| t.to_string()).unwrap_or_else(|| "game".into());
                root.toast(format!("{name} exited with an error"), true, cx);
            }
        }
    }
    cx.notify();
}

pub fn render(root: &RootView, _cx: &mut Context<RootView>) -> impl IntoElement {
    let library = &root.library;
    if library.games.is_empty() {
        let message = if library.scanning {
            "Scanning game libraries…"
        } else {
            "No games found — install some from the Store tab"
        };
        return div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_color(theme::text_dim())
            .text_lg()
            .child(message)
            .into_any_element();
    }

    let selected = library.selected;
    let selected_row = selected / COLUMNS;
    let total_rows = library.games.len().div_ceil(COLUMNS);
    let first_row = selected_row
        .saturating_sub(1)
        .min(total_rows.saturating_sub(VISIBLE_ROWS));

    let rows = (first_row..(first_row + VISIBLE_ROWS).min(total_rows)).map(|row| {
        let start = row * COLUMNS;
        let end = (start + COLUMNS).min(library.games.len());
        div()
            .flex()
            .flex_row()
            .gap_4()
            .children((start..end).map(|index| tile(&library.games[index], index == selected)))
    });

    div()
        .flex()
        .flex_col()
        .size_full()
        .px_8()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_sm()
                .text_color(theme::text_dim())
                .child(format!("{} games", library.games.len()))
                .child(if library.gamescope {
                    "gamescope: on (Start toggles)"
                } else {
                    "gamescope: off (Start toggles)"
                }),
        )
        .children(rows)
        .into_any_element()
}

fn tile(game: &Game, selected: bool) -> impl IntoElement {
    let cover = match &game.artwork {
        Some(path) => img(path.clone())
            .size_full()
            .object_fit(gpui::ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_xl()
            .text_color(theme::text_dim())
            .child(game.title.chars().next().unwrap_or('?').to_string())
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .w(px(220.))
        .gap_1()
        .child(
            div()
                .h(px(120.))
                .w_full()
                .rounded_md()
                .overflow_hidden()
                .bg(theme::panel())
                .border_2()
                .border_color(if selected {
                    theme::accent()
                } else {
                    theme::panel()
                })
                .child(cover),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_sm()
                .child(
                    div()
                        .text_color(if selected { theme::text() } else { theme::text_dim() })
                        .child(game.title.clone()),
                )
                .child(
                    div()
                        .text_color(theme::accent_dim())
                        .child(game.source.label()),
                ),
        )
}
