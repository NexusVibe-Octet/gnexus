//! OS-independent implementations: a recording/logging injector used by
//! `g13d --dry-run` and by the unit tests, plus null watchers.

use std::sync::Mutex;

use super::{
    ForegroundApp, ForegroundWatcher, GamepadState, InputInjector, MediaCommand, MediaInfo,
    MediaProvider, PlatformError, PlatformResult,
};
use crate::keycode::{KeyCode, MouseButton};

/// One synthetic event, as seen by [`RecordingInjector`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Injected {
    Key(KeyCode, bool),
    Button(MouseButton, bool),
    Wheel(i32, bool),
    Move(i32, i32),
    Gamepad(GamepadState),
    GamepadUnplugged,
}

/// Logs every event (`info` level) and keeps them in memory. Nothing reaches the OS.
#[derive(Default)]
pub struct RecordingInjector {
    events: Mutex<Vec<Injected>>,
    log: bool,
}

impl RecordingInjector {
    /// Silent recorder (tests).
    pub fn new() -> Self {
        Self::default()
    }

    /// Recorder that also logs each event (dry-run mode).
    pub fn logging() -> Self {
        Self { events: Mutex::new(Vec::new()), log: true }
    }

    /// Take and clear the recorded events.
    pub fn take(&self) -> Vec<Injected> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    fn push(&self, ev: Injected) -> PlatformResult {
        if self.log {
            log::info!("[dry-run] {ev:?}");
        }
        self.events.lock().unwrap().push(ev);
        Ok(())
    }
}

impl InputInjector for RecordingInjector {
    fn key(&self, key: KeyCode, down: bool) -> PlatformResult {
        self.push(Injected::Key(key, down))
    }
    fn mouse_button(&self, button: MouseButton, down: bool) -> PlatformResult {
        self.push(Injected::Button(button, down))
    }
    fn mouse_wheel(&self, delta: i32, horizontal: bool) -> PlatformResult {
        self.push(Injected::Wheel(delta, horizontal))
    }
    fn mouse_move(&self, dx: i32, dy: i32) -> PlatformResult {
        self.push(Injected::Move(dx, dy))
    }
    fn gamepad(&self, state: &GamepadState) -> PlatformResult {
        self.push(Injected::Gamepad(*state))
    }
    fn gamepad_unplug(&self) -> PlatformResult {
        self.push(Injected::GamepadUnplugged)
    }
}

pub struct NullForegroundWatcher;

impl ForegroundWatcher for NullForegroundWatcher {
    fn current(&mut self) -> PlatformResult<Option<ForegroundApp>> {
        Err(PlatformError::Unsupported("foreground detection"))
    }
}

pub struct NullMediaProvider;

impl MediaProvider for NullMediaProvider {
    fn now_playing(&mut self) -> PlatformResult<Option<MediaInfo>> {
        Ok(None)
    }
    fn send(&mut self, _cmd: MediaCommand) -> PlatformResult {
        Err(PlatformError::Unsupported("media control"))
    }
}
