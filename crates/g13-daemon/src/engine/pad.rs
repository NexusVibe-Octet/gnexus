//! Virtual gamepad: the stick "gamepad" mode and gamepad-button bindings.
//!
//! The engine keeps one [`Pad`]: how many inputs hold each button, and the
//! calibrated stick position. [`Pad::state`] folds both into the
//! [`GamepadState`] sent to the OS (Windows: Xbox 360 controller via
//! ViGEmBus).
//!
//! Stick calibration (the G13's stick is far from ideal):
//! * the **rest position** drifts (measured X 0x7C–0x80, Y 0x6F–0x80) and is
//!   re-learnt whenever the stick stays still near the middle for
//!   [`SETTLE`];
//! * the **travel** is shorter than 0..=255: the range starts at
//!   ±[`INITIAL_SPAN`] around the centre and grows with every position seen,
//!   so full deflection always reaches ±1.0.

use std::time::{Duration, Instant};

use g13_core::StickPosition;

use crate::platform::GamepadState;

/// A button of the virtual (Xbox 360) controller. LT / RT are triggers,
/// fully pressed while held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    Back,
    Start,
    /// Left stick click.
    LS,
    /// Right stick click.
    RS,
    Up,
    Down,
    Left,
    Right,
}

impl PadButton {
    pub const ALL: [PadButton; 16] = [
        Self::A,
        Self::B,
        Self::X,
        Self::Y,
        Self::LB,
        Self::RB,
        Self::LT,
        Self::RT,
        Self::Back,
        Self::Start,
        Self::LS,
        Self::RS,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&b| b == self).unwrap_or(0)
    }

    /// Identifier used in profile files (`G1 = { pad = "A" }`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
            Self::LB => "LB",
            Self::RB => "RB",
            Self::LT => "LT",
            Self::RT => "RT",
            Self::Back => "Back",
            Self::Start => "Start",
            Self::LS => "LS",
            Self::RS => "RS",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    /// Case-insensitive; also accepts `L3` / `R3`, `Select`, `Dpad*`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix("Dpad").or_else(|| s.strip_prefix("dpad")).unwrap_or(s);
        let alias = match s.to_ascii_lowercase().as_str() {
            "l3" => Some(Self::LS),
            "r3" => Some(Self::RS),
            "select" | "view" => Some(Self::Back),
            "menu" => Some(Self::Start),
            _ => None,
        };
        alias.or_else(|| Self::ALL.into_iter().find(|b| b.id().eq_ignore_ascii_case(s)))
    }

    /// XInput button bit (`None` for the triggers).
    pub const fn xinput_bit(self) -> Option<u16> {
        Some(match self {
            Self::Up => 0x0001,
            Self::Down => 0x0002,
            Self::Left => 0x0004,
            Self::Right => 0x0008,
            Self::Start => 0x0010,
            Self::Back => 0x0020,
            Self::LS => 0x0040,
            Self::RS => 0x0080,
            Self::LB => 0x0100,
            Self::RB => 0x0200,
            Self::A => 0x1000,
            Self::B => 0x2000,
            Self::X => 0x4000,
            Self::Y => 0x8000,
            Self::LT | Self::RT => return None,
        })
    }
}

/// Which stick of the virtual controller the G13's stick drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadStick {
    #[default]
    Left,
    Right,
}

/// The stick must stay this still near the middle to become the new rest position.
pub const SETTLE: Duration = Duration::from_millis(1500);
/// "Near the middle" for re-centring, in raw units around 128.
const SETTLE_WINDOW: u8 = 0x16;
/// Assumed travel on each side of the centre until a larger one is seen.
const INITIAL_SPAN: u8 = 0x50;

#[derive(Debug, Clone)]
struct Calibration {
    center: (f32, f32),
    min: (u8, u8),
    max: (u8, u8),
    /// Position last reported and since when (for re-centring).
    last: Option<(StickPosition, Instant)>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            center: (128.0, 128.0),
            min: (128 - INITIAL_SPAN, 128 - INITIAL_SPAN),
            max: (128 + INITIAL_SPAN, 128 + INITIAL_SPAN),
            last: None,
        }
    }
}

impl Calibration {
    fn observe(&mut self, pos: StickPosition, now: Instant) {
        self.min = (self.min.0.min(pos.raw_x), self.min.1.min(pos.raw_y));
        self.max = (self.max.0.max(pos.raw_x), self.max.1.max(pos.raw_y));
        self.last = Some((pos, now));
    }

    /// Re-centre on a still, near-middle position. `true` if the centre moved.
    fn settle(&mut self, now: Instant) -> bool {
        let Some((pos, since)) = self.last else { return false };
        let near = |v: u8| v.abs_diff(128) <= SETTLE_WINDOW;
        if !near(pos.raw_x) || !near(pos.raw_y) || now.duration_since(since) < SETTLE {
            return false;
        }
        let c = (f32::from(pos.raw_x), f32::from(pos.raw_y));
        let moved = c != self.center;
        self.center = c;
        moved
    }

    /// -1.0..=1.0 on each axis, +y = up.
    fn normalize(&self, pos: StickPosition) -> (f32, f32) {
        let axis = |v: u8, c: f32, lo: u8, hi: u8| {
            let d = f32::from(v) - c;
            let span = if d >= 0.0 { f32::from(hi) - c } else { c - f32::from(lo) };
            if span <= 0.0 { 0.0 } else { (d / span).clamp(-1.0, 1.0) }
        };
        let x = axis(pos.raw_x, self.center.0, self.min.0, self.max.0);
        let y = axis(pos.raw_y, self.center.1, self.min.1, self.max.1);
        (x, -y)
    }
}

