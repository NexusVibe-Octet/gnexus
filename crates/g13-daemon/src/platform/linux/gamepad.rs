//! Virtual Xbox 360 controller through `/dev/uinput` (stands in for ViGEmBus).
//!
//! The device looks exactly like what the kernel's `xpad` driver makes of a wired
//! Xbox 360 pad (`045E:028E`, same axes, buttons and ranges), so SDL, Steam and
//! Proton games recognise it as one:
//! - sticks `ABS_X/Y`, `ABS_RX/RY` in -32768..=32767, **Y pointing down** (xpad
//!   reports `!y`: XInput's +Y is up);
//! - triggers `ABS_Z` (LT), `ABS_RZ` (RT) in 0..=255;
//! - D-pad on the hat `ABS_HAT0X/Y` (-1..=1), as xpad does by default;
//! - buttons `BTN_A/B/X/Y`, `BTN_TL/TR`, `BTN_SELECT/START/MODE`, `BTN_THUMBL/R`.
//!
//! Plugged in on the first update, removed by [`LinuxPad::unplug`] (or when dropped:
//! the kernel then releases whatever was still pressed).

use std::sync::Mutex;

use evdev::uinput::VirtualDevice;
use evdev::{AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode, UinputAbsSetup};

use crate::platform::{GamepadState, PlatformError, PlatformResult};

pub const GAMEPAD_NAME: &str = "Microsoft X-Box 360 pad";

/// XInput button bit → evdev button (the D-pad bits go to the hat instead).
const BUTTONS: [(u16, KeyCode); 11] = [
    (0x0010, KeyCode::BTN_START),
    (0x0020, KeyCode::BTN_SELECT),
    (0x0040, KeyCode::BTN_THUMBL),
    (0x0080, KeyCode::BTN_THUMBR),
    (0x0100, KeyCode::BTN_TL),
    (0x0200, KeyCode::BTN_TR),
    (0x0400, KeyCode::BTN_MODE),
    (0x1000, KeyCode::BTN_SOUTH), // A
    (0x2000, KeyCode::BTN_EAST),  // B
    (0x4000, KeyCode::BTN_NORTH), // X (xpad: BTN_X == BTN_NORTH)
    (0x8000, KeyCode::BTN_WEST),  // Y (xpad: BTN_Y == BTN_WEST)
];
const DPAD_UP: u16 = 0x0001;
const DPAD_DOWN: u16 = 0x0002;
const DPAD_LEFT: u16 = 0x0004;
const DPAD_RIGHT: u16 = 0x0008;

fn os(e: std::io::Error) -> PlatformError {
    PlatformError::Os(format!("manette virtuelle (uinput) : {e}"))
}

/// Can a virtual controller be created? (`/dev/uinput` writable)
pub fn available() -> PlatformResult {
    std::fs::OpenOptions::new().write(true).open("/dev/uinput").map(drop).map_err(|e| {
        PlatformError::Os(format!(
            "/dev/uinput : {e} (installez deploy/linux/70-gnexus-uaccess.rules et uinput.conf)"
        ))
    })
}

fn build() -> std::io::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for (_, k) in BUTTONS {
        keys.insert(k);
    }
    let stick = AbsInfo::new(0, -32768, 32767, 16, 128, 0);
    let trigger = AbsInfo::new(0, 0, 255, 0, 0, 0);
    let hat = AbsInfo::new(0, -1, 1, 0, 0, 0);
    let mut b = VirtualDevice::builder()?
        .name(GAMEPAD_NAME)
        .input_id(InputId::new(BusType::BUS_USB, 0x045E, 0x028E, 0x0110))
        .with_keys(&keys)?;
    for (axis, info) in [
        (AbsoluteAxisCode::ABS_X, stick),
        (AbsoluteAxisCode::ABS_Y, stick),
        (AbsoluteAxisCode::ABS_RX, stick),
        (AbsoluteAxisCode::ABS_RY, stick),
        (AbsoluteAxisCode::ABS_Z, trigger),
        (AbsoluteAxisCode::ABS_RZ, trigger),
        (AbsoluteAxisCode::ABS_HAT0X, hat),
        (AbsoluteAxisCode::ABS_HAT0Y, hat),
    ] {
        b = b.with_absolute_axis(&UinputAbsSetup::new(axis, info))?;
    }
    b.build()
}

