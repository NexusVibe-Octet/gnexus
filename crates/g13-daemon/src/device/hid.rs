//! HID I/O thread with hot-plug support.
//!
//! Life cycle:
//! ```text
//!   ┌──────────────┐  open OK   ┌───────────────────────────┐
//!   │ Disconnected │──────────▶ │ Session: read → track →    │
//!   │ (retry every │            │ emit events; drain cmds    │
//!   │  reconnect_  │ ◀──────────│ (any I/O error ends it)    │
//!   │  interval)   │  error     └───────────────────────────┘
//!   └──────────────┘  → release_all() + Disconnected
//! ```
//! The daemon never panics on unplug: the session ends, held keys are
//! released, and the worker goes back to polling `hidapi` until the G13
//! reappears.
//!
//! LCD images (992 bytes, ~24 ms on the wire) are written by a second thread
//! ("g13-lcd") through its own handle: written here, they would hold back the
//! key reads for that long at every frame. Only the newest image is sent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use std::sync::Mutex;

use g13_core::{BacklightReport, G13_PID, G13_VID, G13InputReport, G13LcdBuffer};
use hidapi::{HidApi, HidDevice, HidError};

use super::tracker::InputTracker;
use super::{DeviceCommand, DeviceEvent};

/// How input reports are obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    /// Blocking reads on the Interrupt IN endpoint (`read_timeout`). Lowest
    /// latency and CPU; the default.
    Interrupt,
    /// Actively request report 0x01 (`HidD_GetInputReport`) at a fixed rate.
    /// This is what the exploration CLI's `monitor` command uses; kept as a
    /// fallback in case the interrupt pipe misbehaves on a given setup.
    Poll,
}

#[derive(Debug, Clone)]
pub struct HidConfig {
    pub read_mode: ReadMode,
    /// Read timeout (Interrupt) or polling period (Poll). Also bounds the
    /// latency of outgoing commands.
    pub io_interval: Duration,
    /// Delay between reconnection attempts while unplugged.
    pub reconnect_interval: Duration,
    /// Raw-unit noise filter for stick events.
    pub stick_hysteresis: u8,
}

impl Default for HidConfig {
    fn default() -> Self {
        Self {
            read_mode: ReadMode::Interrupt,
            io_interval: Duration::from_millis(5),
            reconnect_interval: Duration::from_millis(1000),
            stick_hysteresis: 3,
        }
    }
}

/// Why a session ended.
enum SessionEnd {
    Shutdown,
    EngineGone,
    Io(HidError),
}

pub struct HidWorker {
    cfg: HidConfig,
    events: Sender<DeviceEvent>,
    commands: Receiver<DeviceCommand>,
    shutdown: Arc<AtomicBool>,
    tracker: InputTracker,
}

impl HidWorker {
    /// Spawn the worker thread.
    pub fn spawn(
        cfg: HidConfig,
        events: Sender<DeviceEvent>,
        commands: Receiver<DeviceCommand>,
        shutdown: Arc<AtomicBool>,
    ) -> std::io::Result<JoinHandle<()>> {
        let tracker = InputTracker::new(cfg.stick_hysteresis);
        let mut worker = Self { cfg, events, commands, shutdown, tracker };
        thread::Builder::new().name("g13-hid".into()).spawn(move || worker.run())
    }

