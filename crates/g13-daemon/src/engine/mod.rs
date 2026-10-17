//! The mapping engine: consumes [`DeviceEvent`]s, keeps the M-bank / MR /
//! backlight state, and turns G-key transitions into OS input through the
//! [`InputInjector`].
//!
//! Invariants:
//! * **No stuck keys.** What a G-key *pressed* is remembered in `held`, and the
//!   release always undoes exactly that, even if the bank or profile changed
//!   in between. Disconnect, profile switch and shutdown release everything.
//! * The engine never blocks on I/O: device output goes through a channel,
//!   macros run on their own threads.

pub mod action;
pub mod bindings;
pub mod demo;
pub mod macros;
pub mod pad;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use g13_core::{G13Key, G13LcdBuffer, RgbColor, StickPosition};

use crate::device::{DeviceCommand, DeviceEvent};
use crate::keycode::{KeyChord, MouseButton};
use crate::platform::{InputInjector, PlatformResult};

use action::{Action, MacroMode};
use bindings::{MBank, Profile, SLOTS, StickDir, StickMode};
use macros::MacroHandle;
use pad::{Pad, PadButton};

/// Engine loop period when idle (drives `tick()`: macro bookkeeping now,
/// stick-to-mouse integration later).
pub const TICK: Duration = Duration::from_millis(10);

/// Notifications for the other subsystems (LCD manager, IPC clients).
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// Any physical key pressed / released (live view in the GUI).
    Key { key: G13Key, pressed: bool },
    /// Stick direction entered / left (keys mode).
    StickDir { dir: StickDir, pressed: bool },
    /// BD or L1..L4 pressed / released — routed to the LCD applets.
    LcdButton { key: G13Key, pressed: bool },
    /// The profile pinned everywhere changed from the LCD (`None` = automatic).
    Pinned(Option<String>),
    BankChanged(MBank),
    /// New profile applied (its display name).
    ProfileChanged(String),
    /// Id (file stem) of the profile now active, sent by the profile manager.
    ActiveProfile(String),
    /// BD key: backlight + LCD on (`true`) or standby (`false`).
    DisplayPower(bool),
    /// MR state toggled.
    RecordMode(bool),
    /// The overlay key was pressed: show / hide the in-game overlay.
    OverlayToggle,
    /// The virtual gamepad could not be created or updated (driver missing…).
    GamepadError(String),
    /// New state of the mouse (connected, battery, onboard settings…).
    Mouse(Box<crate::mouse::MouseStatus>),
    /// New state of the headset (on / off, battery, sidetone, equalizer…).
    Headset(Box<crate::headset::HeadsetStatus>),
    /// New LCD image (960 bytes: 6 pages × 160 columns, bit 0 = top), for
    /// the GUI's live preview.
    LcdFrame(Box<[u8; 960]>),
    Connected(bool),
}

/// Where a tap-dance key is in its gesture.
#[derive(Default)]
enum Dance {
    #[default]
    Idle,
    /// Down since `since` (`second`: the second press of the gesture).
    Pressed { td: Arc<action::TapDance>, since: std::time::Instant, second: bool },
    /// Released after a tap, waiting for a second press until `since` + TAP_TERM.
    Released { td: Arc<action::TapDance>, since: std::time::Instant },
    /// Its hold / tap-hold action is held (`held[slot]`) until the key goes up.
    Holding,
}

/// What a physical G-key is currently doing.
enum Held {
    Idle,
    Chord(KeyChord),
    Button(MouseButton),
    Macro(MacroHandle),
    Pad(PadButton),
}

pub struct Engine {
    injector: Arc<dyn InputInjector>,
    device: Sender<DeviceCommand>,
    subscribers: Vec<Sender<UiEvent>>,

    profile: Profile,
    bank: MBank,
    /// Indexed by binding slot (G-keys, then stick directions).
    held: [Held; SLOTS],
    /// Tap-dance keys: where each one is in its gesture.
    dance: [Dance; SLOTS],
    toggled: [Option<MacroHandle>; SLOTS],
    once: Vec<(usize, MacroHandle)>,

