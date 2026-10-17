//! Platform-neutral description of what can be *emitted* towards the OS:
//! keyboard keys, key chords and mouse buttons.
//!
//! ## Convention: physical positions
//!
//! A [`KeyCode`] names a **physical key position** using its US-QWERTY label,
//! exactly like USB HID usages or Linux `KEY_*` codes. The daemon injects
//! *scan codes*, not characters, so the active keyboard layout decides which
//! character comes out:
//!
//! | `KeyCode` | US QWERTY | FR AZERTY |
//! |-----------|-----------|-----------|
//! | `W`       | W         | Z         |
//! | `A`       | A         | Q         |
//! | `Q`       | Q         | A         |
//! | `M`       | M         | ,         |
//! | `Semicolon` | ;       | M         |
//!
//! This is what games expect (DirectInput / Raw Input read scan codes) and it
//! matches Linux evdev 1:1. A future UI can display layout-aware labels.

use core::fmt;

/// How a key is sent through Win32 `SendInput`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinKey {
    /// Set-1 scan code (`KEYEVENTF_SCANCODE`), with the `E0` prefix flag.
    Scan { code: u16, extended: bool },
    /// Virtual-key code only (media keys, Pause), no scan code.
    Vk(u16),
}

macro_rules! keycodes {
    ( $( $variant:ident => $name:literal, $win:expr, $evdev:literal; )* ) => {
        /// A physical keyboard key (see module docs for the naming convention).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum KeyCode { $( $variant, )* }

        impl KeyCode {
            /// Every key code, in declaration order.
            pub const ALL: &'static [KeyCode] = &[ $( KeyCode::$variant, )* ];

            /// Canonical name, used by config files and IPC.
            pub const fn name(self) -> &'static str {
                match self { $( KeyCode::$variant => $name, )* }
            }

            /// Win32 injection descriptor.
            pub const fn win(self) -> WinKey {
                match self { $( KeyCode::$variant => $win, )* }
            }

            /// Linux `input-event-codes.h` `KEY_*` value (for `/dev/uinput`).
            pub const fn evdev(self) -> u16 {
                match self { $( KeyCode::$variant => $evdev, )* }
            }
        }
    };
}

const fn s(code: u16) -> WinKey {
    WinKey::Scan { code, extended: false }
}
const fn e(code: u16) -> WinKey {
    WinKey::Scan { code, extended: true }
}
const fn vk(code: u16) -> WinKey {
    WinKey::Vk(code)
}

