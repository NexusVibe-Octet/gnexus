//! Native Windows back-ends (Win32 via `windows-sys`).

pub mod audio;
pub mod elevate;
pub mod foreground;
pub mod gamepad;
pub mod injector;
pub mod media;
pub mod recorder;

/// Run this process at high priority. A game that keeps every core busy made Windows
/// hold back the G13 threads at normal priority: keys waited 6 ms in the median and
/// over 60 ms one time in ten (up to 0.7 s); at high priority, 0.3 ms and 0.5 ms
/// (measured in game). G Nexus itself uses well under 1 % of the CPU.
pub fn raise_priority() {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, HIGH_PRIORITY_CLASS, SetPriorityClass};
    // SAFETY: the pseudo-handle of the current process needs no closing.
    if unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) } == 0 {
        log::warn!("could not raise the process priority");
    }
}
