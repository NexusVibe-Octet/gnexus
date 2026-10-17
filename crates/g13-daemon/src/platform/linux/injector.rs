//! `/dev/uinput` injector: a virtual keyboard and a virtual mouse (crate `evdev`).
//!
//! - Keys are sent as **evdev codes** (`KeyCode::evdev()`), i.e. physical positions:
//!   the compositor applies the layout (AZERTY…), exactly like a real keyboard.
//! - Two devices rather than one, so that libinput treats each as what it is (the
//!   mouse gets the pointer settings) and the macro recorder can recognise them by
//!   name ([`KEYBOARD_NAME`], [`MOUSE_NAME`]) to ignore G Nexus's own output.
//! - Works the same under Wayland and X11: it sits below the compositor.
//! - No stuck key: when a device is destroyed (drop, crash, kill), the kernel
//!   itself releases what it still held down.
//!
//! Access: `/dev/uinput` must be writable (udev rule `uaccess`, see `deploy/linux/`).

use std::io;
use std::sync::Mutex;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, EventType, InputEvent, InputId, RelativeAxisCode};

use super::gamepad::LinuxPad;
use crate::keycode::{KeyCode, MouseButton};
use crate::platform::{GamepadState, InputInjector, PlatformError, PlatformResult};

pub const KEYBOARD_NAME: &str = "G Nexus virtual keyboard";
pub const MOUSE_NAME: &str = "G Nexus virtual mouse";

/// One wheel notch in high-resolution units (`REL_WHEEL_HI_RES`).
const HI_RES_NOTCH: i32 = 120;

/// Keyboard, mouse and (plugged in on demand) Xbox 360 controller through `/dev/uinput`.
pub struct UinputInjector {
    keyboard: Mutex<VirtualDevice>,
    mouse: Mutex<VirtualDevice>,
    pad: LinuxPad,
}

fn os(what: &str, e: io::Error) -> PlatformError {
    let hint = if e.kind() == io::ErrorKind::PermissionDenied || e.kind() == io::ErrorKind::NotFound {
        " (accès à /dev/uinput : installez deploy/linux/70-gnexus-uaccess.rules et uinput.conf)"
    } else {
        ""
    };
    PlatformError::Os(format!("{what} : {e}{hint}"))
}

fn id() -> InputId {
    InputId::new(BusType::BUS_VIRTUAL, 0, 0, 1)
}

impl UinputInjector {
    pub fn new() -> PlatformResult<Self> {
        let mut keys = AttributeSet::<evdev::KeyCode>::new();
        for k in KeyCode::ALL {
            keys.insert(evdev::KeyCode::new(k.evdev()));
        }
        let keyboard = VirtualDevice::builder()
            .and_then(|b| b.name(KEYBOARD_NAME).input_id(id()).with_keys(&keys))
            .and_then(|b| b.build())
            .map_err(|e| os("clavier virtuel", e))?;

        let mut buttons = AttributeSet::<evdev::KeyCode>::new();
        for b in MouseButton::ALL {
            buttons.insert(evdev::KeyCode::new(b.evdev()));
        }
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        for a in [
            RelativeAxisCode::REL_X,
            RelativeAxisCode::REL_Y,
            RelativeAxisCode::REL_WHEEL,
            RelativeAxisCode::REL_HWHEEL,
            RelativeAxisCode::REL_WHEEL_HI_RES,
            RelativeAxisCode::REL_HWHEEL_HI_RES,
        ] {
            axes.insert(a);
        }
        let mouse = VirtualDevice::builder()
            .and_then(|b| b.name(MOUSE_NAME).input_id(id()).with_keys(&buttons))
            .and_then(|b| b.with_relative_axes(&axes))
            .and_then(|b| b.build())
            .map_err(|e| os("souris virtuelle", e))?;

        Ok(Self { keyboard: Mutex::new(keyboard), mouse: Mutex::new(mouse), pad: LinuxPad::default() })
    }
}

fn emit(dev: &Mutex<VirtualDevice>, events: &[InputEvent]) -> PlatformResult {
    let mut dev = dev.lock().unwrap_or_else(|e| e.into_inner());
    dev.emit(events).map_err(|e| PlatformError::Os(format!("uinput : {e}")))
}

fn key_event(code: u16, down: bool) -> InputEvent {
    InputEvent::new(EventType::KEY.0, code, i32::from(down))
}

fn rel(axis: RelativeAxisCode, value: i32) -> InputEvent {
    InputEvent::new(EventType::RELATIVE.0, axis.0, value)
}

impl InputInjector for UinputInjector {
    fn key(&self, key: KeyCode, down: bool) -> PlatformResult {
        emit(&self.keyboard, &[key_event(key.evdev(), down)])
    }

    fn mouse_button(&self, button: MouseButton, down: bool) -> PlatformResult {
        emit(&self.mouse, &[key_event(button.evdev(), down)])
    }

    fn mouse_wheel(&self, delta: i32, horizontal: bool) -> PlatformResult {
        if delta == 0 {
            return Ok(());
        }
        // Both resolutions, as real wheels do: libinput reads the high-resolution one.
        let (low, high) = if horizontal {
            (RelativeAxisCode::REL_HWHEEL, RelativeAxisCode::REL_HWHEEL_HI_RES)
        } else {
            (RelativeAxisCode::REL_WHEEL, RelativeAxisCode::REL_WHEEL_HI_RES)
        };
        emit(&self.mouse, &[rel(low, delta), rel(high, delta.saturating_mul(HI_RES_NOTCH))])
    }

    fn mouse_move(&self, dx: i32, dy: i32) -> PlatformResult {
        let mut events = Vec::with_capacity(2);
        if dx != 0 {
            events.push(rel(RelativeAxisCode::REL_X, dx));
        }
        if dy != 0 {
            events.push(rel(RelativeAxisCode::REL_Y, dy));
        }
        if events.is_empty() {
            return Ok(());
        }
        emit(&self.mouse, &events)
    }

    fn gamepad(&self, state: &GamepadState) -> PlatformResult {
        self.pad.update(state)
    }

    fn gamepad_unplug(&self) -> PlatformResult {
        self.pad.unplug()
    }
}
