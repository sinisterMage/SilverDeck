//! All set.

use gpui::{div, prelude::*, Context};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::theme;

use crate::root::InstallerRoot;
use crate::screens::{button, frame, power};

pub fn handle_nav(root: &mut InstallerRoot, event: NavEvent, cx: &mut Context<InstallerRoot>) {
    if event == NavEvent::Confirm {
        power(root, true, cx);
    }
}

pub fn render(_root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_6()
        .child(
            div()
                .text_color(theme::text_dim())
                .child("You can unplug the USB stick once the screen goes dark."),
        )
        .child(button("Restart", true, false));
    frame("All set.", "SilverDeck is installed.", body)
}
