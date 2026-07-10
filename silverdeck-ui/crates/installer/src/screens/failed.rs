//! Friendly failure screen; the technical detail is one press away, never in
//! the way.

use gpui::{div, prelude::*, px, Context};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::theme;

use crate::engine;
use crate::root::InstallerRoot;
use crate::screens::{button, disks, frame, power};

const BUTTONS: usize = 3; // Try again, Show/Hide details, Power off

#[derive(Default)]
pub struct FailedState {
    pub reason: String,
    pub tail: Vec<String>,
    pub selected: usize,
    pub details: bool,
}

pub fn handle_nav(root: &mut InstallerRoot, event: NavEvent, cx: &mut Context<InstallerRoot>) {
    match event {
        NavEvent::Left | NavEvent::Up => {
            root.failed.selected = root.failed.selected.saturating_sub(1);
        }
        NavEvent::Right | NavEvent::Down => {
            root.failed.selected = (root.failed.selected + 1).min(BUTTONS - 1);
        }
        NavEvent::Confirm => match root.failed.selected {
            0 => disks::enter(root, cx),
            1 => root.failed.details = !root.failed.details,
            _ => power(root, false, cx),
        },
        _ => {}
    }
}

pub fn render(root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let f = &root.failed;
    let selected = f.selected;
    let details_label = if f.details {
        "Hide details"
    } else {
        "Show details"
    };

    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_5()
        .child(div().text_color(theme::text_dim()).child(f.reason.clone()))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .child(button("Try again", selected == 0, false))
                .child(button(details_label, selected == 1, false))
                .child(button("Power off", selected == 2, false)),
        )
        .when(f.details, |d| {
            let visible: Vec<String> = f.tail.iter().rev().take(14).rev().cloned().collect();
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .p_3()
                    .rounded_md()
                    .bg(theme::bg())
                    .w(px(760.))
                    .h(px(280.))
                    .text_sm()
                    .children(if visible.is_empty() {
                        vec!["(no output captured)".to_owned()]
                    } else {
                        visible
                    })
                    .child(
                        div()
                            .text_color(theme::text_dim())
                            .child(format!("Full log: {}", engine::log_path().display())),
                    ),
            )
        });

    frame(
        "Something went wrong.",
        "Your console is unchanged or partially written — trying again is safe.",
        body,
    )
}
