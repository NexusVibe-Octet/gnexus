//! Profile files on disk: one `<id>.toml` per profile in a directory
//! (by default `<config dir>/g13d/profiles`, from the `directories` crate:
//! `%APPDATA%\g13d\config\profiles` on Windows, `~/.config/g13d/profiles`
//! on Linux).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use directories::ProjectDirs;

use super::format::{ProfileDoc, from_toml, to_toml};
use super::rules;
use crate::engine::bindings::Profile;
use crate::engine::demo::demo_profile;
use crate::platform::ForegroundApp;

/// Id (file stem) of the generated default profile.
pub const DEMO_ID: &str = "demo";

const DEMO_HEADER: &str = "\
# Profil g13d généré automatiquement à partir du profil de démo.
# Format : voir docs/ARCHITECTURE.md (section « Profils »).
# Les touches sont des positions physiques (libellés US) : \"W\" donne Z en AZERTY.
#
# Pour l'associer à une application, ajoutez par exemple :
# [[rules]]
# exe = \"notepad.exe\"

";

/// Default profile directory, if the OS provides a config location.
pub fn default_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "g13d").map(|d| d.config_dir().join("profiles"))
}

/// Modification stamp used to detect edits.
type Stamp = (Option<SystemTime>, u64);

#[derive(Debug, Clone)]
pub struct ProfileEntry {
    pub id: String,
    /// `None` for the built-in fallback.
    pub path: Option<PathBuf>,
    pub doc: ProfileDoc,
}

#[derive(Debug)]
pub struct ProfileStore {
    dir: Option<PathBuf>,
    entries: BTreeMap<String, ProfileEntry>,
    /// Stamp of every `.toml` seen, including those that failed to parse
    /// (so a broken file is reported once per edit, not every scan).
    stamps: BTreeMap<String, Stamp>,
    default_id: String,
}

/// Write the built-in demo profile as `demo.toml` (default profile) in `dir`.
pub fn write_demo(dir: &Path) -> io::Result<()> {
    let path = dir.join(format!("{DEMO_ID}.toml"));
    let doc = ProfileDoc { profile: demo_profile(), rules: Vec::new(), is_default: true, number: None };
    let text = to_toml(&doc).map_err(|e| io::Error::other(e.0))?;
    fs::write(&path, format!("{DEMO_HEADER}{text}"))?;
    log::info!("created default profile {}", path.display());
    Ok(())
}

impl ProfileStore {
    /// Open `dir`, creating it and writing `demo.toml` if it holds no profile.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        if list_toml(&dir)?.is_empty() {
            write_demo(&dir)?;
        }
        let mut store =
            Self { dir: Some(dir), entries: BTreeMap::new(), stamps: BTreeMap::new(), default_id: String::new() };
        store.rescan();
        Ok(store)
    }

    /// In-memory store holding only the built-in demo profile (no files).
    pub fn builtin() -> Self {
        let mut store =
            Self { dir: None, entries: BTreeMap::new(), stamps: BTreeMap::new(), default_id: String::new() };
        store.update_default();
        store
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn get(&self, id: &str) -> Option<&ProfileEntry> {
        self.entries.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Profiles in display order (their `number`, then id): what G13 Studio
    /// and the LCD list show.
    pub fn ordered(&self) -> Vec<&ProfileEntry> {
        let mut v: Vec<&ProfileEntry> = self.entries.values().collect();
        v.sort_by(|a, b| a.doc.number.unwrap_or(u32::MAX).cmp(&b.doc.number.unwrap_or(u32::MAX)).then(a.id.cmp(&b.id)));
        v
    }

    /// Profile used when no rule matches: the one marked `default = true`
    /// (first by id if several), else `demo`, else the first one.
    pub fn default_id(&self) -> &str {
        &self.default_id
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.entries.get(id).map(|e| &e.doc.profile)
    }

    /// Profile for the given foreground app: first profile (by id) with a
    /// matching rule, otherwise the default one.
    pub fn select(&self, app: &ForegroundApp) -> &str {
        let candidates = self.entries.iter().map(|(id, e)| (id.as_str(), e.doc.rules.as_slice()));
        rules::select(candidates, app).unwrap_or(&self.default_id)
    }

    /// Re-read added / modified files and drop deleted ones. Returns the ids
    /// whose content changed (added, modified or removed).
    pub fn rescan(&mut self) -> Vec<String> {
        let Some(dir) = self.dir.clone() else { return Vec::new() };
        let files = match list_toml(&dir) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("cannot list {}: {e}", dir.display());
                return Vec::new();
            }
        };

        let mut changed = Vec::new();
        let present: Vec<String> = files.iter().map(|(id, _)| id.clone()).collect();
        for id in self.stamps.keys().cloned().collect::<Vec<_>>() {
            if !present.contains(&id) {
                self.stamps.remove(&id);
                if self.entries.remove(&id).is_some() {
                    log::info!("profile '{id}' removed");
                    changed.push(id);
                }
            }
        }

        for (id, path) in files {
            let stamp = stamp(&path);
            if self.stamps.get(&id) == Some(&stamp) {
                continue;
            }
            self.stamps.insert(id.clone(), stamp);
            match load(&path) {
                Ok(doc) => {
                    log::info!("profile '{id}' loaded ({})", path.display());
                    self.entries.insert(id.clone(), ProfileEntry { id: id.clone(), path: Some(path), doc });
                    changed.push(id);
                }
                // Keep the previous good version, if any.
                Err(e) => log::warn!("{}: {e}", path.display()),
            }
        }

        self.update_default();
        changed
    }

    fn update_default(&mut self) {
        if self.entries.is_empty() {
            let doc = ProfileDoc { profile: demo_profile(), rules: Vec::new(), is_default: true, number: None };
            self.entries.insert(DEMO_ID.into(), ProfileEntry { id: DEMO_ID.into(), path: None, doc });
        } else if self.entries.len() > 1 {
            // A real profile showed up: drop the built-in placeholder.
            if self.entries.get(DEMO_ID).is_some_and(|e| e.path.is_none()) {
                self.entries.remove(DEMO_ID);
            }
        }
        let marked = self.entries.values().find(|e| e.doc.is_default).map(|e| e.id.clone());
        self.default_id = marked
            .or_else(|| self.entries.contains_key(DEMO_ID).then(|| DEMO_ID.to_string()))
            .or_else(|| self.entries.keys().next().cloned())
            .unwrap_or_default();
    }
}

fn load(path: &Path) -> Result<ProfileDoc, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    from_toml(&text).map_err(|e| e.0)
}

fn stamp(path: &Path) -> Stamp {
    match fs::metadata(path) {
        Ok(m) => (m.modified().ok(), m.len()),
        Err(_) => (None, 0),
    }
}

/// `(id, path)` of every `*.toml` in `dir`, sorted by id.
fn list_toml(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut v = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file()
            && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("toml"))
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            v.push((stem.to_string(), path));
        }
    }
    v.sort();
    Ok(v)
}