fn abs(axis: AbsoluteAxisCode, value: i32) -> InputEvent {
    InputEvent::new(EventType::ABSOLUTE.0, axis.0, value)
}

/// -1, 0 or 1 from two opposite D-pad bits.
fn hat(buttons: u16, neg: u16, pos: u16) -> i32 {
    i32::from(buttons & pos != 0) - i32::from(buttons & neg != 0)
}

/// The events describing `st` (the kernel drops those whose value did not change).
fn events(st: &GamepadState) -> Vec<InputEvent> {
    let mut ev = vec![
        abs(AbsoluteAxisCode::ABS_X, i32::from(st.left_x)),
        abs(AbsoluteAxisCode::ABS_Y, i32::from(!st.left_y)),
        abs(AbsoluteAxisCode::ABS_RX, i32::from(st.right_x)),
        abs(AbsoluteAxisCode::ABS_RY, i32::from(!st.right_y)),
        abs(AbsoluteAxisCode::ABS_Z, i32::from(st.left_trigger)),
        abs(AbsoluteAxisCode::ABS_RZ, i32::from(st.right_trigger)),
        abs(AbsoluteAxisCode::ABS_HAT0X, hat(st.buttons, DPAD_LEFT, DPAD_RIGHT)),
        abs(AbsoluteAxisCode::ABS_HAT0Y, hat(st.buttons, DPAD_UP, DPAD_DOWN)),
    ];
    for (bit, key) in BUTTONS {
        ev.push(InputEvent::new(EventType::KEY.0, key.0, i32::from(st.buttons & bit != 0)));
    }
    ev
}

/// One virtual controller, plugged in on the first update.
#[derive(Default)]
pub struct LinuxPad {
    dev: Mutex<Option<VirtualDevice>>,
}

impl LinuxPad {
    pub fn update(&self, st: &GamepadState) -> PlatformResult {
        let mut guard = self.dev.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            *guard = Some(build().map_err(os)?);
            log::info!("virtual Xbox 360 controller plugged in");
        }
        let res = guard.as_mut().map_or(Ok(()), |d| d.emit(&events(st)));
        if let Err(e) = res {
            *guard = None;
            return Err(os(e));
        }
        Ok(())
    }

    pub fn unplug(&self) -> PlatformResult {
        let mut guard = self.dev.lock().unwrap_or_else(|p| p.into_inner());
        if guard.take().is_some() {
            log::info!("virtual Xbox 360 controller unplugged");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(ev: &[InputEvent], ty: EventType, code: u16) -> i32 {
        ev.iter().find(|e| e.event_type() == ty && e.code() == code).unwrap().value()
    }

    #[test]
    fn xinput_state_maps_like_xpad() {
        let st = GamepadState {
            left_x: 32767,
            left_y: 32767, // up
            right_y: -32768,
            left_trigger: 255,
            buttons: 0x1000 | 0x0001 | 0x0008, // A, D-pad up + right
            ..GamepadState::default()
        };
        let ev = events(&st);
        let a = |c: AbsoluteAxisCode| value(&ev, EventType::ABSOLUTE, c.0);
        assert_eq!(a(AbsoluteAxisCode::ABS_X), 32767);
        assert_eq!(a(AbsoluteAxisCode::ABS_Y), -32768, "up is negative");
        assert_eq!(a(AbsoluteAxisCode::ABS_RY), 32767, "down is positive");
        assert_eq!(a(AbsoluteAxisCode::ABS_Z), 255);
        assert_eq!(a(AbsoluteAxisCode::ABS_HAT0X), 1);
        assert_eq!(a(AbsoluteAxisCode::ABS_HAT0Y), -1);
        assert_eq!(value(&ev, EventType::KEY, KeyCode::BTN_SOUTH.0), 1);
        assert_eq!(value(&ev, EventType::KEY, KeyCode::BTN_EAST.0), 0);
    }
}
