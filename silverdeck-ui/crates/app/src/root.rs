//! Root view: owns all app state, routes navigation (keyboard actions and
//! gamepad events converge on `handle_nav`), renders the tab frame.

use gpui::{
    actions, div, prelude::*, px, App, Context, FocusHandle, KeyBinding, KeyDownEvent,
    SharedString, Window,
};
use silverdeck_input::NavEvent;

use crate::{library, modal, settings, store_view, theme};

actions!(
    silverdeck,
    [MoveUp, MoveDown, MoveLeft, MoveRight, Confirm, Back, TabNext, TabPrev, Menu]
);

pub fn init_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, None),
        KeyBinding::new("down", MoveDown, None),
        KeyBinding::new("left", MoveLeft, None),
        KeyBinding::new("right", MoveRight, None),
        KeyBinding::new("enter", Confirm, None),
        KeyBinding::new("escape", Back, None),
        KeyBinding::new("tab", TabNext, None),
        KeyBinding::new("shift-tab", TabPrev, None),
        KeyBinding::new("f1", Menu, None),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Library,
    Store,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Library, Tab::Store, Tab::Settings];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Library => "Library",
            Tab::Store => "Store",
            Tab::Settings => "Settings",
        }
    }
}

pub struct RootView {
    pub focus_handle: FocusHandle,
    pub tab: Tab,
    pub library: library::LibraryState,
    pub store: store_view::StoreState,
    pub settings: settings::SettingsState,
    pub modal: Option<modal::Modal>,
    /// Transient status line: (message, is_error).
    pub toast: Option<(SharedString, bool)>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let mut this = RootView {
            focus_handle,
            tab: Tab::Library,
            library: library::LibraryState::default(),
            store: store_view::StoreState::default(),
            settings: settings::SettingsState::default(),
            modal: None,
            toast: None,
        };
        library::rescan(&mut this, cx);
        store_view::load_catalog(&mut this, cx);
        settings::refresh(&mut this, cx);
        this
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
        // A running game owns the screen; swallow everything.
        if self.library.session.is_some() {
            return;
        }
        if self.modal.is_some() {
            modal::handle_nav(self, event, cx);
            cx.notify();
            return;
        }
        match event {
            NavEvent::TabNext => self.cycle_tab(1, cx),
            NavEvent::TabPrev => self.cycle_tab(-1, cx),
            _ => match self.tab {
                Tab::Library => library::handle_nav(self, event, cx),
                Tab::Store => store_view::handle_nav(self, event, cx),
                Tab::Settings => settings::handle_nav(self, event, cx),
            },
        }
        cx.notify();
    }

    fn cycle_tab(&mut self, direction: isize, cx: &mut Context<Self>) {
        let index = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        let next = (index + direction).rem_euclid(Tab::ALL.len() as isize) as usize;
        self.tab = Tab::ALL[next];
        if self.tab == Tab::Settings {
            settings::refresh(self, cx);
        }
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // Free-text entry (Wi-Fi password) accepts physical keyboards too;
        // everything else is driven by the bound navigation keys.
        if self.modal.is_some() {
            modal::handle_key(self, event, cx);
        }
    }

    fn tab_bar(&self) -> impl IntoElement {
        let active = self.tab;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_8()
            .py_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_color(theme::accent())
                    .child("SilverDeck"),
            )
            .child(div().w(px(24.)))
            .children(Tab::ALL.iter().map(move |tab| {
                let is_active = *tab == active;
                div()
                    .px_4()
                    .py_1()
                    .rounded_md()
                    .text_lg()
                    .when(is_active, |d| {
                        d.bg(theme::panel_hi()).text_color(theme::text())
                    })
                    .when(!is_active, |d| d.text_color(theme::text_dim()))
                    .child(tab.title())
            }))
            .child(div().flex_grow())
            .child(
                div()
                    .text_sm()
                    .text_color(theme::text_dim())
                    .child("LB/RB switch · A select · B back"),
            )
    }

    fn status_bar(&self) -> impl IntoElement {
        let (message, color) = match (&self.toast, &self.library.session) {
            (_, Some(title)) => (format!("Running: {title}").into(), theme::ok()),
            (Some((msg, true)), _) => (msg.clone(), theme::err()),
            (Some((msg, false)), _) => (msg.clone(), theme::text_dim()),
            (None, None) => (SharedString::from(""), theme::text_dim()),
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
            .child(
                div()
                    .text_color(theme::text_dim())
                    .child(self.settings.net_status.clone()),
            )
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.tab {
            Tab::Library => library::render(self, cx).into_any_element(),
            Tab::Store => store_view::render(self, cx).into_any_element(),
            Tab::Settings => settings::render(self, cx).into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .key_context("SilverDeck")
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
            .on_action(
                cx.listener(|this, _: &TabNext, _, cx| this.handle_nav(NavEvent::TabNext, cx)),
            )
            .on_action(
                cx.listener(|this, _: &TabPrev, _, cx| this.handle_nav(NavEvent::TabPrev, cx)),
            )
            .on_action(cx.listener(|this, _: &Menu, _, cx| this.handle_nav(NavEvent::Menu, cx)))
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::text())
            .child(self.tab_bar())
            .child(div().flex_grow().min_h_0().child(content))
            .child(self.status_bar())
            .when(self.modal.is_some(), |d| d.child(modal::render(self)))
    }
}
