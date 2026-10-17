//! Profile files as seen by the GUI: list / load / save / create / delete.
//! The JSON model is `g13_daemon::profiles::FileDto` (same structure as the
//! TOML file); every save is validated with the daemon's own `from_dto`.
//! The engine picks changes up through its hot reload.

use std::fs;
use std::path::{Path, PathBuf};

use g13_core::G13Key;
use g13_daemon::profiles::{FileDto, dto_to_toml, from_dto};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProfileSummary {
    pub id: String,
    /// Stable position in the list (the number shown on the profile button).
    pub number: u32,
    pub name: String,
    pub default: bool,
    /// Executable names / titles the profile is bound to.
    pub apps: Vec<String>,
    /// Set when the file cannot be read or is invalid.
    pub error: Option<String>,
}

/// Ids are file stems; only allow safe characters (no path traversal).
fn file(dir: &Path, id: &str) -> Result<PathBuf, String> {
    let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok { Ok(dir.join(format!("{id}.toml"))) } else { Err(format!("identifiant de profil invalide : {id:?}")) }
}

fn ids(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("toml")))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    v.sort();
    v
}

/// All profiles, ordered by number. Valid profiles without a number get the
/// next free ones (written to their file once), so the list order is stable.
pub fn list(dir: &Path) -> Vec<ProfileSummary> {
    let loaded: Vec<(String, Result<FileDto, String>)> = ids(dir)
        .into_iter()
        .map(|id| {
            let r = load(dir, &id).and_then(|dto| from_dto(dto.clone()).map(|_| dto).map_err(|e| e.0));
            (id, r)
        })
        .collect();
    let mut next = loaded.iter().filter_map(|(_, r)| r.as_ref().ok()?.number).max().unwrap_or(0) + 1;
    let mut out: Vec<ProfileSummary> = loaded
        .into_iter()
        .map(|(id, r)| match r {
            Ok(mut dto) => {
                if dto.number.is_none() {
                    dto.number = Some(next);
                    next += 1;
                    if let Err(e) = write(dir, &id, &dto) {
                        log::warn!("cannot number profile '{id}': {e}");
                    }
                }
                ProfileSummary {
                    number: dto.number.unwrap_or(u32::MAX),
                    name: dto.name,
                    default: dto.default,
                    apps: dto.rules.iter().filter_map(|r| r.exe.clone().or_else(|| r.title.clone())).collect(),
                    error: None,
                    id,
                }
            }
            Err(e) => ProfileSummary { number: u32::MAX, name: id.clone(), id, default: false, apps: vec![], error: Some(e) },
        })
        .collect();
    out.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.id.cmp(&b.id)));
    out
}

pub fn load(dir: &Path, id: &str) -> Result<FileDto, String> {
    let text = fs::read_to_string(file(dir, id)?).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

fn write(dir: &Path, id: &str, dto: &FileDto) -> Result<(), String> {
    let text = dto_to_toml(dto).map_err(|e| e.0)?;
    fs::write(file(dir, id)?, text).map_err(|e| e.to_string())
}

/// Validate and write. Marking a profile as default clears the flag elsewhere.
pub fn save(dir: &Path, id: &str, dto: &FileDto) -> Result<(), String> {
    from_dto(dto.clone()).map_err(|e| e.0)?;
    // Keys assigned in any order in the UI are written G1 → G25.
    let mut dto = dto.clone();
    for bank in [&mut dto.banks.m1, &mut dto.banks.m2, &mut dto.banks.m3] {
        let order = |k: &String| G13Key::from_id(k).map_or(u8::MAX, G13Key::bit_index);
        bank.keys.sort_by(|a, _, b, _| order(a).cmp(&order(b)));
    }
    let dto = &dto;
    if dto.default {
        for other in ids(dir).into_iter().filter(|o| o != id) {
            if let Ok(mut o) = load(dir, &other)
                && o.default
            {
                o.default = false;
                write(dir, &other, &o)?;
            }
        }
    }
    write(dir, id, dto)
}

/// New profile named `name`, empty or copied from `from` (without its
/// application rules, so two profiles don't claim the same app). Returns its id.
pub fn create(dir: &Path, name: &str, from: Option<&str>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("le nom ne peut pas être vide".into());
    }
    let mut dto = match from {
        Some(src) => load(dir, src)?,
        None => FileDto::default(),
    };
    dto.name = name.to_string();
    dto.rules.clear();
    create_from(dir, dto)
}

/// Write `dto` as a new profile (next number, never the default, unique id
/// derived from its name). Returns the id.
pub fn create_from(dir: &Path, mut dto: FileDto) -> Result<String, String> {
    from_dto(dto.clone()).map_err(|e| e.0)?;
    dto.default = false;
    dto.number = Some(list(dir).iter().filter(|p| p.number != u32::MAX).map(|p| p.number).max().unwrap_or(0) + 1);

    let base: String = dto
        .name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            _ => '-',
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if base.is_empty() { "profil".to_string() } else { base };
    let existing = ids(dir);
    let id = (1..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
        .find(|c| !existing.contains(c))
        .expect("unbounded range");
    write(dir, &id, &dto)?;
    Ok(id)
}

/// Make profile `id` the one for executable `exe`: add the rule to it and
/// remove the same exe from every other profile (one game, one profile).
pub fn assign_app(dir: &Path, id: &str, exe: &str) -> Result<(), String> {
    let exe = exe.trim();
    if exe.is_empty() {
        return Err("aucune application".into());
    }
    let same = |r: &g13_daemon::profiles::MatchRule| r.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(exe));
    for other in ids(dir).into_iter().filter(|o| o != id) {
        if let Ok(mut dto) = load(dir, &other)
            && dto.rules.iter().any(same)
        {
            dto.rules.retain(|r| !same(r));
            write(dir, &other, &dto)?;
        }
    }
    let mut dto = load(dir, id)?;
    if !dto.rules.iter().any(same) {
        dto.rules.push(g13_daemon::profiles::MatchRule::exe(exe));
    }
    save(dir, id, &dto)
}

pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    if ids(dir).len() <= 1 {
        return Err("impossible de supprimer le dernier profil".into());
    }
    fs::remove_file(file(dir, id)?).map_err(|e| e.to_string())
}
