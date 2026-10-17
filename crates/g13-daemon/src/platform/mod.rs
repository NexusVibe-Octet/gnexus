//! OS abstraction layer.
//!
//! Everything that touches the operating system (other than HID, which
//! `hidapi` already abstracts) goes through one of three traits:
//!
//! | Trait               | Windows (implemented)              | Linux / CachyOS (prepared)                  |
//! |---------------------|------------------------------------|---------------------------------------------|
//! | [`InputInjector`]   | `SendInput` (scan codes)           | `/dev/uinput` virtual keyboard + mouse + Xbox 360 pad |
//! | [`ForegroundWatcher`]| `GetForegroundWindow` + PID → exe | KWin script over D-Bus (others: unsupported) |
//! | [`MediaProvider`]   | `Windows.Media.Control`            | MPRIS over D-Bus                            |
//!
//! The rest of the daemon only sees `dyn Trait` objects returned by the
//! `default_*` factories below, so porting means filling in `platform/linux/*`
//! without touching the engine.

use std::fmt;
use std::sync::Arc;

use crate::keycode::{KeyCode, MouseButton};

pub mod mock;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// Feature not available on this OS / back-end (yet).
    Unsupported(&'static str),
    /// The OS call failed.
    Os(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "unsupported on this platform: {what}"),
            Self::Os(msg) => write!(f, "OS error: {msg}"),
        }
    }
}

impl std::error::Error for PlatformError {}

impl PlatformError {
    /// The message alone, without the "OS error:" prefix (shown in the GUI).
    pub fn detail(&self) -> String {
        match self {
            Self::Os(msg) => msg.clone(),
            other => other.to_string(),
        }
    }
}

pub type PlatformResult<T = ()> = Result<T, PlatformError>;

// ---------------------------------------------------------------------------
// Input injection
// ---------------------------------------------------------------------------

/// Virtual gamepad state (XInput ranges). Used by the stick "gamepad" mode
/// and gamepad-button bindings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GamepadState {
    /// Left stick, -32768..=32767, +X = right.
    pub left_x: i16,
    /// Left stick, -32768..=32767, +Y = up.
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
    /// 0..=255.
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// XInput button bitmask (`XINPUT_GAMEPAD_*`).
    pub buttons: u16,
}

/// Emits synthetic keyboard / mouse / gamepad events to the OS.
///
/// Methods take `&self` so one injector can be shared (`Arc`) between the
/// engine thread and macro threads; implementations synchronise internally.
pub trait InputInjector: Send + Sync {
    fn key(&self, key: KeyCode, down: bool) -> PlatformResult;
    fn mouse_button(&self, button: MouseButton, down: bool) -> PlatformResult;
    /// `delta` in notches (+1 = one notch up / right).
    fn mouse_wheel(&self, delta: i32, horizontal: bool) -> PlatformResult;
    /// Relative pointer motion in pixels/mickeys.
    fn mouse_move(&self, dx: i32, dy: i32) -> PlatformResult;
    /// Virtual gamepad (Windows: ViGEmBus, Linux: uinput Xbox 360 pad). The
    /// controller is plugged in on the first call.
    fn gamepad(&self, _state: &GamepadState) -> PlatformResult {
        Err(PlatformError::Unsupported("virtual gamepad"))
    }
    /// Remove the virtual controller (games stop seeing it).
    fn gamepad_unplug(&self) -> PlatformResult {
        Ok(())
    }
}

/// Can a virtual gamepad be created here? `Err` says why not (driver missing…).
pub fn virtual_gamepad_available() -> PlatformResult {
    #[cfg(windows)]
    {
        windows::gamepad::available()
    }
    #[cfg(target_os = "linux")]
    {
        linux::gamepad::available()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(PlatformError::Unsupported("virtual gamepad"))
    }
}

/// Create the native injector for the current OS.
pub fn default_injector() -> PlatformResult<Arc<dyn InputInjector>> {
    #[cfg(windows)]
    {
        Ok(Arc::new(windows::injector::SendInputInjector::new()))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(linux::injector::UinputInjector::new()?))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(PlatformError::Unsupported("input injection"))
    }
}

// ---------------------------------------------------------------------------
// Audio output volume (headset page)
// ---------------------------------------------------------------------------

/// Volume of a playback device, as the system shows it (Windows mixer, PipeWire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputVolume {
    /// Name of the output ("Haut-parleurs (PRO X Wireless Gaming Headset)").
    pub device: String,
    /// 0..100.
    pub volume: u8,
    pub muted: bool,
}

/// Volume of the playback device whose name contains `name` (`None`: no such device).
pub fn output_volume(name: &str) -> PlatformResult<Option<OutputVolume>> {
    #[cfg(windows)]
    {
        windows::audio::output_volume(name)
    }
    #[cfg(target_os = "linux")]
    {
        linux::audio::output_volume(name)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = name;
        Err(PlatformError::Unsupported("output volume"))
    }
}

