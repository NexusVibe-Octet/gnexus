//! Linux: drives the uinput injector through a fixed, harmless sequence, to check it
//! end to end (a window with keyboard focus must be there to receive it).
//!
//! ```text
//! cargo run -p g13-daemon --example uinput_probe
//! ```
//!
//! Sequence: W, Q, A, Z, 1, Shift+W (physical positions: « z a q w & Z » in AZERTY),
//! pointer moves, wheel up / down / right / left, left, right, middle, back, forward clicks.
//!
//! `uinput_probe pad`: plugs the virtual Xbox 360 controller in, moves it, reads it back
//! from its `/dev/input/event*` node (joysticks get `uaccess` from systemd) and unplugs it.
//!
//! `uinput_probe hold`: presses W and dies (SIGKILL) without releasing it: the kernel must
//! release it when the device goes away (no endless auto-repeat).

#[cfg(target_os = "linux")]
fn main() {
    use std::thread::sleep;
    use std::time::Duration;

    use g13_daemon::keycode::{KeyCode, MouseButton};
    use g13_daemon::platform::InputInjector;
    use g13_daemon::platform::linux::injector::UinputInjector;

    let inj = UinputInjector::new().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if std::env::args().nth(1).as_deref() == Some("pad") {
        pad(&inj);
        return;
    }
    let devices = std::fs::read_to_string("/proc/bus/input/devices").unwrap_or_default();
    for block in devices.split("\n\n").filter(|b| b.contains("G Nexus")) {
        println!("{block}\n");
    }
    // let the compositor open the new devices
    sleep(Duration::from_millis(1500));
    if std::env::args().nth(1).as_deref() == Some("hold") {
        inj.key(KeyCode::W, true).unwrap();
        sleep(Duration::from_millis(1500));
        println!("W tenue, arrêt brutal");
        // SAFETY: plain libc call, kills this very process.
        unsafe { libc_kill() };
    }
    let step = Duration::from_millis(60);
    let tap = |k: KeyCode| {
        inj.key(k, true).unwrap();
        sleep(step);
        inj.key(k, false).unwrap();
        sleep(step);
    };
    for k in [KeyCode::W, KeyCode::Q, KeyCode::A, KeyCode::Z, KeyCode::Digit1] {
        tap(k);
    }
    inj.key(KeyCode::LeftShift, true).unwrap();
    sleep(step);
    tap(KeyCode::W);
    inj.key(KeyCode::LeftShift, false).unwrap();
    sleep(step);
    for (dx, dy) in [(40, 0), (0, 40), (-40, 0), (0, -40)] {
        inj.mouse_move(dx, dy).unwrap();
        sleep(step);
    }
    for (d, h) in [(1, false), (-1, false), (1, true), (-1, true)] {
        inj.mouse_wheel(d, h).unwrap();
        sleep(step);
    }
    for b in MouseButton::ALL {
        inj.mouse_button(b, true).unwrap();
        sleep(step);
        inj.mouse_button(b, false).unwrap();
        sleep(step);
    }
    sleep(Duration::from_millis(300));
    println!("séquence envoyée");
}

#[cfg(target_os = "linux")]
fn pad(inj: &g13_daemon::platform::linux::injector::UinputInjector) {
    use std::time::Duration;

    use g13_daemon::platform::{GamepadState, InputInjector, linux::gamepad::GAMEPAD_NAME};

    g13_daemon::platform::virtual_gamepad_available().unwrap();
    inj.gamepad(&GamepadState::default()).unwrap();
    std::thread::sleep(Duration::from_millis(800));
    let Some((path, mut dev)) = evdev::enumerate().find(|(_, d)| d.name() == Some(GAMEPAD_NAME)) else {
        eprintln!("manette virtuelle introuvable dans /dev/input");
        std::process::exit(1)
    };
    println!("{} : {:?} {:04X}:{:04X}", path.display(), dev.name(), dev.input_id().vendor(), dev.input_id().product());
    println!("boutons : {:?}", dev.supported_keys().map(|k| k.iter().collect::<Vec<_>>()));
    println!("axes : {:?}", dev.supported_absolute_axes().map(|a| a.iter().collect::<Vec<_>>()));
    let reader = std::thread::spawn(move || {
        let mut seen = Vec::new();
        while seen.len() < 9 {
            for e in dev.fetch_events().unwrap() {
                if e.event_type() != evdev::EventType::SYNCHRONIZATION {
                    seen.push(format!("{:?}", e.destructure()));
                }
            }
        }
        seen
    });
    std::thread::sleep(Duration::from_millis(200));
    // stick up-right, A + D-pad left, right trigger; then everything back to rest
    let st = GamepadState { left_x: 32767, left_y: 32767, right_trigger: 255, buttons: 0x1000 | 0x0004, ..GamepadState::default() };
    inj.gamepad(&st).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    inj.gamepad(&GamepadState::default()).unwrap();
    for e in reader.join().unwrap() {
        println!("  {e}");
    }
    inj.gamepad_unplug().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let still = evdev::enumerate().any(|(_, d)| d.name() == Some(GAMEPAD_NAME));
    println!("après débranchement, encore présente : {still}");
}

/// SIGKILL to ourselves: no destructor runs, as in a crash.
#[cfg(target_os = "linux")]
unsafe fn libc_kill() {
    unsafe extern "C" {
        fn getpid() -> i32;
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe {
        kill(getpid(), 9);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux seulement");
}
