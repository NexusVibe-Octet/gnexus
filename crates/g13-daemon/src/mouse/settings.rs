//! What G13 Studio lets you change on a mouse, and its translation to / from
//! an onboard profile. Only these fields are touched: buttons, G-shift and
//! everything else in the profile stay as they are.

use logi_hidpp::features::rgb::LiveEffect;
use logi_hidpp::onboard::{BUTTON_SLOTS, Button, DPI_SLOTS, LED_SLOTS, Led, Profile, Special};

use crate::keycode::{HidUsage, KeyChord, KeyCode};
use crate::profiles::format::{MacroDto, macro_from_dto};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Factory "signature" animation of the G502 X PLUS (pink → blue gradient).
pub const SIGNATURE_ANIMATION: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseSettings {
    /// 1 to 5 DPI steps, in order.
    pub dpi: Vec<u16>,
    /// Step used at power-on / profile load (index in `dpi`).
    pub default_slot: u8,
    /// Step used while the "DPI shift" (sniper) button is held.
    pub shift_slot: u8,
    /// 125, 250, 500 or 1000.
    pub report_rate_hz: u16,
    pub lighting: Lighting,
    /// What each button does (profile slot order; `None` = disabled). G502 X PLUS:
    /// 0 G1 left, 1 G2 right, 2 G3 wheel click, 3 G4 back, 4 G6 sniper, 5 G5 forward,
    /// 6 / 7 wheel tilt left / right, 8 G9, 9 G8, 10 G7.
    pub buttons: Vec<Option<MouseAction>>,
    /// The same while the G-shift button is held.
    pub gshift: Vec<Option<MouseAction>>,
    /// Macros of the profile (same form as the G13's), for buttons `{ "macro": name }`.
    /// Real time only: the mouse's memory does not keep them.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub macros: IndexMap<String, MacroDto>,
}

/// What a mouse button does, in the same JSON forms as the G13's bindings so
/// that G13 Studio's action picker serves both. The mouse's memory holds one
/// key with modifiers, a mouse button, a media key or a built-in function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MouseAction {
    /// Key with optional modifiers, KeyCode names: `"LeftCtrl+C"`, `"F5"`, `"VolumeUp"`.
    Key(String),
    /// `Left`, `Right`, `Middle`, `Back`, `Forward`.
    Mouse { mouse: String },
    /// Built-in function, see [`SPECIALS`].
    Special { special: String },
    /// Something we do not edit (onboard macro…), kept as is: 4 bytes in hex.
    Raw { raw: String },
    /// One of the profile's macros (real time only).
    Macro {
        #[serde(rename = "macro")]
        name: String,
    },
}

/// Built-in functions of the mouse, by the name the GUI uses.
pub const SPECIALS: [(&str, Special); 12] = [
    ("dpi_up", Special::DpiUp),
    ("dpi_down", Special::DpiDown),
    ("dpi_cycle", Special::DpiCycle),
    ("dpi_default", Special::DpiDefault),
    ("dpi_shift", Special::DpiShift),
    ("gshift", Special::GShift),
    ("profile_next", Special::ProfileNext),
    ("profile_previous", Special::ProfilePrevious),
    ("profile_cycle", Special::ProfileCycle),
    ("battery", Special::BatteryStatus),
    ("wheel_left", Special::WheelLeft),
    ("wheel_right", Special::WheelRight),
];

const MOUSE_BUTTONS: [(&str, u16); 5] = [("Left", 0x01), ("Right", 0x02), ("Middle", 0x04), ("Back", 0x08), ("Forward", 0x10)];

