//! Import / export of Logitech Gaming Software (LGS 9.x) XML profiles.
//!
//! What an LGS profile file looks like (namespace `…/Cassandra/2010.7/Profile`):
//!
//! ```xml
//! <profiles><profile name="WARDOGS" …>
//!   <target path="C:\GAMES\WARDOGS.EXE"/>
//!   <macros>
//!     <macro guid="{…}" name="Z"><keystroke …><key value="W"/></keystroke></macro>
//!     <macro guid="{…}" name="Clic gauche"><mousefunction …><do task="leftclick"/></mousefunction></macro>
//!   </macros>
//!   <assignments devicecategory="Logitech.Gaming.LeftHandedController">   <!-- the G13 -->
//!     <assignment contextid="G4" macroguid="{…}" shiftstate="1" original="false" backup="false"/>
//!   </assignments>
//!   <backlight devicemodel="Logitech.Gaming.LeftHandedController.G13">
//!     <mode color="#ffffff" shiftstate="1"/>
//!   </backlight>
//! </profile></profiles>
//! ```
//!
//! * `shiftstate` 1/2/3 = M1/M2/M3. `contextid` G1–G25 are ours as is (G25 =
//!   stick click); G26/G27/G28/G29 are the stick up/right/down/left buttons.
//! * For one key and one M-state, LGS keeps the previous binding with
//!   `backup="true"`: the active one is `backup="false"`.
//! * Key names are **physical positions** with US labels (`W` is AZERTY Z),
//!   exactly like our [`KeyCode`]s — no layout translation is needed.
//! * Supported macro kinds: `keystroke` (one key or a chord), `mousefunction`
//!   (clicks, back/forward), `hotkeys` copy/paste/cut, `function` media keys.
//!   Others (multi-key sequences, text blocks, Lua scripts, layout-dependent
//!   hotkeys such as undo) are reported, never guessed.

use std::collections::HashMap;
use std::fmt::Write as _;

use indexmap::IndexMap;

use super::ProfileError;
use super::format::{ActionDto, BankDto, FileDto};
use super::rules::MatchRule;
use crate::engine::bindings::MBank;
use crate::keycode::{KeyChord, KeyCode, MouseButton};

const G13_CATEGORY: &str = "Logitech.Gaming.LeftHandedController";
const G13_MODEL: &str = "Logitech.Gaming.LeftHandedController.G13";
const LCORE_PATH: &str = r"C:\PROGRAM FILES\LOGITECH GAMING SOFTWARE\LCORE.EXE";

/// LGS context id ↔ our binding id.
const CONTEXTS: &[(&str, &str)] = &[
    ("G26", "STICK_UP"),
    ("G27", "STICK_RIGHT"),
    ("G28", "STICK_DOWN"),
    ("G29", "STICK_LEFT"),
];

