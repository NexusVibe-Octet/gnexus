//! Whole-configuration backup: every profile file + the app preferences in
//! one JSON file (`.g13backup`), for export / import and before a reset.
//!
//! Import and reset first save the current configuration to
//! `<config>/backups/` so that nothing is ever lost by mistake. Scheduled copies
//! (Paramètres, every day / week / month) go to `<config>/backups/auto/`.
//!
//! Besides the G13 profiles, a backup holds the other configuration files
//! ([`EXTRA_FILES`]: mouse profiles, equalizer profiles, macro library).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::Settings;

pub const FORMAT: &str = "g13studio-backup";
pub const EXTENSION: &str = "g13backup";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    /// Local time of the export, for the user.
    pub created: String,
    pub settings: Settings,
    /// "Start with Windows" (stored by the autostart plugin, not in `Settings`).
    pub autostart: bool,
    /// Profile id (file stem) → TOML text, exactly as on disk.
    pub profiles: BTreeMap<String, String>,
    /// Other configuration files ([`EXTRA_FILES`]) → their text (absent in older backups).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
}

/// Configuration files next to the `profiles` directory that a backup carries.
pub const EXTRA_FILES: [&str; 3] = ["mouse-profiles.json", "headset-eq.json", "macros.json"];

fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn toml_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("toml")))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p)))
        .collect();
    v.sort();
    v
}

/// Snapshot of the current configuration.
pub fn collect(dir: &Path, settings: &Settings, autostart: bool) -> Result<Backup, String> {
    let mut profiles = BTreeMap::new();
    for (id, path) in toml_files(dir) {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        profiles.insert(id, text);
    }
    let mut files = BTreeMap::new();
    if let Some(config) = dir.parent() {
        for name in EXTRA_FILES {
            if let Ok(text) = fs::read_to_string(config.join(name)) {
                files.insert(name.to_string(), text);
            }
        }
    }
    Ok(Backup {
        format: FORMAT.into(),
        version: 1,
        created: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        settings: settings.clone(),
        autostart,
        profiles,
        files,
    })
}

