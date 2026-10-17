use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use g13_core::{G13Key as K, RgbColor};

use super::action::{Action, Macro, MacroMode, MacroStep};
use super::bindings::{BankBindings, Input, MBank, Profile, StickDir, StickMode};
use super::{Engine, UiEvent};
use crate::device::{DeviceCommand, DeviceEvent};
use crate::keycode::{KeyCode as C, MouseButton};
use super::pad::PadButton;
use crate::platform::GamepadState;
use crate::platform::mock::{Injected, RecordingInjector};

struct Rig {
    engine: Engine,
    inj: Arc<RecordingInjector>,
    dev: Receiver<DeviceCommand>,
    ui: Receiver<UiEvent>,
}

fn rig(profile: Profile) -> Rig {
    let inj = Arc::new(RecordingInjector::new());
    let (tx, dev) = channel();
    let mut engine = Engine::new(inj.clone(), tx, profile);
    let ui = engine.subscribe();
    Rig { engine, inj, dev, ui }
}

fn press(e: &mut Engine, key: K) {
    e.handle(DeviceEvent::Key { key, pressed: true });
}
fn release(e: &mut Engine, key: K) {
    e.handle(DeviceEvent::Key { key, pressed: false });
}

fn two_bank_profile() -> Profile {
    let m1 = BankBindings::colored(RgbColor::RED)
        .with(K::G1, Action::key("Ctrl+Shift+Esc"))
        .with(K::G2, Action::MouseButton(MouseButton::Right))
        .with(K::G3, Action::Wheel { delta: -2, horizontal: false });
    let m2 = BankBindings::colored(RgbColor::BLUE)
        .with(K::G1, Action::key("F5"));
    Profile { name: "t".into(), banks: [m1, m2, BankBindings::default()], ..Profile::default() }
}

#[test]
fn chord_pressed_and_released_in_reverse() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::G1);
    release(&mut r.engine, K::G1);
    assert_eq!(
        r.inj.take(),
        vec![
            Injected::Key(C::LeftCtrl, true),
            Injected::Key(C::LeftShift, true),
            Injected::Key(C::Escape, true),
            Injected::Key(C::Escape, false),
            Injected::Key(C::LeftShift, false),
            Injected::Key(C::LeftCtrl, false),
        ]
    );
}

#[test]
fn mouse_and_wheel() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::G2);
    press(&mut r.engine, K::G3);
    release(&mut r.engine, K::G3);
    release(&mut r.engine, K::G2);
    assert_eq!(
        r.inj.take(),
        vec![
            Injected::Button(MouseButton::Right, true),
            Injected::Wheel(-2, false),
            Injected::Button(MouseButton::Right, false),
        ]
    );
}

#[test]
fn unbound_and_special_keys_inject_nothing() {
    let mut r = rig(two_bank_profile());
    for k in [K::G10, K::L1, K::Light, K::MR] {
        press(&mut r.engine, k);
        release(&mut r.engine, k);
    }
    assert!(r.inj.take().is_empty());
}

#[test]
fn bank_switch_updates_leds_color_and_bindings() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::M2);
    release(&mut r.engine, K::M2);
    assert_eq!(r.engine.active_bank(), MBank::M2);
    let cmds: Vec<_> = r.dev.try_iter().collect();
    assert_eq!(
        cmds,
        vec![
            DeviceCommand::SetBacklight(RgbColor::BLUE),
            DeviceCommand::SetMKeyLeds(MBank::M2.leds(false)),
        ]
    );
    assert_eq!(
        r.ui.try_iter().collect::<Vec<_>>(),
        vec![
            UiEvent::Key { key: K::M2, pressed: true },
            UiEvent::BankChanged(MBank::M2),
            UiEvent::Key { key: K::M2, pressed: false },
        ]
    );

    press(&mut r.engine, K::G1);
    release(&mut r.engine, K::G1);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::F5, true), Injected::Key(C::F5, false)]);
}

