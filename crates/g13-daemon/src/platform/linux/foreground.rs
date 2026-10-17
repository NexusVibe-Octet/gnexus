//! Active window detection on Linux.
//!
//! Wayland has no generic "which window has the focus" API: each compositor has
//! its own. Supported today:
//!
//! - **KDE Plasma (KWin)**, Wayland or X11: G Nexus loads a tiny KWin script
//!   (D-Bus `org.kde.KWin /Scripting`) that calls G Nexus back on the session bus
//!   (at its unique name, so instances never mix) whenever the active window, a
//!   window's title or the window list changes. No polling.
//! - Anything else: [`ForegroundWatcher::current`] says "unsupported" and the
//!   automatic profile switching stays off (the profile manager logs it once).
//!
//! The executable name comes from `/proc/<pid>`: for Wine / Proton games it is the
//! Windows `.exe` (from the command line), so the profile rules written under
//! Windows keep working; otherwise the file name of `/proc/<pid>/exe`.
//!
//! One tracker per process ([`tracker`]), shared by every watcher.

use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;

use crate::platform::{ForegroundApp, ForegroundWatcher, PlatformError, PlatformResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    KWin,
    Unknown,
}

/// What the KWin script sends: one window.
#[derive(Debug, Clone, Default, Deserialize)]
struct KWindow {
    #[serde(default)]
    pid: i64,
    /// Wayland app-id / X11 WM_CLASS ("org.kde.konsole", "steam_app_1245620").
    #[serde(default)]
    class: String,
    #[serde(default)]
    title: String,
}

/// The KWin script's message: the active window and the windows of the task bar.
#[derive(Debug, Clone, Default, Deserialize)]
struct Snapshot {
    #[serde(default)]
    active: Option<KWindow>,
    #[serde(default)]
    windows: Vec<KWindow>,
}

/// Executable name for a process: the Windows `.exe` of a Wine / Proton program,
/// else the file name of `/proc/<pid>/exe` (or `comm` when that is not readable).
fn process_name(pid: i64) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    let proc = format!("/proc/{pid}");
    if let Ok(cmdline) = std::fs::read(format!("{proc}/cmdline")) {
        let exe = cmdline
            .split(|&b| b == 0)
            .take(4)
            .map(String::from_utf8_lossy)
            .find(|a| a.to_ascii_lowercase().ends_with(".exe"))
            .map(|a| a.rsplit(['\\', '/']).next().unwrap_or(&a).to_string());
        if exe.is_some() {
            return exe;
        }
    }
    if let Ok(path) = std::fs::read_link(format!("{proc}/exe"))
        && let Some(name) = path.file_name()
    {
        return Some(name.to_string_lossy().into_owned());
    }
    std::fs::read_to_string(format!("{proc}/comm")).ok().map(|s| s.trim().to_string())
}

fn app_of(w: &KWindow) -> ForegroundApp {
    ForegroundApp {
        process_name: process_name(w.pid).unwrap_or_default(),
        exe_path: (w.pid > 0)
            .then(|| std::fs::read_link(format!("/proc/{}/exe", w.pid)).ok())
            .flatten()
            .map(|p| p.to_string_lossy().into_owned()),
        title: w.title.clone(),
        app_id: (!w.class.is_empty()).then(|| w.class.clone()),
    }
}

// ---------------------------------------------------------------------------
// KWin
// ---------------------------------------------------------------------------

const PLUGIN: &str = "gnexus-focus";
const OBJECT: &str = "/org/g13/studio/Focus";
const INTERFACE: &str = "org.g13.studio.Focus";

/// Title of G Nexus's in-game overlay on Linux: the KWin script recognises it (in this
/// process only) and docks it at `position` ("left", "center", "right").
pub fn overlay_title(position: &str) -> String {
    format!("{OVERLAY_TITLE} [{position}]")
}
const OVERLAY_TITLE: &str = "G Nexus — overlay";
/// Overlay panel: width and largest height (logical px), margin from the screen edge.
const OVERLAY_W: u32 = 780;
const OVERLAY_MAX_H: u32 = 940;
const OVERLAY_MARGIN: u32 = 24;

