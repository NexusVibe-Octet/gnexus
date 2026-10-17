//! The headset thread: sole owner of the headset dongle. Finds it, notices when
//! the headset is switched on or off, keeps a [`HeadsetStatus`] up to date
//! (battery, sidetone, power-off delay, equalizer) and changes them on request.
//! Every change is read back from the headset.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hidapi::HidApi;
use serde::Serialize;

use super::{Equalizer, Headset, battery_percent, find_headsets, product_name};

/// Look for the dongle this often while none is plugged in.
const DISCOVERY: Duration = Duration::from_secs(3);
/// Is the headset on? (the dongle stays there when it is off)
const ONLINE_EVERY: Duration = Duration::from_secs(3);
const BATTERY_EVERY: Duration = Duration::from_secs(30);
/// Windows volume of the headset (it moves with the headset's wheel).
const VOLUME_EVERY: Duration = Duration::from_millis(700);
/// The application in the foreground decides the equalizer profile.
const FOCUS_EVERY: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct HeadsetStatus {
    /// The USB dongle is plugged in.
    pub present: bool,
    /// The headset is on and answers.
    pub connected: bool,
    pub name: String,
    pub battery: Option<u8>,
    pub voltage_mv: Option<u16>,
    pub charging: bool,
    /// Mic heard in the headset, 0..100.
    pub sidetone: Option<u8>,
    /// Automatic power-off after this many minutes without sound (0 = never).
    pub sleep_minutes: Option<u8>,
    pub equalizer: Option<Equalizer>,
    /// G13 Studio's equalizer effect is installed on the headset's output (Windows).
    pub eq_installed: bool,
    /// Equalizer profiles (default one, one per application…), the one applied now, the
    /// default one, the one tested everywhere.
    pub eq_profiles: Vec<super::eq::EqProfile>,
    pub eq_active: usize,
    pub eq_default: usize,
    pub eq_pinned: Option<usize>,
    /// Windows volume of the headset's output (0..100), its mute, its name.
    pub volume: Option<u8>,
    pub muted: bool,
    pub audio_device: Option<String>,
    /// Outcome of the last change, for the GUI.
    pub message: Option<String>,
    pub error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadsetCommand {
    SetSidetone(u8),
    SetSleep(u8),
    SetEqualizer(Vec<i8>),
    /// Windows volume of the headset's output, 0..100.
    SetVolume(u8),
    SetMute(bool),
    /// Read everything again (after installing / removing the equalizer effect).
    Refresh,
    /// Save equalizer profile `index` (`None`: add it); it is applied at once (heard while edited).
    SaveEqProfile { index: Option<usize>, profile: super::eq::EqProfile },
    DeleteEqProfile(usize),
    /// Apply this profile now (chosen in the GUI), until the foreground application changes.
    UseEqProfile(usize),
    SetEqDefault(usize),
    /// "Tester partout": this profile in every application until `None`.
    PinEqProfile(Option<usize>),
}

/// Which equalizer profile is applied, and why.
struct EqState {
    profiles: super::eq::EqProfiles,
    pinned: Option<usize>,
    active: usize,
    /// Foreground executable the profile was chosen for.
    exe: Option<String>,
}

pub struct HeadsetWorker {
    commands: Receiver<HeadsetCommand>,
    publish: Box<dyn Fn(HeadsetStatus) + Send>,
    shutdown: Arc<AtomicBool>,
    focus: crate::mouse::Focus,
    eq: EqState,
}

impl HeadsetWorker {
    pub fn spawn(
        commands: Receiver<HeadsetCommand>,
        publish: impl Fn(HeadsetStatus) + Send + 'static,
        shutdown: Arc<AtomicBool>,
        focus: crate::mouse::Focus,
    ) -> std::io::Result<JoinHandle<()>> {
        let profiles = super::eq::EqProfiles::load();
        let active = profiles.default;
        let mut w = Self {
            commands,
            publish: Box::new(publish),
            shutdown,
            focus,
            eq: EqState { profiles, pinned: None, active, exe: None },
        };
        thread::Builder::new().name("headset".into()).spawn(move || w.run())
    }

    fn stopping(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    fn nap(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.stopping() {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        self.stopping()
    }

    fn run(&mut self) {
        let mut api = loop {
            match HidApi::new() {
                Ok(a) => break a,
                Err(e) => {
                    log::error!("headset: hidapi init failed: {e}");
                    if self.nap(DISCOVERY) {
                        return;
                    }
                }
            }
        };
        let mut announced = false;
        while !self.stopping() {
            let _ = api.refresh_devices();
            match find_headsets(&api).into_iter().next() {
                Some(h) => {
                    log::info!("headset dongle found ({:04X})", h.product_id);
                    announced = false;
                    self.session(h);
                    log::info!("headset dongle gone");
                    (self.publish)(HeadsetStatus::default());
                }
                None if !announced => {
                    announced = true;
                    (self.publish)(HeadsetStatus::default());
                }
                None => {}
            }
            while self.commands.try_recv().is_ok() {}
            if self.nap(DISCOVERY) {
                return;
            }
        }
    }

    /// Everything the headset tells (or only "present" when it is off).
    fn read(h: &mut Headset) -> HeadsetStatus {
        let mut st = HeadsetStatus {
            present: true,
            name: product_name(h.product_id).to_string(),
            ..HeadsetStatus::default()
        };
        // the output's equalizer and volume are Windows settings: there even while the headset is off
        Self::read_volume(h, &mut st);
        Self::read_eq(h, &mut st);
        if !h.resolve() {
            return st;
        }
        st.connected = true;
        if let Some(n) = h.name() {
            st.name = n.trim_end_matches(" Gaming Headset").to_string();
        }
        if let Ok((mv, charging)) = h.battery() {
            st.voltage_mv = Some(mv);
            st.battery = battery_percent(mv);
            st.charging = charging;
        }
        st.sidetone = h.sidetone().ok();
        st.sleep_minutes = h.sleep_minutes().ok();
        st
    }

    /// G13 Studio's equalizer (see [`super::eq`]); the headset's own one (0x8310) takes the
    /// settings but does not change the sound.
    fn read_eq(h: &Headset, st: &mut HeadsetStatus) {
        st.equalizer = Some(super::eq::view(&super::eq::load()));
        st.eq_installed = super::eq::installed(product_name(h.product_id));
    }

    /// Windows volume of the headset's output (found by the headset's name).
    fn read_volume(h: &Headset, st: &mut HeadsetStatus) -> bool {
        let v = crate::platform::output_volume(product_name(h.product_id)).ok().flatten();
        let (volume, muted, device) = match v {
            Some(v) => (Some(v.volume), v.muted, Some(v.device)),
            None => (None, false, None),
        };
        let changed = volume != st.volume || muted != st.muted || device != st.audio_device;
        st.volume = volume;
        st.muted = muted;
        st.audio_device = device;
        changed
    }

    /// The equalizer profiles in the status.
    fn fill_eq(&self, st: &mut HeadsetStatus) {
        st.eq_profiles = self.eq.profiles.profiles.clone();
        st.eq_active = self.eq.active;
        st.eq_default = self.eq.profiles.default;
        st.eq_pinned = self.eq.pinned;
    }

    /// Apply profile `i` (its gains go to the audio effect).
    fn apply_eq(&mut self, i: usize, st: &mut HeadsetStatus) {
        let Some(p) = self.eq.profiles.profiles.get(i) else { return };
        self.eq.active = i;
        match super::eq::save_gains(&p.gains) {
            Ok(s) => st.equalizer = Some(super::eq::view(&s)),
            Err(e) => log::warn!("headset: equalizer: {e}"),
        }
        self.fill_eq(st);
    }

    /// The profile the rules give (tested everywhere, application, default) applied.
    fn apply_chosen(&mut self, st: &mut HeadsetStatus) {
        let i = self.eq.profiles.choose(self.eq.exe.as_deref(), self.eq.pinned);
        self.apply_eq(i, st);
    }

    /// Changes to the equalizer profiles; `Err`: refused.
    fn edit_eq(&mut self, cmd: HeadsetCommand, st: &mut HeadsetStatus) -> Result<(), String> {
        let n = self.eq.profiles.profiles.len();
        match cmd {
            HeadsetCommand::SaveEqProfile { index, profile } => {
                let i = match index {
                    Some(i) if i < n => {
                        self.eq.profiles.profiles[i] = profile;
                        i
                    }
                    Some(_) => return Err("profil inconnu".into()),
                    None if n >= super::eq::MAX_PROFILES => {
                        return Err(format!("{} profils au plus", super::eq::MAX_PROFILES));
                    }
                    None => {
                        self.eq.profiles.profiles.push(profile);
                        n
                    }
                };
                self.eq.profiles.save()?;
                // heard while it is edited
                self.apply_eq(i, st);
            }
            HeadsetCommand::DeleteEqProfile(i) => {
                if i == self.eq.profiles.default {
                    return Err("c'est le profil par défaut : choisissez-en un autre par défaut d'abord".into());
                }
                if i >= n {
                    return Err("profil inconnu".into());
                }
                self.eq.profiles.profiles.remove(i);
                if self.eq.profiles.default > i {
                    self.eq.profiles.default -= 1;
                }
                self.eq.pinned = match self.eq.pinned {
                    Some(p) if p == i => None,
                    Some(p) if p > i => Some(p - 1),
                    p => p,
                };
                self.eq.profiles.save()?;
                self.apply_chosen(st);
            }
            HeadsetCommand::UseEqProfile(i) if i < n => self.apply_eq(i, st),
            HeadsetCommand::SetEqDefault(i) if i < n => {
                self.eq.profiles.default = i;
                self.eq.profiles.save()?;
                self.apply_chosen(st);
            }
            HeadsetCommand::PinEqProfile(p) if p.is_none_or(|p| p < n) => {
                self.eq.pinned = p;
                self.apply_chosen(st);
            }
            _ => return Err("profil inconnu".into()),
        }
        Ok(())
    }

    fn session(&mut self, mut h: Headset) {
        let mut status = Self::read(&mut h);
        self.apply_chosen(&mut status);
        (self.publish)(status.clone());
        let mut focus_at = Instant::now() + FOCUS_EVERY;
        let (mut online_at, mut battery_at) = (Instant::now() + ONLINE_EVERY, Instant::now() + BATTERY_EVERY);
        let mut volume_at = Instant::now() + VOLUME_EVERY;
        loop {
            if self.stopping() {
                return;
            }
            match self.commands.recv_timeout(Duration::from_millis(250)) {
                Ok(HeadsetCommand::Refresh) => {
                    status = Self::read(&mut h);
                    self.apply_chosen(&mut status);
                    (self.publish)(status.clone());
                }
                Ok(
                    cmd @ (HeadsetCommand::SaveEqProfile { .. }
                    | HeadsetCommand::DeleteEqProfile(_)
                    | HeadsetCommand::UseEqProfile(_)
                    | HeadsetCommand::SetEqDefault(_)
                    | HeadsetCommand::PinEqProfile(_)),
                ) => {
                    if let Err(e) = self.edit_eq(cmd, &mut status) {
                        status.message = Some(e);
                        status.error = true;
                    }
                    (self.publish)(status.clone());
                    status.message = None;
                    status.error = false;
                }
                Ok(HeadsetCommand::SetEqualizer(g)) => {
                    match super::eq::save_gains(&g) {
                        Ok(s) => status.equalizer = Some(super::eq::view(&s)),
                        Err(e) => {
                            status.message = Some(e);
                            status.error = true;
                        }
                    }
                    (self.publish)(status.clone());
                    status.message = None;
                    status.error = false;
                }
                Ok(cmd @ (HeadsetCommand::SetVolume(_) | HeadsetCommand::SetMute(_))) => {
                    // Windows volume: works even while the headset is off
                    let (v, m) = match cmd {
                        HeadsetCommand::SetVolume(v) => (Some(v), None),
                        HeadsetCommand::SetMute(m) => (None, Some(m)),
                        _ => (None, None),
                    };
                    if let Err(e) = crate::platform::set_output_volume(product_name(h.product_id), v, m) {
                        status.message = Some(e.detail());
                        status.error = true;
                    }
                    Self::read_volume(&h, &mut status);
                    (self.publish)(status.clone());
                    status.message = None;
                    status.error = false;
                }
                Ok(cmd) => {
                    let r = if !status.connected {
                        Err("le casque est éteint".to_string())
                    } else {
                        match cmd {
                            HeadsetCommand::SetSidetone(v) => h.set_sidetone(v).map(|v| {
                                status.sidetone = Some(v);
                                format!("Retour micro : {v} %")
                            }),
                            HeadsetCommand::SetSleep(m) => h.set_sleep_minutes(m).map(|m| {
                                status.sleep_minutes = Some(m);
                                "Mise en veille enregistrée".to_string()
                            }),
                            _ => Ok(String::new()),
                        }
                    };
                    match r {
                        Ok(m) => {
                            status.message = Some(m);
                            status.error = false;
                        }
                        Err(e) => {
                            log::warn!("headset: {e}");
                            status.message = Some(e);
                            status.error = true;
                        }
                    }
                    (self.publish)(status.clone());
                    status.message = None;
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            if now >= online_at {
                online_at = now + ONLINE_EVERY;
                let on = h.link.call(0, 1, &[0, 0, 0x5A]).is_ok();
                if on != status.connected {
                    log::info!("headset {}", if on { "on" } else { "off" });
                    status = Self::read(&mut h);
                    self.fill_eq(&mut status);
                    (self.publish)(status.clone());
                    battery_at = now + BATTERY_EVERY;
                } else if !on {
                    // the dongle itself may have been unplugged
                    if h.link.dev.get_product_string().is_err() {
                        return;
                    }
                }
            }
            if now >= focus_at {
                focus_at = now + FOCUS_EVERY;
                let exe = self.focus.lock().ok().and_then(|f| f.clone());
                if exe != self.eq.exe {
                    self.eq.exe = exe;
                    let before = self.eq.active;
                    self.apply_chosen(&mut status);
                    if self.eq.active != before {
                        (self.publish)(status.clone());
                    }
                }
            }
            if now >= volume_at {
                volume_at = now + VOLUME_EVERY;
                if Self::read_volume(&h, &mut status) {
                    (self.publish)(status.clone());
                }
            }
            if status.connected && now >= battery_at {
                battery_at = now + BATTERY_EVERY;
                if let Ok((mv, charging)) = h.battery() {
                    let pct = battery_percent(mv);
                    if pct != status.battery || charging != status.charging {
                        status.battery = pct;
                        status.charging = charging;
                        status.voltage_mv = Some(mv);
                        (self.publish)(status.clone());
                    }
                }
            }
        }
    }
}