/// LGS key name ↔ our `KeyCode` name, for names that differ. Letters, digits
/// and F1–F24 are identical. The first entry for a key is used on export;
/// the others are accepted on import (names compared without `_`/case).
const KEY_NAMES: &[(&str, &str)] = &[
    ("ESCAPE", "Escape"),
    ("TILDE", "Grave"),
    ("MINUS", "Minus"),
    ("EQUAL", "Equal"),
    ("EQUALS", "Equal"),
    ("BACKSPACE", "Backspace"),
    ("TAB", "Tab"),
    ("LBRACKET", "LeftBracket"),
    ("OPEN_BRACKET", "LeftBracket"),
    ("RBRACKET", "RightBracket"),
    ("CLOSE_BRACKET", "RightBracket"),
    ("BACKSLASH", "Backslash"),
    ("CAPSLOCK", "CapsLock"),
    ("SEMICOLON", "Semicolon"),
    ("QUOTE", "Apostrophe"),
    ("APOSTROPHE", "Apostrophe"),
    ("ENTER", "Enter"),
    ("RETURN", "Enter"),
    ("LSHIFT", "LeftShift"),
    ("RSHIFT", "RightShift"),
    ("COMMA", "Comma"),
    ("PERIOD", "Period"),
    ("SLASH", "Slash"),
    ("NON_US_SLASH", "IntlBackslash"),
    ("LCTRL", "LeftCtrl"),
    ("RCTRL", "RightCtrl"),
    ("LALT", "LeftAlt"),
    ("RALT", "RightAlt"),
    ("LGUI", "LeftMeta"),
    ("LWIN", "LeftMeta"),
    ("RGUI", "RightMeta"),
    ("RWIN", "RightMeta"),
    ("APPKEY", "Menu"),
    ("APPLICATION", "Menu"),
    ("SPACEBAR", "Space"),
    ("SPACE", "Space"),
    ("PRINTSCREEN", "PrintScreen"),
    ("SCROLLLOCK", "ScrollLock"),
    ("PAUSE", "Pause"),
    ("PAUSE_BREAK", "Pause"),
    ("INSERT", "Insert"),
    ("HOME", "Home"),
    ("PAGEUP", "PageUp"),
    ("DELETE", "Delete"),
    ("END", "End"),
    ("PAGEDOWN", "PageDown"),
    ("UP", "Up"),
    ("ARROW_UP", "Up"),
    ("DOWN", "Down"),
    ("ARROW_DOWN", "Down"),
    ("LEFT", "Left"),
    ("ARROW_LEFT", "Left"),
    ("RIGHT", "Right"),
    ("ARROW_RIGHT", "Right"),
    ("NUMLOCK", "NumLock"),
    ("NUMSLASH", "NumpadDivide"),
    ("NUMASTERISK", "NumpadMultiply"),
    ("NUMMULTIPLY", "NumpadMultiply"),
    ("NUMMINUS", "NumpadSubtract"),
    ("NUMPLUS", "NumpadAdd"),
    ("NUMENTER", "NumpadEnter"),
    ("NUMPERIOD", "NumpadDecimal"),
    ("NUMDECIMAL", "NumpadDecimal"),
    ("NUM0", "Numpad0"),
    ("NUM1", "Numpad1"),
    ("NUM2", "Numpad2"),
    ("NUM3", "Numpad3"),
    ("NUM4", "Numpad4"),
    ("NUM5", "Numpad5"),
    ("NUM6", "Numpad6"),
    ("NUM7", "Numpad7"),
    ("NUM8", "Numpad8"),
    ("NUM9", "Numpad9"),
];

/// LGS `mousefunction` task ↔ our mouse button.
const MOUSE_TASKS: &[(&str, MouseButton)] = &[
    ("leftclick", MouseButton::Left),
    ("rightclick", MouseButton::Right),
    ("middleclick", MouseButton::Middle),
    ("back", MouseButton::Back),
    ("forward", MouseButton::Forward),
];

/// LGS `hotkeys` tasks that do not depend on the keyboard layout.
const HOTKEYS: &[(&str, &str)] = &[("copy", "LeftCtrl+C"), ("paste", "LeftCtrl+V"), ("cut", "LeftCtrl+X")];

/// LGS `function` tasks (media) ↔ our key names. Matched by substring.
const MEDIA_TASKS: &[(&str, &str)] = &[
    ("playpause", "MediaPlayPause"),
    ("play", "MediaPlayPause"),
    ("next", "MediaNext"),
    ("previous", "MediaPrevious"),
    ("prev", "MediaPrevious"),
    ("stop", "MediaStop"),
    ("mute", "VolumeMute"),
    ("volumeup", "VolumeUp"),
    ("volup", "VolumeUp"),
    ("volumedown", "VolumeDown"),
    ("voldown", "VolumeDown"),
];

fn norm(s: &str) -> String {
    s.chars().filter(|c| *c != '_' && *c != ' ').collect::<String>().to_ascii_uppercase()
}

fn lgs_to_key(name: &str) -> Option<KeyCode> {
    let n = norm(name);
    KEY_NAMES
        .iter()
        .find(|(lgs, _)| norm(lgs) == n)
        .and_then(|(_, ours)| KeyCode::parse(ours))
        .or_else(|| {
            // letters, digits, F1–F24
            let k = KeyCode::parse(&n)?;
            (n.len() == 1 || (n.starts_with('F') && n[1..].parse::<u8>().is_ok())).then_some(k)
        })
}

fn key_to_lgs(key: KeyCode) -> Option<String> {
    if let Some((lgs, _)) = KEY_NAMES.iter().find(|(_, ours)| KeyCode::parse(ours) == Some(key)) {
        return Some((*lgs).to_string());
    }
    let n = key.name();
    let simple = n.len() == 1 || (n.starts_with('F') && n[1..].parse::<u8>().is_ok());
    simple.then(|| n.to_ascii_uppercase())
}

