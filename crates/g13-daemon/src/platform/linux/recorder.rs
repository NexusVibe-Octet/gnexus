//! Macro recording: reads the real keyboards and mice from `/dev/input/event*`
//! (evdev), without grabbing them (they keep working normally).
//!
//! - G Nexus's own virtual devices (keyboard, mouse, Xbox 360 pad) and the G13
//!   (which the kernel's `hid-lg-g15` driver also exposes as an input device) are
//!   left out, as Windows' hooks leave out injected input.
//! - Times come from the kernel's event timestamps, so the polling below does
//!   not blur the recorded delays.
//! - Access: reading `/dev/input/event*` needs the `uaccess` rule
//!   `deploy/linux/71-gnexus-record.rules` (or the `input` group); without it the
//!   recording fails with a message saying so.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use evdev::{Device, EventSummary, KeyCode as EvKey, RelativeAxisCode};

use super::gamepad::GAMEPAD_NAME;
use super::injector::{KEYBOARD_NAME, MOUSE_NAME};
use crate::keycode::{KeyCode, MouseButton};
use crate::platform::{InputRecorder, PlatformError, PlatformResult};
use crate::recording::Recorded;

/// How often the devices are read (the delays themselves come from the kernel).
const POLL: Duration = Duration::from_millis(5);
const MAX_EVENTS: usize = 100_000;
const LOGITECH: u16 = 0x046D;
const G13: u16 = 0xC21C;

fn key_of(code: u16) -> Option<KeyCode> {
    KeyCode::ALL.iter().copied().find(|k| k.evdev() == code)
}

fn button_of(code: u16) -> Option<MouseButton> {
    MouseButton::ALL.iter().copied().find(|b| b.evdev() == code)
}

/// A keyboard or a mouse that is not one of ours (nor the G13)?
fn wanted(dev: &Device) -> bool {
    let name = dev.name().unwrap_or_default();
    if [KEYBOARD_NAME, MOUSE_NAME, GAMEPAD_NAME].contains(&name) {
        return false;
    }
    let id = dev.input_id();
    if id.vendor() == LOGITECH && id.product() == G13 {
        return false;
    }
    dev.supported_keys().is_some_and(|k| k.contains(EvKey::KEY_A) || k.contains(EvKey::BTN_LEFT))
}

/// What one evdev event means for a macro (`None`: nothing, e.g. auto-repeat, motion).
fn recorded(e: &EventSummary) -> Option<Recorded> {
    match *e {
        EventSummary::Key(_, k, v @ (0 | 1)) => {
            let down = v == 1;
            button_of(k.0).map(|b| Recorded::Mouse(b, down)).or_else(|| key_of(k.0).map(|k| Recorded::Key(k, down)))
        }
        EventSummary::RelativeAxis(_, RelativeAxisCode::REL_WHEEL, n) if n != 0 => Some(Recorded::Wheel(n)),
        _ => None,
    }
}

pub struct EvdevRecorder {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Vec<(Duration, Recorded)>>>,
}

pub fn start() -> PlatformResult<Box<dyn InputRecorder>> {
    let mut devices = Vec::new();
    let mut opened = 0;
    // `enumerate` silently skips the nodes it cannot open
    for (path, dev) in evdev::enumerate() {
        opened += 1;
        if wanted(&dev) && dev.set_nonblocking(true).is_ok() {
            log::debug!("recording from {} ({})", path.display(), dev.name().unwrap_or("?"));
            devices.push(dev);
        }
    }
    if devices.is_empty() {
        // only joysticks (uaccess by default) or nothing at all could be opened
        let no_access = std::fs::read_dir("/dev/input")
            .map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("event")).count() > opened)
            .unwrap_or(true);
        let hint = if no_access {
            "aucun accès à /dev/input : installez deploy/linux/71-gnexus-record.rules"
        } else {
            "aucun clavier ni souris trouvé"
        };
        return Err(PlatformError::Os(format!("enregistrement impossible : {hint}")));
    }
    let start = SystemTime::now();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::Builder::new()
        .name("macro-record".into())
        .spawn(move || {
            let mut events = Vec::new();
            while !flag.load(Ordering::Relaxed) {
                for dev in &mut devices {
                    let Ok(batch) = dev.fetch_events() else { continue };
                    for e in batch {
                        if let Some(r) = recorded(&e.destructure())
                            && events.len() < MAX_EVENTS
                        {
                            let t = e.timestamp().duration_since(start).unwrap_or_default();
                            events.push((t, r));
                        }
                    }
                }
                std::thread::sleep(POLL);
            }
            // several devices: put them back in time order
            events.sort_by_key(|(t, _)| *t);
            events
        })
        .map_err(|e| PlatformError::Os(e.to_string()))?;
    Ok(Box::new(EvdevRecorder { stop, thread: Some(thread) }))
}

impl InputRecorder for EvdevRecorder {
    fn stop(mut self: Box<Self>) -> Vec<(Duration, Recorded)> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().and_then(|t| t.join().ok()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::{EventType, InputEvent};

    fn ev(ty: EventType, code: u16, value: i32) -> Option<Recorded> {
        recorded(&InputEvent::new(ty.0, code, value).destructure())
    }

    #[test]
    fn evdev_events_become_macro_input() {
        assert_eq!(ev(EventType::KEY, 17, 1), Some(Recorded::Key(KeyCode::W, true)), "physical W");
        assert_eq!(ev(EventType::KEY, 17, 0), Some(Recorded::Key(KeyCode::W, false)));
        assert_eq!(ev(EventType::KEY, 17, 2), None, "auto-repeat");
        assert_eq!(ev(EventType::KEY, 0x110, 1), Some(Recorded::Mouse(MouseButton::Left, true)));
        assert_eq!(ev(EventType::KEY, 0x113, 0), Some(Recorded::Mouse(MouseButton::Back, false)));
        assert_eq!(ev(EventType::RELATIVE, RelativeAxisCode::REL_WHEEL.0, -1), Some(Recorded::Wheel(-1)));
        assert_eq!(ev(EventType::RELATIVE, RelativeAxisCode::REL_X.0, 5), None, "motion");
    }
}
