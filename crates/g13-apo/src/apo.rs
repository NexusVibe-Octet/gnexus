//! The COM object loaded by the audio engine, and the DLL entry points.
//!
//! The audio engine *aggregates* its effects (COM aggregation: it creates them with an
//! outer object and asks for their `IUnknown`), which `windows-rs` objects cannot do. So
//! the COM layer is written by hand here: a non-delegating `IUnknown` that owns the
//! object, and the interfaces the engine uses, whose `IUnknown` methods go to the outer
//! object. The work itself is done by [`Core`].

use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::mem::offset_of;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use g13_dsp::{EqSettings, Equalizer, MAX_CHANNELS};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_INVALIDARG, E_NOINTERFACE, E_OUTOFMEMORY, E_POINTER, HANDLE, S_FALSE, S_OK};
use windows::Win32::Media::Audio::Apo::{
    APO_CONNECTION_DESCRIPTOR, APO_CONNECTION_PROPERTY, APO_FLAG, APO_FLAG_DEFAULT, APO_FLAG_INPLACE, APO_REG_PROPERTIES,
    APOERR_FORMAT_NOT_SUPPORTED, APOInitBaseStruct, APOInitSystemEffects2, BUFFER_SILENT, IAudioMediaType,
    IAudioProcessingObject, IAudioProcessingObject_Vtbl, IAudioProcessingObjectConfiguration,
    IAudioProcessingObjectConfiguration_Vtbl, IAudioProcessingObjectRT, IAudioProcessingObjectRT_Vtbl, IAudioSystemEffects,
    IAudioSystemEffects_Vtbl, IAudioSystemEffects2, IAudioSystemEffects2_Vtbl, UNCOMPRESSEDAUDIOFORMAT,
};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{CoTaskMemAlloc, IClassFactory, IClassFactory_Impl};
use windows_core::{BOOL, GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, Ref, Result, implement};

pub const CLSID: GUID = GUID::from_u128(0x5F2C9E61_7B1D_4C3A_9E8B_13C0A7D4E2F1);

/// How often the settings file is checked for changes.
const WATCH_EVERY: Duration = Duration::from_millis(150);

// ---------------------------------------------------------------------------------------
// The work
// ---------------------------------------------------------------------------------------

/// Settings shared between the file watcher and the audio thread.
struct Shared {
    settings: Mutex<Option<EqSettings>>,
    /// Bumped at every new settings: the audio thread compares it with what it applied.
    version: AtomicU32,
    stop: AtomicBool,
    /// The audio thread has processed sound (logged by the watcher, not by the audio thread).
    processing: AtomicBool,
}

/// State of the audio thread. Set up in `LockForProcess` and used by `APOProcess`, which the
/// audio engine never runs at the same time.
struct Rt {
    eq: Option<Equalizer>,
    channels: usize,
    rate: f32,
    applied: u32,
}

struct Core {
    shared: Arc<Shared>,
    rt: UnsafeCell<Rt>,
    watcher: Mutex<Option<JoinHandle<()>>>,
}

