//! Macro playback: each running macro gets its own lightweight thread so
//! that delays never block the input loop.
//!
//! Cancellation is immediate (the thread is unparked out of its delay) and a
//! cancelled or finished macro always releases every key / button it left
//! pressed, so a macro can never leave a stuck modifier behind.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use super::action::{Macro, MacroStep};
use crate::keycode::{KeyCode, MouseButton};
use crate::platform::{InputInjector, PlatformResult};

/// Minimum duration of one loop iteration for looping macros without delays,
/// to avoid flooding the OS input queue.
pub const MIN_LOOP_PERIOD: Duration = Duration::from_millis(10);

#[derive(Default)]
struct Shared {
    cancel: AtomicBool,
    finished: AtomicBool,
}

/// Handle on a running macro. Dropping it does **not** stop the macro.
pub struct MacroHandle {
    shared: Arc<Shared>,
    thread: Thread,
    pub name: String,
}

impl MacroHandle {
    /// Request the macro to stop as soon as possible.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Release);
        self.thread.unpark();
    }

    pub fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::Acquire)
    }

    /// Wait (bounded) for the macro thread to finish. Mostly for tests/shutdown.
    pub fn wait(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while !self.is_finished() {
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(1));
        }
        true
    }
}

/// Start playing `mac`; if `looping`, repeat until cancelled.
pub fn spawn(
    injector: Arc<dyn InputInjector>,
    mac: Arc<Macro>,
    looping: bool,
) -> std::io::Result<MacroHandle> {
    let shared = Arc::new(Shared::default());
    let name = mac.name.clone();
    let thread_shared = shared.clone();
    let join = thread::Builder::new()
        .name(format!("g13-macro:{name}"))
        .spawn(move || {
            Player { injector: &*injector, shared: &thread_shared, keys: Vec::new(), buttons: Vec::new() }
                .play(&mac, looping);
            thread_shared.finished.store(true, Ordering::Release);
        })?;
    Ok(MacroHandle { shared, thread: join.thread().clone(), name })
}

struct Player<'a> {
    injector: &'a dyn InputInjector,
    shared: &'a Shared,
    keys: Vec<KeyCode>,
    buttons: Vec<MouseButton>,
}

impl Player<'_> {
    fn cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::Acquire)
    }

    fn play(&mut self, mac: &Macro, looping: bool) {
        'outer: loop {
            let started = Instant::now();
            for step in &mac.steps {
                if self.cancelled() {
                    break 'outer;
                }
                match *step {
                    MacroStep::KeyDown(k) => {
                        report(self.injector.key(k, true));
                        self.keys.push(k);
                    }
                    MacroStep::KeyUp(k) => {
                        report(self.injector.key(k, false));
                        if let Some(i) = self.keys.iter().rposition(|&x| x == k) {
                            self.keys.remove(i);
                        }
                    }
                    MacroStep::MouseDown(b) => {
                        report(self.injector.mouse_button(b, true));
                        self.buttons.push(b);
                    }
                    MacroStep::MouseUp(b) => {
                        report(self.injector.mouse_button(b, false));
                        if let Some(i) = self.buttons.iter().rposition(|&x| x == b) {
                            self.buttons.remove(i);
                        }
                    }
                    MacroStep::Wheel(d) => report(self.injector.mouse_wheel(d, false)),
                    MacroStep::Delay(d) => {
                        if !self.sleep(d) {
                            break 'outer;
                        }
                    }
                }
            }
            if !looping {
                break;
            }
            let elapsed = started.elapsed();
            if elapsed < MIN_LOOP_PERIOD && !self.sleep(MIN_LOOP_PERIOD - elapsed) {
                break;
            }
        }
        self.release_leftovers();
    }

    /// Cancellable sleep. Returns `false` if cancelled.
    fn sleep(&self, d: Duration) -> bool {
        let deadline = Instant::now() + d;
        loop {
            if self.cancelled() {
                return false;
            }
            let now = Instant::now();
            if now >= deadline {
                return true;
            }
            thread::park_timeout(deadline - now);
        }
    }

    fn release_leftovers(&mut self) {
        for b in self.buttons.drain(..).rev() {
            report(self.injector.mouse_button(b, false));
        }
        for k in self.keys.drain(..).rev() {
            report(self.injector.key(k, false));
        }
    }
}

fn report(r: PlatformResult) {
    if let Err(e) = r {
        log::warn!("macro injection failed: {e}");
    }
}
