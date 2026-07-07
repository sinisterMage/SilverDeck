//! Gamepad input: a dedicated gilrs polling thread that translates buttons,
//! d-pad and left-stick motion into high-level navigation events. The app
//! consumes the channel on GPUI's foreground executor and dispatches the
//! matching actions, so keyboard and controller share one navigation path.

use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gilrs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavEvent {
    Up,
    Down,
    Left,
    Right,
    /// A / Enter — activate the focused element.
    Confirm,
    /// B / Escape — dismiss, go back.
    Back,
    /// RB — next tab.
    TabNext,
    /// LB — previous tab.
    TabPrev,
    /// Start — contextual menu.
    Menu,
}

const STICK_DEADZONE: f32 = 0.5;
const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);
const POLL_INTERVAL: Duration = Duration::from_millis(4);

/// Spawn the polling thread; events arrive on the returned channel until the
/// receiver is dropped. Returns None when no gamepad backend is available
/// (e.g. containers without /dev/input) — the UI then runs keyboard-only.
pub fn spawn_gamepad_thread() -> Option<async_channel::Receiver<NavEvent>> {
    let gilrs = match Gilrs::new() {
        Ok(g) => g,
        Err(err) => {
            log::warn!("gamepad support unavailable: {err}");
            return None;
        }
    };
    let (tx, rx) = async_channel::bounded(64);
    std::thread::Builder::new()
        .name("silverdeck-gamepad".into())
        .spawn(move || poll_loop(gilrs, tx))
        .ok()?;
    Some(rx)
}

fn poll_loop(mut gilrs: Gilrs, tx: async_channel::Sender<NavEvent>) {
    // Held-direction state for stick/d-pad auto-repeat.
    let mut held: Option<(NavEvent, Instant, bool)> = None; // (dir, next_fire, repeating)
    let mut stick = (0.0f32, 0.0f32);

    loop {
        while let Some(event) = gilrs.next_event() {
            let nav = match event.event {
                EventType::ButtonPressed(button, _) => match button {
                    Button::South => Some(NavEvent::Confirm),
                    Button::East => Some(NavEvent::Back),
                    Button::RightTrigger => Some(NavEvent::TabNext),
                    Button::LeftTrigger => Some(NavEvent::TabPrev),
                    Button::Start => Some(NavEvent::Menu),
                    Button::DPadUp => hold(&mut held, NavEvent::Up),
                    Button::DPadDown => hold(&mut held, NavEvent::Down),
                    Button::DPadLeft => hold(&mut held, NavEvent::Left),
                    Button::DPadRight => hold(&mut held, NavEvent::Right),
                    _ => None,
                },
                EventType::ButtonReleased(
                    Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight,
                    _,
                ) => {
                    held = None;
                    None
                }
                EventType::AxisChanged(axis, value, _) => {
                    match axis {
                        Axis::LeftStickX => stick.0 = value,
                        Axis::LeftStickY => stick.1 = value,
                        _ => {}
                    }
                    match stick_direction(stick) {
                        Some(dir) => hold(&mut held, dir),
                        None => {
                            held = None;
                            None
                        }
                    }
                }
                EventType::Connected => {
                    log::info!("gamepad connected: {:?}", event.id);
                    None
                }
                EventType::Disconnected => {
                    log::info!("gamepad disconnected: {:?}", event.id);
                    held = None;
                    None
                }
                _ => None,
            };
            if let Some(nav) = nav {
                if tx.send_blocking(nav).is_err() {
                    return; // UI gone
                }
            }
        }

        // Auto-repeat for a held direction.
        if let Some((dir, next_fire, repeating)) = held {
            if Instant::now() >= next_fire {
                if tx.send_blocking(dir).is_err() {
                    return;
                }
                held = Some((dir, Instant::now() + REPEAT_INTERVAL, true));
                let _ = repeating;
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Register a fresh direction hold and emit the initial event; re-registering
/// the same direction (stick jitter) keeps the existing repeat schedule.
fn hold(held: &mut Option<(NavEvent, Instant, bool)>, dir: NavEvent) -> Option<NavEvent> {
    if let Some((current, _, _)) = held {
        if *current == dir {
            return None;
        }
    }
    *held = Some((dir, Instant::now() + REPEAT_DELAY, false));
    Some(dir)
}

fn stick_direction((x, y): (f32, f32)) -> Option<NavEvent> {
    if x.abs() < STICK_DEADZONE && y.abs() < STICK_DEADZONE {
        return None;
    }
    Some(if x.abs() > y.abs() {
        if x > 0.0 {
            NavEvent::Right
        } else {
            NavEvent::Left
        }
    } else if y > 0.0 {
        // gilrs Y axis: up is positive.
        NavEvent::Up
    } else {
        NavEvent::Down
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_deadzone_and_dominant_axis() {
        assert_eq!(stick_direction((0.2, 0.2)), None);
        assert_eq!(stick_direction((0.9, 0.1)), Some(NavEvent::Right));
        assert_eq!(stick_direction((-0.9, 0.2)), Some(NavEvent::Left));
        assert_eq!(stick_direction((0.1, 0.9)), Some(NavEvent::Up));
        assert_eq!(stick_direction((0.1, -0.9)), Some(NavEvent::Down));
    }

    #[test]
    fn hold_emits_once_until_direction_changes() {
        let mut held = None;
        assert_eq!(hold(&mut held, NavEvent::Down), Some(NavEvent::Down));
        assert_eq!(hold(&mut held, NavEvent::Down), None);
        assert_eq!(hold(&mut held, NavEvent::Up), Some(NavEvent::Up));
    }
}
