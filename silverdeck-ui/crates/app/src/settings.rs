//! Settings tab: Wi-Fi, volume, atomic OS updates, power.

use gpui::{div, prelude::*, Context};
use silverdeck_input::NavEvent;
use silverdeck_system::{Audio, HostRunner, JournalTail, Network, Power, Updates, WifiNetwork};

use crate::modal::{Modal, PendingAction};
use crate::root::RootView;
use crate::theme;

static RUNNER: HostRunner = HostRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    WifiToggle,
    Wifi(usize),
    Volume,
    Update,
    Reboot,
    Poweroff,
}

#[derive(Default)]
pub struct SettingsState {
    pub selected: usize,
    pub wifi_enabled: bool,
    pub networks: Vec<WifiNetwork>,
    pub volume: u8,
    pub muted: bool,
    pub net_status: String,
    pub update_log: Vec<String>,
    pub journal_tail: Option<JournalTail>,
    pub refreshing: bool,
}

pub fn rows(state: &SettingsState) -> Vec<Row> {
    let mut rows = vec![Row::WifiToggle];
    rows.extend((0..state.networks.len()).map(Row::Wifi));
    rows.extend([Row::Volume, Row::Update, Row::Reboot, Row::Poweroff]);
    rows
}

pub fn refresh(root: &mut RootView, cx: &mut Context<RootView>) {
    if root.settings.refreshing {
        return;
    }
    root.settings.refreshing = true;
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let snapshot = background
            .spawn(async move {
                let network = Network(&RUNNER);
                let audio = Audio(&RUNNER);
                let wifi_enabled = network.wifi_enabled().unwrap_or(false);
                let networks = if wifi_enabled {
                    network.scan().unwrap_or_default()
                } else {
                    Vec::new()
                };
                let (volume, muted) = audio.volume().unwrap_or((0, false));
                let net_status = network.status().unwrap_or_default();
                (wifi_enabled, networks, volume, muted, net_status)
            })
            .await;
        this.update(cx, |root, cx| {
            let (wifi_enabled, networks, volume, muted, net_status) = snapshot;
            let s = &mut root.settings;
            s.refreshing = false;
            s.wifi_enabled = wifi_enabled;
            s.networks = networks;
            s.volume = volume;
            s.muted = muted;
            s.net_status = net_status;
            s.selected = s.selected.min(rows(s).len() - 1);
            cx.notify();
        })
        .ok();
    })
    .detach();
}

pub fn handle_nav(root: &mut RootView, event: NavEvent, cx: &mut Context<RootView>) {
    let row_list = rows(&root.settings);
    let current = row_list[root.settings.selected.min(row_list.len() - 1)];
    match event {
        NavEvent::Up => {
            root.settings.selected = root.settings.selected.saturating_sub(1);
        }
        NavEvent::Down => {
            root.settings.selected = (root.settings.selected + 1).min(row_list.len() - 1);
        }
        NavEvent::Left | NavEvent::Right if current == Row::Volume => {
            let delta: i16 = if event == NavEvent::Left { -5 } else { 5 };
            let volume = (i16::from(root.settings.volume) + delta).clamp(0, 100) as u8;
            root.settings.volume = volume;
            run_quietly(cx, move || Audio(&RUNNER).set_volume(volume));
        }
        NavEvent::Confirm => activate(root, current, cx),
        NavEvent::Menu => refresh(root, cx),
        _ => {}
    }
}

fn activate(root: &mut RootView, row: Row, cx: &mut Context<RootView>) {
    match row {
        Row::WifiToggle => {
            let target = !root.settings.wifi_enabled;
            root.settings.wifi_enabled = target;
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move { Network(&RUNNER).set_wifi_enabled(target) })
                    .await;
                this.update(cx, |root, cx| {
                    if let Err(err) = result {
                        root.toast(format!("{err:#}"), true, cx);
                    }
                    refresh(root, cx);
                })
                .ok();
            })
            .detach();
        }
        Row::Wifi(index) => {
            let Some(network) = root.settings.networks.get(index) else {
                return;
            };
            if network.connected {
                root.toast(format!("already connected to {}", network.ssid), false, cx);
            } else if network.secured {
                root.modal = Some(Modal::wifi_password(network.ssid.clone()));
            } else {
                connect_wifi(root, network.ssid.clone(), None, cx);
            }
        }
        Row::Volume => {
            let muted = !root.settings.muted;
            root.settings.muted = muted;
            run_quietly(cx, move || Audio(&RUNNER).set_muted(muted));
        }
        Row::Update => start_update(root, cx),
        Row::Reboot => {
            root.modal = Some(Modal::confirm(
                "Restart the console?",
                PendingAction::Reboot,
            ));
        }
        Row::Poweroff => {
            root.modal = Some(Modal::confirm(
                "Power off the console?",
                PendingAction::Poweroff,
            ));
        }
    }
}

