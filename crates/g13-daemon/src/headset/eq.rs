//! G13 Studio's own equalizer for the headset's sound (the headset's built-in one
//! does not change the sound, and G HUB's is driven through a private channel).
//!
//! - Windows: the `g13-apo` audio effect, installed once on the headset's output
//!   (administrator rights), reads the settings file written here and applies it live.
//! - Linux: the same settings drive a PipeWire filter chain in front of the headset's
//!   output (`platform::linux::eq`), installed once without root and updated live.

use std::path::PathBuf;

use g13_dsp::{EqSettings, MAX_DB, MIN_DB};

use super::Equalizer;

/// Where the settings are kept: the file the Windows audio effect reads.
#[cfg(windows)]
fn settings_path() -> PathBuf {
    g13_apo::settings_path()
}

/// Where the settings are kept: next to the profiles (`~/.config/g13d/audio-eq.bin`).
#[cfg(not(windows))]
fn settings_path() -> PathBuf {
    crate::profiles::store::default_dir()
        .and_then(|p| p.parent().map(|p| p.join("audio-eq.bin")))
        .unwrap_or_else(|| PathBuf::from("audio-eq.bin"))
}

/// Current settings (the default flat equalizer if none were saved).
pub fn load() -> EqSettings {
    std::fs::read(settings_path()).ok().and_then(|b| EqSettings::from_bytes(&b)).unwrap_or_default()
}

/// Save new gains (one per band, dB); the effect picks them up within a fraction of a second.
pub fn save_gains(gains: &[i8]) -> Result<EqSettings, String> {
    let mut s = load();
    if gains.len() != s.bands.len() {
        return Err(format!("{} bandes attendues", s.bands.len()));
    }
    for (b, &g) in s.bands.iter_mut().zip(gains) {
        b.1 = f32::from(g).clamp(MIN_DB, MAX_DB);
    }
    let path = settings_path();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(&path, s.to_bytes()).map_err(|e| format!("{} : {e} (installez l'égaliseur)", path.display()))?;
    #[cfg(target_os = "linux")]
    if let Err(e) = crate::platform::linux::eq::apply(&s) {
        log::warn!("equalizer: {e}");
    }
    Ok(s)
}

/// Install the equalizer in front of the output named `name` (Linux: no rights needed;
/// Windows goes through the elevated setup helper of the GUI instead).
#[cfg(target_os = "linux")]
pub fn install(name: &str) -> Result<(), String> {
    let target = crate::platform::linux::audio::node_name(name)
        .map_err(|e| e.detail())?
        .ok_or_else(|| format!("sortie audio « {name} » introuvable"))?;
    crate::platform::linux::eq::install(&load(), &target).map_err(|e| e.detail())
}

/// Remove the equalizer (Linux).
#[cfg(target_os = "linux")]
pub fn uninstall() -> Result<(), String> {
    crate::platform::linux::eq::uninstall().map_err(|e| e.detail())
}

/// The settings as the GUI shows them.
pub fn view(s: &EqSettings) -> Equalizer {
    Equalizer {
        freqs: s.bands.iter().map(|&(f, _)| f.round() as u16).collect(),
        gains: s.bands.iter().map(|&(_, g)| g.round() as i8).collect(),
        min_db: MIN_DB as i8,
        max_db: MAX_DB as i8,
    }
}

/// Is the equalizer effect installed on the output named `name`?
pub fn installed(name: &str) -> bool {
    #[cfg(windows)]
    {
        crate::platform::windows::audio::endpoint_key(name)
            .ok()
            .flatten()
            .is_some_and(|k| g13_apo::setup::installed(&k))
    }
    #[cfg(target_os = "linux")]
    {
        let _ = name;
        crate::platform::linux::eq::installed()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = name;
        false
    }
}

/// A G13 Studio equalizer profile: the default one serves every application, the
/// others the applications they are bound to (in the foreground).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EqProfile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub apps: Vec<String>,
    /// One gain per band (dB).
    pub gains: Vec<i8>,
}

/// `<config>/headset-eq.json`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EqProfiles {
    #[serde(default)]
    pub default: usize,
    #[serde(default)]
    pub profiles: Vec<EqProfile>,
}

/// At most this many equalizer profiles.
pub const MAX_PROFILES: usize = 8;

impl EqProfiles {
    fn path() -> Option<std::path::PathBuf> {
        crate::profiles::store::default_dir().and_then(|p| p.parent().map(|p| p.join("headset-eq.json")))
    }

    /// The profiles; the first time, one profile with the equalizer as it is now.
    pub fn load() -> Self {
        let mut p: Self = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default();
        if p.profiles.is_empty() {
            let now = view(&load());
            p.profiles.push(EqProfile { name: String::new(), apps: Vec::new(), gains: now.gains });
            p.default = 0;
        }
        if p.default >= p.profiles.len() {
            p.default = 0;
        }
        p
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("aucun dossier de configuration")?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Profile to use: the one tested everywhere, else the one bound to `exe`, else the default.
    pub fn choose(&self, exe: Option<&str>, pinned: Option<usize>) -> usize {
        if let Some(p) = pinned.filter(|&p| p < self.profiles.len()) {
            return p;
        }
        exe.and_then(|exe| self.profiles.iter().position(|p| p.apps.iter().any(|a| a.eq_ignore_ascii_case(exe))))
            .unwrap_or(self.default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_choice() {
        let p = |apps: &[&str]| EqProfile { name: String::new(), apps: apps.iter().map(|a| a.to_string()).collect(), gains: vec![0; 10] };
        let eq = EqProfiles { default: 1, profiles: vec![p(&["game.exe"]), p(&[]), p(&["vlc.exe"])] };
        assert_eq!(eq.choose(Some("GAME.exe"), None), 0);
        assert_eq!(eq.choose(Some("other.exe"), None), 1, "the default one");
        assert_eq!(eq.choose(None, None), 1);
        assert_eq!(eq.choose(Some("vlc.exe"), Some(0)), 0, "tested everywhere");
        assert_eq!(eq.choose(Some("vlc.exe"), Some(9)), 2, "out of range: ignored");
    }
}