/// Write back the other configuration files of `backup` (the running mouse and
/// headset threads read them again at the next start of the app).
pub fn restore_files(dir: &Path, backup: &Backup) -> Result<(), String> {
    let config = dir.parent().ok_or("aucun dossier de configuration")?;
    for (name, text) in &backup.files {
        if EXTRA_FILES.contains(&name.as_str()) {
            let path = config.join(name);
            fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Days between two scheduled backups (`None` = off).
pub fn period_days(period: &str) -> Option<u64> {
    match period {
        "daily" => Some(1),
        "weekly" => Some(7),
        "monthly" => Some(30),
        _ => None,
    }
}

/// `<config>/backups/auto/`.
pub fn auto_dir(dir: &Path) -> Option<PathBuf> {
    dir.parent().map(|p| p.join("backups").join("auto"))
}

/// Scheduled backups, oldest first (their names carry the date).
fn auto_files(auto: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(auto)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
        .collect();
    v.sort();
    v
}

/// Date of the newest scheduled backup, for the settings page.
pub fn last_auto(dir: &Path) -> Option<String> {
    let last = auto_files(&auto_dir(dir)?).pop()?;
    let t: chrono::DateTime<chrono::Local> = fs::metadata(&last).ok()?.modified().ok()?.into();
    Some(t.format("%Y-%m-%d %H:%M").to_string())
}

/// A scheduled backup: written when the period has passed since the last one
/// (`force`: now) and the configuration changed since; only the `keep` newest stay.
/// Returns the file written.
pub fn auto_backup(dir: &Path, settings: &Settings, autostart: bool, force: bool) -> Result<Option<PathBuf>, String> {
    let auto = auto_dir(dir).ok_or("aucun dossier de configuration")?;
    let files = auto_files(&auto);
    let last = files.last();
    if !force {
        let Some(days) = period_days(&settings.auto_backup) else { return Ok(None) };
        let age = last
            .and_then(|p| fs::metadata(p).ok()?.modified().ok()?.elapsed().ok())
            .map(|d| d.as_secs());
        if age.is_some_and(|s| s < days * 86_400) {
            return Ok(None);
        }
    }
    let b = collect(dir, settings, autostart)?;
    if !force {
        // nothing changed since the last copy: keep it, no new file
        let same = last
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Backup>(&t).ok())
            .is_some_and(|o| o.profiles == b.profiles && o.files == b.files && o.settings == b.settings);
        if same {
            if let Some(p) = last {
                // restart the period from now
                let _ = fs::File::options().append(true).open(p).and_then(|f| f.set_modified(std::time::SystemTime::now()));
            }
            return Ok(None);
        }
    }
    let path = auto.join(format!("G Nexus {}.{EXTENSION}", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
    write(&path, &b)?;
    let files = auto_files(&auto);
    let keep = settings.auto_backup_keep.max(1) as usize;
    for old in files.iter().take(files.len().saturating_sub(keep)) {
        let _ = fs::remove_file(old);
    }
    Ok(Some(path))
}

pub fn write(path: &Path, backup: &Backup) -> Result<(), String> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(backup).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read and check a backup: right format, safe ids, every profile valid.
pub fn parse(text: &str) -> Result<Backup, String> {
    let b: Backup = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("ce fichier n'est pas une sauvegarde G Nexus ({e})"))?;
    if b.format != FORMAT {
        return Err("ce fichier n'est pas une sauvegarde G Nexus".into());
    }
    if b.version > 1 {
        return Err("sauvegarde créée par une version plus récente de G Nexus".into());
    }
    if b.profiles.is_empty() {
        return Err("la sauvegarde ne contient aucun profil".into());
    }
    for (id, text) in &b.profiles {
        if !safe_id(id) {
            return Err(format!("identifiant de profil invalide : {id:?}"));
        }
        g13_daemon::profiles::from_toml(text).map_err(|e| format!("profil « {id} » : {}", e.0))?;
    }
    Ok(b)
}

/// `<config>/backups/<label>-<date>.g13backup`, next to the profiles directory.
pub fn auto_path(dir: &Path, label: &str) -> Option<PathBuf> {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    dir.parent().map(|p| p.join("backups").join(format!("{label}-{stamp}.{EXTENSION}")))
}

/// Replace every profile file by those of `backup`.
pub fn restore_profiles(dir: &Path, backup: &Backup) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (id, path) in toml_files(dir) {
        if !backup.profiles.contains_key(&id) {
            fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    for (id, text) in &backup.profiles {
        let path = dir.join(format!("{id}.toml"));
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Delete every profile and write the demo profile back.
pub fn reset_profiles(dir: &Path) -> Result<(), String> {
    for (_, path) in toml_files(dir) {
        fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    g13_daemon::profiles::store::write_demo(dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_backups_skip_unchanged_and_rotate() {
        let dir = tmp("auto");
        fs::write(dir.join("a.toml"), "name = \"A\"\n").unwrap();
        fs::write(dir.parent().unwrap().join("macros.json"), "{}").unwrap();
        let s = Settings { auto_backup_keep: 2, ..Settings::default() };
        let first = auto_backup(&dir, &s, false, false).unwrap().expect("first copy");
        let b = parse(&fs::read_to_string(&first).unwrap()).unwrap();
        assert_eq!(b.files.get("macros.json").map(String::as_str), Some("{}"));
        // within the period: nothing
        assert!(auto_backup(&dir, &s, false, false).unwrap().is_none());
        // forced copies rotate: only the 2 newest stay
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(1100));
            auto_backup(&dir, &s, false, true).unwrap().unwrap();
        }
        assert_eq!(auto_files(&auto_dir(&dir).unwrap()).len(), 2);
        // off: never
        let off = Settings { auto_backup: "off".into(), ..s };
        assert!(auto_backup(&dir, &off, false, false).unwrap().is_none());
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("g13-backup-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("profiles")).unwrap();
        d.join("profiles")
    }

    #[test]
    fn round_trip_restore_and_reset() {
        let dir = tmp("rt");
        fs::write(dir.join("a.toml"), "name = \"A\"\n").unwrap();
        fs::write(dir.join("b.toml"), "name = \"B\"\n").unwrap();
        let b = collect(&dir, &Settings::default(), true).unwrap();
        let text = serde_json::to_string(&b).unwrap();
        let back = parse(&text).unwrap();
        assert_eq!(back.profiles.len(), 2);

        // restore replaces: c is removed, a and b come back
        fs::remove_file(dir.join("a.toml")).unwrap();
        fs::write(dir.join("c.toml"), "name = \"C\"\n").unwrap();
        restore_profiles(&dir, &back).unwrap();
        let ids: Vec<String> = toml_files(&dir).into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids, ["a", "b"]);

        reset_profiles(&dir).unwrap();
        let ids: Vec<String> = toml_files(&dir).into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids, ["demo"]);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn rejects_bad_files() {
        assert!(parse("{}").is_err());
        let mut b = Backup {
            format: FORMAT.into(),
            version: 1,
            created: String::new(),
            settings: Settings::default(),
            autostart: false,
            profiles: BTreeMap::from([("../evil".into(), "name = \"x\"\n".into())]),
            files: BTreeMap::new(),
        };
        assert!(parse(&serde_json::to_string(&b).unwrap()).is_err(), "path traversal");
        b.profiles = BTreeMap::from([("ok".into(), "name = \"x\"\n[banks.M1]\nG1 = \"NotAKey\"\n".into())]);
        assert!(parse(&serde_json::to_string(&b).unwrap()).is_err(), "invalid profile");
    }
}
