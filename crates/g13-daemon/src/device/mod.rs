//! Everything between the USB cable and the engine.
//!
//! ```text
//!  G13 ──HID──▶ [hid::HidWorker thread] ──DeviceEvent──▶ engine
//!                     ▲                                    │
//!                     └────────────DeviceCommand───────────┘
//! ```
//!
//! The worker thread exclusively owns the `HidDevice`: it reads input
//! reports, turns them into edge events via [`tracker::InputTracker`], and
//! executes outgoing commands (LEDs, backlight, LCD frames) between reads.

pub mod hid;
pub mod tracker;

use g13_core::{G13Key, G13LcdBuffer, MKeyLeds, RgbColor, StickPosition};

/// Device → engine.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceEvent {
    /// The G13 was opened (initial plug or hot-replug). The engine must
    /// re-push its LED / backlight / LCD state.
    Connected,
    /// The G13 disappeared. Release events for every held key have already
    /// been emitted before this one.
    Disconnected,
    /// A key changed state (edge-triggered, never repeated).
    Key { key: G13Key, pressed: bool },
    /// The stick moved by more than the configured hysteresis.
    Stick(StickPosition),
}

/// Engine → device.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceCommand {
    SetMKeyLeds(MKeyLeds),
    SetBacklight(RgbColor),
    DrawLcd(Box<G13LcdBuffer>),
}