#[test]
fn release_after_bank_switch_undoes_original_action() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::G1); // Ctrl+Shift+Esc held (M1)
    press(&mut r.engine, K::M2);
    release(&mut r.engine, K::G1); // must release Ctrl+Shift+Esc, not F5
    let ev = r.inj.take();
    assert_eq!(ev.len(), 6);
    assert_eq!(ev[3..], [
        Injected::Key(C::Escape, false),
        Injected::Key(C::LeftShift, false),
        Injected::Key(C::LeftCtrl, false),
    ]);
}

#[test]
fn disconnect_releases_held_keys() {
    let mut r = rig(two_bank_profile());
    r.engine.handle(DeviceEvent::Connected);
    press(&mut r.engine, K::G2);
    r.engine.handle(DeviceEvent::Disconnected);
    assert_eq!(
        r.inj.take(),
        vec![Injected::Button(MouseButton::Right, true), Injected::Button(MouseButton::Right, false)]
    );
    assert!(!r.engine.is_connected());
}

#[test]
fn connect_pushes_leds_and_backlight() {
    let mut r = rig(two_bank_profile());
    r.engine.handle(DeviceEvent::Connected);
    let cmds: Vec<_> = r.dev.try_iter().collect();
    assert_eq!(
        cmds,
        vec![
            DeviceCommand::SetMKeyLeds(MBank::M1.leds(false)),
            DeviceCommand::SetBacklight(RgbColor::RED),
        ]
    );
}

#[test]
fn mr_toggles_led_and_bd_goes_to_the_lcd() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::MR);
    assert!(r.engine.is_recording());
    press(&mut r.engine, K::Light);
    // BD no longer touches the backlight: it is an LCD (applet) button
    let cmds: Vec<_> = r.dev.try_iter().collect();
    assert_eq!(cmds, vec![DeviceCommand::SetMKeyLeds(MBank::M1.leds(true))]);
    assert!(r.ui.try_iter().any(|e| e == UiEvent::LcdButton { key: K::Light, pressed: true }));
}

#[test]
fn lcd_buttons_are_forwarded() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::L3);
    release(&mut r.engine, K::L3);
    assert_eq!(
        r.ui.try_iter().filter(|e| !matches!(e, UiEvent::Key { .. })).collect::<Vec<_>>(),
        vec![
            UiEvent::LcdButton { key: K::L3, pressed: true },
            UiEvent::LcdButton { key: K::L3, pressed: false },
        ]
    );
}

#[test]
fn overlay_key_only_toggles_the_overlay() {
    let mut r = rig(two_bank_profile());
    press(&mut r.engine, K::G1); // Ctrl+Shift+Esc held
    r.engine.set_overlay_key(Some(K::G1)); // releases what G1 held
    press(&mut r.engine, K::G1);
    release(&mut r.engine, K::G1);
    let ev = r.inj.take();
    assert_eq!(ev.len(), 6, "press + release of the chord, nothing more: {ev:?}");
    assert_eq!(r.ui.try_iter().filter(|e| *e == UiEvent::OverlayToggle).count(), 1);
    r.engine.set_overlay_key(None);
    press(&mut r.engine, K::G1);
    assert_eq!(r.inj.take().len(), 3);
}

// ---- stick (keys mode) ----------------------------------------------------------

fn stick(e: &mut Engine, x: u8, y: u8) {
    e.handle(DeviceEvent::Stick(g13_core::StickPosition { raw_x: x, raw_y: y }));
}

fn stick_profile() -> Profile {
    let m1 = BankBindings::default()
        .with_input(Input::Stick(StickDir::Up), Action::key("W"))
        .with_input(Input::Stick(StickDir::Right), Action::key("D"));
    let m2 = BankBindings::default().with_input(Input::Stick(StickDir::Up), Action::key("Up"));
    Profile { name: "s".into(), banks: [m1, m2, BankBindings::default()], ..Profile::default() }
}

