//! The one scary screen. "Go back" is the default; the erase button is the
//! destructive variant and needs a deliberate move to reach.

use gpui::{div, prelude::*, Context};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::theme;

use crate::engine;
use crate::root::{InstallerRoot, Screen};
use crate::screens::{button, frame, progress};

#[derive(Default)]
pub struct ConfirmState {
    pub disk: Option<engine::Disk>,
    /// True when the erase button is focused (never the default).
    pub erase: bool,
}

pub fn handle_nav(root: &mut InstallerRoot, event: NavEvent, cx: &mut Context<InstallerRoot>) {
    match event {
        NavEvent::Left | NavEvent::Right | NavEvent::Up | NavEvent::Down => {
            root.confirm.erase = !root.confirm.erase;
        }
        NavEvent::Confirm => {
            if root.confirm.erase {
                progress::start(root, cx);
            } else {
                root.screen = Screen::Disks;
            }
        }
        NavEvent::Back => root.screen = Screen::Disks,
        _ => {}
    }
}

pub fn render(root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let erase = root.confirm.erase;
    let title = match &root.confirm.disk {
        Some(disk) => format!("Install to {}?", disk.title()),
        None => "Install?".to_owned(),
    };
    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_6()
        .child(
            div()
                .text_color(theme::err())
                .child("Everything on this drive will be erased. This can't be undone."),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .child(button("Go back", !erase, false))
                .child(button("Erase and install", erase, true)),
        );
    frame(title, "", body)
}