/// The script KWin runs (Plasma 6 names, Plasma 5 ones as a fallback).
///
/// It also handles G Nexus's in-game overlay, which a Wayland client cannot do by
/// itself: kept above the game, out of the task bar, docked on the active screen
/// and given the focus.
fn kwin_script(bus_name: &str) -> String {
    let pid = std::process::id();
    format!(
        r#"// G Nexus: tells G Nexus which window is active (profiles per application),
// and places its in-game overlay.
const six = typeof workspace.windowList === "function";
const placed = new Set();
function overlay(w) {{
    if (w.pid !== {pid}) return;
    const m = /^{OVERLAY_TITLE} \[(\w+)\]$/.exec(String(w.caption));
    const id = String(w.internalId);
    if (!m || placed.has(id)) return;
    placed.add(id);
    w.keepAbove = true;
    w.skipTaskbar = true;
    w.skipSwitcher = true;
    w.skipPager = true;
    const a = workspace.clientArea(KWin.FullScreenArea, workspace.activeScreen, workspace.currentDesktop);
    const width = Math.min({OVERLAY_W}, a.width - 2 * {OVERLAY_MARGIN});
    const height = Math.min(Math.round(a.height * 0.9), {OVERLAY_MAX_H});
    const x = m[1] === "left" ? a.x + {OVERLAY_MARGIN}
            : m[1] === "center" ? a.x + Math.round((a.width - width) / 2)
            : a.x + a.width - width - {OVERLAY_MARGIN};
    w.frameGeometry = {{ x: x, y: a.y + Math.round((a.height - height) / 2), width: width, height: height }};
    if (six) workspace.activeWindow = w; else workspace.activeClient = w;
}}
function list() {{ return six ? workspace.windowList() : workspace.clientList(); }}
function active() {{ return six ? workspace.activeWindow : workspace.activeClient; }}
function info(w) {{ return w ? {{ pid: w.pid, class: String(w.resourceClass), title: String(w.caption) }} : null; }}
function send() {{
    const windows = list().filter(w => w.normalWindow && !w.skipTaskbar).map(info);
    callDBus("{bus_name}", "{OBJECT}", "{INTERFACE}", "Update",
             JSON.stringify({{ active: info(active()), windows: windows }}));
}}
function watch(w) {{ w.captionChanged.connect(() => {{ overlay(w); if (w === active()) send(); }}); }}
list().forEach(watch);
if (six) {{
    workspace.windowActivated.connect(send);
    workspace.windowAdded.connect(w => {{ watch(w); overlay(w); send(); }});
    workspace.windowRemoved.connect(w => {{ placed.delete(String(w.internalId)); send(); }});
}} else {{
    workspace.clientActivated.connect(send);
    workspace.clientAdded.connect(w => {{ watch(w); overlay(w); send(); }});
    workspace.clientRemoved.connect(send);
}}
send();
"#
    )
}

/// Receives the script's calls.
struct FocusObject {
    state: Arc<Mutex<Option<Snapshot>>>,
}

#[zbus::interface(name = "org.g13.studio.Focus")]
impl FocusObject {
    fn update(&self, json: String) {
        match serde_json::from_str::<Snapshot>(&json) {
            Ok(s) => *self.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(s),
            Err(e) => log::debug!("foreground: bad message from KWin: {e}"),
        }
    }
}

/// The running KWin script and its latest report.
struct Tracker {
    state: Arc<Mutex<Option<Snapshot>>>,
    /// Keeps the object server alive; unloads the script at the end.
    conn: zbus::blocking::Connection,
    script: std::path::PathBuf,
}

fn start_kwin() -> Result<Tracker, String> {
    let state = Arc::new(Mutex::new(None));
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.serve_at(OBJECT, FocusObject { state: state.clone() }))
        .and_then(|b| b.build())
        .map_err(|e| format!("D-Bus : {e}"))?;
    let me = conn.unique_name().ok_or("D-Bus : pas de nom unique")?.to_string();
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, Into::into);
    let path = dir.join(format!("{PLUGIN}.js"));
    std::fs::write(&path, kwin_script(&me)).map_err(|e| format!("{} : {e}", path.display()))?;

    let scripting = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting")
        .map_err(|e| format!("KWin : {e}"))?;
    // a script left by an earlier run (crash): replace it
    let _: Result<bool, _> = scripting.call("unloadScript", &(PLUGIN,));
    let id: i32 = scripting
        .call("loadScript", &(path.to_string_lossy().as_ref(), PLUGIN))
        .map_err(|e| format!("KWin loadScript : {e}"))?;
    if id < 0 {
        return Err("KWin a refusé le script".into());
    }
    let script = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", format!("/Scripting/Script{id}"), "org.kde.kwin.Script")
        .map_err(|e| format!("KWin : {e}"))?;
    script.call::<_, _, ()>("run", &()).map_err(|e| format!("KWin run : {e}"))?;
    log::info!("foreground: KWin script loaded (id {id})");
    Ok(Tracker { state, conn, script: path })
}

