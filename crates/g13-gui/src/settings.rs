//! App preferences (`studio.json` next to the `profiles` directory).
//! "Start with Windows" itself is not stored here: the autostart plugin
//! owns that state (registry `Run` key on Windows, XDG autostart on Linux).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// When launched at login, stay in the notification area (no window).
    pub start_minimized: bool,
    /// Closing the window hides it to the notification area; the G13 keeps working.
    pub close_to_tray: bool,
    /// G-key (`"G25"`…) that opens the in-game overlay; `None` = no overlay.
    pub overlay_key: Option<String>,
    /// Where the overlay panel sits: `"left"`, `"center"` or `"right"`.
    pub overlay_position: String,
    /// Interface language (`"fr"`, `"en"`, `"de"`, `"it"`, `"es"`); `None` = the system's.
    pub language: Option<String>,
    /// Keyboard drawn in the key picker (`"azerty"`, `"us"`, `"qwertz"`, `"it"`, `"es"`); `None` = the language's.
    pub keyboard_layout: Option<String>,
    /// Scheduled backup of the whole configuration: `"off"`, `"daily"`, `"weekly"` or `"monthly"`.
    pub auto_backup: String,
    /// Scheduled backups kept (the oldest are deleted).
    pub auto_backup_keep: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_minimized: true,
            close_to_tray: true,
            overlay_key: Some("G25".into()),
            overlay_position: "right".into(),
            language: None,
            keyboard_layout: None,
            auto_backup: "weekly".into(),
            auto_backup_keep: 10,
        }
    }
}

impl Settings {
    pub fn path(profiles_dir: Option<&Path>) -> Option<PathBuf> {
        profiles_dir.and_then(Path::parent).map(|d| d.join("studio.json"))
    }

    pub fn load(path: Option<&Path>) -> Self {
        path.and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap()
    }

    pub fn save(&self, path: Option<&Path>) -> Result<(), String> {
        let Some(path) = path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_files_get_the_default_overlay_key() {
        let s = Settings::parse(r#"{ "start_minimized": true, "close_to_tray": true }"#);
        assert_eq!(s.overlay_key.as_deref(), Some("G25"));
        let s = Settings::parse(r#"{ "overlay_key": null }"#);
        assert_eq!(s.overlay_key, None, "explicitly disabled stays disabled");
    }
}
