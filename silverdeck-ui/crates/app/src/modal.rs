//! Modal layer: confirmation dialogs, a controller-navigable on-screen
//! keyboard for Wi-Fi passwords, and the streaming OS-update log.

use gpui::{div, prelude::*, px, Context, KeyDownEvent};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::osk::{self, OskOutcome, OskState};

use crate::root::RootView;
use crate::{settings, store_view, theme};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    Install(String),
    Uninstall(String),
    Reboot,
    Poweroff,
}

pub enum Modal {
    Confirm {
        title: String,
        action: PendingAction,
        yes: bool,
    },
    WifiPassword {
        ssid: String,
        osk: OskState,
    },
    UpdateLog,
}

impl Modal {
    pub fn confirm(title: impl Into<String>, action: PendingAction) -> Self {
        Modal::Confirm {
            title: title.into(),
            action,
            yes: false,
        }
    }

    pub fn wifi_password(ssid: String) -> Self {
        Modal::WifiPassword {
            ssid,
            osk: OskState::new(),
        }
    }
}

// --- Navigation ---------------------------------------------------------------

enum Outcome {
    None,
    Close,
    Install(String),
    Uninstall(String),
    Reboot,
    Poweroff,
    ConnectWifi { ssid: String, password: String },
}

pub fn handle_nav(root: &mut RootView, event: NavEvent, cx: &mut Context<RootView>) {
    let outcome = match root.modal.as_mut().expect("modal open") {
        Modal::Confirm { action, yes, .. } => match event {
            NavEvent::Left | NavEvent::Right | NavEvent::Up | NavEvent::Down => {
                *yes = !*yes;
                Outcome::None
            }
            NavEvent::Confirm if *yes => match action.clone() {
                PendingAction::Install(id) => Outcome::Install(id),
                PendingAction::Uninstall(id) => Outcome::Uninstall(id),
                PendingAction::Reboot => Outcome::Reboot,
                PendingAction::Poweroff => Outcome::Poweroff,
            },
            NavEvent::Confirm | NavEvent::Back => Outcome::Close,
            _ => Outcome::None,
        },
        Modal::WifiPassword { ssid, osk } => match osk.handle_nav(event) {
            OskOutcome::None => Outcome::None,
            OskOutcome::Commit(password) => Outcome::ConnectWifi {
                ssid: ssid.clone(),
                password,
            },
            OskOutcome::Cancel => Outcome::Close,
        },
        Modal::UpdateLog => match event {
            NavEvent::Back => Outcome::Close,
            _ => Outcome::None,
        },
    };

    match outcome {
        Outcome::None => {}
        Outcome::Close => close(root),
        Outcome::Install(id) => {
            close(root);
            store_view::begin_install(root, id, cx);
        }
        Outcome::Uninstall(id) => {
            close(root);
            store_view::begin_uninstall(root, id, cx);
        }
        Outcome::Reboot => settings::power_action(root, true, cx),
        Outcome::Poweroff => settings::power_action(root, false, cx),
        Outcome::ConnectWifi { ssid, password } => {
            close(root);
            settings::connect_wifi(root, ssid, Some(password), cx);
        }
    }
}

fn close(root: &mut RootView) {
    if matches!(root.modal, Some(Modal::UpdateLog)) {
        settings::stop_update_view(root);
    }
    root.modal = None;
}

/// Physical-keyboard text entry for the password modal.
pub fn handle_key(root: &mut RootView, event: &KeyDownEvent, cx: &mut Context<RootView>) {
    if let Some(Modal::WifiPassword { osk, .. }) = root.modal.as_mut() {
        if osk.handle_key(event) {
            cx.notify();
        }
    }
}

// --- Rendering ----------------------------------------------------------------

pub fn render(root: &RootView) -> impl IntoElement {
    let modal = root.modal.as_ref().expect("modal open");
    let panel = match modal {
        Modal::Confirm { title, yes, .. } => confirm_panel(title, *yes).into_any_element(),
        Modal::WifiPassword { ssid, osk } => wifi_panel(ssid, osk).into_any_element(),
        Modal::UpdateLog => update_panel(&root.settings.update_log).into_any_element(),
    };
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::scrim())
        .child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .p_6()
                .rounded_lg()
                .bg(theme::panel())
                .border_1()
                .border_color(theme::panel_hi())
                .child(panel),
        )
}

fn confirm_panel(title: &str, yes: bool) -> impl IntoElement {
    let button = |label: &'static str, active: bool| {
        div()
            .px_6()
            .py_2()
            .rounded_md()
            .bg(if active {
                theme::accent_dim()
            } else {
                theme::panel_hi()
            })
            .text_color(if active {
                theme::text()
            } else {
                theme::text_dim()
            })
            .child(label)
    };
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(div().text_lg().child(title.to_owned()))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .justify_center()
                .child(button("Yes", yes))
                .child(button("No", !yes)),
        )
}

fn wifi_panel(ssid: &str, osk: &OskState) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().text_lg().child(format!("Password for {ssid}")))
        .child(osk::render(osk, true))
        .child(
            div()
                .text_sm()
                .text_color(theme::text_dim())
                .child("A key · B backspace/close · Start cancel · typing works too"),
        )
}

fn update_panel(log: &[String]) -> impl IntoElement {
    let visible: Vec<String> = log.iter().rev().take(18).rev().cloned().collect();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .w(px(760.))
        .child(div().text_lg().child("System update"))
        .child(
            div()
                .flex()
                .flex_col()
                .p_3()
                .rounded_md()
                .bg(theme::bg())
                .h(px(360.))
                .text_sm()
                .children(if visible.is_empty() {
                    vec!["waiting for update output…".to_owned()]
                } else {
                    visible
                }),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme::text_dim())
                .child("Update runs atomically; reboot to apply. B closes this view."),
        )
}
