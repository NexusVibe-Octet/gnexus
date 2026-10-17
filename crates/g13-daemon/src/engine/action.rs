//! What a G-key can do.

use std::sync::Arc;
use std::time::Duration;

use super::pad::PadButton;
use crate::keycode::{KeyChord, KeyCode, MouseButton};

/// The action bound to one G-key (G1..G24, G25 stick click) in one M-bank.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Action {
    /// Key does nothing.
    #[default]
    None,
    /// Key or key combination, mirrored: pressed while the G-key is held.
    Key(KeyChord),
    /// Mouse button, mirrored: pressed while the G-key is held.
    MouseButton(MouseButton),
    /// One wheel step per G-key press (`delta` notches).
    Wheel { delta: i32, horizontal: bool },
    /// Run a macro (see [`MacroMode`] for the trigger behaviour).
    Macro(Arc<Macro>),
    /// Virtual gamepad button, mirrored: pressed while the G-key is held.
    Pad(PadButton),
    /// Up to four actions on one key: tap, hold, double tap, tap then hold.
    TapDance(Arc<TapDance>),
}

/// The four actions of a "tap dance" key. An action left at `None` falls back
/// on the tap one (a long press without a hold action does the tap action, held).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TapDance {
    /// Short press.
    pub tap: Action,
    /// Press held longer than [`TAP_TERM`].
    pub hold: Action,
    /// Two short presses.
    pub double_tap: Action,
    /// A short press, then a press held.
    pub tap_hold: Action,
}

/// How long a press may last to count as a tap, and how long the engine waits for
/// a second press.
pub const TAP_TERM: Duration = Duration::from_millis(200);

impl TapDance {
    /// Only the tap action: the key can act at once.
    pub fn tap_only(&self) -> bool {
        matches!((&self.hold, &self.double_tap, &self.tap_hold), (Action::None, Action::None, Action::None))
    }

    /// No second press to wait for after a tap.
    pub fn no_second_press(&self) -> bool {
        matches!((&self.double_tap, &self.tap_hold), (Action::None, Action::None))
    }
}

impl Action {
    pub fn key(chord: &str) -> Self {
        Self::Key(KeyChord::parse(chord).unwrap_or_else(|| panic!("invalid chord {chord:?}")))
    }
}

/// How a macro reacts to its G-key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacroMode {
    /// Play once per press. Pressing again while it runs is ignored.
    #[default]
    Once,
    /// Loop while the G-key is held; stops (and releases its keys) on release.
    RepeatWhileHeld,
    /// First press starts looping, second press stops.
    Toggle,
}

/// One step of a macro.
#[derive(Debug, Clone, PartialEq)]
pub enum MacroStep {
    KeyDown(KeyCode),
    KeyUp(KeyCode),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Wheel(i32),
    Delay(Duration),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Macro {
    pub name: String,
    pub mode: MacroMode,
    pub steps: Vec<MacroStep>,
}

impl Macro {
    /// Sum of all explicit delays in one iteration.
    pub fn total_delay(&self) -> Duration {
        self.steps
            .iter()
            .map(|s| if let MacroStep::Delay(d) = s { *d } else { Duration::ZERO })
            .sum()
    }

    /// Helper: `KeyDown`, `Delay(hold)`, `KeyUp` for each key of `chord`
    /// (modifiers wrap the main key), followed by `Delay(gap)`.
    pub fn tap_steps(chord: &KeyChord, hold: Duration, gap: Duration) -> Vec<MacroStep> {
        let mut v: Vec<MacroStep> = chord.keys().iter().map(|k| MacroStep::KeyDown(*k)).collect();
        v.push(MacroStep::Delay(hold));
        v.extend(chord.keys().iter().rev().map(|k| MacroStep::KeyUp(*k)));
        v.push(MacroStep::Delay(gap));
        v
    }
}