impl Core {
    fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                settings: Mutex::new(None),
                version: AtomicU32::new(0),
                stop: AtomicBool::new(false),
                processing: AtomicBool::new(false),
            }),
            rt: UnsafeCell::new(Rt { eq: None, channels: 0, rate: 48000.0, applied: u32::MAX }),
            watcher: Mutex::new(None),
        }
    }

    /// SAFETY: only from the engine's configuration calls or `APOProcess` (never concurrent).
    #[allow(clippy::mut_from_ref)]
    unsafe fn rt(&self) -> &mut Rt {
        // SAFETY: see above.
        unsafe { &mut *self.rt.get() }
    }

    /// Read the settings file now, then whenever it changes.
    fn start_watcher(&self) {
        let mut w = self.watcher.lock().unwrap_or_else(|e| e.into_inner());
        if w.is_some() {
            return;
        }
        let shared = self.shared.clone();
        let path = crate::settings_path();
        let h = thread::Builder::new().name("g13-apo-settings".into()).spawn(move || {
            let mut last: Option<SystemTime> = None;
            let mut first = true;
            let mut told = false;
            while !shared.stop.load(Ordering::Relaxed) {
                if !told && shared.processing.load(Ordering::Relaxed) {
                    told = true;
                    crate::log("le son passe par l'effet (APOProcess)");
                }
                let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                if first || modified != last {
                    first = false;
                    last = modified;
                    let s = std::fs::read(&path).ok().and_then(|b| EqSettings::from_bytes(&b));
                    crate::log(&match &s {
                        Some(s) => format!("réglages lus : gains {:?}", s.bands.iter().map(|b| b.1).collect::<Vec<_>>()),
                        None => format!("réglages absents ou invalides : {}", path.display()),
                    });
                    if let Ok(mut g) = shared.settings.lock() {
                        *g = s;
                    }
                    shared.version.fetch_add(1, Ordering::Release);
                }
                thread::sleep(WATCH_EVERY);
            }
        });
        *w = h.ok();
    }

    fn reset(&self) {
        // SAFETY: configuration call.
        let rt = unsafe { self.rt() };
        rt.applied = u32::MAX;
        if rt.eq.is_some() {
            rt.eq = Some(Equalizer::new(rt.rate, rt.channels));
        }
    }

    fn registration(&self) -> std::result::Result<*mut APO_REG_PROPERTIES, HRESULT> {
        crate::log("GetRegistrationProperties");
        // SAFETY: COM allocation returned to (and freed by) the caller.
        let p = unsafe { CoTaskMemAlloc(std::mem::size_of::<APO_REG_PROPERTIES>()) } as *mut APO_REG_PROPERTIES;
        if p.is_null() {
            return Err(E_OUTOFMEMORY);
        }
        let mut r = APO_REG_PROPERTIES {
            clsid: CLSID,
            Flags: APO_FLAG(APO_FLAG_DEFAULT.0 | APO_FLAG_INPLACE.0),
            szFriendlyName: [0; 256],
            szCopyrightInfo: [0; 256],
            u32MajorVersion: 1,
            u32MinorVersion: 0,
            u32MinInputConnections: 1,
            u32MaxInputConnections: 1,
            u32MinOutputConnections: 1,
            u32MaxOutputConnections: 1,
            u32MaxInstances: u32::MAX,
            u32NumAPOInterfaces: 1,
            iidAPOInterfaceList: [IAudioProcessingObject::IID],
        };
        copy_name(&mut r.szFriendlyName, crate::FRIENDLY_NAME);
        copy_name(&mut r.szCopyrightInfo, "G13 Studio");
        // SAFETY: `p` points to enough freshly allocated memory.
        unsafe { p.write(r) };
        Ok(p)
    }

    fn initialize(&self, size: u32, data: *const u8) -> HRESULT {
        if size as usize >= std::mem::size_of::<APOInitBaseStruct>() && data.is_null() {
            return E_INVALIDARG;
        }
        // only probed by the system (sound settings): no need to watch the settings
        let discovery = size as usize == std::mem::size_of::<APOInitSystemEffects2>() && {
            // SAFETY: the engine passes an APOInitSystemEffects2 of that size.
            let init = unsafe { &*(data as *const APOInitSystemEffects2) };
            init.InitializeForDiscoveryOnly.as_bool()
        };
        crate::log(&format!("Initialize ({size} octets){}", if discovery { ", découverte seulement" } else { "" }));
        if !discovery {
            self.start_watcher();
        }
        S_OK
    }

    fn lock(&self, desc: &APO_CONNECTION_DESCRIPTOR) -> HRESULT {
        let format = desc.pFormat.as_ref().and_then(float_format);
        crate::log(&format!("LockForProcess : format {format:?}"));
        let Some((channels, rate)) = format else { return APOERR_FORMAT_NOT_SUPPORTED };
        // SAFETY: configuration call.
        let rt = unsafe { self.rt() };
        rt.channels = channels;
        rt.rate = rate;
        rt.eq = (channels <= MAX_CHANNELS).then(|| Equalizer::new(rate, channels));
        rt.applied = u32::MAX;
        self.start_watcher();
        S_OK
    }

    fn unlock(&self) {
        // SAFETY: configuration call.
        unsafe { self.rt() }.eq = None;
    }

    /// The audio thread: no allocation, no waiting.
    fn process(&self, inp: &APO_CONNECTION_PROPERTY, out: &mut APO_CONNECTION_PROPERTY) {
        // SAFETY: see `Rt`.
        let rt = unsafe { self.rt() };
        let frames = inp.u32ValidFrameCount as usize;
        let n = frames * rt.channels;
        let (src, dst) = (inp.pBuffer as *const f32, out.pBuffer as *mut f32);
        if inp.u32BufferFlags != BUFFER_SILENT && n > 0 && !src.is_null() && !dst.is_null() {
            // SAFETY: the engine's buffers hold `frames` frames of `channels` floats.
            unsafe {
                if !std::ptr::eq(src, dst) {
                    std::ptr::copy_nonoverlapping(src, dst, n);
                }
                if let Some(eq) = rt.eq.as_mut() {
                    let v = self.shared.version.load(Ordering::Acquire);
                    if v != rt.applied
                        && let Ok(g) = self.shared.settings.try_lock()
                    {
                        match g.as_ref() {
                            Some(s) => eq.set(s),
                            None => eq.set(&EqSettings { enabled: false, ..EqSettings::default() }),
                        }
                        rt.applied = v;
                    }
                    eq.process(std::slice::from_raw_parts_mut(dst, n));
                }
            }
        }
        out.u32ValidFrameCount = inp.u32ValidFrameCount;
        out.u32BufferFlags = inp.u32BufferFlags;
        self.shared.processing.store(true, Ordering::Relaxed);
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        crate::log("objet libéré");
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.watcher.get_mut().ok().and_then(Option::take) {
            let _ = h.join();
        }
    }
}

