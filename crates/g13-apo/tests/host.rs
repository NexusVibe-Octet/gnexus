//! The effect run as the audio engine would (no registration): format check, lock,
//! processing of a 4 kHz sine, and settings changed while it plays.
#![cfg(windows)]

use std::time::Duration;

use g13_dsp::EqSettings;
use windows::Win32::Media::Audio::Apo::*;
use windows::Win32::Media::Audio::WAVEFORMATEX;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows_core::{BOOL, Interface, Ref, Result, implement};

/// 48 kHz stereo float, as the engine describes it.
#[implement(IAudioMediaType)]
struct Float48;

impl IAudioMediaType_Impl for Float48_Impl {
    fn IsCompressedFormat(&self) -> Result<BOOL> {
        Ok(false.into())
    }
    fn IsEqual(&self, _other: Ref<IAudioMediaType>) -> Result<u32> {
        Ok(0)
    }
    fn GetAudioFormat(&self) -> *mut WAVEFORMATEX {
        std::ptr::null_mut()
    }
    fn GetUncompressedAudioFormat(&self, f: *mut UNCOMPRESSEDAUDIOFORMAT) -> Result<()> {
        // SAFETY: the caller passes a valid struct.
        unsafe {
            *f = UNCOMPRESSEDAUDIOFORMAT {
                guidFormatType: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
                dwSamplesPerFrame: 2,
                dwBytesPerSampleContainer: 4,
                dwValidBitsPerSample: 32,
                fFramesPerSecond: 48000.0,
                dwChannelMask: 3,
            };
        }
        Ok(())
    }
}

/// Peak level (dB) of one second of a 4 kHz sine through the effect (last half).
fn level(rt: &IAudioProcessingObjectRT, t0: &mut usize) -> f32 {
    let mut peak = 0f32;
    for block in 0..100 {
        let mut buf: Vec<f32> = (0..480 * 2)
            .map(|i| {
                let n = *t0 + i / 2;
                (2.0 * std::f32::consts::PI * 4000.0 * n as f32 / 48000.0).sin() * 0.25
            })
            .collect();
        *t0 += 480;
        let mut prop = APO_CONNECTION_PROPERTY {
            pBuffer: buf.as_mut_ptr() as usize,
            u32ValidFrameCount: 480,
            u32BufferFlags: BUFFER_VALID,
            u32Signature: 0,
        };
        let pin: *const APO_CONNECTION_PROPERTY = &prop;
        let mut pout: *mut APO_CONNECTION_PROPERTY = &mut prop;
        // SAFETY: one in-place connection of 480 stereo frames.
        unsafe { rt.APOProcess(1, &pin, 1, &mut pout) };
        if block >= 50 {
            peak = buf.iter().fold(peak, |m, x| m.max(x.abs()));
        }
    }
    20.0 * (peak / 0.25).log10()
}

#[test]
fn equalizes_and_follows_the_settings_file() {
    let file = std::env::temp_dir().join(format!("g13-apo-test-{}.bin", std::process::id()));
    let mut s = EqSettings::default();
    s.bands[7].1 = -12.0; // 4 kHz
    std::fs::write(&file, s.to_bytes()).unwrap();
    // SAFETY: single-threaded test set-up.
    unsafe { std::env::set_var("G13_APO_SETTINGS", &file) };

    let unk = g13_apo::create_instance();
    let apo: IAudioProcessingObject = unk.cast().unwrap();
    let cfg: IAudioProcessingObjectConfiguration = unk.cast().unwrap();
    let rt: IAudioProcessingObjectRT = unk.cast().unwrap();
    let init = APOInitBaseStruct { cbSize: std::mem::size_of::<APOInitBaseStruct>() as u32, clsid: g13_apo::CLSID };
    let mt: IAudioMediaType = Float48.into();
    // SAFETY: COM calls with valid arguments.
    unsafe {
        apo.Initialize(std::slice::from_raw_parts(&init as *const _ as *const u8, init.cbSize as usize)).unwrap();
        apo.IsInputFormatSupported(None, &mt).unwrap();
        let props = apo.GetRegistrationProperties().unwrap();
        assert_eq!((*props).clsid, g13_apo::CLSID);
        windows::Win32::System::Com::CoTaskMemFree(Some(props as *const _));
        let desc = APO_CONNECTION_DESCRIPTOR {
            Type: APO_CONNECTION_BUFFER_TYPE_EXTERNAL,
            pBuffer: 0,
            u32MaxFrameCount: 480,
            pFormat: std::mem::ManuallyDrop::new(Some(mt.clone())),
            u32Signature: 0,
        };
        let p: *const APO_CONNECTION_DESCRIPTOR = &desc;
        cfg.LockForProcess(&[p], &[p]).unwrap();
        assert_eq!(apo.GetInputChannelCount().unwrap(), 2);
    }
    std::thread::sleep(Duration::from_millis(400));
    let mut t = 0;
    let cut = level(&rt, &mut t);
    assert!((cut + 12.0).abs() < 1.0, "4 kHz cut: {cut} dB");

    // settings changed while playing: flat again
    std::fs::write(&file, EqSettings::default().to_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let flat = level(&rt, &mut t);
    assert!(flat.abs() < 0.2, "flat: {flat} dB");

    // SAFETY: COM call.
    unsafe { cfg.UnlockForProcess().unwrap() };
    let _ = std::fs::remove_file(&file);
}
