//! Binding tables: one table per M-bank, one profile = 3 banks.
//!
//! A bank has one slot per bindable [`Input`]: the 25 G-keys (G1..G24 +
//! G25 stick click, slots 0..=24 = their `bit_index`) and the 4 stick
//! directions (slots 25..=28, used in [`StickMode::Keys`]).

use g13_core::{G13Key, MKeyLeds, RgbColor};

use super::action::Action;
pub use super::pad::PadStick;

/// Number of bindable keys (G1..G24 + G25 stick click = bit indices 0..=24).
pub const BINDABLE_KEYS: usize = 25;

/// Total number of binding slots per bank (keys + stick directions).
pub const SLOTS: usize = BINDABLE_KEYS + 4;

/// The three M-banks (M1 / M2 / M3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MBank {
    #[default]
    M1,
    M2,
    M3,
}

impl MBank {
    pub const ALL: [MBank; 3] = [MBank::M1, MBank::M2, MBank::M3];

    pub const fn index(self) -> usize {
        match self {
            Self::M1 => 0,
            Self::M2 => 1,
            Self::M3 => 2,
        }
    }

    pub const fn from_key(key: G13Key) -> Option<Self> {
        match key {
            G13Key::M1 => Some(Self::M1),
            G13Key::M2 => Some(Self::M2),
            G13Key::M3 => Some(Self::M3),
            _ => None,
        }
    }

    /// LED state with only this bank lit (+ MR as requested).
    pub fn leds(self, mr: bool) -> MKeyLeds {
        MKeyLeds::new()
            .m1(self == Self::M1)
            .m2(self == Self::M2)
            .m3(self == Self::M3)
            .mr(mr)
    }
}

/// A stick direction, bindable like a key when the stick is in keys mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickDir {
    Up,
    Down,
    Left,
    Right,
}

impl StickDir {
    pub const ALL: [StickDir; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    pub const fn index(self) -> usize {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }

    pub const fn slot(self) -> usize {
        BINDABLE_KEYS + self.index()
    }

    /// Identifier used in profile files.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Up => "STICK_UP",
            Self::Down => "STICK_DOWN",
            Self::Left => "STICK_LEFT",
            Self::Right => "STICK_RIGHT",
        }
    }
}

/// Anything that can carry a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    Key(G13Key),
    Stick(StickDir),
}

impl Input {
    /// Slot in a [`BankBindings`] table, `None` for non-bindable keys (M, L, BD).
    pub fn slot(self) -> Option<usize> {
        match self {
            Self::Key(k) if k.is_g_key() => Some(k.bit_index() as usize),
            Self::Key(_) => None,
            Self::Stick(d) => Some(d.slot()),
        }
    }

    pub fn from_slot(slot: usize) -> Option<Self> {
        if slot < BINDABLE_KEYS {
            G13Key::from_bit_index(slot as u8).map(Self::Key)
        } else {
            StickDir::ALL.get(slot - BINDABLE_KEYS).copied().map(Self::Stick)
        }
    }

    /// `"G1"`…`"G25"`, `"STICK_UP"`…
    pub fn id(self) -> &'static str {
        match self {
            Self::Key(k) => k.id(),
            Self::Stick(d) => d.id(),
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        G13Key::from_id(s)
            .map(Self::Key)
            .or_else(|| StickDir::ALL.into_iter().find(|d| d.id().eq_ignore_ascii_case(s)).map(Self::Stick))
    }
}

/// Actions for the bindable inputs of one bank.
#[derive(Debug, Clone, PartialEq)]
pub struct BankBindings {
    slots: [Action; SLOTS],
    /// Backlight colour applied when this bank becomes active (None = keep).
    pub color: Option<RgbColor>,
}

impl Default for BankBindings {
    fn default() -> Self {
        Self { slots: std::array::from_fn(|_| Action::None), color: None }
    }
}

impl BankBindings {
    /// Action bound to `key` (`Action::None` for non-bindable keys).
    pub fn get(&self, key: G13Key) -> &Action {
        self.get_input(Input::Key(key))
    }

    pub fn get_input(&self, input: Input) -> &Action {
        static NONE: Action = Action::None;
        input.slot().map_or(&NONE, |s| &self.slots[s])
    }

    /// Action of a raw slot (see [`Input::slot`]).
    pub fn slot(&self, slot: usize) -> &Action {
        &self.slots[slot]
    }

    /// Bind `key`. Returns `false` if the key is not bindable (M-keys, L-keys, BD).
    pub fn set(&mut self, key: G13Key, action: Action) -> bool {
        self.set_input(Input::Key(key), action)
    }

    pub fn set_input(&mut self, input: Input, action: Action) -> bool {
        match input.slot() {
            Some(s) => {
                self.slots[s] = action;
                true
            }
            None => false,
        }
    }

    /// Builder: backlight colour for this bank.
    pub fn colored(color: RgbColor) -> Self {
        Self { color: Some(color), ..Self::default() }
    }

    pub fn with(mut self, key: G13Key, action: Action) -> Self {
        self.set(key, action);
        self
    }

    pub fn with_input(mut self, input: Input, action: Action) -> Self {
        self.set_input(input, action);
        self
    }

    /// Every slot in order: G1..G25, then STICK_UP/DOWN/LEFT/RIGHT.
    pub fn iter(&self) -> impl Iterator<Item = (Input, &Action)> {
        self.slots.iter().enumerate().filter_map(|(i, a)| Input::from_slot(i).map(|k| (k, a)))
    }
}

/// What the analog stick does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StickMode {
    /// The 4 directions act as bindable keys (STICK_UP…).
    #[default]
    Keys,
    /// Analog stick of a virtual Xbox 360 controller (see `engine::pad`).
    Gamepad,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StickConfig {
    pub mode: StickMode,
    /// Dead zone, 0.0..1.0 of the full deflection (per axis in keys mode,
    /// radial in gamepad mode).
    pub deadzone: f32,
    /// Gamepad mode: which controller stick it drives.
    pub target: PadStick,
}

impl Default for StickConfig {
    fn default() -> Self {
        Self { mode: StickMode::Keys, deadzone: 0.35, target: PadStick::Left }
    }
}

/// A full configuration: three banks + stick settings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Profile {
    pub name: String,
    pub banks: [BankBindings; 3],
    pub stick: StickConfig,
}

impl Profile {
    pub fn bank(&self, bank: MBank) -> &BankBindings {
        &self.banks[bank.index()]
    }

    pub fn bank_mut(&mut self, bank: MBank) -> &mut BankBindings {
        &mut self.banks[bank.index()]
    }

    /// Does this profile need the virtual gamepad (stick in gamepad mode or
    /// a gamepad button bound anywhere)?
    pub fn uses_pad(&self) -> bool {
        self.stick.mode == StickMode::Gamepad
            || self.banks.iter().any(|b| b.slots.iter().any(|a| match a {
                Action::Pad(_) => true,
                Action::TapDance(td) => [&td.tap, &td.hold, &td.double_tap, &td.tap_hold].iter().any(|x| matches!(x, Action::Pad(_))),
                _ => false,
            }))
    }
}
