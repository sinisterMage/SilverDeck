//! Store tab: curated Flathub allowlist with install/uninstall + progress.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui::{div, img, prelude::*, px, Context};
use silverdeck_input::NavEvent;
use silverdeck_store::{FlathubClient, InstallEvent, StoreCatalog};
use silverdeck_system::HostRunner;

use crate::modal::{Modal, PendingAction};
use crate::root::RootView;
use crate::{library, theme};

static RUNNER: HostRunner = HostRunner;
const VISIBLE: usize = 8;

#[derive(Clone)]
pub struct StoreApp {
    pub app_id: String,
    pub category: String,
    pub name: String,
    pub summary: String,
    pub icon: Option<PathBuf>,
}

#[derive(Default)]
pub struct StoreState {
    pub apps: Vec<StoreApp>,
    pub selected: usize,
    pub installed: HashSet<String>,
    /// app id -> last seen progress percent.
    pub installing: HashMap<String, u8>,
    pub load_error: Option<String>,
}

pub fn load_catalog(_root: &mut RootView, cx: &mut Context<RootView>) {
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let loaded = background
            .spawn(async move {
                let catalog = StoreCatalog::load_default()?;
                let installed = silverdeck_store::installed_apps(&RUNNER).unwrap_or_default();
                anyhow::Ok((catalog, installed))
            })
            .await;
        let (catalog, installed) = match loaded {
            Ok(pair) => pair,
            Err(err) => {
                this.update(cx, |root, cx| {
                    root.store.load_error = Some(format!("{err:#}"));
                    cx.notify();
                })
                .ok();
                return;
            }
        };
        this.update(cx, |root, cx| {
            root.store.apps = catalog
                .categories
                .iter()
                .flat_map(|category| {
                    category.apps.iter().map(|app_id| StoreApp {
                        app_id: app_id.clone(),
                        category: category.name.clone(),
                        // Placeholder until Flathub metadata lands.
                        name: app_id.rsplit('.').next().unwrap_or(app_id).to_owned(),
                        summary: String::new(),
                        icon: None,
                    })
                })
                .collect();
            root.store.installed = installed;
            cx.notify();
        })
        .ok();

        // Enrich rows with Flathub metadata one app at a time so the UI
        // fills in as results arrive (and still works fully offline).
        let ids: Vec<String> = this
            .update(cx, |root, _| {
                root.store.apps.iter().map(|a| a.app_id.clone()).collect()
            })
            .unwrap_or_default();
        let (tx, rx) = async_channel::unbounded();
        background
            .spawn(async move {
                let client = FlathubClient::new();
                for id in ids {
                    match client.meta(&id) {
                        Ok(meta) => {
                            if tx.send_blocking((id, meta)).is_err() {
                                return;
                            }
                        }
                        Err(err) => log::warn!("flathub metadata failed for {id}: {err:#}"),
                    }
                }
            })
            .detach();
        while let Ok((id, meta)) = rx.recv().await {
            if this
                .update(cx, |root, cx| {
                    if let Some(app) = root.store.apps.iter_mut().find(|a| a.app_id == id) {
                        app.name = meta.name;
                        app.summary = meta.summary;
                        app.icon = meta.icon;
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

pub fn handle_nav(root: &mut RootView, event: NavEvent, cx: &mut Context<RootView>) {
    let count = root.store.apps.len();
    match event {
        NavEvent::Up => move_selection(root, -1, count),
        NavEvent::Down => move_selection(root, 1, count),
        NavEvent::Confirm => {
            let Some(app) = root.store.apps.get(root.store.selected) else {
                return;
            };
            let id = app.app_id.clone();
            let name = app.name.clone();
            if root.store.installing.contains_key(&id) {
                return;
            }
            if root.store.installed.contains(&id) {
                root.toast(format!("{name} is already installed"), false, cx);
                return;
            }
            root.modal = Some(Modal::confirm(
                format!("Install {name}?"),
                PendingAction::Install(id),
            ));
        }
        NavEvent::Menu => {
            let Some(app) = root.store.apps.get(root.store.selected) else {
                return;
            };
            if root.store.installed.contains(&app.app_id)
                && !root.store.installing.contains_key(&app.app_id)
            {
                root.modal = Some(Modal::confirm(
                    format!("Uninstall {}?", app.name),
                    PendingAction::Uninstall(app.app_id.clone()),
                ));
            }
        }
        _ => {}
    }
}

fn move_selection(root: &mut RootView, delta: isize, count: usize) {
    if count == 0 {
        return;
    }
    let next = root.store.selected as isize + delta;
    root.store.selected = next.clamp(0, count as isize - 1) as usize;
}

pub fn begin_install(root: &mut RootView, app_id: String, cx: &mut Context<RootView>) {
    root.store.installing.insert(app_id.clone(), 0);
    let (tx, rx) = async_channel::unbounded();
    let id = app_id.clone();
    cx.background_executor()
        .spawn(async move {
            // The firstboot unit normally adds flathub; retry here in case it
            // hasn't run yet (fresh install without network at boot).
            if let Err(err) = silverdeck_store::ensure_flathub_remote(&RUNNER) {
                log::warn!("flathub remote setup failed: {err:#}");
            }
            let progress_tx = tx.clone();
            if let Err(err) = silverdeck_store::install(&id, move |event| {
                let _ = progress_tx.send_blocking(event);
            }) {
                let _ = tx.send_blocking(InstallEvent::Done {
                    success: false,
                    message: format!("{err:#}"),
                });
            }
        })
        .detach();

    cx.spawn(async move |this, cx| {
        while let Ok(event) = rx.recv().await {
            let done = matches!(event, InstallEvent::Done { .. });
            if this
                .update(cx, |root, cx| on_install_event(root, &app_id, event, cx))
                .is_err()
                || done
            {
                break;
            }
        }
    })
    .detach();
}

fn on_install_event(
    root: &mut RootView,
    app_id: &str,
    event: InstallEvent,
    cx: &mut Context<RootView>,
) {
    match event {
        InstallEvent::Progress(percent) => {
            root.store.installing.insert(app_id.to_owned(), percent);
        }
        InstallEvent::Done { success, message } => {
            root.store.installing.remove(app_id);
            if success {
                root.store.installed.insert(app_id.to_owned());
                library::rescan(root, cx);
            }
            root.toast(message, !success, cx);
        }
    }
    cx.notify();
}

pub fn begin_uninstall(_root: &mut RootView, app_id: String, cx: &mut Context<RootView>) {
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let result = background
            .spawn(async move { silverdeck_store::uninstall(&RUNNER, &app_id).map(|()| app_id) })
            .await;
        this.update(cx, |root, cx| match result {
            Ok(app_id) => {
                root.store.installed.remove(&app_id);
                root.toast(format!("{app_id} removed"), false, cx);
                library::rescan(root, cx);
            }
            Err(err) => root.toast(format!("{err:#}"), true, cx),
        })
        .ok();
    })
    .detach();
}

pub fn render(root: &RootView, _cx: &mut Context<RootView>) -> impl IntoElement {
    let store = &root.store;
    if let Some(error) = &store.load_error {
        return div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_color(theme::err())
            .child(error.clone())
            .into_any_element();
    }
    if store.apps.is_empty() {
        return div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_color(theme::text_dim())
            .child("Loading store catalog…")
            .into_any_element();
    }

    let first = store
        .selected
        .saturating_sub(VISIBLE / 2)
        .min(store.apps.len().saturating_sub(VISIBLE));
    let rows = (first..(first + VISIBLE).min(store.apps.len())).map(|index| {
        let app = &store.apps[index];
        let selected = index == store.selected;
        let show_category = index == first
            || store
                .apps
                .get(index.wrapping_sub(1))
                .is_none_or(|prev| prev.category != app.category);
        row(app, selected, show_category, store)
    });

    div()
        .flex()
        .flex_col()
        .size_full()
        .px_8()
        .gap_1()
        .children(rows)
        .child(
            div()
                .pt_2()
                .text_sm()
                .text_color(theme::text_dim())
                .child("A install · Start uninstall"),
        )
        .into_any_element()
}

fn row(
    app: &StoreApp,
    selected: bool,
    show_category: bool,
    store: &StoreState,
) -> impl IntoElement {
    let status = if let Some(percent) = store.installing.get(&app.app_id) {
        div()
            .text_color(theme::accent())
            .child(format!("installing {percent}%"))
    } else if store.installed.contains(&app.app_id) {
        div()
            .text_color(theme::ok())
            .child("installed ✓".to_string())
    } else {
        div()
            .text_color(theme::text_dim())
            .child("A to install".to_string())
    };

    let icon = match &app.icon {
        Some(path) => img(path.clone())
            .size(px(28.))
            .rounded_sm()
            .into_any_element(),
        None => div()
            .size(px(28.))
            .rounded_sm()
            .bg(theme::panel_hi())
            .into_any_element(),
    };

    div()
        .flex()
        .flex_col()
        .when(show_category, |d| {
            d.child(
                div()
                    .pt_2()
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme::accent_dim())
                    .child(app.category.clone()),
            )
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(if selected {
                    theme::panel_hi()
                } else {
                    theme::bg()
                })
                .child(icon)
                .child(
                    div()
                        .flex_grow()
                        .flex()
                        .flex_col()
                        .child(div().child(app.name.clone()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme::text_dim())
                                .child(app.summary.clone()),
                        ),
                )
                .child(status),
        )
}
