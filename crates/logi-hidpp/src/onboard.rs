//! Onboard memory of gaming mice (feature 0x8100): the profile directory and
//! the profiles themselves (format 1..5, G502 X PLUS = 5), as stored in
//! sectors. Parsing keeps the raw bytes, and setters only touch the bytes
//! they own, so fields we do not understand survive a round trip untouched.
//!
//! Sector layout of a profile (offsets in bytes; the sector is usually 255
//! or 256 bytes, the CRC always sits in its last two):
//!
//! | offset | content |
//! |---|---|
//! | 0 | report interval in ms (1 = 1000 Hz) |
//! | 1, 2 | default DPI slot, "DPI shift" (sniper) slot |
//! | 3..13 | 5 DPI values, u16 little-endian (0 / 0xFFFF = unused) |
//! | 13..16 | profile colour (unused on the G502 X PLUS) |
//! | 16, 17 | power mode, angle snapping |
//! | 28..32 | power-save / power-off timeouts |
//! | 32 + 4·i | button i (16 slots) |
//! | 96 + 4·i | button i with G-shift held |
//! | 160..208 | name, UTF-16LE, 0xFF·48 = none |
//! | 208 + 11·k | LED effects: 0, 1 = normal, 2, 3 = alternate (power-save) |
//! | 252 | custom animation index (G502 X PLUS) |
//! | last 2 | CRC-16/CCITT-FALSE of everything before, big-endian |
//!
//! On the G502 X PLUS the user-visible LEDs are slot 1 (libratbag quirk
//! `G502X_PLUS`: "the second slot controls the user configurable LED").

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF), as Logitech uses it for sectors.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// Does the sector end with a valid CRC?
pub fn crc_ok(sector: &[u8]) -> bool {
    sector.len() > 2 && crc16(&sector[..sector.len() - 2]) == u16::from_be_bytes([sector[sector.len() - 2], sector[sector.len() - 1]])
}

/// Write the CRC in the last two bytes.
pub fn seal(sector: &mut [u8]) {
    let n = sector.len();
    let crc = crc16(&sector[..n - 2]);
    sector[n - 2..].copy_from_slice(&crc.to_be_bytes());
}

/// One entry of the profile directory (sector 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirEntry {
    pub sector: u16,
    pub enabled: bool,
}

/// The profile directory: which sectors hold profiles and which are enabled
/// (the mouse's profile button only cycles through enabled ones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    pub entries: Vec<DirEntry>,
    raw: Vec<u8>,
}

impl Directory {
    pub fn parse(sector: &[u8]) -> Result<Self, String> {
        if !crc_ok(sector) {
            return Err("annuaire des profils : CRC invalide".into());
        }
        let mut entries = Vec::new();
        for e in sector[..sector.len() - 2].as_chunks::<4>().0 {
            let s = u16::from_be_bytes([e[0], e[1]]);
            if s == 0xFFFF {
                break;
            }
            entries.push(DirEntry { sector: s, enabled: e[2] != 0 });
        }
        Ok(Self { entries, raw: sector.to_vec() })
    }

    /// Sector bytes with the entries written back and a fresh CRC.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.raw.clone();
        for (i, e) in self.entries.iter().enumerate() {
            let at = i * 4;
            out[at..at + 2].copy_from_slice(&e.sector.to_be_bytes());
            out[at + 2] = u8::from(e.enabled);
        }
        seal(&mut out);
        out
    }
}

/// What a button does when it lives in the mouse's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Mouse buttons, as a bit mask (0x01 left, 0x02 right, 0x04 middle, 0x08 back, 0x10 forward…).
    Mouse(u16),
    /// Keyboard shortcut: HID modifier bits (0x01 LCtrl, 0x02 LShift, 0x04 LAlt, 0x08 LGui…) + HID usage.
    Key { modifiers: u8, usage: u8 },
    /// Consumer control (media keys…), HID usage.
    Consumer(u16),
    /// Built-in function (see [`Special`]).
    Special(Special),
    Disabled,
    /// Anything else (macros…), kept as is.
    Raw([u8; 4]),
}

/// Built-in mouse functions (`0x90 nn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    WheelLeft,
    WheelRight,
    DpiUp,
    DpiDown,
    DpiCycle,
    DpiDefault,
    /// "Sniper": DPI shift slot while held.
    DpiShift,
    ProfileNext,
    ProfilePrevious,
    ProfileCycle,
    /// G-shift: the alternate button table while held.
    GShift,
    BatteryStatus,
    Other(u8),
}