#[test]
fn stick_directions_press_and_release_with_deadzone() {
    let mut r = rig(stick_profile());
    stick(&mut r.engine, 0x7C, 0x6F); // measured rest position: inside the dead zone
    assert!(r.inj.take().is_empty());
    stick(&mut r.engine, 0x80, 0x00); // full up
    stick(&mut r.engine, 0xFF, 0x00); // up + right (diagonal)
    stick(&mut r.engine, 0xFF, 0x80); // right only
    stick(&mut r.engine, 0x80, 0x80); // back to center
    assert_eq!(
        r.inj.take(),
        vec![
            Injected::Key(C::W, true),
            Injected::Key(C::D, true),
            Injected::Key(C::W, false),
            Injected::Key(C::D, false),
        ]
    );
}

#[test]
fn clicking_the_stick_does_not_trigger_directions() {
    let mut r = rig(stick_profile());
    press(&mut r.engine, K::StickClick);
    stick(&mut r.engine, 0x80, 0x00); // the click tilts the stick up
    release(&mut r.engine, K::StickClick);
    stick(&mut r.engine, 0x80, 0x20); // still tilted right after the release
    assert!(!r.inj.take().contains(&Injected::Key(C::W, true)));
    thread::sleep(super::STICK_CLICK_GRACE + Duration::from_millis(20));
    stick(&mut r.engine, 0x80, 0x00); // a real push afterwards works
    assert!(r.inj.take().contains(&Injected::Key(C::W, true)));
}

#[test]
fn tilt_just_before_the_click_is_undone() {
    let mut r = rig(stick_profile());
    stick(&mut r.engine, 0x80, 0x00); // tilt registers first…
    press(&mut r.engine, K::StickClick); // …then the click
    assert_eq!(r.inj.take(), vec![Injected::Key(C::W, true), Injected::Key(C::W, false)]);
    stick(&mut r.engine, 0x80, 0x80);
    assert!(r.inj.take().is_empty(), "already released");
}

#[test]
fn stick_direction_held_across_bank_switch_releases_original() {
    let mut r = rig(stick_profile());
    stick(&mut r.engine, 0x80, 0x00); // W down (M1)
    press(&mut r.engine, K::M2);
    stick(&mut r.engine, 0x80, 0x80); // must release W, not Up
    assert_eq!(r.inj.take(), vec![Injected::Key(C::W, true), Injected::Key(C::W, false)]);
}

#[test]
fn stick_released_on_disconnect_and_ignored_in_gamepad_mode() {
    let mut r = rig(stick_profile());
    stick(&mut r.engine, 0x80, 0x00);
    r.engine.handle(DeviceEvent::Disconnected);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::W, true), Injected::Key(C::W, false)]);

    let mut p = stick_profile();
    p.stick.mode = StickMode::Gamepad;
    let mut r = rig(p);
    stick(&mut r.engine, 0x80, 0x00);
    assert!(r.inj.take().is_empty(), "no key, and no gamepad while the G13 is not connected");
}

// ---- virtual gamepad ----------------------------------------------------------

#[test]
fn gamepad_stick_drives_the_virtual_controller() {
    let mut p = stick_profile();
    p.stick.mode = StickMode::Gamepad;
    p.stick.deadzone = 0.1;
    let mut r = rig(p);
    r.engine.handle(DeviceEvent::Connected);
    stick(&mut r.engine, 0x80, 0x00); // full up
    stick(&mut r.engine, 0x80, 0x80); // back to rest
    r.engine.handle(DeviceEvent::Disconnected);
    let up = GamepadState { left_y: 32767, ..GamepadState::default() };
    assert_eq!(
        r.inj.take(),
        vec![
            Injected::Gamepad(GamepadState::default()), // plugged in on connect
            Injected::Gamepad(up),                      // no STICK_UP key in gamepad mode
            Injected::Gamepad(GamepadState::default()),
            Injected::GamepadUnplugged,
        ]
    );
}

