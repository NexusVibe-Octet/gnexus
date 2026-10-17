//! Volume of an audio output (Core Audio: `IMMDeviceEnumerator` →
//! `IAudioEndpointVolume`), to show and set a headset's volume in G13 Studio.
//! The output is found by name: its friendly name contains the headset's
//! ("Haut-parleurs (PRO X Wireless Gaming Headset)").

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eRender};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ};
use windows::Win32::System::Variant::VT_LPWSTR;

use crate::platform::{OutputVolume, PlatformError, PlatformResult};

fn err(e: windows::core::Error) -> PlatformError {
    PlatformError::Os(e.message())
}

/// Active playback device whose name contains `name` (case-insensitive), with its name.
fn find(name: &str) -> PlatformResult<Option<(IMMDevice, String)>> {
    let want = name.to_lowercase();
    // SAFETY: COM calls on this thread (initialised here; repeated calls are harmless).
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(err)?;
        let all = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(err)?;
        for i in 0..all.GetCount().map_err(err)? {
            let Ok(dev) = all.Item(i) else { continue };
            let Ok(store) = dev.OpenPropertyStore(STGM_READ) else { continue };
            let Ok(mut v) = store.GetValue(&PKEY_Device_FriendlyName) else { continue };
            let inner = &v.Anonymous.Anonymous;
            let label = if inner.vt == VT_LPWSTR { inner.Anonymous.pwszVal.to_string().unwrap_or_default() } else { String::new() };
            let _ = PropVariantClear(&mut v);
            if label.to_lowercase().contains(&want) {
                return Ok(Some((dev, label)));
            }
        }
    }
    Ok(None)
}

fn endpoint(dev: &IMMDevice) -> PlatformResult<IAudioEndpointVolume> {
    // SAFETY: plain COM activation of the device's volume interface.
    unsafe { dev.Activate(CLSCTX_ALL, None).map_err(err) }
}

pub fn output_volume(name: &str) -> PlatformResult<Option<OutputVolume>> {
    let Some((dev, device)) = find(name)? else { return Ok(None) };
    let ep = endpoint(&dev)?;
    // SAFETY: getters of the endpoint volume.
    let (level, muted) = unsafe { (ep.GetMasterVolumeLevelScalar().map_err(err)?, ep.GetMute().map_err(err)?.as_bool()) };
    Ok(Some(OutputVolume { device, volume: (level * 100.0).round() as u8, muted }))
}

pub fn set_output_volume(name: &str, volume: Option<u8>, mute: Option<bool>) -> PlatformResult {
    let Some((dev, _)) = find(name)? else { return Err(PlatformError::Os("sortie audio du casque introuvable".into())) };
    let ep = endpoint(&dev)?;
    // SAFETY: setters of the endpoint volume (no event context).
    unsafe {
        if let Some(v) = volume {
            ep.SetMasterVolumeLevelScalar(f32::from(v.min(100)) / 100.0, std::ptr::null()).map_err(err)?;
        }
        if let Some(m) = mute {
            ep.SetMute(m, std::ptr::null()).map_err(err)?;
        }
    }
    Ok(())
}

/// Registry key of a playback device (`{guid}` under `MMDevices\Audio\Render`).
pub fn endpoint_key(name: &str) -> PlatformResult<Option<String>> {
    let Some((dev, _)) = find(name)? else { return Ok(None) };
    // SAFETY: the id string is owned by COM; copied, then freed.
    let id = unsafe {
        let p = dev.GetId().map_err(err)?;
        let s = p.to_string().unwrap_or_default();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        s
    };
    Ok(id.rsplit('.').next().map(str::to_string))
}

fn fx_path(key: &str) -> Vec<u16> {
    format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{key}\FxProperties")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// A binary value of the device's audio effect settings (`FxProperties`).
pub fn fx_value(key: &str, value: &str) -> PlatformResult<Option<Vec<u8>>> {
    use windows_sys::Win32::System::Registry::{HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, RegCloseKey, RegOpenKeyExW, RegQueryValueExW};
    let path = fx_path(key);
    let name: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    // SAFETY: plain registry calls with valid buffers; the key is closed.
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_QUERY_VALUE, &mut h) != 0 {
            return Ok(None);
        }
        let mut buf = vec![0u8; 4096];
        let mut len = buf.len() as u32;
        let r = RegQueryValueExW(h, name.as_ptr(), std::ptr::null(), std::ptr::null_mut(), buf.as_mut_ptr(), &mut len);
        RegCloseKey(h);
        if r != 0 {
            return Ok(None);
        }
        buf.truncate(len as usize);
        Ok(Some(buf))
    }
}

