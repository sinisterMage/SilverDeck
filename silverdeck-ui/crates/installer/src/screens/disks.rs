//! Drive picker: friendly names, whole-drive install, live USB never listed
//! (the engine's --list-disks already excludes it).

use gpui::{div, prelude::*, px, Context};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::theme;

use crate::engine;
use crate::root::{InstallerRoot, Screen};
use crate::screens::frame;

#[derive(Default)]
pub struct DisksState {
    pub disks: Vec<engine::Disk>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
}

pub fn enter(root: &mut InstallerRoot, cx: &mut Context<InstallerRoot>) {
    root.screen = Screen::Disks;
    load(root, cx);
}

pub fn load(root: &mut InstallerRoot, cx: &mut Context<InstallerRoot>) {
    if root.disks.loading {
        return;
    }
    root.disks.loading = true;
    root.disks.error = None;
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let result = background.spawn(async move { engine::list_disks() }).await;
        this.update(cx, |root, cx| {
            let d = &mut root.disks;
            d.loading = false;
            match result {
                Ok(disks) => {
                    d.selected = d.selected.min(disks.len().saturating_sub(1));
                    d.disks = disks;
                }
                Err(err) => d.error = Some(format!("{err:#}")),
            }
            cx.notify();
        })
        .ok();
    })
    .detach();
}

pub fn handle_nav(root: &mut InstallerRoot, event: NavEvent, cx: &mut Context<InstallerRoot>) {
    match event {
        NavEvent::Up => root.disks.selected = root.disks.selected.saturating_sub(1),
        NavEvent::Down => {
            root.disks.selected =
                (root.disks.selected + 1).min(root.disks.disks.len().saturating_sub(1));
        }
        NavEvent::Confirm => {
            if let Some(disk) = root.disks.disks.get(root.disks.selected).cloned() {
                root.confirm.disk = Some(disk);
                root.confirm.erase = false;
                root.screen = Screen::Confirm;
            }
        }
        NavEvent::Back => root.screen = Screen::Welcome,
        NavEvent::Menu => load(root, cx),
        _ => {}
    }
}

pub fn render(root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let d = &root.disks;
    let selected = d.selected;

    let status = if d.loading {
        "Looking for drives…".to_owned()
    } else if let Some(err) = &d.error {
        format!("Couldn't look for drives: {err}")
    } else if d.disks.is_empty() {
        "No drives found. Is one plugged in?".to_owned()
    } else {
        String::new()
    };

    let body = div()
        .flex()
        .flex_col()
        .gap_2()
        .w(px(560.))
        .children(d.disks.iter().enumerate().map(|(i, disk)| {
            let active = i == selected;
            div()
                .flex()
                .flex_col()
                .px_4()
                .py_2()
                .rounded_md()
                .bg(if active {
                    theme::panel_hi()
                } else {
                    theme::panel()
                })
                .border_2()
                .border_color(if active {
                    theme::accent()
                } else {
                    theme::panel()
                })
                .child(div().child(disk.title()))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::text_dim())
                        .child(disk.path.clone()),
                )
        }))
        .when(!status.is_empty(), |el| {
            el.child(div().text_sm().text_color(theme::text_dim()).child(status))
        });

    frame(
        "Choose where to install",
        "SilverDeck uses the whole drive.",
        body,
    )
}
