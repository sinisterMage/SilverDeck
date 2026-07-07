//! Modal layer: confirmation dialogs, a controller-navigable on-screen
//! keyboard for Wi-Fi passwords, and the streaming OS-update log.

use gpui::{div, prelude::*, px, Context, KeyDownEvent};
use silverdeck_input::NavEvent;

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
        value: String,
        row: usize,
        col: usize,
        shift: bool,
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
            value: String::new(),
            row: 0,
            col: 0,
            shift: false,
        }
    }
}

// --- On-screen keyboard layout ----------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Char(char),
    Shift,
    Space,
    Backspace,
    Done,
    Cancel,
}

impl Key {
    fn label(&self, shift: bool) -> String {
        match self {
            Key::Char(c) if shift => c.to_ascii_uppercase().to_string(),
            Key::Char(c) => c.to_string(),
            Key::Shift => "⇧".into(),
            Key::Space => "space".into(),
            Key::Backspace => "⌫".into(),
            Key::Done => "done".into(),
            Key::Cancel => "cancel".into(),
        }
    }
}

fn keyboard_rows() -> Vec<Vec<Key>> {
    let mut rows: Vec<Vec<Key>> = ["1234567890", "qwertyuiop", "asdfghjkl-", "zxcvbnm_.@"]
        .iter()
        .map(|row| row.chars().map(Key::Char).collect())
        .collect();
    rows.push(vec![
        Key::Shift,
        Key::Space,
        Key::Backspace,
        Key::Done,
        Key::Cancel,
    ]);
    rows
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
        Modal::WifiPassword {
            ssid,
            value,
            row,
            col,
            shift,
        } => {
            let rows = keyboard_rows();
            match event {
                NavEvent::Up => {
                    *row = row.saturating_sub(1);
                    *col = (*col).min(rows[*row].len() - 1);
                    Outcome::None
                }
                NavEvent::Down => {
                    *row = (*row + 1).min(rows.len() - 1);
                    *col = (*col).min(rows[*row].len() - 1);
                    Outcome::None
                }
                NavEvent::Left => {
                    *col = col.saturating_sub(1);
                    Outcome::None
                }
                NavEvent::Right => {
                    *col = (*col + 1).min(rows[*row].len() - 1);
                    Outcome::None
                }
                NavEvent::Confirm => match rows[*row][*col] {
                    Key::Char(c) => {
                        value.push(if *shift { c.to_ascii_uppercase() } else { c });
                        Outcome::None
                    }
                    Key::Shift => {
                        *shift = !*shift;
                        Outcome::None
                    }
                    Key::Space => {
                        value.push(' ');
                        Outcome::None
                    }
                    Key::Backspace => {
                        value.pop();
                        Outcome::None
                    }
                    Key::Done => Outcome::ConnectWifi {
                        ssid: ssid.clone(),
                        password: value.clone(),
                    },
                    Key::Cancel => Outcome::Close,
                },
                NavEvent::Back => {
                    if value.pop().is_none() {
                        Outcome::Close
                    } else {
                        Outcome::None
                    }
                }
                NavEvent::Menu => Outcome::Close,
                _ => Outcome::None,
            }
        }
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
    if let Some(Modal::WifiPassword { value, .. }) = root.modal.as_mut() {
        let key = event.keystroke.key.as_str();
        if key == "backspace" {
            value.pop();
            cx.notify();
        } else if let Some(text) = &event.keystroke.key_char {
            if text.chars().all(|c| !c.is_control()) {
                value.push_str(text);
                cx.notify();
            }
        }
    }
}

// --- Rendering ----------------------------------------------------------------

pub fn render(root: &RootView) -> impl IntoElement {
    let modal = root.modal.as_ref().expect("modal open");
    let panel = match modal {
        Modal::Confirm { title, yes, .. } => confirm_panel(title, *yes).into_any_element(),
        Modal::WifiPassword {
            ssid,
            value,
            row,
            col,
            shift,
        } => wifi_panel(ssid, value, *row, *col, *shift).into_any_element(),
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
            .bg(if active { theme::accent_dim() } else { theme::panel_hi() })
            .text_color(if active { theme::text() } else { theme::text_dim() })
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

fn wifi_panel(ssid: &str, value: &str, sel_row: usize, sel_col: usize, shift: bool) -> impl IntoElement {
    let masked: String = "•".repeat(value.chars().count());
    let rows = keyboard_rows();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(div().text_lg().child(format!("Password for {ssid}")))
        .child(
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(theme::bg())
                .min_w(px(320.))
                .child(if masked.is_empty() {
                    "…".to_owned()
                } else {
                    masked
                }),
        )
        .children(rows.into_iter().enumerate().map(|(r, keys)| {
            div()
                .flex()
                .flex_row()
                .gap_1()
                .justify_center()
                .children(keys.into_iter().enumerate().map(move |(c, key)| {
                    let active = r == sel_row && c == sel_col;
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .min_w(px(28.))
                        .text_center()
                        .bg(if active { theme::accent_dim() } else { theme::panel_hi() })
                        .child(key.label(shift))
                }))
        }))
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