/// (channels, rate) of a 32-bit float format; `None` for anything else.
fn float_format(t: &IAudioMediaType) -> Option<(usize, f32)> {
    let mut f = UNCOMPRESSEDAUDIOFORMAT::default();
    // SAFETY: plain getter filling the struct.
    unsafe { t.GetUncompressedAudioFormat(&mut f).ok()? };
    (f.guidFormatType == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT && f.dwBytesPerSampleContainer == 4)
        .then_some((f.dwSamplesPerFrame as usize, f.fFramesPerSecond))
}

fn copy_name(dst: &mut [u16; 256], s: &str) {
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(255)) {
        *d = c;
    }
}

// ---------------------------------------------------------------------------------------
// COM, by hand (aggregatable)
// ---------------------------------------------------------------------------------------

/// The COM object: one table pointer per interface, then the reference count, the
/// controlling ("outer") unknown and the work.
#[repr(C)]
struct Obj {
    /// Non-delegating `IUnknown`: owns the object.
    unk: *const IUnknown_Vtbl,
    apo: *const IAudioProcessingObject_Vtbl,
    rt: *const IAudioProcessingObjectRT_Vtbl,
    cfg: *const IAudioProcessingObjectConfiguration_Vtbl,
    /// `IAudioSystemEffects2` (and `IAudioSystemEffects`, its base).
    fx: *const IAudioSystemEffects2_Vtbl,
    refs: AtomicU32,
    /// Where the interfaces' `IUnknown` calls go: the engine's object, or our own `unk`.
    /// Not reference-counted (COM aggregation rule).
    outer: *mut c_void,
    core: Core,
}

/// The object from an interface pointer at `offset` in it.
///
/// SAFETY: `this` must be the pointer of the interface stored at `offset`.
unsafe fn obj<'a>(this: *mut c_void, offset: usize) -> &'a Obj {
    // SAFETY: see above.
    unsafe { &*((this as *mut u8).sub(offset) as *const Obj) }
}

/// SAFETY: `outer` is a live `IUnknown`.
unsafe fn outer_vtbl<'a>(outer: *mut c_void) -> &'a IUnknown_Vtbl {
    // SAFETY: an interface pointer points to its table pointer.
    unsafe { &**(outer as *const *const IUnknown_Vtbl) }
}

// non-delegating IUnknown
unsafe extern "system" fn nd_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is our `unk`.
    unsafe {
        let o = obj(this, offset_of!(Obj, unk));
        let iid = *iid;
        let base = o as *const Obj as *mut u8;
        let (ptr, own) = if iid == IUnknown::IID {
            (base.add(offset_of!(Obj, unk)), true)
        } else if iid == IAudioProcessingObject::IID {
            (base.add(offset_of!(Obj, apo)), false)
        } else if iid == IAudioProcessingObjectRT::IID {
            (base.add(offset_of!(Obj, rt)), false)
        } else if iid == IAudioProcessingObjectConfiguration::IID {
            (base.add(offset_of!(Obj, cfg)), false)
        } else if iid == IAudioSystemEffects::IID || iid == IAudioSystemEffects2::IID {
            (base.add(offset_of!(Obj, fx)), false)
        } else {
            *out = std::ptr::null_mut();
            return E_NOINTERFACE;
        };
        *out = ptr as *mut c_void;
        if own {
            nd_add_ref(this);
        } else {
            // interface pointers count on the controlling unknown
            (outer_vtbl(o.outer).AddRef)(o.outer);
        }
        S_OK
    }
}

