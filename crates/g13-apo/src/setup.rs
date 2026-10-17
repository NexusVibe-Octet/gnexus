//! Installing the effect on an output (administrator rights), removing it, and
//! telling whether it is installed (any user).
//!
//! Install:
//! 1. the DLL is copied to `%ProgramFiles%\G13 Studio\g13_apo.dll` (the audio engine runs as
//!    LOCAL SERVICE and cannot read the user's folders);
//! 2. `%ProgramData%\G13 Studio` is created, writable by users (G13 Studio writes the settings);
//! 3. COM class and APO registration under `HKLM\SOFTWARE\Classes`;
//! 4. the output's endpoint effect slot (`PKEY_FX_EndpointEffectClsid`) points to us; what was
//!    there is kept under `HKLM\SOFTWARE\G13 Studio\AudioEffect` and put back on removal. The
//!    other slots (Logitech's effect) are not touched;
//! 5. the Windows audio service is restarted so that the effect is loaded.
//!
//! Windows' protected audio settings are left alone.

use std::path::PathBuf;
use std::process::Command;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, KEY_WRITE, REG_DWORD, REG_MULTI_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows_core::PCWSTR;

use crate::{CLSID_TEXT, FRIENDLY_NAME};

/// Endpoint effect (EFX) of an output, Windows 8.1+ (`{d04e05a6-…},7`).
const PKEY_FX_ENDPOINT: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7";
/// Processing modes the endpoint effect supports (`PKEY_EFX_ProcessingModes_Supported_For_Streaming`):
/// without it the audio engine does not load the effect.
const PKEY_EFX_MODES: &str = "{d3993a3f-99c2-4402-b5ec-a92a0367664b},7";
/// `AUDIO_SIGNALPROCESSINGMODE_DEFAULT`.
const MODE_DEFAULT: &str = "{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}";
/// `IAudioProcessingObject`.
const IID_APO: &str = "{FD7F2B29-24D0-4B5C-B177-592C39F9CA10}";
const OUR_KEY: &str = r"SOFTWARE\G13 Studio\AudioEffect";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn fx_key(endpoint: &str) -> String {
    format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{endpoint}\FxProperties")
}

pub fn dll_path() -> PathBuf {
    let base = std::env::var_os("ProgramFiles").map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from);
    base.join("G13 Studio").join("g13_apo.dll")
}

/// An open HKLM key, closed on drop.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: a key we opened.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