keycodes! {
    Escape => "Escape", s(0x01), 1;
    Digit1 => "1", s(0x02), 2;
    Digit2 => "2", s(0x03), 3;
    Digit3 => "3", s(0x04), 4;
    Digit4 => "4", s(0x05), 5;
    Digit5 => "5", s(0x06), 6;
    Digit6 => "6", s(0x07), 7;
    Digit7 => "7", s(0x08), 8;
    Digit8 => "8", s(0x09), 9;
    Digit9 => "9", s(0x0A), 10;
    Digit0 => "0", s(0x0B), 11;
    Minus => "Minus", s(0x0C), 12;
    Equal => "Equal", s(0x0D), 13;
    Backspace => "Backspace", s(0x0E), 14;
    Tab => "Tab", s(0x0F), 15;
    Q => "Q", s(0x10), 16;
    W => "W", s(0x11), 17;
    E => "E", s(0x12), 18;
    R => "R", s(0x13), 19;
    T => "T", s(0x14), 20;
    Y => "Y", s(0x15), 21;
    U => "U", s(0x16), 22;
    I => "I", s(0x17), 23;
    O => "O", s(0x18), 24;
    P => "P", s(0x19), 25;
    LeftBracket => "LeftBracket", s(0x1A), 26;
    RightBracket => "RightBracket", s(0x1B), 27;
    Enter => "Enter", s(0x1C), 28;
    LeftCtrl => "LeftCtrl", s(0x1D), 29;
    A => "A", s(0x1E), 30;
    S => "S", s(0x1F), 31;
    D => "D", s(0x20), 32;
    F => "F", s(0x21), 33;
    G => "G", s(0x22), 34;
    H => "H", s(0x23), 35;
    J => "J", s(0x24), 36;
    K => "K", s(0x25), 37;
    L => "L", s(0x26), 38;
    Semicolon => "Semicolon", s(0x27), 39;
    Apostrophe => "Apostrophe", s(0x28), 40;
    Grave => "Grave", s(0x29), 41;
    LeftShift => "LeftShift", s(0x2A), 42;
    Backslash => "Backslash", s(0x2B), 43;
    Z => "Z", s(0x2C), 44;
    X => "X", s(0x2D), 45;
    C => "C", s(0x2E), 46;
    V => "V", s(0x2F), 47;
    B => "B", s(0x30), 48;
    N => "N", s(0x31), 49;
    M => "M", s(0x32), 50;
    Comma => "Comma", s(0x33), 51;
    Period => "Period", s(0x34), 52;
    Slash => "Slash", s(0x35), 53;
    RightShift => "RightShift", s(0x36), 54;
    NumpadMultiply => "NumpadMultiply", s(0x37), 55;
    LeftAlt => "LeftAlt", s(0x38), 56;
    Space => "Space", s(0x39), 57;
    CapsLock => "CapsLock", s(0x3A), 58;
    F1 => "F1", s(0x3B), 59;
    F2 => "F2", s(0x3C), 60;
    F3 => "F3", s(0x3D), 61;
    F4 => "F4", s(0x3E), 62;
    F5 => "F5", s(0x3F), 63;
    F6 => "F6", s(0x40), 64;
    F7 => "F7", s(0x41), 65;
    F8 => "F8", s(0x42), 66;
    F9 => "F9", s(0x43), 67;
    F10 => "F10", s(0x44), 68;
    NumLock => "NumLock", e(0x45), 69;
    ScrollLock => "ScrollLock", s(0x46), 70;
    Numpad7 => "Numpad7", s(0x47), 71;
    Numpad8 => "Numpad8", s(0x48), 72;
    Numpad9 => "Numpad9", s(0x49), 73;
    NumpadSubtract => "NumpadSubtract", s(0x4A), 74;
    Numpad4 => "Numpad4", s(0x4B), 75;
    Numpad5 => "Numpad5", s(0x4C), 76;
    Numpad6 => "Numpad6", s(0x4D), 77;
    NumpadAdd => "NumpadAdd", s(0x4E), 78;
    Numpad1 => "Numpad1", s(0x4F), 79;
    Numpad2 => "Numpad2", s(0x50), 80;
    Numpad3 => "Numpad3", s(0x51), 81;
    Numpad0 => "Numpad0", s(0x52), 82;
    NumpadDecimal => "NumpadDecimal", s(0x53), 83;
    // IntlBackslash = the extra ISO key left of Z (`<` `>` on AZERTY).
    IntlBackslash => "IntlBackslash", s(0x56), 86;
    F11 => "F11", s(0x57), 87;
    F12 => "F12", s(0x58), 88;
    F13 => "F13", s(0x64), 183;
    F14 => "F14", s(0x65), 184;
    F15 => "F15", s(0x66), 185;
    F16 => "F16", s(0x67), 186;
    F17 => "F17", s(0x68), 187;
    F18 => "F18", s(0x69), 188;
    F19 => "F19", s(0x6A), 189;
    F20 => "F20", s(0x6B), 190;
    F21 => "F21", s(0x6C), 191;
    F22 => "F22", s(0x6D), 192;
    F23 => "F23", s(0x6E), 193;
    F24 => "F24", s(0x76), 194;
    NumpadEnter => "NumpadEnter", e(0x1C), 96;
    RightCtrl => "RightCtrl", e(0x1D), 97;
    NumpadDivide => "NumpadDivide", e(0x35), 98;
    PrintScreen => "PrintScreen", e(0x37), 99;
    RightAlt => "RightAlt", e(0x38), 100;
    Home => "Home", e(0x47), 102;
    Up => "Up", e(0x48), 103;
    PageUp => "PageUp", e(0x49), 104;
    Left => "Left", e(0x4B), 105;
    Right => "Right", e(0x4D), 106;
    End => "End", e(0x4F), 107;
    Down => "Down", e(0x50), 108;
    PageDown => "PageDown", e(0x51), 109;
    Insert => "Insert", e(0x52), 110;
    Delete => "Delete", e(0x53), 111;
    LeftMeta => "LeftMeta", e(0x5B), 125;
    RightMeta => "RightMeta", e(0x5C), 126;
    Menu => "Menu", e(0x5D), 127;
    Pause => "Pause", vk(0x13), 119;
    VolumeMute => "VolumeMute", vk(0xAD), 113;
    VolumeDown => "VolumeDown", vk(0xAE), 114;
    VolumeUp => "VolumeUp", vk(0xAF), 115;
    MediaNext => "MediaNext", vk(0xB0), 163;
    MediaPrevious => "MediaPrevious", vk(0xB1), 165;
    MediaStop => "MediaStop", vk(0xB2), 166;
    MediaPlayPause => "MediaPlayPause", vk(0xB3), 164;
}

