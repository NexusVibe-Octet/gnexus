//! Built-in demo profile: written as `demo.toml` when the profile directory
//! is empty, and used in memory if no profile file can be loaded.
//!
//! Key names are **physical positions** (US labels): `W` is the key that
//! prints `Z` on AZERTY, so the M1 bank gives ZQSD on a French layout.

use std::sync::Arc;
use std::time::Duration;

use g13_core::{G13Key as K, RgbColor};

use super::action::{Action, Macro, MacroMode, MacroStep};
use super::bindings::{BankBindings, Profile};
use crate::keycode::{KeyChord, MouseButton};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

pub fn demo_profile() -> Profile {
    // M1 — "Jeu": number row on top, movement cluster (WASD positions = ZQSD on AZERTY).
    let m1 = BankBindings::colored(RgbColor::ORANGE)
        .with(K::G1, Action::key("Esc"))
        .with(K::G2, Action::key("1"))
        .with(K::G3, Action::key("2"))
        .with(K::G4, Action::key("3"))
        .with(K::G5, Action::key("4"))
        .with(K::G6, Action::key("5"))
        .with(K::G7, Action::key("6"))
        .with(K::G8, Action::key("Tab"))
        .with(K::G9, Action::key("Q"))
        .with(K::G10, Action::key("W"))
        .with(K::G11, Action::key("E"))
        .with(K::G12, Action::key("R"))
        .with(K::G13, Action::key("T"))
        .with(K::G14, Action::key("M"))
        .with(K::G15, Action::key("LeftShift"))
        .with(K::G16, Action::key("A"))
        .with(K::G17, Action::key("S"))
        .with(K::G18, Action::key("D"))
        .with(K::G19, Action::key("F"))
        .with(K::G20, Action::key("LeftCtrl"))
        .with(K::G21, Action::key("C"))
        .with(K::G22, Action::key("Space"))
        .with(K::G23, Action::key("V"))
        .with(K::G24, Action::key("LeftAlt"))
        .with(K::StickClick, Action::MouseButton(MouseButton::Middle));

    // M2 — "Bureau": shortcuts, wheel, media keys.
    let save = Arc::new(Macro {
        name: "Ctrl+S".into(),
        mode: MacroMode::Once,
        steps: Macro::tap_steps(&KeyChord::parse("Ctrl+S").unwrap(), ms(30), ms(0)),
    });
    let m2 = BankBindings::colored(RgbColor::WHITE)
        .with(K::G1, Action::key("Ctrl+C"))
        .with(K::G2, Action::key("Ctrl+V"))
        .with(K::G3, Action::key("Ctrl+X"))
        .with(K::G4, Action::key("Alt+Tab"))
        .with(K::G5, Action::key("Win+D"))
        .with(K::G6, Action::Macro(save))
        .with(K::G7, Action::Wheel { delta: 1, horizontal: false })
        .with(K::G14, Action::Wheel { delta: -1, horizontal: false })
        .with(K::G8, Action::MouseButton(MouseButton::Back))
        .with(K::G9, Action::MouseButton(MouseButton::Forward))
        .with(K::G20, Action::key("MediaPrevious"))
        .with(K::G21, Action::key("MediaPlayPause"))
        .with(K::G22, Action::key("MediaNext"))
        .with(K::G23, Action::key("VolumeDown"))
        .with(K::G24, Action::key("VolumeUp"));

    // M3 — "Macros": demonstrates the three trigger modes.
    let click = |name: &str, mode| {
        Arc::new(Macro {
            name: name.into(),
            mode,
            steps: vec![
                MacroStep::MouseDown(MouseButton::Left),
                MacroStep::Delay(ms(20)),
                MacroStep::MouseUp(MouseButton::Left),
                MacroStep::Delay(ms(80)),
            ],
        })
    };
    let m3 = BankBindings::colored(RgbColor::CYAN)
        .with(K::G1, Action::Macro(click("autoclick-hold", MacroMode::RepeatWhileHeld)))
        .with(K::G2, Action::Macro(click("autoclick-toggle", MacroMode::Toggle)))
        .with(K::G3, Action::Macro(click("single-click", MacroMode::Once)));

    Profile { name: "Démo".into(), banks: [m1, m2, m3], ..Profile::default() }
}
