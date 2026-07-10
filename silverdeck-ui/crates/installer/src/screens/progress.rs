//! Install progress: stage headline from the engine's STEP markers, a bar
//! that creeps within each stage as output lines arrive, and the last log
//! line dimmed underneath for the curious.

use gpui::{div, prelude::*, px, Context};
use silverdeck_ui_kit::theme;

use crate::engine::{self, InstallEvent, Stage};
use crate::root::{InstallerRoot, Screen};
use crate::screens::frame;

const BAR_WIDTH: f32 = 560.;
/// How many log lines the Failed screen can show.
const TAIL_LINES: usize = 200;

#[derive(Default)]
pub struct ProgressState {
    pub stage: Option<Stage>,
    pub percent: f32,
    pub last_line: String,
    pub tail: Vec<String>,
    pub running: bool,
}

pub fn start(root: &mut InstallerRoot, cx: &mut Context<InstallerRoot>) {
    let Some(disk) = root.confirm.disk.clone() else {
        return;
    };
    if root.progress.running {
        return;
    }
    if !engine::running_as_root() {
        root.failed.reason =
            "The installer needs to run as the system administrator. This is a bug in the live session.".to_owned();
        root.failed.tail = Vec::new();
        root.screen = Screen::Failed;
        return;
    }
    root.progress = ProgressState {
        percent: 2.,
        running: true,
        ..ProgressState::default()
    };
    root.screen = Screen::Progress;

    let (tx, rx) = async_channel::unbounded();
    engine::spawn_install(disk.path, tx);
    cx.spawn(async move |this, cx| {
        while let Ok(event) = rx.recv().await {
            let done = matches!(event, InstallEvent::Finished(_));
            if this
                .update(cx, |root, cx| on_event(root, event, cx))
                .is_err()
                || done
            {
                break;
            }
        }
    })
    .detach();
}

fn on_event(root: &mut InstallerRoot, event: InstallEvent, cx: &mut Context<InstallerRoot>) {
    let p = &mut root.progress;
    match event {
        InstallEvent::Stage(stage) => {
            p.stage = Some(stage);
            p.percent = p.percent.max(stage.base_percent());
        }
        InstallEvent::Line(line) => {
            let ceiling = p.stage.map(|s| s.ceiling_percent()).unwrap_or(5.);
            p.percent = (p.percent + 0.4).min(ceiling);
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                p.last_line = trimmed.chars().take(96).collect();
                p.tail.push(trimmed.to_owned());
                let excess = p.tail.len().saturating_sub(TAIL_LINES);
                if excess > 0 {
                    p.tail.drain(..excess);
                }
            }
        }
        InstallEvent::Finished(Ok(())) => {
            p.percent = 100.;
            p.running = false;
            root.screen = Screen::Done;
        }
        InstallEvent::Finished(Err(reason)) => {
            p.running = false;
            root.failed.reason = reason;
            root.failed.tail = p.tail.clone();
            root.failed.selected = 0;
            root.failed.details = false;
            root.screen = Screen::Failed;
        }
    }
    cx.notify();
}

pub fn render(root: &InstallerRoot, _cx: &mut Context<InstallerRoot>) -> impl IntoElement {
    let p = &root.progress;
    let label = p.stage.map(|s| s.label()).unwrap_or("Starting…");
    let fraction = (p.percent / 100.).clamp(0., 1.);

    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_4()
        .child(
            div()
                .h(px(10.))
                .w(px(BAR_WIDTH))
                .rounded_md()
                .bg(theme::panel_hi())
                .child(
                    div()
                        .h_full()
                        .w(px(BAR_WIDTH * fraction))
                        .rounded_md()
                        .bg(theme::accent()),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme::text_dim())
                .child(p.last_line.clone()),
        );

    frame(label, "This takes a few minutes. Keep the power on.", body)
}