impl KeyCode {
    /// Parse a key name (case-insensitive). Accepts canonical names plus a few
    /// common aliases (`Esc`, `Ctrl`, `Shift`, `Alt`, `Win`, `Super`, `AltGr`…).
    pub fn parse(s: &str) -> Option<Self> {
        let alias = match s.to_ascii_lowercase().as_str() {
            "esc" => Some(Self::Escape),
            "ctrl" | "control" | "lctrl" => Some(Self::LeftCtrl),
            "rctrl" => Some(Self::RightCtrl),
            "shift" | "lshift" => Some(Self::LeftShift),
            "rshift" => Some(Self::RightShift),
            "alt" | "lalt" => Some(Self::LeftAlt),
            "altgr" | "ralt" => Some(Self::RightAlt),
            "win" | "super" | "meta" | "cmd" => Some(Self::LeftMeta),
            "return" => Some(Self::Enter),
            "del" => Some(Self::Delete),
            "ins" => Some(Self::Insert),
            "pgup" => Some(Self::PageUp),
            "pgdn" | "pgdown" => Some(Self::PageDown),
            "playpause" => Some(Self::MediaPlayPause),
            _ => None,
        };
        alias.or_else(|| Self::ALL.iter().copied().find(|k| k.name().eq_ignore_ascii_case(s)))
    }

    /// True for Ctrl / Shift / Alt / Meta (left or right).
    pub const fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::LeftCtrl
                | Self::RightCtrl
                | Self::LeftShift
                | Self::RightShift
                | Self::LeftAlt
                | Self::RightAlt
                | Self::LeftMeta
                | Self::RightMeta
        )
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A key in USB HID terms: what the onboard memory of Logitech mice stores
/// for a "keyboard" button (usage page 7 key + modifier bits, or a consumer
/// control for media keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidUsage {
    /// Keyboard page usage (0x04 = A…).
    Key(u8),
    /// Modifier bit: 0x01 LCtrl, 0x02 LShift, 0x04 LAlt, 0x08 LGui, 0x10…0x80 right ones.
    Modifier(u8),
    /// Consumer page usage (0xCD = play / pause…).
    Consumer(u16),
}