impl Special {
    fn code(self) -> u8 {
        match self {
            Self::WheelLeft => 1,
            Self::WheelRight => 2,
            Self::DpiUp => 3,
            Self::DpiDown => 4,
            Self::DpiCycle => 5,
            Self::DpiDefault => 6,
            Self::DpiShift => 7,
            Self::ProfileNext => 8,
            Self::ProfilePrevious => 9,
            Self::ProfileCycle => 10,
            Self::GShift => 11,
            Self::BatteryStatus => 12,
            Self::Other(c) => c,
        }
    }

    fn from_code(c: u8) -> Self {
        match c {
            1 => Self::WheelLeft,
            2 => Self::WheelRight,
            3 => Self::DpiUp,
            4 => Self::DpiDown,
            5 => Self::DpiCycle,
            6 => Self::DpiDefault,
            7 => Self::DpiShift,
            8 => Self::ProfileNext,
            9 => Self::ProfilePrevious,
            10 => Self::ProfileCycle,
            11 => Self::GShift,
            12 => Self::BatteryStatus,
            o => Self::Other(o),
        }
    }
}

impl Button {
    pub fn parse(b: [u8; 4]) -> Self {
        match b {
            [0xFF, 0xFF, 0xFF, 0xFF] => Self::Disabled,
            [0x80, 0x01, hi, lo] => Self::Mouse(u16::from_be_bytes([hi, lo])),
            [0x80, 0x02, modifiers, usage] => Self::Key { modifiers, usage },
            [0x80, 0x03, hi, lo] => Self::Consumer(u16::from_be_bytes([hi, lo])),
            [0x90, code, _, _] => Self::Special(Special::from_code(code)),
            raw => Self::Raw(raw),
        }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        match self {
            Self::Disabled => [0xFF; 4],
            Self::Mouse(m) => {
                let [hi, lo] = m.to_be_bytes();
                [0x80, 0x01, hi, lo]
            }
            Self::Key { modifiers, usage } => [0x80, 0x02, modifiers, usage],
            Self::Consumer(u) => {
                let [hi, lo] = u.to_be_bytes();
                [0x80, 0x03, hi, lo]
            }
            Self::Special(s) => [0x90, s.code(), 0x00, 0x00],
            Self::Raw(r) => r,
        }
    }
}

/// An LED effect slot (11 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Led {
    Off,
    Fixed { rgb: [u8; 3] },
    /// Colour cycle; `intensity` 1..=100 % (0 = 100 %).
    Cycle { period_ms: u16, intensity: u8 },
    Breathing { rgb: [u8; 3], period_ms: u16, waveform: u8, intensity: u8 },
    /// Other modes (wave, starlight, ripple, custom animation…), kept as is.
    Raw([u8; 11]),
}

impl Led {
    pub fn parse(b: &[u8]) -> Self {
        let mut raw = [0u8; 11];
        raw.copy_from_slice(&b[..11]);
        match raw[0] {
            0x00 => Self::Off,
            0x01 => Self::Fixed { rgb: [raw[1], raw[2], raw[3]] },
            0x03 => Self::Cycle { period_ms: u16::from_be_bytes([raw[6], raw[7]]), intensity: raw[8] },
            0x0A => Self::Breathing {
                rgb: [raw[1], raw[2], raw[3]],
                period_ms: u16::from_be_bytes([raw[4], raw[5]]),
                waveform: raw[6],
                intensity: raw[7],
            },
            _ => Self::Raw(raw),
        }
    }

    pub fn to_bytes(self) -> [u8; 11] {
        let mut b = [0u8; 11];
        match self {
            Self::Off => {}
            Self::Fixed { rgb } => {
                b[0] = 0x01;
                b[1..4].copy_from_slice(&rgb);
            }
            Self::Cycle { period_ms, intensity } => {
                b[0] = 0x03;
                b[6..8].copy_from_slice(&period_ms.to_be_bytes());
                b[8] = intensity;
            }
            Self::Breathing { rgb, period_ms, waveform, intensity } => {
                b[0] = 0x0A;
                b[1..4].copy_from_slice(&rgb);
                b[4..6].copy_from_slice(&period_ms.to_be_bytes());
                b[6] = waveform;
                b[7] = intensity;
            }
            Self::Raw(r) => b = r,
        }
        b
    }
}

