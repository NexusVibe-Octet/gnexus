//! # g13-daemon
//!
//! Background service for the Logitech G13. See `docs/ARCHITECTURE.md`.
//!
//! ```text
//! device   ── HID thread, hot-plug, report → edge events, LEDs/RGB/LCD output
//! engine   ── M-banks, bindings, macros, dispatch to the injector
//! platform ── OS traits (InputInjector / ForegroundWatcher / MediaProvider)
//!             windows/ (implemented) · linux/ (prepared stubs) · mock
//! keycode  ── platform-neutral keys/buttons + Win32 scan code & evdev tables
//! profiles ── TOML profiles, exe → profile rules, auto-switch, hot reload
//! runtime  ── HID + engine threads packaged for g13d and the GUI
//! ```

pub mod device;
pub mod engine;
pub mod headset;
pub mod keycode;
pub mod lcd;
pub mod mouse;
pub mod platform;
pub mod profiles;
pub mod recording;
pub mod runtime;