unsafe extern "system" fn nd_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is our `unk`.
    unsafe { obj(this, offset_of!(Obj, unk)) }.refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "system" fn nd_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is our `unk`; the box is freed by the last release only.
    unsafe {
        let o = obj(this, offset_of!(Obj, unk));
        let left = o.refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            drop(Box::from_raw(o as *const Obj as *mut Obj));
        }
        left
    }
}

/// Delegating `IUnknown` methods for the interface stored in field `$f`.
macro_rules! delegating {
    ($q:ident, $a:ident, $r:ident, $f:ident) => {
        unsafe extern "system" fn $q(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
            // SAFETY: `this` is our interface; the outer unknown is alive while it is.
            unsafe {
                let o = obj(this, offset_of!(Obj, $f));
                (outer_vtbl(o.outer).QueryInterface)(o.outer, iid, out)
            }
        }
        unsafe extern "system" fn $a(this: *mut c_void) -> u32 {
            // SAFETY: as above.
            unsafe {
                let o = obj(this, offset_of!(Obj, $f));
                (outer_vtbl(o.outer).AddRef)(o.outer)
            }
        }
        unsafe extern "system" fn $r(this: *mut c_void) -> u32 {
            // SAFETY: as above.
            unsafe {
                let o = obj(this, offset_of!(Obj, $f));
                (outer_vtbl(o.outer).Release)(o.outer)
            }
        }
    };
}

delegating!(apo_query, apo_add_ref, apo_release, apo);
delegating!(rt_query, rt_add_ref, rt_release, rt);
delegating!(cfg_query, cfg_add_ref, cfg_release, cfg);
delegating!(fx_query, fx_add_ref, fx_release, fx);

/// The work of the object behind interface field `$f`.
macro_rules! core_of {
    ($this:expr, $f:ident) => {
        // SAFETY: `this` is our interface stored in field `$f`.
        &unsafe { obj($this, offset_of!(Obj, $f)) }.core
    };
}

// IAudioProcessingObject
unsafe extern "system" fn apo_reset(this: *mut c_void) -> HRESULT {
    core_of!(this, apo).reset();
    S_OK
}
unsafe extern "system" fn apo_latency(_this: *mut c_void, out: *mut i64) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: valid out pointer.
    unsafe { *out = 0 };
    S_OK
}
unsafe extern "system" fn apo_registration(this: *mut c_void, out: *mut *mut APO_REG_PROPERTIES) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    match core_of!(this, apo).registration() {
        Ok(p) => {
            // SAFETY: valid out pointer.
            unsafe { *out = p };
            S_OK
        }
        Err(e) => e,
    }
}
unsafe extern "system" fn apo_initialize(this: *mut c_void, size: u32, data: *const u8) -> HRESULT {
    core_of!(this, apo).initialize(size, data)
}
unsafe extern "system" fn apo_format(_this: *mut c_void, _opposite: *mut c_void, requested: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `requested` is an IAudioMediaType (or null); the one returned is AddRef'ed.
    unsafe {
        *out = std::ptr::null_mut();
        let Some(t) = IAudioMediaType::from_raw_borrowed(&requested) else { return E_POINTER };
        let f = float_format(t);
        crate::log(&format!("format proposé : {f:?}"));
        match f {
            Some((ch, _)) if (1..=MAX_CHANNELS).contains(&ch) => {
                *out = t.clone().into_raw();
                S_OK
            }
            _ => APOERR_FORMAT_NOT_SUPPORTED,
        }
    }
}
unsafe extern "system" fn apo_channels(this: *mut c_void, out: *mut u32) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    let core = core_of!(this, apo);
    // SAFETY: configuration call; valid out pointer.
    unsafe { *out = core.rt().channels as u32 };
    S_OK
}

// IAudioProcessingObjectRT
unsafe extern "system" fn rt_process(
    this: *mut c_void,
    nin: u32,
    ppin: *const *const APO_CONNECTION_PROPERTY,
    nout: u32,
    ppout: *mut *mut APO_CONNECTION_PROPERTY,
) {
    if nin < 1 || nout < 1 || ppin.is_null() || ppout.is_null() {
        return;
    }
    let core = core_of!(this, rt);
    // SAFETY: the engine passes valid connections.
    unsafe { core.process(&**ppin, &mut **ppout) };
}
unsafe extern "system" fn rt_frames(_this: *mut c_void, n: u32) -> u32 {
    n
}