/// First child element named `name` (namespaces ignored).
fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.tag_name().name() == name)
}

/// Result of an import: the new profile plus what could not be converted.
#[derive(Debug, Clone)]
pub struct LgsImport {
    pub dto: FileDto,
    pub skipped: Vec<String>,
}

/// Convert an LGS profile (XML text) into our file model.
pub fn import(xml: &str) -> Result<LgsImport, ProfileError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| ProfileError(format!("XML invalide : {e}")))?;
    let profile = doc
        .descendants()
        .find(|n| n.has_tag_name("profile") || n.tag_name().name() == "profile")
        .ok_or_else(|| ProfileError("aucun <profile> dans ce fichier".into()))?;
    let mut skipped = Vec::new();
    let mut dto = FileDto { name: profile.attribute("name").unwrap_or("Profil LGS").trim().to_string(), ..FileDto::default() };

    // executable → application rule (LGS points profiles without a game at LCore itself)
    if let Some(path) = child(profile, "target").and_then(|t| t.attribute("path")) {
        let exe = path.rsplit(['\\', '/']).next().unwrap_or("").trim();
        if !exe.is_empty() && !exe.eq_ignore_ascii_case("lcore.exe") {
            dto.rules.push(MatchRule::exe(exe));
        }
    }

    // macros: guid → (name, converted action or reason)
    let mut macros: HashMap<String, (String, Result<ActionDto, String>)> = HashMap::new();
    if let Some(list) = child(profile, "macros") {
        for m in list.children().filter(|c| c.tag_name().name() == "macro") {
            let (Some(guid), name) = (m.attribute("guid"), m.attribute("name").unwrap_or("?")) else { continue };
            let body = m.children().find(|c| c.is_element());
            macros.insert(guid.to_uppercase(), (name.to_string(), convert_macro(body)));
        }
    }

    // G13 assignments: keep the active one (backup="false"), custom before default
    let mut chosen: IndexMap<(MBank, String), (bool, String)> = IndexMap::new();
    for group in profile
        .children()
        .filter(|c| c.tag_name().name() == "assignments" && c.attribute("devicecategory") == Some(G13_CATEGORY))
    {
        for a in group.children().filter(|c| c.tag_name().name() == "assignment") {
            if a.attribute("backup") == Some("true") {
                continue;
            }
            let (Some(ctx), Some(guid)) = (a.attribute("contextid"), a.attribute("macroguid")) else { continue };
            let bank = match a.attribute("shiftstate") {
                Some("1") => MBank::M1,
                Some("2") => MBank::M2,
                Some("3") => MBank::M3,
                _ => continue,
            };
            let custom = a.attribute("original") != Some("true");
            let id = CONTEXTS.iter().find(|(c, _)| *c == ctx).map_or(ctx.to_string(), |(_, ours)| (*ours).to_string());
            let key = (bank, id);
            if chosen.get(&key).is_none_or(|(prev_custom, _)| custom || !prev_custom) {
                chosen.insert(key, (custom, guid.to_uppercase()));
            }
        }
    }
    for ((bank, id), (_, guid)) in chosen {
        match macros.get(&guid) {
            Some((_, Ok(action))) => {
                dto.banks.get_mut(bank).keys.insert(id, action.clone());
            }
            Some((name, Err(why))) => skipped.push(format!("{bank:?} {id} « {name} » : {why}")),
            None => skipped.push(format!("{bank:?} {id} : macro introuvable dans le fichier")),
        }
    }

    // backlight colour per M-state
    if let Some(bl) = profile
        .children()
        .find(|c| c.tag_name().name() == "backlight" && c.attribute("devicemodel") == Some(G13_MODEL))
    {
        for m in bl.children().filter(|c| c.tag_name().name() == "mode") {
            let bank = match m.attribute("shiftstate") {
                Some("1") => MBank::M1,
                Some("2") => MBank::M2,
                Some("3") => MBank::M3,
                _ => continue,
            };
            if let Some(c) = m.attribute("color").filter(|c| c.len() == 7 && c.starts_with('#')) {
                dto.banks.get_mut(bank).color = Some(c.to_uppercase());
            }
        }
    }

    // banks in G-key order, like files written by the app
    for bank in MBank::ALL {
        sort_keys(dto.banks.get_mut(bank));
    }
    Ok(LgsImport { dto, skipped })
}