/// Write that value (users may: the audio driver's installer allows it on this key).
pub fn set_fx_value(key: &str, value: &str, data: &[u8]) -> PlatformResult {
    use windows_sys::Win32::System::Registry::{HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_BINARY, RegCloseKey, RegOpenKeyExW, RegSetValueExW};
    let path = fx_path(key);
    let name: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    // SAFETY: plain registry calls with valid buffers; the key is closed.
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        let r = RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_SET_VALUE, &mut h);
        if r != 0 {
            return Err(PlatformError::Os(format!("réglages audio du casque : accès refusé ({r})")));
        }
        let r = RegSetValueExW(h, name.as_ptr(), 0, REG_BINARY, data.as_ptr(), data.len() as u32);
        RegCloseKey(h);
        if r != 0 {
            return Err(PlatformError::Os(format!("réglages audio du casque : écriture refusée ({r})")));
        }
    }
    Ok(())
}

/// Play soft white noise straight to a playback device (shared mode: through its audio
/// effects) for `seconds`; `tick(t)` is called about every 10 ms (tests).
pub fn play_noise(name: &str, seconds: f32, amplitude: f32, tick: &mut dyn FnMut(f32)) -> PlatformResult {
    use windows::Win32::Media::Audio::{AUDCLNT_SHAREMODE_SHARED, IAudioClient, IAudioRenderClient};
    let Some((dev, _)) = find(name)? else { return Err(PlatformError::Os("sortie introuvable".into())) };
    // SAFETY: WASAPI shared-mode rendering of 32-bit float samples (the mix format).
    unsafe {
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).map_err(err)?;
        let fmt = client.GetMixFormat().map_err(err)?;
        let (channels, rate, bits) = ((*fmt).nChannels as usize, (*fmt).nSamplesPerSec as f32, (*fmt).wBitsPerSample);
        if bits != 32 {
            return Err(PlatformError::Os(format!("format {bits} bits non géré")));
        }
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 2_000_000, 0, fmt, None).map_err(err)?;
        let frames = client.GetBufferSize().map_err(err)?;
        let render: IAudioRenderClient = client.GetService().map_err(err)?;
        client.Start().map_err(err)?;
        let mut seed: u32 = 0x1234_5678;
        let mut done = 0f32;
        while done < seconds {
            let pad = client.GetCurrentPadding().map_err(err)?;
            let n = frames - pad;
            if n > 0 {
                let p = render.GetBuffer(n).map_err(err)? as *mut f32;
                for i in 0..(n as usize * channels) {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    *p.add(i) = ((seed as f32 / u32::MAX as f32) * 2.0 - 1.0) * amplitude;
                }
                render.ReleaseBuffer(n, 0).map_err(err)?;
                done += n as f32 / rate;
            }
            tick(done);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = client.Stop();
    }
    Ok(())
}

/// Equalizer settings of Logitech's audio effect ("HX2E", installed with G HUB's driver)
/// on that output, read live by the effect: `u32 ×2`, then 10 band frequencies (Hz),
/// 10 gains (dB) and 10 Q, as `f32`, then fields we leave alone.
const HX2E_EQ: &str = "{42d67d07-c2b8-47f7-8783-109668632ec6},0";
const HX2E_BANDS: usize = 10;

fn f32_at(b: &[u8], i: usize) -> f32 {
    f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

pub fn output_eq(name: &str) -> PlatformResult<Option<crate::platform::OutputEq>> {
    let Some(key) = endpoint_key(name)? else { return Ok(None) };
    let Some(b) = fx_value(&key, HX2E_EQ)? else { return Ok(None) };
    if b.len() < 8 + 12 * HX2E_BANDS {
        return Ok(None);
    }
    let freqs = (0..HX2E_BANDS).map(|i| f32_at(&b, 8 + 4 * i).round() as u32).collect();
    let gains = (0..HX2E_BANDS).map(|i| f32_at(&b, 8 + 4 * (HX2E_BANDS + i))).collect();
    Ok(Some(crate::platform::OutputEq { freqs, gains, min_db: -12.0, max_db: 12.0 }))
}

pub fn set_output_eq(name: &str, gains: &[f32]) -> PlatformResult {
    let key = endpoint_key(name)?.ok_or_else(|| PlatformError::Os("sortie audio du casque introuvable".into()))?;
    let mut b = fx_value(&key, HX2E_EQ)?
        .ok_or_else(|| PlatformError::Os("effet audio Logitech absent (installé avec le pilote de G HUB)".into()))?;
    if b.len() < 8 + 12 * HX2E_BANDS || gains.len() != HX2E_BANDS {
        return Err(PlatformError::Os("réglage d'égaliseur inattendu".into()));
    }
    for (i, g) in gains.iter().enumerate() {
        let at = 8 + 4 * (HX2E_BANDS + i);
        b[at..at + 4].copy_from_slice(&g.clamp(-12.0, 12.0).to_le_bytes());
    }
    set_fx_value(&key, HX2E_EQ, &b)
}
