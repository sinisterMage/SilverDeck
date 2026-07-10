//! Installer root: owns all state, routes navigation (keyboard actions and
//! gamepad events converge on `handle_nav`), renders the current screen.
//!
//! Same shape as the console shell's RootView, with a linear screen flow
//! instead of tabs: Welcome → (Network if offline) → Disks → Confirm →
//! Progress → Done | Failed.

use gpui::{
    actions, div, prelude::*, App, Context, FocusHandle, KeyBinding, KeyDownEvent, SharedString,
    Window,
};
use silverdeck_input::NavEvent;
use silverdeck_ui_kit::theme;

use crate::engine;
use crate::screens::{confirm, disks, done, failed, network, progress, welcome};

actions!(
    silverdeck_installer,
    [MoveUp, MoveDown, MoveLeft, MoveRight, Confirm, Back, Menu]
);

pub fn init_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, None),
        KeyBinding::new("down", MoveDown, None),
        KeyBinding::new("left", MoveLeft, None),
        KeyBinding::new("right", MoveRight, None),
        KeyBinding::new("enter", Confirm, None),
        KeyBinding::new("escape", Back, None),
        KeyBinding::new("f1", Menu, None),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Network,
    Disks,
    Confirm,
    Progress,
    Done,
    Failed,
}

pub struct InstallerRoot {
    pub focus_handle: FocusHandle,
    pub screen: Screen,
    pub welcome: welcome::WelcomeState,
    pub network: network::NetworkState,
    pub disks: disks::DisksState,
    pub confirm: confirm::ConfirmState,
    pub progress: progress::ProgressState,
    pub failed: failed::FailedState,
    /// Transient status line: (message, is_error).
    pub toast: Option<(SharedString, bool)>,
}

impl InstallerRoot {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        InstallerRoot {
            focus_handle,
            screen: Screen::Welcome,
            welcome: welcome::WelcomeState::default(),
            network: network::NetworkState::default(),
            disks: disks::DisksState::default(),
            confirm: confirm::ConfirmState::default(),
            progress: progress::ProgressState::default(),
            failed: failed::FailedState::default(),
            toast: None,
        }
    }

    pub fn toast(
        &mut self,
        message: impl Into<SharedString>,
        is_error: bool,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some((message.into(), is_error));
        cx.notify();
    }

    pub fn handle_nav(&mut self, event: NavEvent, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Welcome => welcome::handle_nav(self, event, cx),
            Screen::Network => network::handle_nav(self, event, cx),
            Screen::Disks => disks::handle_nav(self, event, cx),
            Screen::Confirm => confirm::handle_nav(self, event, cx),
            // An install in flight owns the screen; there is nothing to steer.
            Screen::Progress => {}
            Screen::Done => done::handle_nav(self, event, cx),
            Screen::Failed => failed::handle_nav(self, event, cx),
        }
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // Free-text entry (Wi-Fi password) accepts physical keyboards too.
        if self.screen == Screen::Network {
            network::handle_key(self, event, cx);
        }
    }

    fn footer(&self) -> impl IntoElement {
        let (message, color) = match &self.toast {
            Some((msg, true)) => (msg.clone(), theme::err()),
            Some((msg, false)) => (msg.clone(), theme::text_dim()),
            None => (SharedString::from(""), theme::text_dim()),
        };
        let hint = match self.screen {
            Screen::Welcome | Screen::Done => "A select",
            Screen::Progress => "",
            Screen::Network => "A select · B back · Start rescan",
            _ => "A select · B back",
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_8()
            .py_2()
            .text_sm()
            .text_color(color)
            .child(message)
            .child(div().text_color(theme::text_dim()).child(hint))
    }
}

/// Probe connectivity off the UI thread, then continue to the disk picker or
/// the network screen. Shared by the Welcome screen and Wi-Fi connect flow.
pub fn continue_when_online(_root: &mut InstallerRoot, cx: &mut Context<InstallerRoot>) {
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let online = background
            .spawn(async move { engine::network_online() })
            .await;
        this.update(cx, |root, cx| {
            if online {
                disks::enter(root, cx);
            } else {
                network::enter(root, cx);
            }
            cx.notify();
        })
        .ok();
    })
    .detach();
}

impl Render for InstallerRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.screen {
            Screen::Welcome => welcome::render(self, cx).into_any_element(),
            Screen::Network => network::render(self, cx).into_any_element(),
            Screen::Disks => disks::render(self, cx).into_any_element(),
            Screen::Confirm => confirm::render(self, cx).into_any_element(),
            Screen::Progress => progress::render(self, cx).into_any_element(),
            Screen::Done => done::render(self, cx).into_any_element(),
            Screen::Failed => failed::render(self, cx).into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .key_context("SilverDeckInstaller")
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.handle_nav(NavEvent::Up, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.handle_nav(NavEvent::Down, cx)))
            .on_action(cx.listener(|this, _: &MoveLeft, _, cx| this.handle_nav(NavEvent::Left, cx)))
            .on_action(
                cx.listener(|this, _: &MoveRight, _, cx| this.handle_nav(NavEvent::Right, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Confirm, _, cx| this.handle_nav(NavEvent::Confirm, cx)),
            )
            .on_action(cx.listener(|this, _: &Back, _, cx| this.handle_nav(NavEvent::Back, cx)))
            .on_action(cx.listener(|this, _: &Menu, _, cx| this.handle_nav(NavEvent::Menu, cx)))
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::text())
            .child(div().flex_grow().min_h_0().child(content))
            .child(self.footer())
    }
}
