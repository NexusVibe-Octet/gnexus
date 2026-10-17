//! Real-time mouse profiles ("host mode"): the mouse's onboard profiles are off,
//! G13 Studio picks one of its own profiles for the application in the
//! foreground (profile 1 everywhere else) and carries out what the buttons do.
//!
//! The mouse reports its buttons through MouseButtonSpy (0x8110, as G HUB does):
//! an event with a bit per physical button. A button whose action is a plain
//! mouse button keeps going straight to Windows (remap table, fn 4), so clicks
//! cost no latency and still work if G13 Studio stops; every other button is
//! muted (remap 0) and its action is done here — keys, DPI steps, sniper,
//! G-shift, profile changes, horizontal scroll.
//!
//! This file is the pure part (no I/O): [`HostState::buttons`] turns a new
//! button mask into [`Effect`]s that the mouse thread executes.

use serde::{Deserialize, Serialize};

use super::settings::{MouseAction, MouseSettings};
use crate::keycode::{KeyChord, KeyCode, MouseButton};

/// Physical button reported as bit `n` of the spy mask → profile slot (see
/// [`MouseSettings::buttons`]). Measured on the G502 X PLUS.
pub const SPY_SLOTS: [usize; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/// Buttons held, from a MouseButtonSpy event: bit `n` = physical button `n + 1`
/// (`00 01` = left click, measured).
pub fn spy_mask(d: &[u8; 16]) -> u16 {
    u16::from_be_bytes([d[0], d[1]])
}

/// Remap table that gives every button back to the mouse (physical n → HID n).
pub const IDENTITY: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 0, 0, 0, 0, 0];

/// A G13 Studio mouse profile (used while the onboard profiles are off).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftProfile {
    pub name: String,
    /// Executables (`game.exe`) this profile is for; the first profile also
    /// serves every other application.
    #[serde(default)]
    pub apps: Vec<String>,
    pub settings: MouseSettings,
}

/// Which profile is used: the one tested everywhere (`pinned`), else the one bound
/// to `exe` (case-insensitive), else the default one.
pub fn choose(profiles: &[SoftProfile], exe: Option<&str>, default: usize, pinned: Option<usize>) -> usize {
    let n = profiles.len();
    if let Some(p) = pinned.filter(|&p| p < n) {
        return p;
    }
    let default = if default < n { default } else { 0 };
    let Some(exe) = exe else { return default };
    profiles
        .iter()
        .position(|p| p.apps.iter().any(|a| a.eq_ignore_ascii_case(exe)))
        .unwrap_or(default)
}

/// Remap value of a plain mouse button (HID button number), 0 = sends nothing.
fn native(a: Option<&MouseAction>) -> Option<u8> {
    let Some(MouseAction::Mouse { mouse }) = a else { return None };
    Some(match MouseButton::parse(mouse)? {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 3,
        MouseButton::Back => 4,
        MouseButton::Forward => 5,
    })
}

fn is_gshift(a: Option<&MouseAction>) -> bool {
    matches!(a, Some(MouseAction::Special { special }) if special == "gshift")
}

/// Remap table for `s` (index = physical button − 1): plain mouse buttons go
/// straight to Windows, unless G-shift gives them another action. (In real time,
/// a button with nothing in the G-shift layer keeps its normal action.)
pub fn remap_table(s: &MouseSettings) -> [u8; 16] {
    let has_gshift = s.buttons.iter().any(|a| is_gshift(a.as_ref()));
    let mut t = [0u8; 16];
    for (bit, &slot) in SPY_SLOTS.iter().enumerate() {
        let a = s.buttons.get(slot).and_then(Option::as_ref);
        let shifted = s.gshift.get(slot).and_then(Option::as_ref);
        let same_shifted = !has_gshift || shifted.is_none() || shifted == a;
        if same_shifted {
            t[bit] = native(a).unwrap_or(0);
        }
    }
    t
}

/// What the mouse thread must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Key(KeyCode, bool),
    Mouse(MouseButton, bool),
    /// One notch of horizontal scroll (+1 = right).
    HWheel(i32),
    /// Set the sensor's DPI now.
    Dpi(u16),
    /// Use another profile (index), e.g. with a "next profile" button.
    Profile(usize),
    /// A button bound to a macro was pressed / released.
    MacroDown(String),
    MacroUp(String),
}

/// Held state of the buttons and the DPI.
#[derive(Debug, Clone, Default)]
pub struct HostState {
    /// Last mask received.
    pub mask: u16,
    /// Current DPI step (index in `settings.dpi`).
    pub step: usize,
    sniper: bool,
    gshift: bool,
    /// What each pressed physical button did, to undo exactly that on release.
    held: [Option<MouseAction>; 16],
}

