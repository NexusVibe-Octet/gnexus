//! Create the installed effect the way the audio engine does (COM, in-process),
//! and call its first methods: tells where the engine gives up.

#[cfg(windows)]
fn main() {
    use windows::Win32::Media::Audio::Apo::{IAudioProcessingObject, IAudioSystemEffects2};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
    use windows_core::Interface;
    // SAFETY: plain COM calls.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        match CoCreateInstance::<_, IAudioProcessingObject>(&g13_apo::CLSID, None, CLSCTX_INPROC_SERVER) {
            Ok(apo) => {
                println!("CoCreateInstance : ok");
                match apo.GetRegistrationProperties() {
                    Ok(p) => println!("GetRegistrationProperties : ok, flags {:?}, interfaces {}", (*p).Flags, (*p).u32NumAPOInterfaces),
                    Err(e) => println!("GetRegistrationProperties : {e}"),
                }
                println!("IAudioSystemEffects2 : {:?}", apo.cast::<IAudioSystemEffects2>().map(|_| "ok"));
            }
            Err(e) => println!("CoCreateInstance : {e} (0x{:08X})", e.code().0 as u32),
        }
    }
}

#[cfg(not(windows))]
fn main() {}