    connected: bool,
    recording: bool,
    display_on: bool,
    color: RgbColor,
    stick: Option<StickPosition>,
    /// Stick directions currently "pressed" (keys mode), by `StickDir::index`.
    stick_dirs: [bool; 4],
    /// G-key reserved for the GUI overlay (its profile binding is ignored).
    overlay_key: Option<G13Key>,
    /// Pressing the stick (G25) tilts it: no new direction while it is held
    /// and shortly after (`click_until`).
    click_held: bool,
    click_until: Option<std::time::Instant>,
    /// When each stick direction was last pressed (to undo the tilt that
    /// precedes a click).
    dir_since: [Option<std::time::Instant>; 4],
    /// Virtual gamepad (stick gamepad mode, gamepad-button bindings).
    pad: Pad,
    /// The last gamepad update failed: don't retry (and log) on every stick
    /// event, only after the next connect / profile change.
    pad_failed: bool,
}

/// After releasing the stick click, directions stay blocked this long.
pub const STICK_CLICK_GRACE: Duration = Duration::from_millis(150);

impl Engine {
    pub fn new(injector: Arc<dyn InputInjector>, device: Sender<DeviceCommand>, profile: Profile) -> Self {
        let color = profile.bank(MBank::M1).color.unwrap_or(RgbColor::WHITE);
        Self {
            injector,
            device,
            subscribers: Vec::new(),
            profile,
            bank: MBank::M1,
            held: std::array::from_fn(|_| Held::Idle),
            dance: std::array::from_fn(|_| Dance::Idle),
            toggled: std::array::from_fn(|_| None),
            once: Vec::new(),
            connected: false,
            recording: false,
            display_on: true,
            color,
            stick: None,
            stick_dirs: [false; 4],
            overlay_key: None,
            click_held: false,
            click_until: None,
            dir_since: [None; 4],
            pad: Pad::default(),
            pad_failed: false,
        }
    }

    /// Reserve a G-key for the GUI overlay (`None` = no overlay key). That key
    /// then only emits [`UiEvent::OverlayToggle`]; whatever it was holding is
    /// released first.
    pub fn set_overlay_key(&mut self, key: Option<G13Key>) {
        if let Some(k) = key.filter(|k| k.is_g_key()) {
            self.slot_up(k.bit_index() as usize);
        }
        self.overlay_key = key.filter(|k| k.is_g_key());
    }

