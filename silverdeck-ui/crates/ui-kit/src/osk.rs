//! Controller-navigable on-screen keyboard.
//!
//! Owns the text buffer, cursor, and shift state; callers feed it `NavEvent`s
//! (and optionally physical key presses) and act on the returned [`OskOutcome`].
//! Rendering is a value box plus the key grid; titles and hint lines stay with
//! the caller so each app words them its own way.

use gpui::{div, prelude::*, px, KeyDownEvent};
use silverdeck_input::NavEvent;

use crate::theme;

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

/// What a nav event did to the keyboard.
pub enum OskOutcome {
    /// State may have changed; keep the keyboard open.
    None,
    /// The user picked `done`: here is the entered text.
    Commit(String),
    /// The user backed out (`cancel`, Menu, or Back on an empty buffer).
    Cancel,
}

#[derive(Default)]
pub struct OskState {
    pub value: String,
    row: usize,
    col: usize,
    shift: bool,
}

impl OskState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_nav(&mut self, event: NavEvent) -> OskOutcome {
        let rows = keyboard_rows();
        match event {
            NavEvent::Up => {
                self.row = self.row.saturating_sub(1);
                self.col = self.col.min(rows[self.row].len() - 1);
                OskOutcome::None
            }
            NavEvent::Down => {
                self.row = (self.row + 1).min(rows.len() - 1);
                self.col = self.col.min(rows[self.row].len() - 1);
                OskOutcome::None
            }
            NavEvent::Left => {
                self.col = self.col.saturating_sub(1);
                OskOutcome::None
            }
            NavEvent::Right => {
                self.col = (self.col + 1).min(rows[self.row].len() - 1);
                OskOutcome::None
            }
            NavEvent::Confirm => match rows[self.row][self.col] {
                Key::Char(c) => {
                    self.value.push(if self.shift {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    });
                    OskOutcome::None
                }
                Key::Shift => {
                    self.shift = !self.shift;
                    OskOutcome::None
                }
                Key::Space => {
                    self.value.push(' ');
                    OskOutcome::None
                }
                Key::Backspace => {
                    self.value.pop();
                    OskOutcome::None
                }
                Key::Done => OskOutcome::Commit(self.value.clone()),
                Key::Cancel => OskOutcome::Cancel,
            },
            NavEvent::Back => {
                if self.value.pop().is_none() {
                    OskOutcome::Cancel
                } else {
                    OskOutcome::None
                }
            }
            NavEvent::Menu => OskOutcome::Cancel,
            _ => OskOutcome::None,
        }
    }

    /// Physical-keyboard text entry alongside the on-screen keys.
    /// Returns true when the buffer changed (caller should notify).
    pub fn handle_key(&mut self, event: &KeyDownEvent) -> bool {
        let key = event.keystroke.key.as_str();
        if key == "backspace" {
            self.value.pop();
            return true;
        }
        if let Some(text) = &event.keystroke.key_char {
            if text.chars().all(|c| !c.is_control()) {
                self.value.push_str(text);
                return true;
            }
        }
        false
    }
}

/// The value box (masked when `masked`) and the key grid.
pub fn render(state: &OskState, masked: bool) -> impl IntoElement {
    let shown = if masked {
        "•".repeat(state.value.chars().count())
    } else {
        state.value.clone()
    };
    let rows = keyboard_rows();
    let (sel_row, sel_col, shift) = (state.row, state.col, state.shift);
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(theme::bg())
                .min_w(px(320.))
                .child(if shown.is_empty() {
                    "…".to_owned()
                } else {
                    shown
                }),
        )
        .children(rows.into_iter().enumerate().map(move |(r, keys)| {
            div().flex().flex_row().gap_1().justify_center().children(
                keys.into_iter().enumerate().map(move |(c, key)| {
                    let active = r == sel_row && c == sel_col;
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .min_w(px(28.))
                        .text_center()
                        .bg(if active {
                            theme::accent_dim()
                        } else {
                            theme::panel_hi()
                        })
                        .child(key.label(shift))
                }),
            )
        }))
}