impl Key {
    fn open(path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<Self> {
        let mut h = HKEY::default();
        // SAFETY: valid strings and out pointer.
        let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, PCWSTR(w(path).as_ptr()), None, access, &mut h) };
        (r == ERROR_SUCCESS).then_some(Self(h))
    }

    fn create(path: &str) -> Result<Self, String> {
        let mut h = HKEY::default();
        // SAFETY: valid strings and out pointer.
        let r = unsafe {
            RegCreateKeyExW(HKEY_LOCAL_MACHINE, PCWSTR(w(path).as_ptr()), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None)
        };
        if r == ERROR_SUCCESS { Ok(Self(h)) } else { Err(format!("HKLM\\{path} : {}", r.0)) }
    }

    fn get_string(&self, name: &str) -> Option<String> {
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: valid buffer of `len` bytes.
        let r = unsafe { RegQueryValueExW(self.0, PCWSTR(w(name).as_ptr()), None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len)) };
        if r != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(len as usize / 2);
        Some(String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string())
    }

    fn get_dword(&self, name: &str) -> Option<u32> {
        let mut v = 0u32;
        let mut len = 4u32;
        let n = w(name);
        // SAFETY: valid 4-byte buffer.
        let r = unsafe { RegQueryValueExW(self.0, PCWSTR(n.as_ptr()), None, None, Some(&mut v as *mut u32 as *mut u8), Some(&mut len)) };
        (r == ERROR_SUCCESS).then_some(v)
    }

    fn set_string(&self, name: &str, value: &str) -> Result<(), String> {
        let v = w(value);
        // SAFETY: valid buffer.
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2) };
        let name_buf = w(name);
        let n = if name.is_empty() { PCWSTR::null() } else { PCWSTR(name_buf.as_ptr()) };
        // SAFETY: valid key and buffers.
        let r = unsafe { RegSetValueExW(self.0, n, None, REG_SZ, Some(bytes)) };
        if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("{name} : {}", r.0)) }
    }

    /// REG_MULTI_SZ with one string.
    fn set_multi_string(&self, name: &str, value: &str) -> Result<(), String> {
        let v: Vec<u16> = value.encode_utf16().chain([0, 0]).collect();
        // SAFETY: valid buffer.
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2) };
        // SAFETY: valid key and buffers.
        let r = unsafe { RegSetValueExW(self.0, PCWSTR(w(name).as_ptr()), None, REG_MULTI_SZ, Some(bytes)) };
        if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("{name} : {}", r.0)) }
    }

    fn has_value(&self, name: &str) -> bool {
        // SAFETY: query of the size only.
        unsafe { RegQueryValueExW(self.0, PCWSTR(w(name).as_ptr()), None, None, None, None) == ERROR_SUCCESS }
    }

    fn set_dword(&self, name: &str, value: u32) -> Result<(), String> {
        // SAFETY: valid key and buffer.
        let r = unsafe { RegSetValueExW(self.0, PCWSTR(w(name).as_ptr()), None, REG_DWORD, Some(&value.to_le_bytes())) };
        if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("{name} : {}", r.0)) }
    }

    fn delete_value(&self, name: &str) {
        // SAFETY: valid key and string.
        unsafe {
            let _ = RegDeleteValueW(self.0, PCWSTR(w(name).as_ptr()));
        }
    }
}

fn delete_tree(path: &str) {
    // SAFETY: valid string.
    unsafe {
        let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(w(path).as_ptr()));
    }
}

/// Is our effect the endpoint effect of this output, with its DLL in place? (any user)
pub fn installed(endpoint: &str) -> bool {
    let slot = Key::open(&fx_key(endpoint), KEY_QUERY_VALUE).and_then(|k| k.get_string(PKEY_FX_ENDPOINT));
    slot.is_some_and(|c| c.eq_ignore_ascii_case(CLSID_TEXT)) && dll_path().exists()
}

/// Restart the Windows audio service (the effect is loaded when audio streams start again).
fn restart_audio() -> Result<(), String> {
    let run = |args: &[&str]| Command::new("net").args(args).output().map_err(|e| e.to_string());
    run(&["stop", "audiosrv", "/y"])?;
    let out = run(&["start", "audiosrv"])?;
    // "already started" is fine too
    if out.status.success() || String::from_utf8_lossy(&out.stdout).contains("audiosrv") { Ok(()) } else { Err("service audio non redémarré".into()) }
}

