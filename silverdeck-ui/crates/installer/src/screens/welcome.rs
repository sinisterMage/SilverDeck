//! First screen: the wordmark and the one real choice.

use gpui::{div, prelude::*, Context};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::theme;

use crate::root::{continue_when_online, InstallerRoot};
use crate::screens::{button, power};

const BUTTONS: usize = 2; // Install, Power off

#[derive(Default)]
pub struct WelcomeState {
    pub selected: usize,
}

pub fn handle_nav(root: &mut InstallerRoot, event: NavEvent, cx: &mut Context<InstallerRoot>) {
    match event {
        NavEvent::Up | NavEvent::Left => {
            root.welcome.selected = root.welcome.selected.saturating_sub(1);
        }
        NavEvent::Down | NavEvent::Right => {
            root.welcome.selected = (root.welcome.selected + 1).min(BUTTONS - 1);
        }
        NavEvent::Confirm => match root.welcome.selected {
            0 => continue_when_online(root, cx),
            _ => power(root, false, cx),
        },
        _ => {}
    }
}

pub fn render(root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let selected = root.welcome.selected;
    div()
        .flex()
        .flex_col()
        .size_full()
        .items_center()
        .justify_center()
        .gap_8()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_3xl()
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .text_color(theme::accent())
                        .child("SilverDeck"),
                )
                .child(
                    div()
                        .text_color(theme::text_dim())
                        .child("Welcome. Let's set up your console."),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(button("Install SilverDeck", selected == 0, false))
                .child(button("Power off", selected == 1, false)),
        )
}