impl KeyCode {
    /// The USB HID usage of this key.
    pub const fn hid(self) -> HidUsage {
        use HidUsage::{Consumer as C, Key as K, Modifier as M};
        match self {
            Self::A => K(0x04),
            Self::B => K(0x05),
            Self::C => K(0x06),
            Self::D => K(0x07),
            Self::E => K(0x08),
            Self::F => K(0x09),
            Self::G => K(0x0A),
            Self::H => K(0x0B),
            Self::I => K(0x0C),
            Self::J => K(0x0D),
            Self::K => K(0x0E),
            Self::L => K(0x0F),
            Self::M => K(0x10),
            Self::N => K(0x11),
            Self::O => K(0x12),
            Self::P => K(0x13),
            Self::Q => K(0x14),
            Self::R => K(0x15),
            Self::S => K(0x16),
            Self::T => K(0x17),
            Self::U => K(0x18),
            Self::V => K(0x19),
            Self::W => K(0x1A),
            Self::X => K(0x1B),
            Self::Y => K(0x1C),
            Self::Z => K(0x1D),
            Self::Digit1 => K(0x1E),
            Self::Digit2 => K(0x1F),
            Self::Digit3 => K(0x20),
            Self::Digit4 => K(0x21),
            Self::Digit5 => K(0x22),
            Self::Digit6 => K(0x23),
            Self::Digit7 => K(0x24),
            Self::Digit8 => K(0x25),
            Self::Digit9 => K(0x26),
            Self::Digit0 => K(0x27),
            Self::Enter => K(0x28),
            Self::Escape => K(0x29),
            Self::Backspace => K(0x2A),
            Self::Tab => K(0x2B),
            Self::Space => K(0x2C),
            Self::Minus => K(0x2D),
            Self::Equal => K(0x2E),
            Self::LeftBracket => K(0x2F),
            Self::RightBracket => K(0x30),
            Self::Backslash => K(0x31),
            Self::Semicolon => K(0x33),
            Self::Apostrophe => K(0x34),
            Self::Grave => K(0x35),
            Self::Comma => K(0x36),
            Self::Period => K(0x37),
            Self::Slash => K(0x38),
            Self::CapsLock => K(0x39),
            Self::F1 => K(0x3A),
            Self::F2 => K(0x3B),
            Self::F3 => K(0x3C),
            Self::F4 => K(0x3D),
            Self::F5 => K(0x3E),
            Self::F6 => K(0x3F),
            Self::F7 => K(0x40),
            Self::F8 => K(0x41),
            Self::F9 => K(0x42),
            Self::F10 => K(0x43),
            Self::F11 => K(0x44),
            Self::F12 => K(0x45),
            Self::PrintScreen => K(0x46),
            Self::ScrollLock => K(0x47),
            Self::Pause => K(0x48),
            Self::Insert => K(0x49),
            Self::Home => K(0x4A),
            Self::PageUp => K(0x4B),
            Self::Delete => K(0x4C),
            Self::End => K(0x4D),
            Self::PageDown => K(0x4E),
            Self::Right => K(0x4F),
            Self::Left => K(0x50),
            Self::Down => K(0x51),
            Self::Up => K(0x52),
            Self::NumLock => K(0x53),
            Self::NumpadDivide => K(0x54),
            Self::NumpadMultiply => K(0x55),
            Self::NumpadSubtract => K(0x56),
            Self::NumpadAdd => K(0x57),
            Self::NumpadEnter => K(0x58),
            Self::Numpad1 => K(0x59),
            Self::Numpad2 => K(0x5A),
            Self::Numpad3 => K(0x5B),
            Self::Numpad4 => K(0x5C),
            Self::Numpad5 => K(0x5D),
            Self::Numpad6 => K(0x5E),
            Self::Numpad7 => K(0x5F),
            Self::Numpad8 => K(0x60),
            Self::Numpad9 => K(0x61),
            Self::Numpad0 => K(0x62),
            Self::NumpadDecimal => K(0x63),
            Self::IntlBackslash => K(0x64),
            Self::Menu => K(0x65),
            Self::F13 => K(0x68),
            Self::F14 => K(0x69),
            Self::F15 => K(0x6A),
            Self::F16 => K(0x6B),
            Self::F17 => K(0x6C),
            Self::F18 => K(0x6D),
            Self::F19 => K(0x6E),
            Self::F20 => K(0x6F),
            Self::F21 => K(0x70),
            Self::F22 => K(0x71),
            Self::F23 => K(0x72),
            Self::F24 => K(0x73),
            Self::LeftCtrl => M(0x01),
            Self::LeftShift => M(0x02),
            Self::LeftAlt => M(0x04),
            Self::LeftMeta => M(0x08),
            Self::RightCtrl => M(0x10),
            Self::RightShift => M(0x20),
            Self::RightAlt => M(0x40),
            Self::RightMeta => M(0x80),
            Self::VolumeMute => C(0xE2),
            Self::VolumeUp => C(0xE9),
            Self::VolumeDown => C(0xEA),
            Self::MediaNext => C(0xB5),
            Self::MediaPrevious => C(0xB6),
            Self::MediaStop => C(0xB7),
            Self::MediaPlayPause => C(0xCD),
        }
    }