impl HostState {
    pub fn new(s: &MouseSettings) -> Self {
        Self { step: usize::from(s.default_slot).min(s.dpi.len().saturating_sub(1)), ..Self::default() }
    }

    fn dpi(&self, s: &MouseSettings) -> Option<u16> {
        let i = if self.sniper { usize::from(s.shift_slot) } else { self.step };
        s.dpi.get(i).copied()
    }

    /// Effects of going from the last mask to `mask` (profile `current` of `count`).
    pub fn buttons(&mut self, s: &MouseSettings, mask: u16, current: usize, count: usize) -> Vec<Effect> {
        let mut out = Vec::new();
        let changed = mask ^ self.mask;
        self.mask = mask;
        // releases first: a button let go never stays held
        for bit in (0..16).filter(|b| changed & (1 << b) != 0 && mask & (1 << b) == 0) {
            if let Some(a) = self.held[bit].take() {
                self.release(s, &a, &mut out);
            }
        }
        for bit in (0..16).filter(|b| changed & (1 << b) != 0 && mask & (1 << b) != 0) {
            let Some(&slot) = SPY_SLOTS.get(bit) else { continue };
            let shifted = if self.gshift { s.gshift.get(slot).cloned().flatten() } else { None };
            let Some(a) = shifted.or_else(|| s.buttons.get(slot).cloned().flatten()) else { continue };
            // a plain mouse button already went to Windows (remap table)
            if remap_table(s)[bit] != 0 {
                continue;
            }
            self.press(s, &a, current, count, &mut out);
            self.held[bit] = Some(a);
        }
        out
    }

    /// Everything held is let go (profile change, mouse gone, shutdown).
    pub fn release_all(&mut self, s: &MouseSettings) -> Vec<Effect> {
        let mut out = Vec::new();
        for bit in 0..16 {
            if let Some(a) = self.held[bit].take() {
                self.release(s, &a, &mut out);
            }
        }
        self.mask = 0;
        out
    }

    fn press(&mut self, s: &MouseSettings, a: &MouseAction, current: usize, count: usize, out: &mut Vec<Effect>) {
        match a {
            MouseAction::Key(k) => {
                if let Some(c) = KeyChord::parse(k) {
                    out.extend(c.keys().iter().map(|&k| Effect::Key(k, true)));
                }
            }
            MouseAction::Mouse { mouse } => {
                if let Some(b) = MouseButton::parse(mouse) {
                    out.push(Effect::Mouse(b, true));
                }
            }
            MouseAction::Special { special } => {
                let n = s.dpi.len().max(1);
                let before = self.dpi(s);
                match special.as_str() {
                    "dpi_up" => self.step = (self.step + 1).min(n - 1),
                    "dpi_down" => self.step = self.step.saturating_sub(1),
                    "dpi_cycle" => self.step = (self.step + 1) % n,
                    "dpi_default" => self.step = usize::from(s.default_slot).min(n - 1),
                    "dpi_shift" => self.sniper = true,
                    "gshift" => self.gshift = true,
                    "profile_next" | "profile_cycle" if count > 1 => out.push(Effect::Profile((current + 1) % count)),
                    "profile_previous" if count > 1 => out.push(Effect::Profile((current + count - 1) % count)),
                    "wheel_left" => out.push(Effect::HWheel(-1)),
                    "wheel_right" => out.push(Effect::HWheel(1)),
                    _ => {}
                }
                if let Some(d) = self.dpi(s).filter(|&d| Some(d) != before) {
                    out.push(Effect::Dpi(d));
                }
            }
            MouseAction::Macro { name } => out.push(Effect::MacroDown(name.clone())),
            MouseAction::Raw { .. } => {}
        }
    }