fn sort_keys(b: &mut BankDto) {
    let order = |id: &str| {
        let ids = ["STICK_UP", "STICK_DOWN", "STICK_LEFT", "STICK_RIGHT"];
        id.strip_prefix('G')
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or_else(|| 100 + ids.iter().position(|s| *s == id).unwrap_or(9) as u32)
    };
    b.keys.sort_by(|a, _, c, _| order(a).cmp(&order(c)));
}

fn convert_macro(body: Option<roxmltree::Node<'_, '_>>) -> Result<ActionDto, String> {
    let Some(body) = body else { return Err("macro vide".into()) };
    let task = || body.children().find(|c| c.tag_name().name() == "do").and_then(|d| d.attribute("task")).unwrap_or("");
    match body.tag_name().name() {
        "keystroke" => {
            let mut keys = Vec::new();
            for k in body.children().filter(|c| c.tag_name().name() == "key") {
                let v = k.attribute("value").unwrap_or("");
                keys.push(lgs_to_key(v).ok_or_else(|| format!("touche LGS « {v} » inconnue"))?);
            }
            if keys.is_empty() {
                return Err("frappe sans touche".into());
            }
            Ok(ActionDto::Chord(KeyChord::new(keys).to_string()))
        }
        "mousefunction" => {
            let t = task();
            MOUSE_TASKS
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(t))
                .map(|(_, b)| ActionDto::Mouse { mouse: b.name().into() })
                .ok_or_else(|| format!("fonction souris « {t} » non prise en charge"))
        }
        "hotkeys" => {
            let t = task();
            HOTKEYS
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(t))
                .map(|(_, chord)| ActionDto::Chord((*chord).into()))
                .ok_or_else(|| format!("raccourci « {t} » non importé (dépend de la disposition du clavier)"))
        }
        "function" => {
            let t = task().to_ascii_lowercase().replace(['_', ' '], "");
            MEDIA_TASKS
                .iter()
                .find(|(n, _)| t.contains(n))
                .map(|(_, k)| ActionDto::Chord((*k).into()))
                .ok_or_else(|| format!("fonction « {} » non prise en charge", task()))
        }
        "natural" => Err("fonction d'origine de la touche (sans équivalent)".into()),
        other => Err(format!("macro de type « {other} » non prise en charge (séquence, texte ou script)")),
    }
}