#[test]
fn gamepad_buttons_released_on_profile_switch_and_unplugged() {
    let mut p = Profile::default();
    p.bank_mut(MBank::M1).set(K::G1, Action::Pad(PadButton::A));
    p.bank_mut(MBank::M1).set(K::G2, Action::Pad(PadButton::LT));
    let mut r = rig(p);
    r.engine.handle(DeviceEvent::Connected);
    press(&mut r.engine, K::G1);
    press(&mut r.engine, K::G2);
    r.engine.set_profile(Profile::default()); // no gamepad binding
    assert_eq!(
        r.inj.take(),
        vec![
            Injected::Gamepad(GamepadState::default()),
            Injected::Gamepad(GamepadState { buttons: 0x1000, ..GamepadState::default() }),
            Injected::Gamepad(GamepadState { buttons: 0x1000, left_trigger: 255, ..GamepadState::default() }),
            Injected::Gamepad(GamepadState { left_trigger: 255, ..GamepadState::default() }),
            Injected::Gamepad(GamepadState::default()),
            Injected::GamepadUnplugged,
        ]
    );
    // a profile without gamepad never plugs one in
    r.engine.handle(DeviceEvent::Disconnected);
    r.engine.handle(DeviceEvent::Connected);
    assert!(r.inj.take().is_empty());
}

// ---- macros -----------------------------------------------------------------

fn macro_profile(mode: MacroMode, steps: Vec<MacroStep>) -> Profile {
    let m = Arc::new(Macro { name: "m".into(), mode, steps });
    let mut p = Profile::default();
    p.bank_mut(MBank::M1).set(K::G5, Action::Macro(m));
    p
}

fn wait_quiet(inj: &RecordingInjector) -> Vec<Injected> {
    // collect until nothing new arrives for 60 ms
    let mut all = Vec::new();
    loop {
        thread::sleep(Duration::from_millis(60));
        let new = inj.take();
        if new.is_empty() {
            return all;
        }
        all.extend(new);
    }
}

#[test]
fn macro_once_plays_steps_with_delay() {
    let steps = vec![
        MacroStep::KeyDown(C::A),
        MacroStep::Delay(Duration::from_millis(30)),
        MacroStep::KeyUp(C::A),
    ];
    let mut r = rig(macro_profile(MacroMode::Once, steps));
    let t0 = std::time::Instant::now();
    press(&mut r.engine, K::G5);
    release(&mut r.engine, K::G5);
    // released immediately, but a Once macro keeps playing
    let ev = wait_quiet(&r.inj);
    assert_eq!(ev, vec![Injected::Key(C::A, true), Injected::Key(C::A, false)]);
    assert!(t0.elapsed() >= Duration::from_millis(30));
}

#[test]
fn macro_repeat_while_held_stops_and_releases_on_key_up() {
    let steps = vec![
        MacroStep::KeyDown(C::LeftShift),
        MacroStep::Delay(Duration::from_millis(15)),
        MacroStep::KeyUp(C::LeftShift),
        MacroStep::Delay(Duration::from_millis(15)),
    ];
    let mut r = rig(macro_profile(MacroMode::RepeatWhileHeld, steps));
    press(&mut r.engine, K::G5);
    thread::sleep(Duration::from_millis(100));
    release(&mut r.engine, K::G5);
    let ev = wait_quiet(&r.inj);
    let downs = ev.iter().filter(|e| **e == Injected::Key(C::LeftShift, true)).count();
    let ups = ev.iter().filter(|e| **e == Injected::Key(C::LeftShift, false)).count();
    assert!(downs >= 2, "should have looped, got {ev:?}");
    assert_eq!(downs, ups, "every press must be released: {ev:?}");
    assert_eq!(ev.last(), Some(&Injected::Key(C::LeftShift, false)));
}

#[test]
fn macro_toggle_starts_and_stops() {
    let steps = vec![MacroStep::Wheel(1), MacroStep::Delay(Duration::from_millis(10))];
    let mut r = rig(macro_profile(MacroMode::Toggle, steps));
    press(&mut r.engine, K::G5);
    release(&mut r.engine, K::G5);
    thread::sleep(Duration::from_millis(80));
    assert!(!r.inj.take().is_empty(), "toggle should keep running after release");
    press(&mut r.engine, K::G5);
    release(&mut r.engine, K::G5);
    wait_quiet(&r.inj);
    thread::sleep(Duration::from_millis(50));
    assert!(r.inj.take().is_empty(), "toggle should have stopped");
}