    fn release(&mut self, s: &MouseSettings, a: &MouseAction, out: &mut Vec<Effect>) {
        match a {
            MouseAction::Key(k) => {
                if let Some(c) = KeyChord::parse(k) {
                    out.extend(c.keys().iter().rev().map(|&k| Effect::Key(k, false)));
                }
            }
            MouseAction::Mouse { mouse } => {
                if let Some(b) = MouseButton::parse(mouse) {
                    out.push(Effect::Mouse(b, false));
                }
            }
            MouseAction::Special { special } if special == "dpi_shift" => {
                self.sniper = false;
                if let Some(d) = self.dpi(s) {
                    out.push(Effect::Dpi(d));
                }
            }
            MouseAction::Special { special } if special == "gshift" => self.gshift = false,
            MouseAction::Macro { name } => out.push(Effect::MacroUp(name.clone())),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mouse::settings::Lighting;

    fn m(b: &str) -> Option<MouseAction> {
        Some(MouseAction::Mouse { mouse: b.into() })
    }
    fn sp(n: &str) -> Option<MouseAction> {
        Some(MouseAction::Special { special: n.into() })
    }
    fn k(n: &str) -> Option<MouseAction> {
        Some(MouseAction::Key(n.into()))
    }

    fn settings() -> MouseSettings {
        MouseSettings {
            dpi: vec![800, 1600, 3200],
            default_slot: 1,
            shift_slot: 0,
            report_rate_hz: 1000,
            lighting: Lighting::Off,
            buttons: vec![m("Left"), m("Right"), m("Middle"), m("Back"), sp("dpi_shift"), m("Forward"), sp("wheel_left"),
                sp("wheel_right"), sp("profile_next"), sp("dpi_up"), sp("dpi_down")],
            gshift: vec![None; 11],
            macros: Default::default(),
        }
    }

    #[test]
    fn plain_buttons_stay_native() {
        let t = remap_table(&settings());
        assert_eq!(&t[..11], &[1, 2, 3, 4, 0, 5, 0, 0, 0, 0, 0]);
        let mut s = settings();
        s.buttons[4] = sp("gshift");
        s.gshift[1] = k("LeftCtrl+C");
        // G-shift changes the right click: it is done in software
        assert_eq!(&remap_table(&s)[..3], &[1, 0, 3]);
    }

    #[test]
    fn dpi_steps_and_sniper() {
        let s = settings();
        let mut h = HostState::new(&s);
        assert_eq!(h.buttons(&s, 1 << 9, 0, 1), vec![Effect::Dpi(3200)]);
        assert_eq!(h.buttons(&s, 0, 0, 1), vec![]);
        assert_eq!(h.buttons(&s, 1 << 9, 0, 1), vec![], "already at the top");
        h.buttons(&s, 0, 0, 1);
        assert_eq!(h.buttons(&s, 1 << 4, 0, 1), vec![Effect::Dpi(800)]);
        assert_eq!(h.buttons(&s, 0, 0, 1), vec![Effect::Dpi(3200)]);
    }

    #[test]
    fn keys_are_released_even_after_a_layer_change() {
        let mut s = settings();
        s.buttons[4] = sp("gshift");
        s.buttons[8] = k("LeftCtrl+T");
        s.gshift[8] = k("F5");
        let mut h = HostState::new(&s);
        assert_eq!(h.buttons(&s, 1 << 8, 0, 1), vec![Effect::Key(KeyCode::LeftCtrl, true), Effect::Key(KeyCode::T, true)]);
        h.buttons(&s, (1 << 8) | (1 << 4), 0, 1);
        // let go of the button while G-shift is held: Ctrl+T is released, not F5
        assert_eq!(h.buttons(&s, 1 << 4, 0, 1), vec![Effect::Key(KeyCode::T, false), Effect::Key(KeyCode::LeftCtrl, false)]);
        assert_eq!(h.buttons(&s, (1 << 4) | (1 << 8), 0, 1), vec![Effect::Key(KeyCode::F5, true)]);
        assert_eq!(h.release_all(&s), vec![Effect::Key(KeyCode::F5, false)]);
    }

    #[test]
    fn macro_buttons() {
        let mut s = settings();
        s.buttons[9] = Some(MouseAction::Macro { name: "spam".into() });
        assert_eq!(remap_table(&s)[9], 0);
        let mut h = HostState::new(&s);
        assert_eq!(h.buttons(&s, 1 << 9, 0, 1), vec![Effect::MacroDown("spam".into())]);
        assert_eq!(h.buttons(&s, 0, 0, 1), vec![Effect::MacroUp("spam".into())]);
    }

    #[test]
    fn profile_buttons_and_apps() {
        let s = settings();
        let mut h = HostState::new(&s);
        assert_eq!(h.buttons(&s, 1 << 8, 2, 3), vec![Effect::Profile(0)]);
        assert_eq!(h.buttons(&s, 1 << 6, 0, 3), vec![Effect::HWheel(-1)]);
        let p = |apps: &[&str]| SoftProfile { name: String::new(), apps: apps.iter().map(|a| a.to_string()).collect(), settings: settings() };
        let ps = [p(&[]), p(&["game.exe"])];
        assert_eq!(choose(&ps, Some("Game.EXE"), 0, None), 1);
        assert_eq!(choose(&ps, Some("other.exe"), 0, None), 0);
        assert_eq!(choose(&ps, None, 0, None), 0);
        // another default, a profile tested everywhere
        assert_eq!(choose(&ps, Some("other.exe"), 1, None), 1);
        assert_eq!(choose(&ps, Some("Game.exe"), 0, Some(0)), 0);
        assert_eq!(choose(&ps, None, 9, Some(7)), 0, "out of range: ignored");
    }
}