// ---------------------------------------------------------------------------
// export
// ---------------------------------------------------------------------------

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Random-looking GUID `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` (no OS call:
/// std's per-process random hasher keys, mixed with a counter).
fn guid(counter: &mut u64) -> String {
    use std::hash::{BuildHasher, Hasher};
    let st = std::collections::hash_map::RandomState::new();
    let n = *counter;
    *counter += 1;
    let part = |salt: u64| {
        let mut h = st.build_hasher();
        h.write_u64(n);
        h.write_u64(salt);
        h.finish()
    };
    let (a, b) = (part(1), part(2));
    format!(
        "{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0x0FFF,
        0x8000 | (b >> 48 & 0x3FFF),
        b & 0xFFFF_FFFF_FFFF
    )
}

/// Body of an LGS `<macro>` for one of our actions, or why it can't be exported.
fn export_action(a: &ActionDto) -> Result<(String, String), String> {
    match a {
        ActionDto::Chord(s) => {
            let chord = KeyChord::parse(s).ok_or_else(|| format!("combinaison invalide « {s} »"))?;
            let names = chord
                .keys()
                .iter()
                .map(|k| key_to_lgs(*k).ok_or_else(|| format!("la touche « {k} » n'a pas d'équivalent LGS connu")))
                .collect::<Result<Vec<_>, _>>()?;
            let mut body = String::from(
                "        <keystroke xmlns=\"http://www.logitech.com/Cassandra/2010.1/Macros/Keystroke\">\n",
            );
            for n in &names {
                let _ = writeln!(body, "          <key value=\"{n}\"/>");
            }
            body.push_str("        </keystroke>\n");
            Ok((chord.to_string(), body))
        }
        ActionDto::Mouse { mouse } => {
            let b = MouseButton::parse(mouse).ok_or_else(|| format!("bouton « {mouse} » inconnu"))?;
            let (task, _) = MOUSE_TASKS.iter().find(|(_, mb)| *mb == b).expect("all buttons are listed");
            Ok((
                mouse.clone(),
                format!(
                    "        <mousefunction xmlns=\"http://www.logitech.com/Cassandra/2010.1/Macros/MouseFunction\">\n          <do task=\"{task}\"/>\n        </mousefunction>\n"
                ),
            ))
        }
        ActionDto::Wheel { .. } => Err("la molette n'a pas d'équivalent LGS connu".into()),
        ActionDto::Macro { name } => Err(format!("la macro « {name} » (séquence) n'est pas exportable vers LGS")),
        ActionDto::Pad { pad } => Err(format!("le bouton de manette « {pad} » n'est pas exportable vers LGS")),
        // LGS has one action per key: the tap one
        ActionDto::TapDance { tap: Some(tap), .. } => export_action(tap),
        ActionDto::TapDance { .. } => Err("touche multi-actions sans appui court : pas d'équivalent LGS".into()),
    }
}

/// Convert our profile to an LGS XML document. Returns the XML and the
/// bindings that could not be exported.
pub fn export(dto: &FileDto) -> (String, Vec<String>) {
    let mut counter = 0u64;
    let mut skipped = Vec::new();
    let mut macro_xml = String::new();
    let mut by_content: HashMap<String, String> = HashMap::new(); // body → guid
    let mut assigns = String::new();

    for bank in MBank::ALL {
        let shift = bank.index() + 1;
        for (id, action) in &dto.banks.get(bank).keys {
            let ctx = CONTEXTS.iter().find(|(_, ours)| ours == id).map_or(id.as_str(), |(c, _)| c);
            match export_action(action) {
                Ok((name, body)) => {
                    let g = match by_content.get(&body) {
                        Some(g) => g.clone(),
                        None => {
                            let g = guid(&mut counter);
                            let _ = write!(
                                macro_xml,
                                "      <macro guid=\"{g}\" color=\"4278246655\" name=\"{}\" hidden=\"false\">\n{body}      </macro>\n",
                                xml_escape(&name)
                            );
                            by_content.insert(body, g.clone());
                            g
                        }
                    };
                    let _ = writeln!(
                        assigns,
                        "      <assignment contextid=\"{ctx}\" macroguid=\"{g}\" shiftstate=\"{shift}\" original=\"false\" backup=\"false\"/>"
                    );
                }
                Err(why) => skipped.push(format!("{bank:?} {id} : {why}")),
            }
        }
    }

    let target = dto.rules.iter().find_map(|r| r.exe.clone()).unwrap_or_else(|| LCORE_PATH.to_string());
    let name = xml_escape(&dto.name);
    let mut out = String::new();
    let _ = write!(
        out,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<profiles xmlns=\"http://www.logitech.com/Cassandra/2010.7/Profile\">\n  <profile guid=\"{}\" lock=\"0\" name=\"{name}\" gkeysdk=\"0\" launchable=\"1\" gameid=\"\" gpasupported=\"0\">\n    <description>{name}</description>\n    <target path=\"{}\"/>\n    <signature value=\"\" key=\"\" name=\"\" executable=\"\"/>\n    <macros>\n{macro_xml}    </macros>\n    <assignments devicecategory=\"{G13_CATEGORY}\">\n{assigns}    </assignments>\n",
        guid(&mut counter),
        xml_escape(&target)
    );
    let colors: Vec<(usize, &String)> =
        MBank::ALL.iter().filter_map(|b| dto.banks.get(*b).color.as_ref().map(|c| (b.index() + 1, c))).collect();
    if !colors.is_empty() {
        let _ = writeln!(out, "    <backlight devicemodel=\"{G13_MODEL}\">");
        for (shift, c) in colors {
            let _ = writeln!(out, "      <mode color=\"{}\" shiftstate=\"{shift}\"/>", xml_escape(&c.to_lowercase()));
        }
        out.push_str("    </backlight>\n");
    }
    out.push_str("  </profile>\n</profiles>\n");
    (out, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::from_dto;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<profiles xmlns="http://www.logitech.com/Cassandra/2010.7/Profile">
  <profile guid="{CFEDC0AE-7AAC-49F7-A9A3-2FEF319BE1FA}" lock="0" name="WARDOGS" gkeysdk="0" launchable="1">
    <description>WARDOGS</description>
    <target path="C:\GAMES\Wardogs\wardogs.exe"/>
    <macros>
      <macro guid="{00000000-0000-0000-0000-000000000001}" name="1" original="true"><keystroke xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Keystroke"><key value="1"/></keystroke></macro>
      <macro guid="{00000000-0000-0000-0000-000000000002}" name="ECHAP"><keystroke xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Keystroke"><key value="ESCAPE"/></keystroke></macro>
      <macro guid="{00000000-0000-0000-0000-000000000003}" name="Z"><keystroke xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Keystroke"><key value="W"/></keystroke></macro>
      <macro guid="{00000000-0000-0000-0000-000000000004}" name="Clic gauche"><mousefunction xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/MouseFunction"><do task="leftclick"/></mousefunction></macro>
      <macro guid="{00000000-0000-0000-0000-000000000005}" name="Copier"><hotkeys xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Hotkeys"><do task="copy"/></hotkeys></macro>
      <macro guid="{00000000-0000-0000-0000-000000000006}" name="Annuler"><hotkeys xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Hotkeys"><do task="undo"/></hotkeys></macro>
      <macro guid="{00000000-0000-0000-0000-000000000007}" name="Ctrl+Maj+Échap"><keystroke xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Keystroke"><key value="LCTRL"/><key value="LSHIFT"/><key value="ESCAPE"/></keystroke></macro>
      <macro guid="{00000000-0000-0000-0000-000000000008}" name="2 (PAVE NUM.)"><keystroke xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Keystroke"><key value="NUM2"/></keystroke></macro>
      <macro guid="{00000000-0000-0000-0000-000000000009}" name="Seq"><multikey xmlns="http://www.logitech.com/Cassandra/2010.1/Macros/Multikey"/></macro>
    </macros>
    <assignments devicecategory="Logitech.Gaming.Keyboard">
      <assignment contextid="G1" macroguid="{00000000-0000-0000-0000-000000000003}" shiftstate="1" original="false" backup="false"/>
    </assignments>
    <assignments devicecategory="Logitech.Gaming.LeftHandedController">
      <assignment contextid="G1" macroguid="{00000000-0000-0000-0000-000000000001}" shiftstate="1" original="false" backup="true"/>
      <assignment contextid="G1" macroguid="{00000000-0000-0000-0000-000000000001}" shiftstate="2" original="true" backup="false"/>
      <assignment contextid="G1" macroguid="{00000000-0000-0000-0000-000000000002}" shiftstate="1" original="false" backup="false"/>
      <assignment contextid="G4" macroguid="{00000000-0000-0000-0000-000000000003}" shiftstate="1" original="false" backup="false"/>
      <assignment contextid="G25" macroguid="{00000000-0000-0000-0000-000000000004}" shiftstate="1" original="false" backup="false"/>
      <assignment contextid="G26" macroguid="{00000000-0000-0000-0000-000000000003}" shiftstate="1" original="true" backup="false"/>
      <assignment contextid="G29" macroguid="{00000000-0000-0000-0000-000000000008}" shiftstate="3" original="false" backup="false"/>
      <assignment contextid="G2" macroguid="{00000000-0000-0000-0000-000000000005}" shiftstate="2" original="false" backup="false"/>
      <assignment contextid="G3" macroguid="{00000000-0000-0000-0000-000000000006}" shiftstate="2" original="false" backup="false"/>
      <assignment contextid="G5" macroguid="{00000000-0000-0000-0000-000000000007}" shiftstate="1" original="false" backup="false"/>
      <assignment contextid="G6" macroguid="{00000000-0000-0000-0000-000000000009}" shiftstate="1" original="false" backup="false"/>
    </assignments>
    <backlight devicemodel="Logitech.Gaming.LeftHandedController.G13">
      <mode color="#ff8000" shiftstate="1"/>
      <mode color="#ffffff" shiftstate="2"/>
    </backlight>
  </profile>
</profiles>"##;

    fn key(dto: &FileDto, bank: MBank, id: &str) -> Option<String> {
        dto.banks.get(bank).keys.get(id).map(|a| serde_json::to_string(a).unwrap())
    }

    #[test]
    fn imports_g13_bindings_colours_and_target() {
        let r = import(SAMPLE).unwrap();
        let d = &r.dto;
        assert_eq!(d.name, "WARDOGS");
        assert_eq!(d.rules, vec![MatchRule::exe("wardogs.exe")]);
        // backup="true" ignored: G1/M1 is ESCAPE, not "1"
        assert_eq!(key(d, MBank::M1, "G1").as_deref(), Some(r#""Escape""#));
        assert_eq!(key(d, MBank::M2, "G1").as_deref(), Some(r#""1""#));
        // physical positions kept as is: LGS "W" = our W (AZERTY Z)
        assert_eq!(key(d, MBank::M1, "G4").as_deref(), Some(r#""W""#));
        assert_eq!(key(d, MBank::M1, "STICK_UP").as_deref(), Some(r#""W""#));
        assert_eq!(key(d, MBank::M3, "STICK_LEFT").as_deref(), Some(r#""Numpad2""#));
        assert_eq!(key(d, MBank::M1, "G25").as_deref(), Some(r#"{"mouse":"Left"}"#));
        assert_eq!(key(d, MBank::M2, "G2").as_deref(), Some(r#""LeftCtrl+C""#));
        assert_eq!(key(d, MBank::M1, "G5").as_deref(), Some(r#""LeftCtrl+LeftShift+Escape""#));
        // keyboard (G110…) assignments are not G13 ones; G6 (multikey) is skipped
        assert_eq!(d.banks.get(MBank::M1).keys.len(), 5);
        assert_eq!(d.banks.get(MBank::M1).color.as_deref(), Some("#FF8000"));
        assert_eq!(d.banks.get(MBank::M3).color, None);
        // reported, not guessed
        assert_eq!(r.skipped.len(), 2, "{:?}", r.skipped);
        assert!(r.skipped.iter().any(|s| s.contains("undo")));
        assert!(r.skipped.iter().any(|s| s.contains("multikey")));
        // and it is a valid profile for the engine
        from_dto(r.dto.clone()).unwrap();
    }

    #[test]
    fn export_then_import_roundtrips() {
        let original = import(SAMPLE).unwrap().dto;
        let (xml, skipped) = export(&original);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert!(xml.contains("devicecategory=\"Logitech.Gaming.LeftHandedController\""));
        assert!(xml.contains("contextid=\"G26\""), "stick up exported as G26");
        let back = import(&xml).unwrap();
        assert!(back.skipped.is_empty(), "{:?}", back.skipped);
        for bank in MBank::ALL {
            assert_eq!(back.dto.banks.get(bank).color, original.banks.get(bank).color);
            let a: Vec<_> = original.banks.get(bank).keys.iter().map(|(k, v)| (k.clone(), serde_json::to_string(v).unwrap())).collect();
            let b: Vec<_> = back.dto.banks.get(bank).keys.iter().map(|(k, v)| (k.clone(), serde_json::to_string(v).unwrap())).collect();
            assert_eq!(a, b, "{bank:?}");
        }
        assert_eq!(back.dto.rules, original.rules);
    }

    #[test]
    fn export_reports_what_lgs_cannot_hold() {
        let mut d = FileDto { name: "x & <y>".into(), ..FileDto::default() };
        d.banks.m1.keys.insert("G1".into(), ActionDto::Wheel { wheel: 1, horizontal: false });
        d.banks.m1.keys.insert("G2".into(), ActionDto::Macro { name: "spam".into() });
        d.banks.m1.keys.insert("G3".into(), ActionDto::Chord("Ctrl+C".into()));
        let (xml, skipped) = export(&d);
        assert_eq!(skipped.len(), 2);
        assert!(xml.contains("name=\"x &amp; &lt;y&gt;\""));
        roxmltree::Document::parse(&xml).unwrap();
    }

    /// Manual check on a real LGS export: `G13_LGS_SAMPLE=path cargo test -- --ignored lgs`
    #[test]
    #[ignore]
    fn import_real_file() {
        let path = std::env::var("G13_LGS_SAMPLE").expect("set G13_LGS_SAMPLE");
        let r = import(&std::fs::read_to_string(path).unwrap()).unwrap();
        for bank in MBank::ALL {
            eprintln!("{bank:?} {:?} {:?}", r.dto.banks.get(bank).color, r.dto.banks.get(bank).keys);
        }
        eprintln!("skipped: {:#?}", r.skipped);
        from_dto(r.dto).unwrap();
    }
}
