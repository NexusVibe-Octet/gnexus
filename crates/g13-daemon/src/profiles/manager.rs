//! Runs on the engine thread (called from `Engine::run_with` every tick):
//! polls the foreground app at ~4 Hz to auto-switch profiles and rescans the
//! profile directory every second for hot reload. All switches go through
//! [`Engine::set_profile`], which releases every held key first.

use std::time::{Duration, Instant};

use super::store::ProfileStore;
use crate::engine::{Engine, UiEvent};
use crate::engine::bindings::Profile;
use crate::platform::{ForegroundWatcher, PlatformError};

pub const FOREGROUND_POLL: Duration = Duration::from_millis(250);
pub const RESCAN_PERIOD: Duration = Duration::from_secs(1);

pub struct ProfileManager {
    store: ProfileStore,
    /// `None` = auto-switch disabled (or unsupported on this platform).
    watcher: Option<Box<dyn ForegroundWatcher>>,
    active: String,
    last_exe: Option<String>,
    next_poll: Instant,
    next_scan: Instant,
    /// Profile shown in the GUI: applied while our own window has the focus,
    /// so edits (colour, keys) can be tried on the G13 right away.
    preview: Option<String>,
    /// File name of this executable (to recognise our own window).
    self_exe: Option<String>,
    /// "Test everywhere": this profile is active in every application,
    /// rules and default ignored, until unpinned.
    pinned: Option<String>,
    /// Applications seen in the foreground, most recent first (shared with
    /// the GUI to offer "bind to this game").
    recent: RecentApps,
}

/// Most recent first, at most [`RECENT_MAX`] entries, one per executable.
pub type RecentApps = std::sync::Arc<std::sync::Mutex<Vec<crate::platform::ForegroundApp>>>;
pub const RECENT_MAX: usize = 25;

impl ProfileManager {
    pub fn new(store: ProfileStore, watcher: Option<Box<dyn ForegroundWatcher>>) -> Self {
        let now = Instant::now();
        let active = store.default_id().to_string();
        let self_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        Self {
            store,
            watcher,
            active,
            last_exe: None,
            next_poll: now,
            next_scan: now + RESCAN_PERIOD,
            preview: None,
            self_exe,
            pinned: None,
            recent: RecentApps::default(),
        }
    }

    /// Handle on the recently focused applications (updated at each poll).
    pub fn recent_apps(&self) -> RecentApps {
        self.recent.clone()
    }

    fn remember(&mut self, app: &crate::platform::ForegroundApp) {
        if app.process_name.is_empty()
            || self.self_exe.as_deref().is_some_and(|me| me.eq_ignore_ascii_case(&app.process_name))
        {
            return;
        }
        let mut list = self.recent.lock().unwrap();
        list.retain(|a| !a.process_name.eq_ignore_ascii_case(&app.process_name));
        list.insert(0, app.clone());
        list.truncate(RECENT_MAX);
    }

    /// Pin `id` everywhere (test mode), or `None` to return to automatic switching.
    pub fn set_pinned(&mut self, id: Option<String>) {
        self.pinned = id;
        self.next_poll = Instant::now();
    }

    pub fn pinned(&self) -> Option<&str> {
        self.pinned.as_deref()
    }

    /// Profile to apply while this process's window has the focus (`None` to stop).
    pub fn set_preview(&mut self, id: Option<String>) {
        self.preview = id;
        self.next_poll = Instant::now();
    }

    /// Override the executable name used to recognise our own window (tests).
    pub fn set_self_exe(&mut self, name: &str) {
        self.self_exe = Some(name.to_string());
    }

    pub fn store(&self) -> &ProfileStore {
        &self.store
    }

    pub fn active_id(&self) -> &str {
        &self.active
    }

    /// `exe` is G13 Studio itself.
    pub fn is_self(&self, exe: &str) -> bool {
        self.self_exe.as_deref().is_some_and(|me| me.eq_ignore_ascii_case(exe))
    }

    /// Executable that had the focus at the last poll (the game), if known.
    pub fn foreground_exe(&self) -> Option<&str> {
        self.last_exe.as_deref()
    }

    /// Profile to start the engine with (the default one).
    pub fn initial_profile(&self) -> Profile {
        self.store.profile(&self.active).cloned().unwrap_or_default()
    }

    /// Rate-limited housekeeping; call it as often as you like.
    pub fn tick(&mut self, engine: &mut Engine) {
        let now = Instant::now();
        if now >= self.next_scan {
            self.next_scan = now + RESCAN_PERIOD;
            self.rescan(engine);
        }
        if now >= self.next_poll {
            self.next_poll = now + FOREGROUND_POLL;
            self.poll_foreground(engine);
        }
    }

    /// Switch to the profile matching the focused application, or to the
    /// default profile if no rule matches. While our own window is focused,
    /// the preview profile (if any) wins.
    pub fn poll_foreground(&mut self, engine: &mut Engine) {
        if let Some(id) = self.pinned.clone().filter(|id| self.store.get(id).is_some()) {
            // still track the foreground app (LCD shows it), but don't switch
            if let Some(Ok(Some(app))) = self.watcher.as_mut().map(|w| w.current()) {
                if self.last_exe.as_deref() != Some(app.process_name.as_str()) {
                    self.remember(&app);
                }
                self.last_exe = Some(app.process_name);
            }
            self.activate(&id, engine);
            return;
        }
        let preview = self.preview.clone().filter(|id| self.store.get(id).is_some());
        let Some(watcher) = self.watcher.as_mut() else {
            if let Some(id) = preview {
                self.activate(&id, engine);
            }
            return;
        };
        let app = match watcher.current() {
            Ok(Some(app)) => app,
            // No focused window (lock screen, desktop transition…): keep the current profile.
            Ok(None) => return,
            Err(PlatformError::Unsupported(what)) => {
                log::warn!("profile auto-switch disabled: {what} unsupported");
                self.watcher = None;
                return;
            }
            Err(e) => {
                log::debug!("foreground detection failed: {e}");
                return;
            }
        };
        if self.last_exe.as_deref() != Some(app.process_name.as_str()) {
            log::debug!("foreground: {} — {:?}", app.process_name, app.title);
            self.last_exe = Some(app.process_name.clone());
            self.remember(&app);
        }
        let ours = self.self_exe.as_deref().is_some_and(|me| me.eq_ignore_ascii_case(&app.process_name));
        let target = match preview {
            Some(id) if ours => id,
            _ => self.store.select(&app).to_string(),
        };
        self.activate(&target, engine);
    }

    /// Hot reload: re-read changed files; re-apply the active profile if it
    /// was edited, fall back to the default one if it disappeared.
    pub fn rescan(&mut self, engine: &mut Engine) {
        let changed = self.store.rescan();
        if self.store.get(&self.active).is_none() {
            log::info!("active profile '{}' is gone", self.active);
            let fallback = self.store.default_id().to_string();
            self.active.clear();
            self.activate(&fallback, engine);
        } else if changed.contains(&self.active) {
            log::info!("profile '{}' reloaded", self.active);
            let id = std::mem::take(&mut self.active);
            self.activate(&id, engine);
        }
    }

    fn activate(&mut self, id: &str, engine: &mut Engine) {
        if id == self.active {
            return;
        }
        let Some(profile) = self.store.profile(id) else { return };
        log::info!("profile -> '{id}' ({})", profile.name);
        engine.set_profile(profile.clone());
        self.active = id.to_string();
        engine.emit(UiEvent::ActiveProfile(self.active.clone()));
    }
}
