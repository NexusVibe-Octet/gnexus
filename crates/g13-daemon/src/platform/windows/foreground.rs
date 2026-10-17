//! Foreground application detection via `GetForegroundWindow`.

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

use crate::platform::{ForegroundApp, ForegroundWatcher, PlatformResult};

#[derive(Debug, Default)]
pub struct Win32ForegroundWatcher {
    /// PID → exe path cache for the last seen process (avoids reopening it every poll).
    last_pid: u32,
    last_exe: Option<String>,
}

impl Win32ForegroundWatcher {
    pub fn new() -> Self {
        Self::default()
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: buffer pointer/len are valid; hwnd may be stale, in which case the call returns 0.
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn process_image_path(pid: u32) -> Option<String> {
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            None
        } else {
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }
}

/// Applications with a normal, visible top-level window (what Alt+Tab shows):
/// no tool windows, no owned dialogs, no cloaked (hidden UWP) windows.
/// One entry per executable, with the title of its first window.
pub fn running_apps() -> Vec<ForegroundApp> {
    use windows_sys::Win32::Foundation::LPARAM;
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, IsWindowVisible, WS_EX_TOOLWINDOW,
    };

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
        // SAFETY: lparam is the &mut Vec passed to EnumWindows below, alive for the call.
        let out = unsafe { &mut *(lparam as *mut Vec<HWND>) };
        out.push(hwnd);
        1
    }

    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `hwnds`, which outlives the call.
    unsafe { EnumWindows(Some(collect), &mut hwnds as *mut Vec<HWND> as LPARAM) };

    let mut apps: Vec<ForegroundApp> = Vec::new();
    for hwnd in hwnds {
        // SAFETY: plain queries on window handles from EnumWindows (stale handles just fail).
        let keep = unsafe {
            let mut cloaked = 0u32;
            let cloak_ok = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED as u32,
                &mut cloaked as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
            ) == 0;
            IsWindowVisible(hwnd) != 0
                && GetWindow(hwnd, GW_OWNER).is_null()
                && (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW) == 0
                && !(cloak_ok && cloaked != 0)
        };
        if !keep {
            continue;
        }
        let title = window_title(hwnd);
        if title.trim().is_empty() {
            continue;
        }
        let mut pid = 0u32;
        // SAFETY: valid out-pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        let Some(path) = process_image_path(pid) else { continue };
        let name = path.rsplit(['\\', '/']).next().unwrap_or_default().to_string();
        if name.is_empty() || apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        apps.push(ForegroundApp { process_name: name, exe_path: Some(path), title, app_id: None });
    }
    apps
}

impl ForegroundWatcher for Win32ForegroundWatcher {
    fn current(&mut self) -> PlatformResult<Option<ForegroundApp>> {
        // SAFETY: no preconditions.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return Ok(None);
        }
        let mut pid = 0u32;
        // SAFETY: hwnd comes from GetForegroundWindow, pid is a valid out-pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            return Ok(None);
        }
        if pid != self.last_pid {
            self.last_pid = pid;
            self.last_exe = process_image_path(pid);
        }
        let exe_path = self.last_exe.clone();
        let process_name = exe_path
            .as_deref()
            .and_then(|p| p.rsplit(['\\', '/']).next())
            .unwrap_or_default()
            .to_string();
        Ok(Some(ForegroundApp {
            process_name,
            exe_path,
            title: window_title(hwnd),
            app_id: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    /// Manual check: `cargo test -p g13-daemon running_apps -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn running_apps() {
        for a in super::running_apps() {
            eprintln!("{:<28} {}", a.process_name, a.title);
        }
    }
}
