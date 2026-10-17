//! Logitech HID++ gaming mice (G502 X PLUS…): transport ([`Link`]), the
//! settings G13 Studio edits ([`MouseSettings`]) and the mouse thread
//! ([`MouseWorker`]), which — like the G13's HID thread — is the only one to
//! talk to the device.

pub mod host;
pub mod link;
pub mod settings;
pub mod worker;

pub use host::SoftProfile;
pub use link::{Link, Mouse, find_mice};
pub use settings::{Lighting, MouseAction, MouseSettings};
pub use worker::{Focus, MouseCommand, MouseStatus, MouseWorker, OnboardSlot};
