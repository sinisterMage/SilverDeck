//! silverdeck-ui — the SilverDeck console shell.
//!
//! One fullscreen GPUI window; keyboard and gamepad drive the same
//! `NavEvent` handling on the root view. In the kiosk session sway's
//! `for_window [app_id="silverdeck-ui"] fullscreen enable` rule owns
//! fullscreen enforcement.

mod library;
mod modal;
mod root;
mod settings;
mod store_view;
mod theme;

use gpui::{prelude::*, px, size, App, Application, Bounds, WindowBounds, WindowOptions};
use root::RootView;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    Application::new().run(|cx: &mut App| {
        root::init_key_bindings(cx);

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let mut root_slot = None;
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some(silverdeck_launch::UI_APP_ID.to_owned()),
                ..Default::default()
            },
            |window, cx| {
                let root = cx.new(|cx| RootView::new(window, cx));
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

        // Contract with the QEMU harness and the health check: this line in
        // the journal means the console shell came up.
        println!("SILVERDECK-UI-READY");
        let _ = std::fs::create_dir_all("/run/silverdeck")
            .and_then(|()| std::fs::write("/run/silverdeck/ui-ready", b"1\n"));
    });
}
