//! Hand-editable TOML format of a profile file.
//!
//! ```toml
//! name = "Jeu"
//! number = 2                  # position in the GUI's profile list (optional)
//! default = true              # profile used when no rule matches (optional)
//!
//! [stick]                     # optional, these are the defaults
//! mode = "keys"               # keys (STICK_* bindings) / gamepad (virtual Xbox 360 stick)
//! deadzone = 35               # percent of full deflection
//! target = "left"             # gamepad mode: left / right controller stick
//!
//! [[rules]]                   # auto-switch when one of these apps has focus
//! exe = "eldenring.exe"       # file name, case-insensitive (".exe" optional)
//! [[rules]]
//! title = "Mozilla Firefox"   # substring of the window title, case-insensitive
//!
//! [banks.M1]
//! color = "#FF8000"
//! G1 = "Escape"               # key or chord: "Ctrl+Shift+Esc" (physical US positions)
//! G2 = { mouse = "Right" }    # Left / Right / Middle / Back / Forward
//! G3 = { wheel = -1 }         # notches per press; horizontal = true for tilt
//! G4 = { macro = "spam" }     # see [macros]
//! G5 = { pad = "A" }          # gamepad button: A B X Y LB RB LT RT Back Start LS RS Up Down Left Right
//! STICK_UP = "W"              # stick directions: STICK_UP / _DOWN / _LEFT / _RIGHT
//!
//! [macros.spam]
//! mode = "repeat_while_held"  # once / repeat_while_held / toggle
//! steps = ["down Q", "wait 20", "up Q", "wait 80"]
//! ```
//!
//! Macro steps: `down <key>`, `up <key>`, `mouse_down <button>`,
//! `mouse_up <button>`, `wheel <notches>`, `wait <ms>`.
//!
//! The DTOs below only describe the file; [`from_toml`] / [`to_toml`]
//! convert to and from the engine types and do all the validation.

use std::sync::Arc;
use std::time::Duration;

use g13_core::RgbColor;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::ProfileError;
use super::rules::MatchRule;
use crate::engine::action::{Action, Macro, MacroMode, MacroStep, TapDance};
use crate::engine::bindings::{Input, MBank, PadStick, Profile, StickConfig, StickMode};
use crate::engine::pad::PadButton;
use crate::keycode::{KeyChord, KeyCode, MouseButton};

/// A parsed profile file.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileDoc {
    pub profile: Profile,
    pub rules: Vec<MatchRule>,
    /// `default = true`: fallback profile when no rule matches.
    pub is_default: bool,
    /// Position in the profile list (GUI, LCD), if the file has one.
    pub number: Option<u32>,
}

/// The file as written on disk, before validation. Also the JSON model the
/// GUI edits (same field names), so both share [`from_dto`] validation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileDto {
    pub name: String,
    /// Display position in the GUI (not used by the engine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stick: Option<StickDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<MatchRule>,
    #[serde(default)]
    pub banks: BanksDto,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub macros: IndexMap<String, MacroDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanksDto {
    #[serde(rename = "M1", default)]
    pub m1: BankDto,
    #[serde(rename = "M2", default)]
    pub m2: BankDto,
    #[serde(rename = "M3", default)]
    pub m3: BankDto,
}

impl BanksDto {
    pub fn get(&self, bank: MBank) -> &BankDto {
        match bank {
            MBank::M1 => &self.m1,
            MBank::M2 => &self.m2,
            MBank::M3 => &self.m3,
        }
    }

