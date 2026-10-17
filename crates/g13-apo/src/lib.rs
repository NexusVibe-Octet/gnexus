//! G13 Studio's audio effect for Windows: an APO (audio processing object)
//! loaded by the audio engine (`audiodg.exe`) on the headset's output, which
//! runs the equalizer of `g13-dsp` on the sound.
//!
//! - Registered by G13 Studio's setup helper (administrator, once) as the
//!   output's *endpoint effect* (EFX): the effect already there (Logitech's) is
//!   left in place.
//! - Settings: [`settings_path`] (`%ProgramData%\G13 Studio\audio-eq.bin`, written by
//!   G13 Studio), re-read by a small thread whenever the file changes. The audio
//!   thread picks them up without waiting (`try_lock`), and never allocates.
//! - Only 32-bit float audio is processed (the format of system effects); the
//!   samples are changed in place.

#![allow(non_snake_case)]

use std::path::PathBuf;

/// CLSID of the effect (registered by the setup helper).
pub const CLSID_TEXT: &str = "{5F2C9E61-7B1D-4C3A-9E8B-13C0A7D4E2F1}";
pub const FRIENDLY_NAME: &str = "G13 Studio Equalizer";

/// Where G13 Studio writes the equalizer settings (`g13_dsp::EqSettings::to_bytes`).
/// `G13_APO_SETTINGS` overrides it (tests).
pub fn settings_path() -> PathBuf {
    if let Some(p) = std::env::var_os("G13_APO_SETTINGS") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
    base.join("G13 Studio").join("audio-eq.bin")
}

/// What the effect did (loaded, format, settings), next to the settings: tells whether
/// Windows loaded it. Never written from the audio thread; kept small.
pub fn log(msg: &str) {
    use std::io::Write;
    let path = settings_path().with_file_name("apo.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "{t} [{}] {msg}", std::process::id());
    }
}

#[cfg(windows)]
mod apo;
#[cfg(windows)]
pub mod setup;

#[cfg(windows)]
pub use apo::{CLSID, create_instance};