/// Install on `endpoint` (`{guid}` of the output) the DLL `dll` (administrator).
pub fn install(endpoint: &str, dll: &std::path::Path) -> Result<(), String> {
    let target = dll_path();
    if let Some(d) = target.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    if dll != target {
        // the audio engine may hold the old copy: write next to it, then swap
        let tmp = target.with_extension("dll.new");
        std::fs::copy(dll, &tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        if std::fs::rename(&tmp, &target).is_err() {
            let old = target.with_extension("dll.old");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&target, &old);
            std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    let data = crate::settings_path();
    if let Some(d) = data.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        // users write the settings (SID S-1-5-32-545 = Users, whatever the language)
        let _ = Command::new("icacls").arg(d).args(["/grant", "*S-1-5-32-545:(OI)(CI)M", "/Q"]).output();
    }
    let path = target.display().to_string();
    let clsid = Key::create(&format!(r"SOFTWARE\Classes\CLSID\{CLSID_TEXT}"))?;
    clsid.set_string("", FRIENDLY_NAME)?;
    let inproc = Key::create(&format!(r"SOFTWARE\Classes\CLSID\{CLSID_TEXT}\InprocServer32"))?;
    inproc.set_string("", &path)?;
    inproc.set_string("ThreadingModel", "Both")?;
    let apo = Key::create(&format!(r"SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\{CLSID_TEXT}"))?;
    apo.set_string("FriendlyName", FRIENDLY_NAME)?;
    apo.set_string("Copyright", "G13 Studio")?;
    for (k, v) in [
        ("MajorVersion", 1),
        ("MinorVersion", 0),
        ("Flags", 0x0F),
        ("MinInputConnections", 1),
        ("MaxInputConnections", 1),
        ("MinOutputConnections", 1),
        ("MaxOutputConnections", 1),
        ("MaxInstances", u32::MAX),
        ("NumAPOInterfaces", 1),
    ] {
        apo.set_dword(k, v)?;
    }
    apo.set_string("APOInterface0", IID_APO)?;

    // keep what was in the slot (once: a reinstall must not save our own CLSID)
    let fx = Key::open(&fx_key(endpoint), KEY_QUERY_VALUE | KEY_SET_VALUE).ok_or("réglages audio de la sortie inaccessibles")?;
    let ours = Key::create(OUR_KEY)?;
    let mine = Key::open(OUR_KEY, KEY_QUERY_VALUE);
    if mine.as_ref().and_then(|k| k.get_string("Endpoint")).as_deref() != Some(endpoint) {
        let previous = fx.get_string(PKEY_FX_ENDPOINT).filter(|c| !c.eq_ignore_ascii_case(CLSID_TEXT)).unwrap_or_default();
        ours.set_string("Endpoint", endpoint)?;
        ours.set_string("PreviousEffect", &previous)?;
    }
    fx.set_string(PKEY_FX_ENDPOINT, CLSID_TEXT)?;
    // ours to remove on uninstall only if we added it (also on a reinstall over an older version)
    if !fx.has_value(PKEY_EFX_MODES) {
        fx.set_multi_string(PKEY_EFX_MODES, MODE_DEFAULT)?;
        ours.set_dword("SetModes", 1)?;
    }
    restart_audio()
}

/// Remove the effect and put back what was there (administrator).
pub fn uninstall() -> Result<(), String> {
    let saved = Key::open(OUR_KEY, KEY_QUERY_VALUE);
    let endpoint = saved.as_ref().and_then(|k| k.get_string("Endpoint"));
    let previous = saved.as_ref().and_then(|k| k.get_string("PreviousEffect")).unwrap_or_default();
    let set_modes = saved.as_ref().is_some_and(|k| k.get_dword("SetModes") == Some(1));
    drop(saved);
    if let Some(ep) = endpoint
        && let Some(fx) = Key::open(&fx_key(&ep), KEY_QUERY_VALUE | KEY_SET_VALUE)
        && fx.get_string(PKEY_FX_ENDPOINT).is_some_and(|c| c.eq_ignore_ascii_case(CLSID_TEXT))
    {
        if previous.is_empty() {
            fx.delete_value(PKEY_FX_ENDPOINT);
        } else {
            fx.set_string(PKEY_FX_ENDPOINT, &previous)?;
        }
        if set_modes {
            fx.delete_value(PKEY_EFX_MODES);
        }
    }
    delete_tree(&format!(r"SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\{CLSID_TEXT}"));
    delete_tree(&format!(r"SOFTWARE\Classes\CLSID\{CLSID_TEXT}"));
    delete_tree(OUR_KEY);
    restart_audio()?;
    // free once the audio engine has let it go
    let _ = std::fs::remove_file(dll_path());
    let _ = std::fs::remove_file(dll_path().with_extension("dll.old"));
    Ok(())
}