// IAudioProcessingObjectConfiguration
unsafe extern "system" fn cfg_lock(
    this: *mut c_void,
    nin: u32,
    ppin: *const *const APO_CONNECTION_DESCRIPTOR,
    nout: u32,
    _ppout: *const *const APO_CONNECTION_DESCRIPTOR,
) -> HRESULT {
    if nin < 1 || nout < 1 || ppin.is_null() {
        return E_INVALIDARG;
    }
    let core = core_of!(this, cfg);
    // SAFETY: the engine passes `nin` valid descriptors.
    unsafe { core.lock(&**ppin) }
}
unsafe extern "system" fn cfg_unlock(this: *mut c_void) -> HRESULT {
    core_of!(this, cfg).unlock();
    S_OK
}

// IAudioSystemEffects2
unsafe extern "system" fn fx_effects(_this: *mut c_void, ids: *mut *mut GUID, count: *mut u32, _event: HANDLE) -> HRESULT {
    crate::log("GetEffectsList");
    if ids.is_null() || count.is_null() {
        return E_POINTER;
    }
    // SAFETY: valid out pointers. No effect for Windows' sound settings to switch.
    unsafe {
        *ids = std::ptr::null_mut();
        *count = 0;
    }
    S_OK
}

static UNK_VTBL: IUnknown_Vtbl = IUnknown_Vtbl { QueryInterface: nd_query, AddRef: nd_add_ref, Release: nd_release };
static APO_VTBL: IAudioProcessingObject_Vtbl = IAudioProcessingObject_Vtbl {
    base__: IUnknown_Vtbl { QueryInterface: apo_query, AddRef: apo_add_ref, Release: apo_release },
    Reset: apo_reset,
    GetLatency: apo_latency,
    GetRegistrationProperties: apo_registration,
    Initialize: apo_initialize,
    IsInputFormatSupported: apo_format,
    IsOutputFormatSupported: apo_format,
    GetInputChannelCount: apo_channels,
};
static RT_VTBL: IAudioProcessingObjectRT_Vtbl = IAudioProcessingObjectRT_Vtbl {
    base__: IUnknown_Vtbl { QueryInterface: rt_query, AddRef: rt_add_ref, Release: rt_release },
    APOProcess: rt_process,
    CalcInputFrames: rt_frames,
    CalcOutputFrames: rt_frames,
};
static CFG_VTBL: IAudioProcessingObjectConfiguration_Vtbl = IAudioProcessingObjectConfiguration_Vtbl {
    base__: IUnknown_Vtbl { QueryInterface: cfg_query, AddRef: cfg_add_ref, Release: cfg_release },
    LockForProcess: cfg_lock,
    UnlockForProcess: cfg_unlock,
};
static FX_VTBL: IAudioSystemEffects2_Vtbl = IAudioSystemEffects2_Vtbl {
    base__: IAudioSystemEffects_Vtbl { base__: IUnknown_Vtbl { QueryInterface: fx_query, AddRef: fx_add_ref, Release: fx_release } },
    GetEffectsList: fx_effects,
};

/// Create the object (aggregated if `outer` is not null) and return interface `iid`.
///
/// SAFETY: `outer` is null or a live `IUnknown`; `iid` and `out` are valid.
unsafe fn create(outer: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    // SAFETY: valid pointers (checked).
    unsafe {
        *out = std::ptr::null_mut();
        // an aggregated object first gives its IUnknown (COM rule)
        if !outer.is_null() && *iid != IUnknown::IID {
            return E_NOINTERFACE;
        }
        let b = Box::into_raw(Box::new(Obj {
            unk: &UNK_VTBL,
            apo: &APO_VTBL,
            rt: &RT_VTBL,
            cfg: &CFG_VTBL,
            fx: &FX_VTBL,
            refs: AtomicU32::new(1),
            outer: std::ptr::null_mut(),
            core: Core::new(),
        }));
        let unk = (b as *mut u8).add(offset_of!(Obj, unk)) as *mut c_void;
        (*b).outer = if outer.is_null() { unk } else { outer };
        let hr = nd_query(unk, iid, out);
        nd_release(unk);
        hr
    }
}

