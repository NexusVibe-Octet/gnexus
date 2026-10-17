//! `SendInput`-based keyboard / mouse injection.
//!
//! Keys are sent as **scan codes** (`KEYEVENTF_SCANCODE`) so that games using
//! DirectInput / Raw Input see them, and so the active layout (AZERTY…) is
//! honoured. Media keys and Pause have no reliable scan code and are sent as
//! virtual-key codes.
//!
//! Limitation (Windows UIPI): events are silently dropped when the focused
//! window belongs to an elevated (administrator) process and `g13d` is not
//! elevated itself.

use std::io;
use std::mem::size_of;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};

use super::gamepad::VigemPad;
use crate::keycode::{KeyCode, MouseButton, WinKey};
use crate::platform::{GamepadState, InputInjector, PlatformError, PlatformResult};

/// Value stored in `dwExtraInfo` of every injected event ("G13D"), so that a
/// low-level hook (macro recorder) can recognise and ignore our own output.
pub const INJECT_TAG: usize = 0x4731_3344;

const WHEEL_DELTA: i32 = 120;
const XBUTTON1: u32 = 0x0001;
const XBUTTON2: u32 = 0x0002;

/// Keyboard / mouse through `SendInput`, gamepad through ViGEmBus.
#[derive(Debug, Default)]
pub struct SendInputInjector {
    pad: VigemPad,
}

impl SendInputInjector {
    pub fn new() -> Self {
        Self::default()
    }
}

fn keyboard_input(key: KeyCode, down: bool) -> INPUT {
    let up_flag = if down { 0 } else { KEYEVENTF_KEYUP };
    let (w_vk, w_scan, flags) = match key.win() {
        WinKey::Scan { code, extended } => {
            let ext = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
            (0, code, KEYEVENTF_SCANCODE | ext | up_flag)
        }
        WinKey::Vk(vk) => (vk, 0, up_flag),
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: w_vk,
                wScan: w_scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECT_TAG,
            },
        },
    }
}

fn mouse_input(dx: i32, dy: i32, data: u32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECT_TAG,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> PlatformResult {
    // SAFETY: `inputs` is a valid slice of initialised INPUT structs and the
    // size argument matches the struct layout from windows-sys.
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(PlatformError::Os(format!(
            "SendInput injected {sent}/{} events: {}",
            inputs.len(),
            io::Error::last_os_error()
        )))
    }
}

impl InputInjector for SendInputInjector {
    fn key(&self, key: KeyCode, down: bool) -> PlatformResult {
        send(&[keyboard_input(key, down)])
    }

    fn mouse_button(&self, button: MouseButton, down: bool) -> PlatformResult {
        let (flags, data) = match (button, down) {
            (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
            (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
            (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
            (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
            (MouseButton::Back, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
            (MouseButton::Back, false) => (MOUSEEVENTF_XUP, XBUTTON1),
            (MouseButton::Forward, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
            (MouseButton::Forward, false) => (MOUSEEVENTF_XUP, XBUTTON2),
        };
        send(&[mouse_input(0, 0, data, flags)])
    }

    fn mouse_wheel(&self, delta: i32, horizontal: bool) -> PlatformResult {
        let flags = if horizontal { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL };
        // mouseData is a DWORD interpreted as signed by Windows.
        let data = delta.saturating_mul(WHEEL_DELTA) as u32;
        send(&[mouse_input(0, 0, data, flags)])
    }

    fn mouse_move(&self, dx: i32, dy: i32) -> PlatformResult {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        send(&[mouse_input(dx, dy, 0, MOUSEEVENTF_MOVE)])
    }

    fn gamepad(&self, state: &GamepadState) -> PlatformResult {
        self.pad.update(state)
    }

    fn gamepad_unplug(&self) -> PlatformResult {
        self.pad.unplug()
    }
}