pub const DPI_SLOTS: usize = 5;
pub const BUTTON_SLOTS: usize = 16;
pub const LED_SLOTS: usize = 4;
const BUTTONS: usize = 32;
const GSHIFT: usize = 96;
const NAME: usize = 160;
const NAME_LEN: usize = 48;
const LEDS: usize = 208;

/// A profile sector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    raw: Vec<u8>,
}

impl Profile {
    /// Parse a profile sector. `check_crc = false` for factory (ROM) profiles, which have none.
    pub fn parse(sector: &[u8], check_crc: bool) -> Result<Self, String> {
        if sector.len() < 254 {
            return Err(format!("secteur trop court ({} octets)", sector.len()));
        }
        if check_crc && !crc_ok(sector) {
            return Err("profil : CRC invalide".into());
        }
        Ok(Self { raw: sector.to_vec() })
    }

    /// Sector bytes with a fresh CRC.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.raw.clone();
        seal(&mut out);
        out
    }

    /// Report interval in ms (1 = 1000 Hz, 2 = 500 Hz, 4 = 250 Hz, 8 = 125 Hz).
    pub fn report_interval_ms(&self) -> u8 {
        self.raw[0].max(1)
    }
    pub fn set_report_interval_ms(&mut self, ms: u8) {
        self.raw[0] = ms.clamp(1, 8);
    }

    /// DPI of slot `i` (0..5); `None` when the slot is unused.
    pub fn dpi(&self, i: usize) -> Option<u16> {
        let v = u16::from_le_bytes([self.raw[3 + 2 * i], self.raw[4 + 2 * i]]);
        (v != 0 && v != 0xFFFF).then_some(v)
    }
    pub fn set_dpi(&mut self, i: usize, dpi: Option<u16>) {
        let v = dpi.unwrap_or(0);
        self.raw[3 + 2 * i..5 + 2 * i].copy_from_slice(&v.to_le_bytes());
    }

    pub fn default_dpi_slot(&self) -> u8 {
        self.raw[1]
    }
    pub fn set_default_dpi_slot(&mut self, i: u8) {
        self.raw[1] = i.min(DPI_SLOTS as u8 - 1);
    }
    pub fn shift_dpi_slot(&self) -> u8 {
        self.raw[2]
    }
    pub fn set_shift_dpi_slot(&mut self, i: u8) {
        self.raw[2] = i.min(DPI_SLOTS as u8 - 1);
    }

    fn slot4(&self, at: usize) -> [u8; 4] {
        [self.raw[at], self.raw[at + 1], self.raw[at + 2], self.raw[at + 3]]
    }

    /// Button `i` (0..16), normal or with G-shift held.
    pub fn button(&self, i: usize, gshift: bool) -> Button {
        Button::parse(self.slot4(if gshift { GSHIFT } else { BUTTONS } + 4 * i))
    }
    pub fn set_button(&mut self, i: usize, gshift: bool, b: Button) {
        let at = if gshift { GSHIFT } else { BUTTONS } + 4 * i;
        self.raw[at..at + 4].copy_from_slice(&b.to_bytes());
    }

    /// LED slot `k` (0..4).
    pub fn led(&self, k: usize) -> Led {
        Led::parse(&self.raw[LEDS + 11 * k..LEDS + 11 * k + 11])
    }
    pub fn set_led(&mut self, k: usize, led: Led) {
        self.raw[LEDS + 11 * k..LEDS + 11 * k + 11].copy_from_slice(&led.to_bytes());
    }

    /// Profile name (`None` when unset).
    pub fn name(&self) -> Option<String> {
        let b = &self.raw[NAME..NAME + NAME_LEN];
        if b.iter().all(|&x| x == 0xFF) {
            return None;
        }
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&u| u != 0).collect();
        Some(String::from_utf16_lossy(&units))
    }
    /// Set the name (23 UTF-16 units at most); `None` clears it.
    pub fn set_name(&mut self, name: Option<&str>) {
        let b = &mut self.raw[NAME..NAME + NAME_LEN];
        b.fill(0xFF);
        if let Some(n) = name {
            b.fill(0);
            for (i, u) in n.encode_utf16().take(NAME_LEN / 2 - 1).enumerate() {
                b[2 * i..2 * i + 2].copy_from_slice(&u.to_le_bytes());
            }
        }
    }

    /// Animation played instead of the LED effects (G502 X PLUS: byte 252, 3 = factory
    /// "signature" gradient). Must be 0 for the LED slots to show; libratbag clears it too.
    pub fn custom_animation(&self) -> u8 {
        self.raw[252]
    }
    pub fn set_custom_animation(&mut self, index: u8) {
        self.raw[252] = index;
    }

    /// Take the whole lighting (4 LED slots + animation byte) from `other`,
    /// e.g. the factory ROM profile to get the factory gradient back exactly.
    pub fn copy_lighting_from(&mut self, other: &Profile) {
        self.raw[LEDS..=252].copy_from_slice(&other.raw[LEDS..=252]);
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &[u8] = include_bytes!("../tests/data/g502xplus_directory.bin");
    const P1: &[u8] = include_bytes!("../tests/data/g502xplus_profile1.bin");
    const P2: &[u8] = include_bytes!("../tests/data/g502xplus_profile2.bin");
    const ROM: &[u8] = include_bytes!("../tests/data/g502xplus_rom1.bin");

    #[test]
    fn crc_matches_the_mouse() {
        assert!(crc_ok(DIR) && crc_ok(P1) && crc_ok(P2));
        assert_eq!(crc16(b"123456789"), 0x29B1, "CRC-16/CCITT-FALSE check value");
    }

    #[test]
    fn directory() {
        let d = Directory::parse(DIR).unwrap();
        assert_eq!(d.entries.len(), 5);
        assert_eq!(d.entries[0], DirEntry { sector: 1, enabled: true });
        assert!(d.entries[1].enabled && !d.entries[2].enabled);
        assert_eq!(d.to_bytes(), DIR, "untouched round trip");
    }

    #[test]
    fn profile_fields() {
        let p = Profile::parse(P1, true).unwrap();
        assert_eq!(p.report_interval_ms(), 1);
        assert_eq!((0..5).map(|i| p.dpi(i).unwrap()).collect::<Vec<_>>(), [800, 1200, 1600, 2400, 3200]);
        assert_eq!((p.default_dpi_slot(), p.shift_dpi_slot()), (2, 0));
        assert_eq!(p.button(0, false), Button::Mouse(1));
        assert_eq!(p.button(4, false), Button::Special(Special::DpiShift));
        assert_eq!(p.button(8, false), Button::Special(Special::ProfileCycle));
        assert_eq!(p.button(0, true), Button::Disabled);
        assert_eq!(p.name(), None);
        assert_eq!(p.custom_animation(), 3, "factory signature gradient");
        assert_eq!(p.to_bytes(), P1, "untouched round trip");

        let p2 = Profile::parse(P2, true).unwrap();
        assert_eq!(p2.report_interval_ms(), 2);
        assert_eq!(p2.button(4, false), Button::Special(Special::GShift));
        assert_eq!(p2.button(2, true), Button::Key { modifiers: 0x01, usage: 0x17 }, "Ctrl+T");

        assert!(Profile::parse(ROM, false).is_ok());
    }

    #[test]
    fn edits_round_trip() {
        let mut p = Profile::parse(P1, true).unwrap();
        p.set_dpi(0, Some(1000));
        p.set_report_interval_ms(2);
        p.set_led(1, Led::Fixed { rgb: [0x19, 0xE3, 0xCF] });
        p.set_led(0, Led::Breathing { rgb: [255, 0, 0], period_ms: 3000, waveform: 1, intensity: 80 });
        p.set_button(5, true, Button::Key { modifiers: 0x01, usage: 0x06 });
        p.set_name(Some("Jeu"));
        let bytes = p.to_bytes();
        assert!(crc_ok(&bytes));
        let q = Profile::parse(&bytes, true).unwrap();
        assert_eq!(q.dpi(0), Some(1000));
        assert_eq!(q.report_interval_ms(), 2);
        assert_eq!(q.led(1), Led::Fixed { rgb: [0x19, 0xE3, 0xCF] });
        assert_eq!(q.led(0), Led::Breathing { rgb: [255, 0, 0], period_ms: 3000, waveform: 1, intensity: 80 });
        assert_eq!(q.button(5, true), Button::Key { modifiers: 0x01, usage: 0x06 });
        assert_eq!(q.name().as_deref(), Some("Jeu"));
        // nothing else moved
        let changed: Vec<usize> = (0..P1.len() - 2).filter(|&i| bytes[i] != P1[i]).collect();
        assert!(changed.iter().all(|&i| i == 0 || (3..5).contains(&i) || (116..120).contains(&i) || (160..208).contains(&i) || (208..230).contains(&i)), "{changed:?}");
        p.set_name(None);
        assert_eq!(p.name(), None);
    }
}
