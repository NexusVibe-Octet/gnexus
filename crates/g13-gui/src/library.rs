//! Macro library (`<config>/macros.json`): every macro made in G Nexus, for
//! the G13 and the mouse, so that one made in a profile can be used in any other.
//! A profile keeps its own copy (the engine and the mouse thread read the
//! profile); the library holds the latest version of each macro by name.
//!
//! The first time, it is filled with the macros already in the G13 profiles
//! and in the mouse profiles.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use g13_daemon::profiles::FileDto;
use g13_daemon::profiles::format::MacroDto;

pub type Library = BTreeMap<String, MacroDto>;

fn path(profiles_dir: &Path) -> Option<PathBuf> {
    profiles_dir.parent().map(|p| p.join("macros.json"))
}

/// The library, built from the existing profiles if it does not exist yet.
pub fn load(profiles_dir: &Path) -> Library {
    let Some(p) = path(profiles_dir) else { return Library::new() };
    if let Ok(text) = fs::read_to_string(&p) {
        return serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
    }
    let lib = seed(profiles_dir);
    let _ = save(profiles_dir, &lib);
    lib
}

pub fn save(profiles_dir: &Path, lib: &Library) -> Result<(), String> {
    let p = path(profiles_dir).ok_or("aucun dossier de configuration")?;
    let text = serde_json::to_string_pretty(lib).map_err(|e| e.to_string())?;
    fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
}

/// Macros of the G13 profiles (`*.toml`) and of the mouse profiles (`mouse-profiles.json`).
fn seed(profiles_dir: &Path) -> Library {
    let mut lib = Library::new();
    for e in fs::read_dir(profiles_dir).into_iter().flatten().flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("toml")) {
            continue;
        }
        let Some(dto) = fs::read_to_string(&p).ok().and_then(|t| toml::from_str::<FileDto>(&t).ok()) else { continue };
        for (name, m) in dto.macros {
            lib.entry(name).or_insert(m);
        }
    }
    let mouse = profiles_dir.parent().map(|p| p.join("mouse-profiles.json"));
    if let Some(v) = mouse
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok())
    {
        for prof in v["profiles"].as_array().into_iter().flatten() {
            let Some(macros) = prof["settings"]["macros"].as_object() else { continue };
            for (name, m) in macros {
                if let Ok(m) = serde_json::from_value::<MacroDto>(m.clone()) {
                    lib.entry(name.clone()).or_insert(m);
                }
            }
        }
    }
    lib
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_from_the_profiles_then_saved() {
        let root = std::env::temp_dir().join(format!("g13-lib-{}", std::process::id()));
        let dir = root.join("profiles");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.toml"), "name = \"A\"\n[macros.spam]\nsteps = [\"down A\", \"wait 30\", \"up A\"]\n").unwrap();
        fs::write(
            root.join("mouse-profiles.json"),
            r#"{"profiles":[{"name":"","settings":{"macros":{"reload":{"mode":"once","steps":["down R","up R"]}}}}]}"#,
        )
        .unwrap();
        let lib = load(&dir);
        assert_eq!(lib.keys().collect::<Vec<_>>(), ["reload", "spam"]);
        assert!(root.join("macros.json").exists());
        // once saved, the file is what counts
        save(&dir, &Library::new()).unwrap();
        assert!(load(&dir).is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