    /// The key with this HID usage, if we know it.
    pub fn from_hid(u: HidUsage) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.hid() == u)
    }
}

/// A combination such as `Ctrl+Shift+Esc`: keys are pressed in order and
/// released in reverse order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    keys: Vec<KeyCode>,
}

impl KeyChord {
    pub fn new(keys: impl Into<Vec<KeyCode>>) -> Self {
        let mut keys = keys.into();
        // Modifiers first, keep the user's relative order otherwise.
        keys.sort_by_key(|k| !k.is_modifier());
        keys.dedup();
        Self { keys }
    }

    pub fn single(key: KeyCode) -> Self {
        Self { keys: vec![key] }
    }

    /// Parse `"Ctrl+Shift+Esc"`, `"Win+D"`, `"F5"`…
    pub fn parse(s: &str) -> Option<Self> {
        let keys = s
            .split('+')
            .map(|p| KeyCode::parse(p.trim()))
            .collect::<Option<Vec<_>>>()?;
        if keys.is_empty() {
            return None;
        }
        Some(Self::new(keys))
    }

    /// Keys in press order.
    pub fn keys(&self) -> &[KeyCode] {
        &self.keys
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, k) in self.keys.iter().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            f.write_str(k.name())?;
        }
        Ok(())
    }
}

/// Mouse buttons that can be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// Side button "back" (XButton1 / BTN_SIDE).
    Back,
    /// Side button "forward" (XButton2 / BTN_EXTRA).
    Forward,
}

impl MouseButton {
    pub const ALL: [MouseButton; 5] = [Self::Left, Self::Right, Self::Middle, Self::Back, Self::Forward];

    /// Canonical name, used by config files and IPC.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Middle => "Middle",
            Self::Back => "Back",
            Self::Forward => "Forward",
        }
    }

    /// Parse a button name (case-insensitive). Also accepts `X1` / `X2`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "x1" => Some(Self::Back),
            "x2" => Some(Self::Forward),
            _ => Self::ALL.iter().copied().find(|b| b.name().eq_ignore_ascii_case(s)),
        }
    }

    /// Linux `BTN_*` code.
    pub const fn evdev(self) -> u16 {
        match self {
            Self::Left => 0x110,
            Self::Right => 0x111,
            Self::Middle => 0x112,
            Self::Back => 0x113,
            Self::Forward => 0x114,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_parse_back() {
        for (i, a) in KeyCode::ALL.iter().enumerate() {
            assert_eq!(KeyCode::parse(a.name()), Some(*a));
            for b in &KeyCode::ALL[i + 1..] {
                assert_ne!(a.name(), b.name());
                assert_ne!(a.evdev(), b.evdev(), "{a} / {b}");
                assert_ne!(a.win(), b.win(), "{a} / {b}");
            }
        }
    }

    #[test]
    fn every_key_has_a_distinct_hid_usage() {
        for (i, a) in KeyCode::ALL.iter().enumerate() {
            assert_eq!(KeyCode::from_hid(a.hid()), Some(*a), "{a}");
            for b in &KeyCode::ALL[i + 1..] {
                assert_ne!(a.hid(), b.hid(), "{a} / {b}");
            }
            assert_eq!(matches!(a.hid(), HidUsage::Modifier(_)), a.is_modifier(), "{a}");
        }
        assert_eq!(KeyCode::W.hid(), HidUsage::Key(0x1A), "physical W (Z on AZERTY)");
    }

    #[test]
    fn chord_parsing_puts_modifiers_first() {
        let c = KeyChord::parse("Esc+shift+Ctrl").unwrap();
        assert_eq!(c.keys(), &[KeyCode::LeftShift, KeyCode::LeftCtrl, KeyCode::Escape]);
        assert_eq!(c.to_string(), "LeftShift+LeftCtrl+Escape");
        assert!(KeyChord::parse("Ctrl+Nope").is_none());
    }
}
