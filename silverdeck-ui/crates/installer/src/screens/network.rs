//! Shown only when the connectivity probe fails: pick a Wi-Fi network (or fix
//! the cable) and retry. The install needs the package mirrors, so there is
//! no skip — only "Check again".

use gpui::{div, prelude::*, px, Context, KeyDownEvent};
use silverdeck_input::NavEvent;
use silverdeck_system::{Network, WifiNetwork};
use silverdeck_ui_kit::osk::{self, OskOutcome, OskState};
use silverdeck_ui_kit::theme;

use crate::engine;
use crate::root::{continue_when_online, InstallerRoot, Screen};
use crate::screens::{frame, RUNNER};

#[derive(Default)]
pub struct NetworkState {
    pub networks: Vec<WifiNetwork>,
    /// 0 = "Check again", 1.. = networks.
    pub selected: usize,
    pub scanning: bool,
    pub connecting: bool,
    /// Open password entry: (ssid, keyboard).
    pub osk: Option<(String, OskState)>,
}

pub fn enter(root: &mut InstallerRoot, cx: &mut Context<InstallerRoot>) {
    root.screen = Screen::Network;
    refresh(root, cx);
}

pub fn refresh(root: &mut InstallerRoot, cx: &mut Context<InstallerRoot>) {
    if root.network.scanning {
        return;
    }
    root.network.scanning = true;
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let networks = background
            .spawn(async move {
                if engine::fake_mode() {
                    return vec![
                        WifiNetwork {
                            ssid: "HomeNet".into(),
                            signal: 87,
                            secured: true,
                            connected: false,
                        },
                        WifiNetwork {
                            ssid: "CoffeeShop".into(),
                            signal: 55,
                            secured: false,
                            connected: false,
                        },
                    ];
                }
                Network(&RUNNER).scan().unwrap_or_default()
            })
            .await;
        this.update(cx, |root, cx| {
            let n = &mut root.network;
            n.scanning = false;
            n.networks = networks;
            n.selected = n.selected.min(n.networks.len());
            cx.notify();
        })
        .ok();
    })
    .detach();
}

fn connect(
    root: &mut InstallerRoot,
    ssid: String,
    password: Option<String>,
    cx: &mut Context<InstallerRoot>,
) {
    if root.network.connecting {
        return;
    }
    root.network.connecting = true;
    root.toast(format!("Connecting to {ssid}…"), false, cx);
    let background = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        let result = background
            .spawn(async move {
                if engine::fake_mode() {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    std::env::remove_var("SILVERDECK_FAKE_OFFLINE");
                    return Ok(ssid);
                }
                Network(&RUNNER)
                    .connect(&ssid, password.as_deref())
                    .map(|()| ssid)
            })
            .await;
        this.update(cx, |root, cx| {
            root.network.connecting = false;
            match result {
                Ok(ssid) => {
                    root.toast(format!("Connected to {ssid}"), false, cx);
                    continue_when_online(root, cx);
                }
                Err(err) => root.toast(format!("Couldn't connect: {err:#}"), true, cx),
            }
            cx.notify();
        })
        .ok();
    })
    .detach();
}

pub fn handle_nav(root: &mut InstallerRoot, event: NavEvent, cx: &mut Context<InstallerRoot>) {
    if let Some((ssid, osk)) = root.network.osk.as_mut() {
        match osk.handle_nav(event) {
            OskOutcome::None => {}
            OskOutcome::Commit(password) => {
                let ssid = ssid.clone();
                root.network.osk = None;
                connect(root, ssid, Some(password), cx);
            }
            OskOutcome::Cancel => root.network.osk = None,
        }
        return;
    }
    let rows = root.network.networks.len() + 1;
    match event {
        NavEvent::Up => root.network.selected = root.network.selected.saturating_sub(1),
        NavEvent::Down => root.network.selected = (root.network.selected + 1).min(rows - 1),
        NavEvent::Confirm => {
            if root.network.selected == 0 {
                root.toast("Checking the connection…", false, cx);
                continue_when_online(root, cx);
            } else if let Some(network) = root
                .network
                .networks
                .get(root.network.selected - 1)
                .cloned()
            {
                if network.secured {
                    root.network.osk = Some((network.ssid, OskState::new()));
                } else {
                    connect(root, network.ssid, None, cx);
                }
            }
        }
        NavEvent::Back => root.screen = Screen::Welcome,
        NavEvent::Menu => refresh(root, cx),
        _ => {}
    }
}

/// Physical-keyboard text entry for the password keyboard.
pub fn handle_key(root: &mut InstallerRoot, event: &KeyDownEvent, cx: &mut Context<InstallerRoot>) {
    if let Some((_, osk)) = root.network.osk.as_mut() {
        if osk.handle_key(event) {
            cx.notify();
        }
    }
}

fn signal_label(network: &WifiNetwork) -> String {
    let strength = match network.signal {
        0..=33 => "weak",
        34..=66 => "good",
        _ => "strong",
    };
    if network.secured {
        format!("{strength} · secured")
    } else {
        strength.to_owned()
    }
}

pub fn render(root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let n = &root.network;
    let selected = n.selected;
    let mut rows = vec![row("Check again".to_owned(), String::new(), selected == 0)];
    rows.extend(n.networks.iter().enumerate().map(|(i, network)| {
        row(
            network.ssid.clone(),
            signal_label(network),
            selected == i + 1,
        )
    }));

    let status = if n.scanning {
        "Looking for Wi-Fi networks…"
    } else if n.networks.is_empty() {
        "No Wi-Fi networks found. A network cable works too."
    } else {
        ""
    };

    let body = div()
        .flex()
        .flex_col()
        .gap_2()
        .w(px(560.))
        .children(rows)
        .when(!status.is_empty(), |d| {
            d.child(
                div()
                    .text_sm()
                    .text_color(theme::text_dim())
                    .child(status.to_owned()),
            )
        });

    frame(
        "Connect to the internet",
        "SilverDeck needs a connection to download the newest version.",
        body,
    )
    .when(n.osk.is_some(), |d| d.child(password_overlay(root)))
}

fn row(title: String, detail: String, active: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .py_2()
        .rounded_md()
        .bg(if active {
            theme::panel_hi()
        } else {
            theme::panel()
        })
        .border_2()
        .border_color(if active {
            theme::accent()
        } else {
            theme::panel()
        })
        .child(div().child(title))
        .child(div().text_sm().text_color(theme::text_dim()).child(detail))
}

fn password_overlay(root: &InstallerRoot) -> impl IntoElement {
    let (ssid, osk) = root.network.osk.as_ref().expect("osk open");
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
                .gap_3()
                .p_6()
                .rounded_lg()
                .bg(theme::panel())
                .border_1()
                .border_color(theme::panel_hi())
                .child(div().text_lg().child(format!("Password for {ssid}")))
                .child(osk::render(osk, true))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::text_dim())
                        .child("A key · B backspace/close · typing works too"),
                ),
        )
}