/// Set the volume (0..100) and / or the mute of that device.
pub fn set_output_volume(name: &str, volume: Option<u8>, mute: Option<bool>) -> PlatformResult {
    #[cfg(windows)]
    {
        windows::audio::set_output_volume(name, volume, mute)
    }
    #[cfg(target_os = "linux")]
    {
        linux::audio::set_output_volume(name, volume, mute)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (name, volume, mute);
        Err(PlatformError::Unsupported("output volume"))
    }
}

/// Equalizer applied by the system to a playback device (Windows: the audio effect of
/// Logitech's driver on the headset; Linux: PipeWire, later).
#[derive(Debug, Clone, PartialEq)]
pub struct OutputEq {
    /// Band centre frequencies (Hz).
    pub freqs: Vec<u32>,
    /// Gain per band (dB).
    pub gains: Vec<f32>,
    pub min_db: f32,
    pub max_db: f32,
}

/// Equalizer of the playback device whose name contains `name` (`None`: no such effect).
pub fn output_eq(name: &str) -> PlatformResult<Option<OutputEq>> {
    #[cfg(windows)]
    {
        windows::audio::output_eq(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(PlatformError::Unsupported("output equalizer"))
    }
}

/// Set its gains (one per band, dB); the effect applies them at once.
pub fn set_output_eq(name: &str, gains: &[f32]) -> PlatformResult {
    #[cfg(windows)]
    {
        windows::audio::set_output_eq(name, gains)
    }
    #[cfg(not(windows))]
    {
        let _ = (name, gains);
        Err(PlatformError::Unsupported("output equalizer"))
    }
}

// ---------------------------------------------------------------------------
// Macro recording
// ---------------------------------------------------------------------------

/// A recording in progress (RECORD button): real keyboard and mouse input,
/// not what G13 Studio itself injects.
pub trait InputRecorder: Send {
    /// Stop and return what was captured (time since the start, input).
    fn stop(self: Box<Self>) -> Vec<(std::time::Duration, crate::recording::Recorded)>;
}

/// Start capturing the keyboard and mouse buttons for a macro.
pub fn start_recording() -> PlatformResult<Box<dyn InputRecorder>> {
    #[cfg(windows)]
    {
        windows::recorder::start()
    }
    #[cfg(target_os = "linux")]
    {
        linux::recorder::start()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(PlatformError::Unsupported("macro recording"))
    }
}

// ---------------------------------------------------------------------------
// Foreground application detection
// ---------------------------------------------------------------------------

/// The application currently holding keyboard focus.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForegroundApp {
    /// Executable file name, e.g. `"eldenring.exe"` / `"firefox"`.
    pub process_name: String,
    /// Full path to the executable when available.
    pub exe_path: Option<String>,
    /// Window title (may be empty).
    pub title: String,
    /// Wayland app-id / X11 WM_CLASS when available (Linux).
    pub app_id: Option<String>,
}

/// Polled by the profile manager (~4 Hz) to auto-switch profiles.
pub trait ForegroundWatcher: Send {
    fn current(&mut self) -> PlatformResult<Option<ForegroundApp>>;
}

/// Applications currently showing a normal window (to pick a game to bind a
/// profile to). Empty where not implemented yet.
pub fn running_apps() -> Vec<ForegroundApp> {
    #[cfg(windows)]
    {
        windows::foreground::running_apps()
    }
    #[cfg(target_os = "linux")]
    {
        linux::foreground::running_apps()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Vec::new()
    }
}

/// Undo what the platform layer set up in the system (Linux: the KWin script).
/// Called when G Nexus stops; nothing to do on Windows.
pub fn shutdown() {
    #[cfg(target_os = "linux")]
    linux::foreground::shutdown();
}

pub fn default_foreground_watcher() -> Box<dyn ForegroundWatcher> {
    #[cfg(windows)]
    {
        Box::new(windows::foreground::Win32ForegroundWatcher::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::foreground::LinuxForegroundWatcher::detect())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Box::new(mock::NullForegroundWatcher)
    }
}

// ---------------------------------------------------------------------------
// Media (LCD media applet)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub status: PlaybackStatus,
    /// Position / duration in seconds, when the player exposes them.
    pub position_s: Option<f64>,
    pub duration_s: Option<f64>,
    /// Source app (e.g. `"Spotify.exe"`, `"org.mpris.MediaPlayer2.spotify"`).
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
    Stop,
}

pub trait MediaProvider: Send {
    fn now_playing(&mut self) -> PlatformResult<Option<MediaInfo>>;
    fn send(&mut self, cmd: MediaCommand) -> PlatformResult;
}

pub fn default_media_provider() -> Box<dyn MediaProvider> {
    #[cfg(windows)]
    {
        Box::new(windows::media::WinMediaProvider::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::media::MprisProvider::new())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Box::new(mock::NullMediaProvider)
    }
}