pub fn connect_wifi(
    root: &mut RootView,
    ssid: String,
    password: Option<String>,
    cx: &mut Context<RootView>,
) {
    root.toast(format!("connecting to {ssid}…"), false, cx);
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let result = background
            .spawn(async move {
                Network(&RUNNER)
                    .connect(&ssid, password.as_deref())
                    .map(|()| ssid)
            })
            .await;
        this.update(cx, |root, cx| {
            match result {
                Ok(ssid) => root.toast(format!("connected to {ssid}"), false, cx),
                Err(err) => root.toast(format!("{err:#}"), true, cx),
            }
            refresh(root, cx);
        })
        .ok();
    })
    .detach();
}

pub fn start_update(root: &mut RootView, cx: &mut Context<RootView>) {
    root.settings.update_log.clear();
    root.modal = Some(Modal::UpdateLog);

    let (tx, rx) = async_channel::unbounded();
    match silverdeck_system::tail_update_log(move |line| {
        let _ = tx.send_blocking(line);
    }) {
        Ok(tail) => root.settings.journal_tail = Some(tail),
        Err(err) => root.toast(format!("journal unavailable: {err:#}"), true, cx),
    }
    cx.spawn(async move |this, cx| {
        while let Ok(line) = rx.recv().await {
            if this
                .update(cx, |root, cx| {
                    let log = &mut root.settings.update_log;
                    log.push(line);
                    let excess = log.len().saturating_sub(500);
                    if excess > 0 {
                        log.drain(..excess);
                    }
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();

    run_quietly(cx, || Updates(&RUNNER).start());
}

pub fn stop_update_view(root: &mut RootView) {
    root.settings.journal_tail = None; // drop kills journalctl
}

pub fn power_action(root: &mut RootView, reboot: bool, cx: &mut Context<RootView>) {
    let result = if reboot {
        Power(&RUNNER).reboot()
    } else {
        Power(&RUNNER).poweroff()
    };
    if let Err(err) = result {
        root.toast(format!("{err:#}"), true, cx);
    }
}

/// Fire-and-forget a fallible command on the background executor.
fn run_quietly(
    cx: &mut Context<RootView>,
    op: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) {
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = op() {
                log::warn!("system command failed: {err:#}");
            }
        })
        .detach();
}

pub fn render(root: &RootView, _cx: &mut Context<RootView>) -> impl IntoElement {
    let state = &root.settings;
    let row_list = rows(state);
    let selected = state.selected.min(row_list.len() - 1);

    div()
        .flex()
        .flex_col()
        .size_full()
        .px_8()
        .gap_1()
        .children(row_list.into_iter().enumerate().map(|(index, row)| {
            let is_selected = index == selected;
            let (label, value) = describe(state, row);
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(if is_selected {
                    theme::panel_hi()
                } else {
                    theme::bg()
                })
                .child(div().child(label))
                .child(div().text_color(theme::text_dim()).child(value))
        }))
        .child(
            div()
                .pt_2()
                .text_sm()
                .text_color(theme::text_dim())
                .child("A select · ←/→ volume · Start rescan"),
        )
}

fn describe(state: &SettingsState, row: Row) -> (String, String) {
    match row {
        Row::WifiToggle => (
            "Wi-Fi".into(),
            if state.wifi_enabled { "on" } else { "off" }.into(),
        ),
        Row::Wifi(index) => {
            let network = &state.networks[index];
            let mut status = format!("signal {}%", network.signal);
            if network.secured {
                status.push_str(" · secured");
            }
            if network.connected {
                status.push_str(" · connected");
            }
            (format!("  {}", network.ssid), status)
        }
        Row::Volume => {
            let bar: String = (0..10)
                .map(|i| if i < state.volume / 10 { '█' } else { '░' })
                .collect();
            (
                "Volume".into(),
                if state.muted {
                    "muted".into()
                } else {
                    format!("{bar} {}%", state.volume)
                },
            )
        }
        Row::Update => ("System update".into(), "atomic, auto-rollback".into()),
        Row::Reboot => ("Restart".into(), String::new()),
        Row::Poweroff => ("Power off".into(), String::new()),
    }
}