/// Radial dead zone, rescaled so that output starts at 0 at its edge.
fn apply_deadzone((x, y): (f32, f32), dz: f32) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    if mag <= dz || mag == 0.0 {
        return (0.0, 0.0);
    }
    let scaled = ((mag - dz) / (1.0 - dz)).min(1.0);
    (x / mag * scaled, y / mag * scaled)
}

fn to_axis(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Virtual controller state kept by the engine.
#[derive(Debug, Clone, Default)]
pub struct Pad {
    /// How many bound inputs currently hold each button (by [`PadButton::index`]).
    held: [u8; 16],
    calib: Calibration,
    /// Last state sent to the OS; `None` = controller unplugged.
    pub sent: Option<GamepadState>,
}

impl Pad {
    pub fn press(&mut self, b: PadButton) {
        let n = &mut self.held[b.index()];
        *n = n.saturating_add(1);
    }

    pub fn release(&mut self, b: PadButton) {
        let n = &mut self.held[b.index()];
        *n = n.saturating_sub(1);
    }

    pub fn release_all(&mut self) {
        self.held = [0; 16];
    }

    pub fn observe(&mut self, pos: StickPosition, now: Instant) {
        self.calib.observe(pos, now);
    }

    /// See [`Calibration::settle`].
    pub fn settle(&mut self, now: Instant) -> bool {
        self.calib.settle(now)
    }

    /// The controller state: held buttons, plus the stick when `stick` is
    /// given (gamepad mode) — `(position, dead zone, target stick)`.
    pub fn state(&self, stick: Option<(StickPosition, f32, PadStick)>) -> GamepadState {
        let mut st = GamepadState::default();
        for b in PadButton::ALL {
            if self.held[b.index()] == 0 {
                continue;
            }
            match b {
                PadButton::LT => st.left_trigger = 255,
                PadButton::RT => st.right_trigger = 255,
                _ => st.buttons |= b.xinput_bit().unwrap_or(0),
            }
        }
        if let Some((pos, dz, target)) = stick {
            let (x, y) = apply_deadzone(self.calib.normalize(pos), dz.clamp(0.0, 0.95));
            let (x, y) = (to_axis(x), to_axis(y));
            match target {
                PadStick::Left => (st.left_x, st.left_y) = (x, y),
                PadStick::Right => (st.right_x, st.right_y) = (x, y),
            }
        }
        st
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: u8, y: u8) -> StickPosition {
        StickPosition { raw_x: x, raw_y: y }
    }

    #[test]
    fn parse_ids_and_aliases() {
        for b in PadButton::ALL {
            assert_eq!(PadButton::parse(b.id()), Some(b));
        }
        assert_eq!(PadButton::parse("l3"), Some(PadButton::LS));
        assert_eq!(PadButton::parse("DpadUp"), Some(PadButton::Up));
        assert_eq!(PadButton::parse("select"), Some(PadButton::Back));
        assert_eq!(PadButton::parse("Z"), None);
    }

    #[test]
    fn buttons_and_triggers() {
        let mut p = Pad::default();
        p.press(PadButton::A);
        p.press(PadButton::A); // two G-keys on the same button
        p.press(PadButton::RT);
        let st = p.state(None);
        assert_eq!(st.buttons, 0x1000);
        assert_eq!(st.right_trigger, 255);
        p.release(PadButton::A);
        assert_eq!(p.state(None).buttons, 0x1000, "still held by the other key");
        p.release(PadButton::A);
        p.release(PadButton::A); // extra release does not underflow
        assert_eq!(p.state(None).buttons, 0);
        p.release_all();
        assert_eq!(p.state(None), GamepadState::default());
    }

    #[test]
    fn stick_full_deflection_and_deadzone() {
        let mut p = Pad::default();
        let t = Instant::now();
        // initial span: 128 ± 0x50 reaches the ends
        let st = p.state(Some((pos(128 + 0x50, 128), 0.1, PadStick::Left)));
        assert_eq!(st.left_x, 32767);
        let st = p.state(Some((pos(128, 128 - 0x50), 0.1, PadStick::Left)));
        assert_eq!(st.left_y, 32767, "up is +Y");
        // a wider travel is learnt: 0xF0 becomes full right, 0xD0 is no longer
        p.observe(pos(0xF0, 128), t);
        assert_eq!(p.state(Some((pos(0xF0, 128), 0.1, PadStick::Left))).left_x, 32767);
        assert!(p.state(Some((pos(0xD0, 128), 0.1, PadStick::Left))).left_x < 30000);
        // inside the dead zone
        let st = p.state(Some((pos(132, 124), 0.1, PadStick::Right)));
        assert_eq!((st.right_x, st.right_y, st.left_x), (0, 0, 0));
    }

    #[test]
    fn rest_position_is_relearnt() {
        let mut p = Pad::default();
        let t = Instant::now();
        let rest = pos(0x7E, 0x6F); // measured worst case
        p.observe(rest, t);
        // before settling, a small dead zone sees the offset as a push up
        assert!(p.state(Some((rest, 0.1, PadStick::Left))).left_y > 0);
        assert!(!p.settle(t + Duration::from_millis(500)));
        assert!(p.settle(t + SETTLE));
        assert_eq!(p.state(Some((rest, 0.1, PadStick::Left))), GamepadState::default());
        // held far from the middle: never becomes the centre
        p.observe(pos(0xE0, 128), t);
        assert!(!p.settle(t + SETTLE * 4));
    }
}
