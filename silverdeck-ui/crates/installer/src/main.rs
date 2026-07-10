//! silverdeck-installer — the SilverDeck GUI installer.
//!
//! One fullscreen GPUI window on the live ISO; keyboard and gamepad drive the
//! same `NavEvent` handling on the root view (identical to the console
//! shell). In the live kiosk session sway's
//! `for_window [app_id="silverdeck-installer"] fullscreen enable` rule owns
//! fullscreen enforcement. The actual install work is done by the tested bash
//! engine (`silverdeck-install`); see `engine.rs` for the contract.

mod engine;
mod root;
mod screens;

use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};
use root::InstallerRoot;

const APP_ID: &str = "silverdeck-installer";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    Application::new().run(|cx: &mut App| {
        root::init_key_bindings(cx);

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let mut root_slot = None;
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some(APP_ID.to_owned()),
                ..Default::default()
            },
            |window, cx| {
                let root = cx.new(|cx| InstallerRoot::new(window, cx));
                root_slot = Some(root.clone());
                root
            },
        )
        .expect("failed to open window");

        // Gamepad → NavEvent → the same handler keyboard actions use.
        if let Some(gamepad) = silverdeck_input::spawn_gamepad_thread() {
            let root = root_slot.clone().expect("window opened");
            cx.spawn(async move |cx| {
                while let Ok(event) = gamepad.recv().await {
                    if root
                        .update(cx, |root, cx| root.handle_nav(event, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }

        // Journal-greppable readiness marker (mirrors SILVERDECK-UI-READY).
        println!("SILVERDECK-INSTALLER-READY");
    });
}
