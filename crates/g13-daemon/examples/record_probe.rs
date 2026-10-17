//! Linux: checks the macro recorder without anyone touching the keyboard.
//!
//! A test keyboard (uinput, a name of its own) types « W, wait, Q » and clicks; G Nexus's
//! own injector types E at the same time. The recording must hold the test input only
//! (G Nexus's output is left out, as on Windows), with the real delays.
//!
//! ```text
//! cargo run -p g13-daemon --example record_probe
//! ```

#[cfg(target_os = "linux")]
fn main() {
    use std::thread::sleep;
    use std::time::Duration;

    use evdev::uinput::VirtualDevice;
    use evdev::{AttributeSet, EventType, InputEvent, KeyCode as Ev};
    use g13_daemon::keycode::KeyCode;
    use g13_daemon::platform::InputInjector;
    use g13_daemon::platform::linux::injector::UinputInjector;
    use g13_daemon::recording::{StepOptions, to_steps};

    let ours = UinputInjector::new().expect("injecteur G Nexus");
    let mut keys = AttributeSet::<Ev>::new();
    for k in [Ev::KEY_W, Ev::KEY_Q, Ev::BTN_LEFT, Ev::BTN_RIGHT] {
        keys.insert(k);
    }
    let mut test = VirtualDevice::builder()
        .and_then(|b| b.name("record_probe test keyboard").with_keys(&keys))
        .and_then(|b| b.build())
        .expect("clavier de test");
    sleep(Duration::from_millis(800));

    let rec = match g13_daemon::platform::start_recording() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e.detail());
            std::process::exit(1)
        }
    };
    let mut key = |k: Ev, v: i32| test.emit(&[InputEvent::new(EventType::KEY.0, k.0, v)]).unwrap();
    key(Ev::KEY_W, 1);
    ours.key(KeyCode::E, true).unwrap();
    sleep(Duration::from_millis(50));
    key(Ev::KEY_W, 0);
    ours.key(KeyCode::E, false).unwrap();
    sleep(Duration::from_millis(300));
    key(Ev::KEY_Q, 1);
    key(Ev::KEY_Q, 0);
    sleep(Duration::from_millis(40));
    key(Ev::BTN_RIGHT, 1);
    key(Ev::BTN_RIGHT, 0);
    sleep(Duration::from_millis(100));
    let events = rec.stop();
    let steps = to_steps(&events, StepOptions { delays: true, drop_stop_click: false });
    println!("{}", steps.join("\n"));
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux seulement");
}
