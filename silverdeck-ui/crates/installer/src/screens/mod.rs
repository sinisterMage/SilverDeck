//! Installer screens. Each module owns its state and exports the same
//! `handle_nav(root, event, cx)` / `render(root, cx)` pair the console
//! shell's tabs use.

pub mod confirm;
pub mod disks;
pub mod done;
pub mod failed;
pub mod network;
pub mod progress;
pub mod welcome;

use gpui::{div, prelude::*, px, Context, Div, SharedString};
use silverdeck_system::{HostRunner, Power};
use silverdeck_ui_kit::theme;

use crate::engine;
use crate::root::InstallerRoot;

pub static RUNNER: HostRunner = HostRunner;

/// Centered single-panel layout every screen uses: headline, optional
/// subtitle, then the screen's own body.
pub fn frame(
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    body: impl IntoElement,
) -> Div {
    let subtitle: SharedString = subtitle.into();
    div()
        .flex()
        .flex_col()
        .size_full()
        .items_center()
        .justify_center()
        .gap_6()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xl()
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .child(title.into()),
                )
                .when(!subtitle.is_empty(), |d| {
                    d.child(div().text_color(theme::text_dim()).child(subtitle))
                }),
        )
        .child(body.into_any_element())
}

/// A focusable action button; `danger` renders the destructive variant.
pub fn button(label: impl Into<SharedString>, active: bool, danger: bool) -> Div {
    let bg = match (active, danger) {
        (true, true) => theme::err(),
        (true, false) => theme::accent_dim(),
        (false, _) => theme::panel_hi(),
    };
    let fg = if active {
        if danger {
            theme::bg()
        } else {
            theme::text()
        }
    } else {
        theme::text_dim()
    };
    // Flexbox centering, not text_center(): gpui 0.2.2 aligns text against the
    // measurement width (the whole window when the button is a column item),
    // which throws the label far outside the button.
    div()
        .flex()
        .items_center()
        .justify_center()
        .px_8()
        .py_2()
        .rounded_md()
        .min_w(px(260.))
        .bg(bg)
        .text_color(fg)
        .child(label.into())
}

/// Reboot or power off in the background; in fake mode just say so.
pub fn power(root: &mut InstallerRoot, reboot: bool, cx: &mut Context<InstallerRoot>) {
    if engine::fake_mode() {
        let action = if reboot { "restart" } else { "power off" };
        root.toast(format!("(fake install) would {action} now"), false, cx);
        return;
    }
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let result = background
            .spawn(async move {
                if reboot {
                    Power(&RUNNER).reboot()
                } else {
                    Power(&RUNNER).poweroff()
                }
            })
            .await;
        if let Err(err) = result {
            this.update(cx, |root, cx| root.toast(format!("{err:#}"), true, cx))
                .ok();
        }
    })
    .detach();
}
