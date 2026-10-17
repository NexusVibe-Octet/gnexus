//! Macro recording with low-level hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) on a
//! thread of their own, with a message loop. Synthetic input (`SendInput`,
//! G13 Studio's own keys and macros) is ignored, and so is pointer motion.

use std::sync::Mutex;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::keycode::{KeyCode, MouseButton, WinKey};
use crate::platform::{InputRecorder, PlatformError, PlatformResult};
use crate::recording::Recorded;

/// The capture in progress (hook procedures have no context pointer).
static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);

struct Capture {
    start: Instant,
    events: Vec<(Duration, Recorded)>,
}

fn push(e: Recorded) {
    if let Ok(mut c) = CAPTURE.lock()
        && let Some(c) = c.as_mut()
        && c.events.len() < 100_000
    {
        let t = c.start.elapsed();
        c.events.push((t, e));
    }
}

/// Key code for a hooked key: its scan code (with the E0 flag), else its virtual key.
fn key_of(k: &KBDLLHOOKSTRUCT) -> Option<KeyCode> {
    let extended = k.flags & LLKHF_EXTENDED != 0;
    let scan = k.scanCode as u16;
    KeyCode::ALL
        .iter()
        .copied()
        .find(|c| matches!(c.win(), WinKey::Scan { code, extended: e } if code == scan && e == extended))
        .or_else(|| KeyCode::ALL.iter().copied().find(|c| matches!(c.win(), WinKey::Vk(v) if u32::from(v) == k.vkCode)))
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_KEYBOARD_LL, lparam points to a KBDLLHOOKSTRUCT.
        let k = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if k.flags & LLKHF_INJECTED == 0 {
            let down = match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
                WM_KEYUP | WM_SYSKEYUP => Some(false),
                _ => None,
            };
            if let (Some(down), Some(key)) = (down, key_of(k)) {
                push(Recorded::Key(key, down));
            }
        }
    }
    // SAFETY: passing the event on, as every hook must.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_MOUSE_LL, lparam points to an MSLLHOOKSTRUCT.
        let m = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        if m.flags & LLMHF_INJECTED == 0 {
            let x = (m.mouseData >> 16) as u16;
            let xbutton = if x == 1 { MouseButton::Back } else { MouseButton::Forward };
            let e = match wparam as u32 {
                WM_LBUTTONDOWN => Some(Recorded::Mouse(MouseButton::Left, true)),
                WM_LBUTTONUP => Some(Recorded::Mouse(MouseButton::Left, false)),
                WM_RBUTTONDOWN => Some(Recorded::Mouse(MouseButton::Right, true)),
                WM_RBUTTONUP => Some(Recorded::Mouse(MouseButton::Right, false)),
                WM_MBUTTONDOWN => Some(Recorded::Mouse(MouseButton::Middle, true)),
                WM_MBUTTONUP => Some(Recorded::Mouse(MouseButton::Middle, false)),
                WM_XBUTTONDOWN => Some(Recorded::Mouse(xbutton, true)),
                WM_XBUTTONUP => Some(Recorded::Mouse(xbutton, false)),
                WM_MOUSEWHEEL => Some(Recorded::Wheel(i32::from(x as i16) / 120)),
                _ => None,
            };
            if let Some(e) = e {
                push(e);
            }
        }
    }
    // SAFETY: passing the event on, as every hook must.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

pub struct HookRecorder {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

pub fn start() -> PlatformResult<Box<dyn InputRecorder>> {
    {
        let mut c = CAPTURE.lock().map_err(|_| PlatformError::Os("enregistrement : verrou".into()))?;
        if c.is_some() {
            return Err(PlatformError::Os("un enregistrement est déjà en cours".into()));
        }
        *c = Some(Capture { start: Instant::now(), events: Vec::new() });
    }
    let (tx, rx) = mpsc::channel::<Result<u32, String>>();
    let thread = std::thread::Builder::new()
        .name("macro-record".into())
        .spawn(move || {
            // SAFETY: hooks are installed and removed on this thread, which pumps messages
            // until WM_QUIT (low-level hooks are called through this thread's message loop).
            unsafe {
                let module = GetModuleHandleW(std::ptr::null());
                let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module, 0);
                let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), module, 0);
                if kb.is_null() || ms.is_null() {
                    let _ = tx.send(Err("impossible d'écouter le clavier et la souris".into()));
                } else {
                    let _ = tx.send(Ok(GetCurrentThreadId()));
                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
                }
                if !kb.is_null() {
                    UnhookWindowsHookEx(kb);
                }
                if !ms.is_null() {
                    UnhookWindowsHookEx(ms);
                }
            }
        })
        .map_err(|e| PlatformError::Os(e.to_string()))?;
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(thread_id)) => Ok(Box::new(HookRecorder { thread_id, thread: Some(thread) })),
        other => {
            if let Ok(mut c) = CAPTURE.lock() {
                *c = None;
            }
            Err(PlatformError::Os(match other {
                Ok(Err(e)) => e,
                _ => "l'écoute du clavier n'a pas démarré".into(),
            }))
        }
    }
}

impl InputRecorder for HookRecorder {
    fn stop(mut self: Box<Self>) -> Vec<(Duration, Recorded)> {
        // SAFETY: plain Win32 call to a thread we started.
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        CAPTURE.lock().ok().and_then(|mut c| c.take()).map(|c| c.events).unwrap_or_default()
    }
}