static TRACKER: OnceLock<Result<Tracker, String>> = OnceLock::new();

/// Remove the KWin script (G Nexus stops): it would call a name nobody owns any more.
pub fn shutdown() {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Some(Ok(t)) = TRACKER.get()
        && !DONE.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        if let Ok(p) = zbus::blocking::Proxy::new(&t.conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting") {
            let _: Result<bool, _> = p.call("unloadScript", &(PLUGIN,));
        }
        let _ = std::fs::remove_file(&t.script);
        log::debug!("foreground: KWin script unloaded");
    }
}

/// The process-wide tracker (started on first use; `Err` says why there is none).
fn tracker(backend: Backend) -> &'static Result<Tracker, String> {
    TRACKER.get_or_init(|| match backend {
        Backend::KWin => start_kwin().inspect_err(|e| log::warn!("foreground: {e}")),
        Backend::Unknown => Err("foreground detection outside KDE Plasma".into()),
    })
}

/// `Unsupported` when there is no tracker: that will not change (it is tried once, and
/// the cause logged), so the profile manager turns automatic switching off for good.
fn snapshot(backend: Backend) -> PlatformResult<Option<Snapshot>> {
    match (tracker(backend), backend) {
        (Ok(t), _) => Ok(t.state.lock().unwrap_or_else(|e| e.into_inner()).clone()),
        (Err(_), Backend::Unknown) => Err(PlatformError::Unsupported("foreground detection outside KDE Plasma")),
        (Err(_), Backend::KWin) => Err(PlatformError::Unsupported("foreground detection (KWin script failed)")),
    }
}

/// Start the KWin script now if it is not running yet (the overlay needs it even with
/// automatic profile switching off). Does nothing outside KDE Plasma.
pub fn ensure_started() {
    let _ = tracker(detect_backend());
}

/// Windows shown in the task bar (with a title, as on Windows), one per executable.
pub fn running_apps() -> Vec<ForegroundApp> {
    let Ok(Some(s)) = snapshot(detect_backend()) else { return Vec::new() };
    let mut apps: Vec<ForegroundApp> = Vec::new();
    for w in s.windows.iter().filter(|w| !w.title.trim().is_empty()) {
        let app = app_of(w);
        if !app.process_name.is_empty() && !apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(&app.process_name)) {
            apps.push(app);
        }
    }
    apps
}

fn detect_backend() -> Backend {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_uppercase();
    if desktop.split(':').any(|d| d == "KDE") { Backend::KWin } else { Backend::Unknown }
}

pub struct LinuxForegroundWatcher {
    backend: Backend,
}

impl LinuxForegroundWatcher {
    /// Choose a back-end from the session environment.
    pub fn detect() -> Self {
        let backend = detect_backend();
        log::debug!("foreground watcher back-end: {backend:?}");
        Self { backend }
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }
}

impl ForegroundWatcher for LinuxForegroundWatcher {
    fn current(&mut self) -> PlatformResult<Option<ForegroundApp>> {
        Ok(snapshot(self.backend)?.and_then(|s| s.active).map(|w| app_of(&w)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kwin_message_parses() {
        let s: Snapshot = serde_json::from_str(
            r#"{"active":{"pid":1,"class":"org.kde.konsole","title":"T"},"windows":[{"pid":1,"class":"a","title":""}]}"#,
        )
        .unwrap();
        assert_eq!(s.active.unwrap().class, "org.kde.konsole");
        assert_eq!(s.windows.len(), 1);
        let none: Snapshot = serde_json::from_str(r#"{"active":null,"windows":[]}"#).unwrap();
        assert!(none.active.is_none());
    }

    #[test]
    fn script_recognises_the_overlay_title() {
        let js = kwin_script(":1.42");
        assert!(js.contains(r"const m = /^G Nexus — overlay \[(\w+)\]$/.exec(String(w.caption));"), "{js}");
        assert!(js.contains(&format!("if (w.pid !== {}) return;", std::process::id())));
        assert!(js.contains(r#"callDBus(":1.42", "/org/g13/studio/Focus""#));
        assert_eq!(overlay_title("right"), "G Nexus — overlay [right]");
    }

    #[test]
    fn our_own_process_has_a_name() {
        let me = process_name(i64::from(std::process::id())).unwrap();
        assert!(!me.is_empty());
        assert_eq!(process_name(0), None);
    }
}