/// A new, non-aggregated effect object (test host).
pub fn create_instance() -> IUnknown {
    let mut p = std::ptr::null_mut();
    // SAFETY: no outer object; valid out pointer.
    let hr = unsafe { create(std::ptr::null_mut(), &IUnknown::IID, &mut p) };
    assert!(hr.is_ok() && !p.is_null(), "create_instance");
    // SAFETY: an AddRef'ed IUnknown.
    unsafe { IUnknown::from_raw(p) }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<IUnknown>, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        let outer_raw = outer.as_ref().map_or(std::ptr::null_mut(), |u| u.as_raw());
        // SAFETY: COM arguments.
        let hr = unsafe { create(outer_raw, riid, ppv) };
        // SAFETY: `riid` is valid for the call.
        crate::log(&format!(
            "CreateInstance {:?}{} → 0x{:08X}",
            unsafe { riid.as_ref() },
            if outer_raw.is_null() { "" } else { " (agrégé)" },
            hr.0 as u32
        ));
        hr.ok()
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        crate::log(&format!("LockServer {}", lock.as_bool()));
        Ok(())
    }
}

/// COM entry point: the class factory of [`CLSID`].
///
/// # Safety
/// Called by COM with valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if rclsid.is_null() || ppv.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: valid pointers (checked above).
    unsafe {
        let hr = if *rclsid != CLSID {
            CLASS_E_CLASSNOTAVAILABLE
        } else {
            let f: IClassFactory = Factory.into();
            f.query(riid, ppv)
        };
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        crate::log(&format!("DllGetClassObject {:?} {:?} → 0x{:08X} ({exe})", *rclsid, riid.as_ref(), hr.0 as u32));
        hr
    }
}

/// The audio engine keeps the DLL loaded.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    crate::log("DllCanUnloadNow");
    S_FALSE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the audio engine's outer object: counts references, answers its own
    /// IUnknown and hands every other interface to the inner (aggregated) object.
    #[repr(C)]
    struct Outer {
        vtbl: *const IUnknown_Vtbl,
        refs: AtomicU32,
        inner: *mut c_void,
    }

    unsafe extern "system" fn o_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
        // SAFETY: test object.
        unsafe {
            let o = &*(this as *const Outer);
            if *iid == IUnknown::IID {
                *out = this;
                o_add(this);
                return S_OK;
            }
            (outer_vtbl(o.inner).QueryInterface)(o.inner, iid, out)
        }
    }
    unsafe extern "system" fn o_add(this: *mut c_void) -> u32 {
        // SAFETY: test object.
        unsafe { &*(this as *const Outer) }.refs.fetch_add(1, Ordering::AcqRel) + 1
    }
    unsafe extern "system" fn o_release(this: *mut c_void) -> u32 {
        // SAFETY: test object.
        unsafe { &*(this as *const Outer) }.refs.fetch_sub(1, Ordering::AcqRel) - 1
    }
    static O_VTBL: IUnknown_Vtbl = IUnknown_Vtbl { QueryInterface: o_query, AddRef: o_add, Release: o_release };

    #[test]
    fn aggregated_like_the_audio_engine_does() {
        let mut outer = Outer { vtbl: &O_VTBL, refs: AtomicU32::new(1), inner: std::ptr::null_mut() };
        let op = &mut outer as *mut Outer as *mut c_void;
        // SAFETY: test of the COM layer.
        unsafe {
            let mut inner = std::ptr::null_mut();
            assert_eq!(create(op, &IAudioProcessingObject::IID, &mut inner), E_NOINTERFACE, "IUnknown first");
            assert_eq!(create(op, &IUnknown::IID, &mut inner), S_OK);
            outer.inner = inner;
            // the engine asks the outer object for the APO interfaces
            let mut apo = std::ptr::null_mut();
            assert_eq!(o_query(op, &IAudioProcessingObject::IID, &mut apo), S_OK);
            assert_eq!(outer.refs.load(Ordering::Acquire), 2, "counted on the outer object");
            let apo = IAudioProcessingObject::from_raw(apo);
            let p = apo.GetRegistrationProperties().unwrap();
            assert_eq!((*p).clsid, CLSID);
            windows::Win32::System::Com::CoTaskMemFree(Some(p as *const _));
            // from an interface, QueryInterface goes back to the outer object
            let rt: IAudioProcessingObjectRT = apo.cast().unwrap();
            let _fx: IAudioSystemEffects2 = rt.cast().unwrap();
            drop(rt);
            drop(_fx);
            drop(apo);
            assert_eq!(outer.refs.load(Ordering::Acquire), 1);
            // the outer object releases the inner one: freed
            assert_eq!(nd_release(inner), 0);
        }
    }
}