    fn stopping(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    fn run(&mut self) {
        let mut api = loop {
            match HidApi::new() {
                Ok(api) => break api,
                Err(e) => {
                    log::error!("hidapi init failed: {e}; retrying");
                    if self.sleep_interruptible(self.cfg.reconnect_interval) {
                        return;
                    }
                }
            }
        };

        let mut announced_missing = false;
        while !self.stopping() {
            match api.open(G13_VID, G13_PID) {
                Ok(dev) => {
                    announced_missing = false;
                    log::info!("G13 connected");
                    if self.events.send(DeviceEvent::Connected).is_err() {
                        return;
                    }
                    // a second handle, for the LCD writer (without it, images are written inline)
                    let lcd = api.open(G13_VID, G13_PID).ok().and_then(LcdWriter::spawn);
                    let end = self.session(&dev, lcd.as_ref());
                    drop(lcd);
                    drop(dev);

                    // Never leave keys stuck down, whatever ended the session.
                    let events = &self.events;
                    self.tracker.release_all(|e| {
                        let _ = events.send(e);
                    });
                    let _ = self.events.send(DeviceEvent::Disconnected);

                    match end {
                        SessionEnd::Shutdown | SessionEnd::EngineGone => return,
                        SessionEnd::Io(e) => log::warn!("G13 disconnected ({e}); waiting for it to come back"),
                    }
                }
                Err(e) => {
                    if !announced_missing {
                        log::info!("G13 not found ({e}); waiting for it to be plugged in");
                        announced_missing = true;
                    }
                }
            }

            // While unplugged, outgoing commands are meaningless: the engine
            // re-sends its full state on the next `Connected`.
            loop {
                match self.commands.try_recv() {
                    Ok(_) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if self.sleep_interruptible(self.cfg.reconnect_interval) {
                return;
            }
            if let Err(e) = api.refresh_devices() {
                log::debug!("hidapi refresh failed: {e}");
            }
        }
    }

    fn session(&mut self, dev: &HidDevice, lcd: Option<&LcdWriter>) -> SessionEnd {
        let timeout_ms = self.cfg.io_interval.as_millis().clamp(1, i32::MAX as u128) as i32;
        let mut buf = [0u8; 64];

        loop {
            if self.stopping() {
                return SessionEnd::Shutdown;
            }

            let read = match self.cfg.read_mode {
                ReadMode::Interrupt => dev.read_timeout(&mut buf, timeout_ms),
                ReadMode::Poll => {
                    buf[0] = G13InputReport::REPORT_ID;
                    let r = dev.get_input_report(&mut buf[..G13InputReport::REPORT_LEN]);
                    thread::sleep(self.cfg.io_interval);
                    r
                }
            };
            match read {
                Ok(0) => {} // timeout, nothing new
                Ok(n) => {
                    if let Some(report) = G13InputReport::from_bytes(&buf[..n]) {
                        let events = &self.events;
                        let mut engine_gone = false;
                        self.tracker.update(&report, |e| {
                            engine_gone |= events.send(e).is_err();
                        });
                        if engine_gone {
                            return SessionEnd::EngineGone;
                        }
                    } else {
                        log::trace!("ignored report: {:02X?}", &buf[..n]);
                    }
                }
                Err(e) => return SessionEnd::Io(e),
            }

            // Execute pending commands between two reads.
            loop {
                match self.commands.try_recv() {
                    Ok(DeviceCommand::DrawLcd(fb)) if lcd.is_some() => lcd.unwrap().send(fb),
                    Ok(cmd) => match execute(dev, &cmd) {
                        Ok(()) => log::debug!("sent {:?}", CommandKind(&cmd)),
                        // A failing feature report usually means the device is gone;
                        // the next read will confirm and end the session.
                        Err(e) => log::warn!("command {:?} failed: {e}", CommandKind(&cmd)),
                    },
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return SessionEnd::EngineGone,
                }
            }
        }
    }

    /// Sleep for `d`, waking early on shutdown. Returns `true` if shutting down.
    fn sleep_interruptible(&self, d: Duration) -> bool {
        let step = Duration::from_millis(50);
        let mut left = d;
        while !left.is_zero() {
            if self.stopping() {
                return true;
            }
            let s = left.min(step);
            thread::sleep(s);
            left -= s;
        }
        self.stopping()
    }
}

/// Writes LCD images on its own thread and handle; the newest image wins.
struct LcdWriter {
    slot: Arc<Mutex<Option<Box<G13LcdBuffer>>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LcdWriter {
    fn spawn(dev: HidDevice) -> Option<Self> {
        let slot: Arc<Mutex<Option<Box<G13LcdBuffer>>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (slot.clone(), stop.clone());
        let thread = thread::Builder::new()
            .name("g13-lcd".into())
            .spawn(move || {
                while !st.load(Ordering::Relaxed) {
                    let frame = s.lock().unwrap().take();
                    match frame {
                        Some(fb) => {
                            // an error means the G13 is gone: the read loop ends the session
                            if let Err(e) = dev.write(fb.as_report()) {
                                log::debug!("LCD write failed: {e}");
                            }
                        }
                        None => thread::park_timeout(Duration::from_millis(200)),
                    }
                }
            })
            .map_err(|e| log::warn!("no LCD thread ({e}): images written inline"))
            .ok()?;
        Some(Self { slot, stop, thread: Some(thread) })
    }

    fn send(&self, fb: Box<G13LcdBuffer>) {
        *self.slot.lock().unwrap() = Some(fb);
        if let Some(t) = &self.thread {
            t.thread().unpark();
        }
    }
}

impl Drop for LcdWriter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

fn execute(dev: &HidDevice, cmd: &DeviceCommand) -> Result<(), HidError> {
    match cmd {
        DeviceCommand::SetMKeyLeds(leds) => dev.send_feature_report(&leds.to_report()),
        DeviceCommand::SetBacklight(color) => {
            dev.send_feature_report(BacklightReport::new(*color).as_bytes())
        }
        DeviceCommand::DrawLcd(fb) => dev.write(fb.as_report()).map(|_| ()),
    }
}

/// Debug helper that avoids dumping 992-byte LCD frames into the log.
struct CommandKind<'a>(&'a DeviceCommand);

impl std::fmt::Debug for CommandKind<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            DeviceCommand::DrawLcd(_) => f.write_str("DrawLcd"),
            other => write!(f, "{other:?}"),
        }
    }
}