/// Onboard button → action.
pub fn action_of(b: Button) -> Option<MouseAction> {
    let raw = |b: Button| MouseAction::Raw { raw: b.to_bytes().iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ") };
    Some(match b {
        Button::Disabled => return None,
        Button::Mouse(m) => match MOUSE_BUTTONS.iter().find(|(_, mask)| *mask == m) {
            Some((name, _)) => MouseAction::Mouse { mouse: (*name).into() },
            None => raw(b),
        },
        Button::Key { modifiers, usage } => {
            let mut keys: Vec<KeyCode> = (0..8)
                .filter(|bit| modifiers & (1 << bit) != 0)
                .filter_map(|bit| KeyCode::from_hid(HidUsage::Modifier(1 << bit)))
                .collect();
            if usage != 0 {
                match KeyCode::from_hid(HidUsage::Key(usage)) {
                    Some(k) => keys.push(k),
                    None => return Some(raw(b)),
                }
            }
            if keys.is_empty() {
                return Some(raw(b));
            }
            MouseAction::Key(KeyChord::new(keys).to_string())
        }
        Button::Consumer(u) => match KeyCode::from_hid(HidUsage::Consumer(u)) {
            Some(k) => MouseAction::Key(k.name().into()),
            None => raw(b),
        },
        Button::Special(s) => match SPECIALS.iter().find(|(_, x)| *x == s) {
            Some((name, _)) => MouseAction::Special { special: (*name).into() },
            None => raw(b),
        },
        Button::Raw(_) => raw(b),
    })
}

/// Action → onboard button, or why the mouse cannot store it.
pub fn button_of(a: Option<&MouseAction>) -> Result<Button, String> {
    let Some(a) = a else { return Ok(Button::Disabled) };
    Ok(match a {
        MouseAction::Macro { name } => {
            return Err(format!(
                "macro « {name} » : les macros ne marchent qu'en temps réel (profils intégrés désactivés)"
            ));
        }
        MouseAction::Mouse { mouse } => {
            let m = MOUSE_BUTTONS.iter().find(|(n, _)| n.eq_ignore_ascii_case(mouse)).ok_or_else(|| format!("bouton de souris inconnu : {mouse}"))?;
            Button::Mouse(m.1)
        }
        MouseAction::Special { special } => {
            let s = SPECIALS.iter().find(|(n, _)| n == special).ok_or_else(|| format!("fonction inconnue : {special}"))?;
            Button::Special(s.1)
        }
        MouseAction::Raw { raw } => {
            let v: Vec<u8> = raw.split_whitespace().filter_map(|x| u8::from_str_radix(x, 16).ok()).collect();
            let b: [u8; 4] = v.try_into().map_err(|_| format!("valeur brute invalide : {raw}"))?;
            Button::parse(b)
        }
        MouseAction::Key(s) => {
            let chord = KeyChord::parse(s).ok_or_else(|| format!("touche inconnue : {s}"))?;
            let mut modifiers = 0u8;
            let mut others = Vec::new();
            for k in chord.keys() {
                match k.hid() {
                    HidUsage::Modifier(bit) => modifiers |= bit,
                    u => others.push((*k, u)),
                }
            }
            match others.as_slice() {
                [] => Button::Key { modifiers, usage: 0 },
                [(_, HidUsage::Key(usage))] => Button::Key { modifiers, usage: *usage },
                [(_, HidUsage::Consumer(u))] if modifiers == 0 => Button::Consumer(*u),
                [(k, HidUsage::Consumer(_))] => return Err(format!("{k} ne se combine pas avec Ctrl / Maj / Alt / Win sur la souris")),
                _ => return Err("un bouton de souris ne garde qu'une touche (avec Ctrl / Maj / Alt / Win)".into()),
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Lighting {
    /// The mouse's own factory effect (G502 X PLUS: solid colours drifting over time).
    Signature,
    /// Gradient across the LEDs, drifting slowly, drawn by G13 Studio while it runs
    /// (per-LED colours, like G HUB). Without G13 Studio the mouse shows its
    /// factory effect, which is what gets saved in its memory.
    Gradient {
        from: String,
        to: String,
        period_ms: u16,
        /// Drift the other way (LED order on the strip).
        reverse: bool,
    },
    Off,
    Fixed {
        color: String,
    },
    Breathing {
        color: String,
        period_ms: u16,
    },
    Cycle {
        period_ms: u16,
    },
    /// An effect set by another program that we do not edit (kept as is).
    Other,
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn rgb(s: &str) -> Result<[u8; 3], String> {
    let h = s.trim().trim_start_matches('#');
    let v = (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten().ok_or_else(|| format!("couleur invalide : {s}"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Settings of an onboard profile. The G502 X PLUS shows LED slot 1.
pub fn from_profile(p: &Profile) -> MouseSettings {
    let dpi: Vec<u16> = (0..DPI_SLOTS).map_while(|i| p.dpi(i)).collect();
    let last = dpi.len().saturating_sub(1) as u8;
    let lighting = if p.custom_animation() != 0 {
        Lighting::Signature
    } else {
        match p.led(1) {
            Led::Off => Lighting::Off,
            Led::Fixed { rgb } => Lighting::Fixed { color: hex(rgb) },
            Led::Breathing { rgb, period_ms, .. } => Lighting::Breathing { color: hex(rgb), period_ms },
            Led::Cycle { period_ms, .. } => Lighting::Cycle { period_ms },
            Led::Raw(_) => Lighting::Other,
        }
    };
    MouseSettings {
        dpi,
        default_slot: p.default_dpi_slot().min(last),
        shift_slot: p.shift_dpi_slot().min(last),
        report_rate_hz: (1000 / u16::from(p.report_interval_ms())).max(125),
        lighting,
        buttons: (0..BUTTON_SLOTS).map(|i| action_of(p.button(i, false))).collect(),
        gshift: (0..BUTTON_SLOTS).map(|i| action_of(p.button(i, true))).collect(),
        macros: IndexMap::new(),
    }
}

/// Colours of `n` LEDs for the gradient at time `t_ms`: from → to → from
/// along the strip (so the drift loops without a seam), half a loop visible.
pub fn gradient_frame(from: [u8; 3], to: [u8; 3], n: usize, t_ms: u64, period_ms: u16, reverse: bool) -> Vec<[u8; 3]> {
    let phase = (t_ms % u64::from(period_ms.max(1))) as f32 / f32::from(period_ms.max(1));
    (0..n)
        .map(|i| {
            let pos = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            let pos = if reverse { 1.0 - pos } else { pos };
            let x = (pos * 0.5 - phase).rem_euclid(1.0) * 2.0; // 0..2: from → to → from
            let k = if x <= 1.0 { x } else { 2.0 - x };
            let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
            [mix(from[0], to[0]), mix(from[1], to[1]), mix(from[2], to[2])]
        })
        .collect()
}

/// `#RRGGBB` → bytes (for the gradient colours).
pub fn parse_color(s: &str) -> Result<[u8; 3], String> {
    rgb(s)
}

/// Check `s` against what the sensor accepts.
/// `realtime`: for a G13 Studio profile (macros allowed), else for the mouse's memory.
pub fn validate(s: &MouseSettings, dpi_min: u16, dpi_max: u16, dpi_step: u16, realtime: bool) -> Result<(), String> {
    if s.dpi.is_empty() || s.dpi.len() > DPI_SLOTS {
        return Err(format!("de 1 à {DPI_SLOTS} paliers DPI"));
    }
    for &d in &s.dpi {
        if d < dpi_min || d > dpi_max || (dpi_step > 1 && !(d - dpi_min).is_multiple_of(dpi_step)) {
            return Err(format!("{d} DPI impossible (de {dpi_min} à {dpi_max} par pas de {dpi_step})"));
        }
    }
    if usize::from(s.default_slot) >= s.dpi.len() || usize::from(s.shift_slot) >= s.dpi.len() {
        return Err("palier par défaut ou « sniper » hors de la liste".into());
    }
    if ![125, 250, 500, 1000].contains(&s.report_rate_hz) {
        return Err(format!("fréquence {} Hz impossible", s.report_rate_hz));
    }
    if s.buttons.len() > BUTTON_SLOTS || s.gshift.len() > BUTTON_SLOTS {
        return Err("trop de boutons".into());
    }
    for (name, m) in &s.macros {
        macro_from_dto(name, m).map_err(|e| e.to_string())?;
    }
    for (i, a) in s.buttons.iter().chain(&s.gshift).enumerate() {
        if realtime && let Some(MouseAction::Macro { name }) = a {
            if !s.macros.contains_key(name) {
                return Err(format!("bouton {} : macro « {name} » introuvable", i % BUTTON_SLOTS + 1));
            }
            continue;
        }
        button_of(a.as_ref()).map_err(|e| format!("bouton {} : {e}", i % BUTTON_SLOTS + 1))?;
    }
    // without a left click the mouse would be hard to use: refuse
    let left = MouseAction::Mouse { mouse: "Left".into() };
    if !s.buttons.is_empty() && !s.buttons.iter().any(|a| a.as_ref() == Some(&left)) {
        return Err("gardez le clic gauche sur au moins un bouton".into());
    }
    Ok(())
}

/// The effect to show live for `l` (`None` = give the lighting back to the
/// mouse: factory gradient or an effect we do not edit).
pub fn live_effect(l: &Lighting) -> Result<Option<LiveEffect>, String> {
    Ok(match l {
        Lighting::Signature | Lighting::Other | Lighting::Gradient { .. } => None,
        Lighting::Off => Some(LiveEffect::Off),
        Lighting::Fixed { color } => Some(LiveEffect::Static(rgb(color)?)),
        Lighting::Breathing { color, period_ms } => {
            Some(LiveEffect::Breathing { rgb: rgb(color)?, period_ms: (*period_ms).clamp(1000, 20000) })
        }
        Lighting::Cycle { period_ms } => Some(LiveEffect::Cycle { period_ms: (*period_ms).clamp(1000, 20000) }),
    })
}

/// Write `s` into the profile (normal and power-save LED slots alike).
/// `factory` is the mouse's ROM profile: the factory gradient is copied from it.
pub fn apply(p: &mut Profile, s: &MouseSettings, factory: Option<&Profile>) -> Result<(), String> {
    for i in 0..DPI_SLOTS {
        p.set_dpi(i, s.dpi.get(i).copied());
    }
    p.set_default_dpi_slot(s.default_slot);
    p.set_shift_dpi_slot(s.shift_slot);
    p.set_report_interval_ms((1000 / s.report_rate_hz.max(125)) as u8);
    for (i, a) in s.buttons.iter().enumerate().take(BUTTON_SLOTS) {
        p.set_button(i, false, button_of(a.as_ref())?);
    }
    for (i, a) in s.gshift.iter().enumerate().take(BUTTON_SLOTS) {
        p.set_button(i, true, button_of(a.as_ref())?);
    }
    let led = match &s.lighting {
        // the gradient needs G13 Studio: in the mouse's memory, its factory effect
        Lighting::Signature | Lighting::Gradient { .. } => {
            match factory {
                // exactly what the mouse showed out of the box (LED slots + animation)
                Some(f) => p.copy_lighting_from(f),
                None if p.custom_animation() == 0 => p.set_custom_animation(SIGNATURE_ANIMATION),
                None => {}
            }
            return Ok(());
        }
        Lighting::Other => return Ok(()),
        Lighting::Off => Led::Off,
        Lighting::Fixed { color } => Led::Fixed { rgb: rgb(color)? },
        Lighting::Breathing { color, period_ms } => {
            Led::Breathing { rgb: rgb(color)?, period_ms: (*period_ms).clamp(1000, 20000), waveform: 0, intensity: 0 }
        }
        Lighting::Cycle { period_ms } => Led::Cycle { period_ms: (*period_ms).clamp(1000, 20000), intensity: 0 },
    };
    // the animation would cover the LEDs
    p.set_custom_animation(0);
    for k in 0..LED_SLOTS {
        p.set_led(k, led);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const P1: &[u8] = include_bytes!("../../../logi-hidpp/tests/data/g502xplus_profile1.bin");
    const ROM: &[u8] = include_bytes!("../../../logi-hidpp/tests/data/g502xplus_rom1.bin");
    const P2: &[u8] = include_bytes!("../../../logi-hidpp/tests/data/g502xplus_profile2.bin");

    #[test]
    fn reads_the_real_profile() {
        let p = Profile::parse(P1, true).unwrap();
        let s = from_profile(&p);
        assert_eq!(s.dpi, [800, 1200, 1600, 2400, 3200]);
        assert_eq!((s.default_slot, s.shift_slot, s.report_rate_hz), (2, 0, 1000));
        assert_eq!(s.lighting, Lighting::Signature);
        validate(&s, 100, 25600, 50, false).unwrap();
    }

    #[test]
    fn apply_then_read_back() {
        let mut p = Profile::parse(P1, true).unwrap();
        let before_buttons: Vec<_> = (0..16).map(|i| (p.button(i, false), p.button(i, true))).collect();
        let s = MouseSettings {
            dpi: vec![400, 1600, 3200],
            default_slot: 1,
            shift_slot: 0,
            report_rate_hz: 500,
            lighting: Lighting::Fixed { color: "#19E3CF".into() },
            ..from_profile(&p)
        };
        apply(&mut p, &s, None).unwrap();
        let q = Profile::parse(&p.to_bytes(), true).unwrap();
        assert_eq!(from_profile(&q), s);
        assert_eq!(q.custom_animation(), 0);
        let after_buttons: Vec<_> = (0..16).map(|i| (q.button(i, false), q.button(i, true))).collect();
        assert_eq!(before_buttons, after_buttons, "buttons untouched");

        // back to the factory gradient: copied from the ROM profile, byte for byte
        let rom = Profile::parse(ROM, false).unwrap();
        let mut s2 = s.clone();
        s2.lighting = Lighting::Signature;
        apply(&mut p, &s2, Some(&rom)).unwrap();
        assert_eq!(p.custom_animation(), SIGNATURE_ANIMATION);
        assert_eq!(&p.raw()[208..253], &ROM[208..253]);
        assert_eq!(from_profile(&p).lighting, Lighting::Signature);
        // the user's original profile had the same lighting as the factory one
        assert_eq!(&P1[208..253], &ROM[208..253]);
    }

    #[test]
    fn buttons_round_trip() {
        let p2 = Profile::parse(P2, true).unwrap();
        let s = from_profile(&p2);
        assert_eq!(s.buttons[0], Some(MouseAction::Mouse { mouse: "Left".into() }));
        assert_eq!(s.buttons[4], Some(MouseAction::Special { special: "gshift".into() }));
        assert_eq!(s.buttons[9], Some(MouseAction::Special { special: "dpi_up".into() }));
        assert_eq!(s.gshift[2], Some(MouseAction::Key("LeftCtrl+T".into())), "the user's Ctrl+T");
        assert_eq!(s.gshift[4], None);
        // reading then writing changes nothing
        let mut q = p2.clone();
        apply(&mut q, &s, None).unwrap();
        assert_eq!(q.to_bytes(), P2);

        // every kind of action survives a round trip
        for a in [
            MouseAction::Key("LeftCtrl+LeftShift+Escape".into()),
            MouseAction::Key("VolumeUp".into()),
            MouseAction::Key("LeftAlt".into()),
            MouseAction::Mouse { mouse: "Forward".into() },
            MouseAction::Special { special: "dpi_shift".into() },
        ] {
            let b = button_of(Some(&a)).unwrap();
            assert_eq!(action_of(b), Some(a));
        }
        assert_eq!(button_of(None).unwrap(), Button::Disabled);
        assert!(button_of(Some(&MouseAction::Key("LeftCtrl+A+B".into()))).is_err(), "two keys");
        assert!(button_of(Some(&MouseAction::Key("LeftCtrl+VolumeUp".into()))).is_err());

        // a profile without any left click is refused
        let mut s2 = s.clone();
        s2.buttons[0] = Some(MouseAction::Key("A".into()));
        assert!(validate(&s2, 100, 25600, 50, false).is_err());
        s2.buttons[3] = Some(MouseAction::Mouse { mouse: "Left".into() });
        validate(&s2, 100, 25600, 50, false).unwrap();
    }

    #[test]
    fn gradient_frames() {
        let (pink, blue) = ([0xFF, 0x4F, 0xB3], [0x3D, 0x7B, 0xFF]);
        let f = gradient_frame(pink, blue, 9, 0, 6000, false);
        assert_eq!(f.len(), 9);
        assert_eq!(f[0], pink, "first LED starts on the first colour");
        assert_eq!(f[8], blue, "last LED on the second one");
        let r = gradient_frame(pink, blue, 9, 0, 6000, true);
        assert_eq!((r[0], r[8]), (blue, pink), "reverse");
        // half a period later the colours have moved along the strip, and it loops
        assert_ne!(gradient_frame(pink, blue, 9, 3000, 6000, false), f);
        assert_eq!(gradient_frame(pink, blue, 9, 6000, 6000, false), f);
        // saving a gradient keeps the mouse's factory effect in its memory
        let mut p = Profile::parse(P1, true).unwrap();
        let rom = Profile::parse(ROM, false).unwrap();
        let mut s = from_profile(&p);
        s.lighting = Lighting::Gradient { from: "#FF4FB3".into(), to: "#3D7BFF".into(), period_ms: 6000, reverse: false };
        apply(&mut p, &s, Some(&rom)).unwrap();
        assert_eq!(&p.raw()[208..253], &ROM[208..253]);
    }

    #[test]
    fn rejects_bad_values() {
        let p = Profile::parse(P1, true).unwrap();
        let mut s = from_profile(&p);
        s.dpi[0] = 825;
        assert!(validate(&s, 100, 25600, 50, false).is_err(), "not on a 50 step");
        let mut s = from_profile(&p);
        s.report_rate_hz = 333;
        assert!(validate(&s, 100, 25600, 50, false).is_err());
        let mut s = from_profile(&p);
        s.default_slot = 7;
        assert!(validate(&s, 100, 25600, 50, false).is_err());
        assert!(rgb("#12345").is_err() && rgb("19e3cf").is_ok());
    }
}