    pub fn get_mut(&mut self, bank: MBank) -> &mut BankDto {
        match bank {
            MBank::M1 => &mut self.m1,
            MBank::M2 => &mut self.m2,
            MBank::M3 => &mut self.m3,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// G-key id (`"G1"`…`"G25"`) → action.
    #[serde(flatten)]
    pub keys: IndexMap<String, ActionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ActionDto {
    Chord(String),
    Mouse {
        mouse: String,
    },
    Wheel {
        wheel: i32,
        #[serde(default, skip_serializing_if = "is_false")]
        horizontal: bool,
    },
    Macro {
        #[serde(rename = "macro")]
        name: String,
    },
    Pad {
        pad: String,
    },
    /// `{ tap = …, hold = …, double_tap = …, tap_hold = … }` (each optional).
    TapDance {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tap: Option<Box<ActionDto>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hold: Option<Box<ActionDto>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        double_tap: Option<Box<ActionDto>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tap_hold: Option<Box<ActionDto>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroDto {
    #[serde(default)]
    pub mode: ModeDto,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeDto {
    #[default]
    Once,
    RepeatWhileHeld,
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StickDto {
    #[serde(default)]
    pub mode: StickModeDto,
    /// Percent of full deflection.
    #[serde(default = "default_deadzone")]
    pub deadzone: u8,
    #[serde(default, skip_serializing_if = "is_left")]
    pub target: PadStickDto,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadStickDto {
    #[default]
    Left,
    Right,
}

fn is_left(t: &PadStickDto) -> bool {
    *t == PadStickDto::Left
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StickModeDto {
    #[default]
    Keys,
    Gamepad,
}

fn default_deadzone() -> u8 {
    (StickConfig::default().deadzone * 100.0).round() as u8
}

fn is_false(b: &bool) -> bool {
    !*b
}

// ---------------------------------------------------------------------------
// TOML → engine
// ---------------------------------------------------------------------------

/// Parse and validate a profile file.
pub fn from_toml(text: &str) -> Result<ProfileDoc, ProfileError> {
    let dto: FileDto = toml::from_str(text).map_err(|e| ProfileError(e.to_string()))?;
    from_dto(dto)
}

/// A macro of the file model, checked (also used for the mouse's macros).
pub fn macro_from_dto(name: &str, m: &MacroDto) -> Result<Macro, ProfileError> {
    let steps = m
        .steps
        .iter()
        .map(|s| parse_step(s).ok_or_else(|| ProfileError(format!("macro '{name}': invalid step {s:?}"))))
        .collect::<Result<Vec<_>, _>>()?;
    let mode = match m.mode {
        ModeDto::Once => MacroMode::Once,
        ModeDto::RepeatWhileHeld => MacroMode::RepeatWhileHeld,
        ModeDto::Toggle => MacroMode::Toggle,
    };
    Ok(Macro { name: name.to_string(), mode, steps })
}

/// Validate a file model (from TOML or from the GUI's JSON).
pub fn from_dto(dto: FileDto) -> Result<ProfileDoc, ProfileError> {
    let mut macros = IndexMap::new();
    for (name, m) in &dto.macros {
        macros.insert(name.clone(), Arc::new(macro_from_dto(name, m)?));
    }

    let mut profile = Profile { name: dto.name, ..Profile::default() };
    if let Some(s) = &dto.stick {
        if s.deadzone > 95 {
            return Err(ProfileError(format!("stick: deadzone {}% out of range (0..95)", s.deadzone)));
        }
        profile.stick = StickConfig {
            mode: match s.mode {
                StickModeDto::Keys => StickMode::Keys,
                StickModeDto::Gamepad => StickMode::Gamepad,
            },
            deadzone: f32::from(s.deadzone) / 100.0,
            target: match s.target {
                PadStickDto::Left => PadStick::Left,
                PadStickDto::Right => PadStick::Right,
            },
        };
    }
    for bank in MBank::ALL {
        let src = dto.banks.get(bank);
        let dst = profile.bank_mut(bank);
        if let Some(c) = &src.color {
            dst.color = Some(parse_color(c).ok_or_else(|| ProfileError(format!("{bank:?}: invalid color {c:?}")))?);
        }
        for (id, a) in &src.keys {
            let input = Input::from_id(id).ok_or_else(|| ProfileError(format!("{bank:?}: unknown key {id:?}")))?;
            if input.slot().is_none() {
                return Err(ProfileError(format!("{bank:?}: {id} cannot be bound (only G1..G25, STICK_*)")));
            }
            let action = parse_action(a, &macros).map_err(|e| ProfileError(format!("{bank:?}.{id}: {e}")))?;
            dst.set_input(input, action);
        }
    }

    for r in &dto.rules {
        if r.exe.is_none() && r.title.is_none() {
            return Err(ProfileError("rule without `exe` nor `title`".into()));
        }
    }

    Ok(ProfileDoc { profile, rules: dto.rules, is_default: dto.default, number: dto.number })
}

fn parse_action(a: &ActionDto, macros: &IndexMap<String, Arc<Macro>>) -> Result<Action, String> {
    Ok(match a {
        ActionDto::Chord(s) => Action::Key(KeyChord::parse(s).ok_or_else(|| format!("invalid key/chord {s:?}"))?),
        ActionDto::Mouse { mouse } => {
            Action::MouseButton(MouseButton::parse(mouse).ok_or_else(|| format!("invalid mouse button {mouse:?}"))?)
        }
        ActionDto::Wheel { wheel, horizontal } => Action::Wheel { delta: *wheel, horizontal: *horizontal },
        ActionDto::Macro { name } => {
            Action::Macro(macros.get(name).cloned().ok_or_else(|| format!("undefined macro {name:?}"))?)
        }
        ActionDto::Pad { pad } => Action::Pad(PadButton::parse(pad).ok_or_else(|| format!("invalid gamepad button {pad:?}"))?),
        ActionDto::TapDance { tap, hold, double_tap, tap_hold } => {
            let one = |a: &Option<Box<ActionDto>>| -> Result<Action, String> {
                match a.as_deref() {
                    None => Ok(Action::None),
                    Some(ActionDto::TapDance { .. }) => Err("a tap dance cannot hold another one".into()),
                    Some(a) => parse_action(a, macros),
                }
            };
            let td = TapDance { tap: one(tap)?, hold: one(hold)?, double_tap: one(double_tap)?, tap_hold: one(tap_hold)? };
            if td.tap_only() { td.tap } else { Action::TapDance(Arc::new(td)) }
        }
    })
}

fn parse_step(s: &str) -> Option<MacroStep> {
    let (op, arg) = s.trim().split_once(char::is_whitespace)?;
    let arg = arg.trim();
    Some(match op.to_ascii_lowercase().as_str() {
        "down" => MacroStep::KeyDown(KeyCode::parse(arg)?),
        "up" => MacroStep::KeyUp(KeyCode::parse(arg)?),
        "mouse_down" => MacroStep::MouseDown(MouseButton::parse(arg)?),
        "mouse_up" => MacroStep::MouseUp(MouseButton::parse(arg)?),
        "wheel" => MacroStep::Wheel(arg.parse().ok()?),
        "wait" => MacroStep::Delay(Duration::from_millis(arg.strip_suffix("ms").unwrap_or(arg).trim().parse().ok()?)),
        // "wait_random 500 30": 500 ms plus a random 0..=30 ms
        "wait_random" => {
            let mut n = arg.split_whitespace().map(|v| v.strip_suffix("ms").unwrap_or(v).parse::<u64>());
            let base = n.next()?.ok()?;
            let extra = n.next().unwrap_or(Ok(0)).ok()?;
            if n.next().is_some() {
                return None;
            }
            MacroStep::RandomDelay { base: Duration::from_millis(base), extra: Duration::from_millis(extra) }
        }
        _ => return None,
    })
}

fn parse_color(s: &str) -> Option<RgbColor> {
    let hex = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(RgbColor::new(ch(0)?, ch(2)?, ch(4)?))
}

// ---------------------------------------------------------------------------
// engine → TOML
// ---------------------------------------------------------------------------

/// Serialise a profile. Macros are written once in `[macros]`, keyed by
/// name; two *different* macros with the same name are an error.
pub fn to_toml(doc: &ProfileDoc) -> Result<String, ProfileError> {
    emit(&to_dto(doc)?)
}

/// Write an already-validated file model (the GUI saves its JSON model this
/// way, which keeps macros that are defined but not bound to any key).
pub fn dto_to_toml(dto: &FileDto) -> Result<String, ProfileError> {
    emit(dto)
}

/// One action → file model (`None` for no action); its macros are collected in `seen`.
fn action_to_dto(action: &Action, seen: &mut IndexMap<String, Arc<Macro>>) -> Result<Option<ActionDto>, ProfileError> {
    Ok(Some(match action {
        Action::None => return Ok(None),
        Action::Key(chord) => ActionDto::Chord(chord.to_string()),
        Action::MouseButton(b) => ActionDto::Mouse { mouse: b.name().into() },
        Action::Wheel { delta, horizontal } => ActionDto::Wheel { wheel: *delta, horizontal: *horizontal },
        Action::Macro(m) => {
            match seen.get(&m.name) {
                Some(prev) if **prev != **m => {
                    return Err(ProfileError(format!("two different macros are named {:?}", m.name)));
                }
                Some(_) => {}
                None => {
                    seen.insert(m.name.clone(), m.clone());
                }
            }
            ActionDto::Macro { name: m.name.clone() }
        }
        Action::Pad(b) => ActionDto::Pad { pad: b.id().into() },
        Action::TapDance(td) => {
            let mut one = |a: &Action| action_to_dto(a, seen).map(|d| d.map(Box::new));
            ActionDto::TapDance { tap: one(&td.tap)?, hold: one(&td.hold)?, double_tap: one(&td.double_tap)?, tap_hold: one(&td.tap_hold)? }
        }
    }))
}

/// Engine types → file model. Only macros bound to a key are included.
pub fn to_dto(doc: &ProfileDoc) -> Result<FileDto, ProfileError> {
    let stick = doc.profile.stick;
    let mut dto = FileDto {
        name: doc.profile.name.clone(),
        number: doc.number,
        default: doc.is_default,
        rules: doc.rules.clone(),
        stick: (stick != StickConfig::default()).then(|| StickDto {
            mode: match stick.mode {
                StickMode::Keys => StickModeDto::Keys,
                StickMode::Gamepad => StickModeDto::Gamepad,
            },
            deadzone: (stick.deadzone * 100.0).round() as u8,
            target: match stick.target {
                PadStick::Left => PadStickDto::Left,
                PadStick::Right => PadStickDto::Right,
            },
        }),
        ..FileDto::default()
    };
    let mut seen: IndexMap<String, Arc<Macro>> = IndexMap::new();

    for bank in MBank::ALL {
        let src = doc.profile.bank(bank);
        let dst = dto.banks.get_mut(bank);
        dst.color = src.color.map(|c| format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b));
        for (key, action) in src.iter() {
            let Some(a) = action_to_dto(action, &mut seen)? else { continue };
            dst.keys.insert(key.id().to_string(), a);
        }
    }

    for (name, m) in seen {
        let mode = match m.mode {
            MacroMode::Once => ModeDto::Once,
            MacroMode::RepeatWhileHeld => ModeDto::RepeatWhileHeld,
            MacroMode::Toggle => ModeDto::Toggle,
        };
        dto.macros.insert(name, MacroDto { mode, steps: m.steps.iter().map(format_step).collect() });
    }

    Ok(dto)
}

/// Write the file by hand around serde so that bank entries stay one line
/// each, in G-key order (`G25 = { mouse = "Middle" }`), instead of being
/// expanded into `[banks.M1.G25]` sub-tables.
fn emit(dto: &FileDto) -> Result<String, ProfileError> {
    #[derive(Serialize)]
    struct Head<'a> {
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        number: Option<u32>,
        #[serde(skip_serializing_if = "is_false")]
        default: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        stick: Option<&'a StickDto>,
        #[serde(skip_serializing_if = "<[_]>::is_empty")]
        rules: &'a [MatchRule],
    }
    #[derive(Serialize)]
    struct Tail<'a> {
        #[serde(skip_serializing_if = "IndexMap::is_empty")]
        macros: &'a IndexMap<String, MacroDto>,
    }
    let err = |e: toml::ser::Error| ProfileError(e.to_string());

    let head = Head {
        name: &dto.name,
        number: dto.number,
        default: dto.default,
        stick: dto.stick.as_ref(),
        rules: &dto.rules,
    };
    let mut out = toml::to_string_pretty(&head)
        .map_err(err)?;
    for bank in MBank::ALL {
        let b = dto.banks.get(bank);
        if b.color.is_none() && b.keys.is_empty() {
            continue;
        }
        out.push_str(&format!("\n[banks.{bank:?}]\n"));
        if let Some(c) = &b.color {
            out.push_str(&format!("color = {}\n", toml::Value::String(c.clone())));
        }
        for (id, a) in &b.keys {
            let v = toml::Value::try_from(a).map_err(err)?;
            out.push_str(&format!("{id} = {v}\n"));
        }
    }
    let tail = toml::to_string_pretty(&Tail { macros: &dto.macros }).map_err(err)?;
    if !tail.is_empty() {
        out.push('\n');
        out.push_str(&tail);
    }
    Ok(out)
}

fn format_step(s: &MacroStep) -> String {
    match s {
        MacroStep::KeyDown(k) => format!("down {k}"),
        MacroStep::KeyUp(k) => format!("up {k}"),
        MacroStep::MouseDown(b) => format!("mouse_down {}", b.name()),
        MacroStep::MouseUp(b) => format!("mouse_up {}", b.name()),
        MacroStep::Wheel(d) => format!("wheel {d}"),
        // Sub-millisecond precision is not representable in the file.
        MacroStep::Delay(d) => format!("wait {}", d.as_millis()),
        MacroStep::RandomDelay { base, extra } => format!("wait_random {} {}", base.as_millis(), extra.as_millis()),
    }
}

#[cfg(test)]
mod random_wait_tests {
    use super::*;

    #[test]
    fn wait_random_parses_and_round_trips() {
        let ms = Duration::from_millis;
        let step = parse_step("wait_random 500 30").unwrap();
        assert_eq!(step, MacroStep::RandomDelay { base: ms(500), extra: ms(30) });
        assert_eq!(format_step(&step), "wait_random 500 30");
        assert_eq!(parse_step("wait_random 120"), Some(MacroStep::RandomDelay { base: ms(120), extra: ms(0) }));
        assert_eq!(parse_step("wait_random 500ms 30ms"), Some(MacroStep::RandomDelay { base: ms(500), extra: ms(30) }));
        assert!(parse_step("wait_random 500 30 9").is_none());
        assert!(parse_step("wait_random x").is_none());
    }
}

#[cfg(test)]
mod tap_dance_tests {
    use super::*;

    #[test]
    fn tap_dance_round_trip() {
        let text = "name = \"T\"\n[banks.M1]\nG1 = { tap = \"A\", hold = \"LeftCtrl\", double_tap = { macro = \"m\" } }\nG2 = { tap = \"B\" }\n[macros.m]\nsteps = [\"down X\", \"up X\"]\n";
        let doc = from_toml(text).unwrap();
        let g1 = doc.profile.bank(MBank::M1).get(g13_core::G13Key::G1).clone();
        let Action::TapDance(td) = g1 else { panic!("tap dance expected") };
        assert!(matches!(td.hold, Action::Key(_)) && matches!(td.double_tap, Action::Macro(_)) && td.tap_hold == Action::None);
        // a tap alone is a plain key
        assert!(matches!(doc.profile.bank(MBank::M1).get(g13_core::G13Key::G2), Action::Key(_)));
        let back = to_toml(&doc).unwrap();
        assert!(back.contains("double_tap"), "{back}");
        assert_eq!(from_toml(&back).unwrap().profile, doc.profile);
        assert!(from_toml("name = \"T\"\n[banks.M1]\nG1 = { tap = { tap = \"A\" } }\n").is_err(), "no nesting");
    }
}