    /// Receive [`UiEvent`]s (LCD manager, IPC server…).
    pub fn subscribe(&mut self) -> Receiver<UiEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.subscribers.push(tx);
        rx
    }

    // ---- accessors (used by IPC / LCD status applet later, and by tests) ----

    pub fn active_bank(&self) -> MBank {
        self.bank
    }
    pub fn profile(&self) -> &Profile {
        &self.profile
    }
    pub fn is_recording(&self) -> bool {
        self.recording
    }
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    pub fn stick(&self) -> Option<StickPosition> {
        self.stick
    }

    // ---- main loop ----------------------------------------------------------

    /// Process events until `shutdown` is set or the device thread exits.
    pub fn run(&mut self, events: &Receiver<DeviceEvent>, shutdown: &AtomicBool) {
        self.run_with(events, shutdown, |_| {});
    }

    /// Like [`Self::run`], calling `on_tick` on the engine thread after every
    /// tick (at least every [`TICK`]) — used by the profile manager.
    pub fn run_with(
        &mut self,
        events: &Receiver<DeviceEvent>,
        shutdown: &AtomicBool,
        mut on_tick: impl FnMut(&mut Self),
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            match events.recv_timeout(TICK) {
                Ok(ev) => {
                    self.handle(ev);
                    // Drain bursts without waiting.
                    while let Ok(ev) = events.try_recv() {
                        self.handle(ev);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.tick();
            on_tick(self);
        }
        self.shutdown();
    }

    pub fn handle(&mut self, ev: DeviceEvent) {
        match ev {
            DeviceEvent::Connected => {
                self.connected = true;
                self.push_device_state();
                self.notify(UiEvent::Connected(true));
                self.sync_pad();
            }
            DeviceEvent::Disconnected => {
                self.connected = false;
                self.release_all();
                self.stick = None;
                self.unplug_pad();
                self.notify(UiEvent::Connected(false));
            }
            DeviceEvent::Key { key, pressed } => self.on_key(key, pressed),
            DeviceEvent::Stick(pos) => {
                log::trace!("stick {pos:?}");
                self.stick = Some(pos);
                self.pad.observe(pos, std::time::Instant::now());
                self.update_stick_dirs();
                self.push_pad();
            }
        }
    }

    /// Keys mode: turn the stick position into press / release edges of the
    /// 4 direction slots (releases first, like the key tracker).
    fn update_stick_dirs(&mut self) {
        let mut want = match (self.profile.stick.mode, self.stick) {
            (StickMode::Keys, Some(pos)) => {
                let (x, y) = pos.normalized();
                let dz = self.profile.stick.deadzone.clamp(0.0, 0.95);
                [y > dz, y < -dz, x < -dz, x > dz]
            }
            _ => [false; 4],
        };
        // clicking the stick tilts it: don't let that start a direction
        let clicking = self.click_held || self.click_until.is_some_and(|t| std::time::Instant::now() < t);
        if clicking {
            for (w, held) in want.iter_mut().zip(self.stick_dirs) {
                *w &= held;
            }
        }
        for pressing in [false, true] {
            for d in StickDir::ALL {
                let i = d.index();
                if self.stick_dirs[i] != want[i] && want[i] == pressing {
                    self.stick_dirs[i] = pressing;
                    self.dir_since[i] = pressing.then(std::time::Instant::now);
                    log::debug!("{} {}", d.id(), if pressing { "pressed" } else { "released" });
                    self.notify(UiEvent::StickDir { dir: d, pressed: pressing });
                    if pressing {
                        self.slot_down(d.slot());
                    } else {
                        self.slot_up(d.slot());
                    }
                }
            }
        }
    }

    /// Periodic housekeeping.
    pub fn tick(&mut self) {
        self.dance_tick(std::time::Instant::now());
        self.once.retain(|(_, h)| !h.is_finished());
        for slot in &mut self.toggled {
            if slot.as_ref().is_some_and(MacroHandle::is_finished) {
                *slot = None;
            }
        }
        if self.pad.settle(std::time::Instant::now()) {
            self.push_pad();
        }
    }

    /// Release everything, stop all macros and unplug the virtual gamepad
    /// (called on exit).
    pub fn shutdown(&mut self) {
        self.release_all();
        self.unplug_pad();
    }

    /// Replace the active profile (auto-switch / IPC). Held keys are released
    /// first; the M-bank is kept.
    pub fn set_profile(&mut self, profile: Profile) {
        self.release_all();
        self.profile = profile;
        if let Some(c) = self.profile.bank(self.bank).color {
            self.color = c;
        }
        self.push_device_state();
        self.notify(UiEvent::ProfileChanged(self.profile.name.clone()));
        // A stick already pushed past the dead zone acts on the new profile.
        self.update_stick_dirs();
        self.sync_pad();
    }

    // ---- key handling ---------------------------------------------------------

    fn on_key(&mut self, key: G13Key, pressed: bool) {
        log::debug!("{} {}", key.id(), if pressed { "pressed" } else { "released" });
        self.notify(UiEvent::Key { key, pressed });

        if key == G13Key::StickClick {
            self.click_held = pressed;
            self.click_until = (!pressed).then(|| std::time::Instant::now() + STICK_CLICK_GRACE);
            if pressed {
                // the stick tilts *before* the click registers: undo directions
                // that started just before it
                for d in StickDir::ALL {
                    let i = d.index();
                    if self.stick_dirs[i] && self.dir_since[i].is_some_and(|t| t.elapsed() < STICK_CLICK_GRACE) {
                        self.stick_dirs[i] = false;
                        self.dir_since[i] = None;
                        self.notify(UiEvent::StickDir { dir: d, pressed: false });
                        self.slot_up(d.slot());
                    }
                }
            }
        }

        if self.overlay_key == Some(key) {
            if pressed {
                self.notify(UiEvent::OverlayToggle);
            }
            return;
        }

        if key.is_g_key() {
            let slot = key.bit_index() as usize;
            if pressed {
                self.slot_down(slot);
            } else {
                self.slot_up(slot);
            }
            return;
        }

        match key {
            G13Key::M1 | G13Key::M2 | G13Key::M3 if pressed => {
                if let Some(bank) = MBank::from_key(key) {
                    self.switch_bank(bank);
                }
            }
            G13Key::MR if pressed => self.toggle_record(),
            // BD (round button left of L1) cycles the LCD applets, like LGS;
            // the backlight has its own hardware button handled by the G13.
            G13Key::Light | G13Key::L1 | G13Key::L2 | G13Key::L3 | G13Key::L4 => {
                self.notify(UiEvent::LcdButton { key, pressed });
            }
            _ => {}
        }
    }

    /// A bindable input (G-key or stick direction) went down.
    fn slot_down(&mut self, slot: usize) {
        let now = std::time::Instant::now();
        // a second press within the tap term: the gesture goes on
        if let Dance::Released { td, .. } = std::mem::take(&mut self.dance[slot]) {
            self.dance[slot] = Dance::Pressed { td, since: now, second: true };
            return;
        }
        // Defensive: a press without a release (should not happen) — undo first.
        self.slot_up(slot);

        let action = self.profile.bank(self.bank).slot(slot).clone();
        if let Action::TapDance(td) = &action {
            if td.tap_only() {
                self.held[slot] = self.start_action(slot, td.tap.clone());
            } else {
                self.dance[slot] = Dance::Pressed { td: td.clone(), since: now, second: false };
            }
            return;
        }
        self.held[slot] = self.start_action(slot, action);
    }

    /// Start `action` for this slot; what to undo when the key goes up.
    fn start_action(&mut self, slot: usize, action: Action) -> Held {
        match action {
            Action::None => Held::Idle,
            Action::Key(chord) => {
                for &k in chord.keys() {
                    self.inject(|i| i.key(k, true));
                }
                Held::Chord(chord)
            }
            Action::MouseButton(b) => {
                self.inject(|i| i.mouse_button(b, true));
                Held::Button(b)
            }
            Action::Wheel { delta, horizontal } => {
                self.inject(|i| i.mouse_wheel(delta, horizontal));
                Held::Idle
            }
            Action::Macro(mac) => match mac.mode {
                MacroMode::Once => {
                    if self.once.iter().any(|(s, h)| *s == slot && !h.is_finished()) {
                        log::debug!("macro '{}' still running, press ignored", mac.name);
                    } else if let Some(h) = self.start_macro(mac, false) {
                        self.once.push((slot, h));
                    }
                    Held::Idle
                }
                MacroMode::RepeatWhileHeld => match self.start_macro(mac, true) {
                    Some(h) => Held::Macro(h),
                    None => Held::Idle,
                },
                MacroMode::Toggle => {
                    match self.toggled[slot].take() {
                        Some(running) if !running.is_finished() => {
                            log::debug!("macro '{}' toggled off", running.name);
                            running.cancel();
                        }
                        _ => self.toggled[slot] = self.start_macro(mac, true),
                    }
                    Held::Idle
                }
            },
            Action::Pad(b) => {
                self.pad.press(b);
                self.push_pad();
                Held::Pad(b)
            }
            // not nested (refused when the profile is read)
            Action::TapDance(_) => Held::Idle,
        }
    }

    /// A tap: the action pressed and released at once (a macro plays once).
    fn fire(&mut self, slot: usize, action: Action) {
        if let Action::Macro(mac) = action {
            if let Some(h) = self.start_macro(mac, false) {
                self.once.push((slot, h));
            }
            return;
        }
        self.held[slot] = self.start_action(slot, action);
        self.release_held(slot);
    }

    /// Tap-dance keys whose time is up: held long enough (hold / tap-hold), or no
    /// second press came (tap).
    fn dance_tick(&mut self, now: std::time::Instant) {
        for slot in 0..SLOTS {
            match &self.dance[slot] {
                Dance::Pressed { since, .. } | Dance::Released { since, .. } if now.duration_since(*since) < action::TAP_TERM => {}
                Dance::Pressed { td, second, .. } => {
                    let a = if *second { &td.tap_hold } else { &td.hold };
                    let a = if matches!(a, Action::None) { td.tap.clone() } else { a.clone() };
                    self.dance[slot] = Dance::Holding;
                    self.held[slot] = self.start_action(slot, a);
                }
                Dance::Released { td, .. } => {
                    let tap = td.tap.clone();
                    self.dance[slot] = Dance::Idle;
                    self.fire(slot, tap);
                }
                Dance::Idle | Dance::Holding => {}
            }
        }
    }

    /// Undo exactly what `slot_down` did for this slot.
    fn slot_up(&mut self, slot: usize) {
        match std::mem::take(&mut self.dance[slot]) {
            Dance::Idle => {}
            Dance::Holding => {}
            // released within the tap term
            Dance::Pressed { td, second: false, .. } if td.no_second_press() => self.fire(slot, td.tap.clone()),
            Dance::Pressed { td, second: false, .. } => {
                self.dance[slot] = Dance::Released { td, since: std::time::Instant::now() };
                return;
            }
            Dance::Pressed { td, second: true, .. } => {
                if matches!(td.double_tap, Action::None) {
                    self.fire(slot, td.tap.clone());
                    self.fire(slot, td.tap.clone());
                } else {
                    self.fire(slot, td.double_tap.clone());
                }
                return;
            }
            // a release while waiting for a second press cannot happen
            Dance::Released { td, since } => {
                self.dance[slot] = Dance::Released { td, since };
                return;
            }
        }
        self.release_held(slot);
    }

    /// Let go of what the slot holds.
    fn release_held(&mut self, slot: usize) {
        match std::mem::replace(&mut self.held[slot], Held::Idle) {
            Held::Idle => {}
            Held::Chord(chord) => {
                for &k in chord.keys().iter().rev() {
                    self.inject(|i| i.key(k, false));
                }
            }
            Held::Button(b) => self.inject(|i| i.mouse_button(b, false)),
            Held::Macro(h) => h.cancel(),
            Held::Pad(b) => {
                self.pad.release(b);
                self.push_pad();
            }
        }
    }

    fn start_macro(&self, mac: Arc<action::Macro>, looping: bool) -> Option<MacroHandle> {
        match macros::spawn(self.injector.clone(), mac, looping) {
            Ok(h) => Some(h),
            Err(e) => {
                log::error!("could not start macro thread: {e}");
                None
            }
        }
    }

    fn release_all(&mut self) {
        for slot in 0..SLOTS {
            // a gesture in progress is dropped: nothing was pressed yet, or it is held
            self.dance[slot] = Dance::Idle;
            self.release_held(slot);
        }
        self.stick_dirs = [false; 4];
        for slot in &mut self.toggled {
            if let Some(h) = slot.take() {
                h.cancel();
            }
        }
        for (_, h) in self.once.drain(..) {
            h.cancel();
        }
        self.pad.release_all();
        self.push_pad();
    }

    // ---- virtual gamepad ----------------------------------------------------------

    /// Plug the virtual gamepad in when the profile needs it, unplug it otherwise.
    fn sync_pad(&mut self) {
        self.pad_failed = false;
        if self.connected && self.profile.uses_pad() {
            self.push_pad();
        } else {
            self.unplug_pad();
        }
    }

    /// Send the gamepad state if it changed (plugging the controller in on
    /// the first update). Nothing happens while no gamepad is needed.
    fn push_pad(&mut self) {
        let needed = self.connected && self.profile.uses_pad();
        if self.pad_failed || (self.pad.sent.is_none() && !needed) {
            return;
        }
        let stick = match (self.profile.stick.mode, self.stick) {
            (StickMode::Gamepad, Some(pos)) => Some((pos, self.profile.stick.deadzone, self.profile.stick.target)),
            _ => None,
        };
        let st = self.pad.state(stick);
        if self.pad.sent == Some(st) {
            return;
        }
        match self.injector.gamepad(&st) {
            Ok(()) => self.pad.sent = Some(st),
            Err(e) => {
                log::warn!("virtual gamepad: {e}");
                self.pad_failed = true;
                self.pad.sent = None;
                self.notify(UiEvent::GamepadError(e.detail()));
            }
        }
    }

    fn unplug_pad(&mut self) {
        if self.pad.sent.take().is_some()
            && let Err(e) = self.injector.gamepad_unplug()
        {
            log::warn!("virtual gamepad unplug: {e}");
        }
    }

    // ---- M-keys / MR / BD -------------------------------------------------------

    fn switch_bank(&mut self, bank: MBank) {
        if bank != self.bank {
            log::info!("bank {:?} -> {:?}", self.bank, bank);
            self.bank = bank;
            if let Some(c) = self.profile.bank(bank).color {
                self.color = c;
                if self.display_on {
                    self.send(DeviceCommand::SetBacklight(c));
                }
            }
            self.notify(UiEvent::BankChanged(bank));
        }
        self.send(DeviceCommand::SetMKeyLeds(bank.leds(self.recording)));
    }

    fn toggle_record(&mut self) {
        self.recording = !self.recording;
        // On-the-fly recording (MR → G-key → keystrokes → MR) needs a global
        // keyboard hook (WH_KEYBOARD_LL / evdev grab) and lands with the
        // profile manager; for now MR only drives its LED and notifies.
        log::info!("macro record mode {}", if self.recording { "ON (capture not implemented yet)" } else { "OFF" });
        self.send(DeviceCommand::SetMKeyLeds(self.bank.leds(self.recording)));
        self.notify(UiEvent::RecordMode(self.recording));
    }

    fn push_device_state(&self) {
        self.send(DeviceCommand::SetMKeyLeds(self.bank.leds(self.recording)));
        let c = if self.display_on { self.color } else { RgbColor::BLACK };
        self.send(DeviceCommand::SetBacklight(c));
    }

    // ---- plumbing -------------------------------------------------------------

    fn inject(&self, f: impl FnOnce(&dyn InputInjector) -> PlatformResult) {
        if let Err(e) = f(&*self.injector) {
            log::warn!("injection failed: {e}");
        }
    }

    fn send(&self, cmd: DeviceCommand) {
        // Fails only when the HID thread is gone (shutdown); nothing to do then.
        let _ = self.device.send(cmd);
    }

    /// Show `frame` on the G13's LCD (through the HID thread) and publish it
    /// for live previews.
    pub fn draw_lcd(&mut self, frame: &G13LcdBuffer) {
        if self.connected {
            self.send(DeviceCommand::DrawLcd(Box::new(*frame)));
        }
        let mut px = [0u8; 960];
        px.copy_from_slice(frame.pixels());
        self.notify(UiEvent::LcdFrame(Box::new(px)));
    }

    pub fn active_bank_name(&self) -> &'static str {
        match self.bank {
            MBank::M1 => "M1",
            MBank::M2 => "M2",
            MBank::M3 => "M3",
        }
    }

    /// Forward an event from a component running on the engine thread
    /// (profile manager…) to the subscribers.
    pub fn emit(&mut self, ev: UiEvent) {
        self.notify(ev);
    }

    fn notify(&mut self, ev: UiEvent) {
        self.subscribers.retain(|s| s.send(ev.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests;