#[test]
fn shutdown_stops_toggled_macros_and_releases_keys() {
    let steps = vec![MacroStep::KeyDown(C::B), MacroStep::Delay(Duration::from_secs(10))];
    let mut r = rig(macro_profile(MacroMode::Toggle, steps));
    press(&mut r.engine, K::G5);
    thread::sleep(Duration::from_millis(30));
    r.engine.shutdown();
    let ev = wait_quiet(&r.inj);
    assert_eq!(ev, vec![Injected::Key(C::B, true), Injected::Key(C::B, false)]);
}

#[test]
fn demo_profile_is_consistent() {
    let p = super::demo::demo_profile();
    assert!(matches!(p.bank(MBank::M1).get(K::G10), Action::Key(c) if c.keys() == [C::W]));
    // non-bindable keys can't be bound
    let mut b = BankBindings::default();
    assert!(!b.set(K::M1, Action::key("A")));
    assert!(b.set(K::StickClick, Action::key("A")));
}

// ---- tap dance ----------------------------------------------------------------------

fn dance_profile() -> Profile {
    let td = super::action::TapDance {
        tap: Action::key("A"),
        hold: Action::key("B"),
        double_tap: Action::key("C"),
        tap_hold: Action::key("D"),
    };
    let m1 = BankBindings::default()
        .with(K::G1, Action::TapDance(Arc::new(td)))
        .with(K::G2, Action::TapDance(Arc::new(super::action::TapDance { tap: Action::key("E"), hold: Action::key("F"), ..Default::default() })));
    Profile { name: "td".into(), banks: [m1, BankBindings::default(), BankBindings::default()], ..Profile::default() }
}

/// Let the tap term run out, ticking like the engine loop.
fn wait_term(e: &mut Engine) {
    thread::sleep(super::action::TAP_TERM + Duration::from_millis(40));
    e.tick();
}

fn tapped(k: C) -> Vec<Injected> {
    vec![Injected::Key(k, true), Injected::Key(k, false)]
}

#[test]
fn tap_dance_tap_waits_for_a_second_press() {
    let mut r = rig(dance_profile());
    press(&mut r.engine, K::G1);
    release(&mut r.engine, K::G1);
    assert_eq!(r.inj.take(), vec![], "a double tap may still come");
    wait_term(&mut r.engine);
    assert_eq!(r.inj.take(), tapped(C::A));
}

#[test]
fn tap_dance_hold_is_held_until_release() {
    let mut r = rig(dance_profile());
    press(&mut r.engine, K::G1);
    wait_term(&mut r.engine);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::B, true)]);
    release(&mut r.engine, K::G1);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::B, false)]);
}

#[test]
fn tap_dance_double_tap_and_tap_hold() {
    let mut r = rig(dance_profile());
    for _ in 0..2 {
        press(&mut r.engine, K::G1);
        release(&mut r.engine, K::G1);
    }
    assert_eq!(r.inj.take(), tapped(C::C));
    wait_term(&mut r.engine);
    assert_eq!(r.inj.take(), vec![], "nothing more after a double tap");

    press(&mut r.engine, K::G1);
    release(&mut r.engine, K::G1);
    press(&mut r.engine, K::G1);
    wait_term(&mut r.engine);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::D, true)]);
    release(&mut r.engine, K::G1);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::D, false)]);
}

#[test]
fn tap_dance_without_second_press_taps_on_release() {
    let mut r = rig(dance_profile());
    // G2: tap + hold only, nothing to wait for after a short press
    press(&mut r.engine, K::G2);
    release(&mut r.engine, K::G2);
    assert_eq!(r.inj.take(), tapped(C::E));
    // a profile change in the middle of a hold lets go of it
    press(&mut r.engine, K::G2);
    wait_term(&mut r.engine);
    assert_eq!(r.inj.take(), vec![Injected::Key(C::F, true)]);
    r.engine.set_profile(dance_profile());
    assert_eq!(r.inj.take(), vec![Injected::Key(C::F, false)]);
}
