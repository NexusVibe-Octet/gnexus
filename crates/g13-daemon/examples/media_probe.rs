//! Checks the media and volume back-ends: prints what is playing, sends play / pause,
//! and moves the volume of an output by one step, then puts everything back.
//!
//! ```text
//! cargo run -p g13-daemon --example media_probe -- "PRO X Wireless"
//! ```

use g13_daemon::platform::{self, MediaCommand, default_media_provider};

fn main() {
    let mut media = default_media_provider();
    println!("en cours : {:#?}", media.now_playing());
    println!("lecture / pause : {:?}", media.send(MediaCommand::PlayPause));
    std::thread::sleep(std::time::Duration::from_millis(300));
    println!("après : {:?}", media.now_playing().map(|i| i.map(|i| i.status)));
    println!("suivant : {:?}", media.send(MediaCommand::Next));

    let Some(name) = std::env::args().nth(1) else { return };
    let before = platform::output_volume(&name);
    println!("volume : {before:?}");
    let Ok(Some(v)) = before else { return };
    let step = if v.volume < 100 { v.volume + 1 } else { v.volume - 1 };
    println!("réglé à {step} : {:?}", platform::set_output_volume(&name, Some(step), Some(!v.muted)));
    println!("relu : {:?}", platform::output_volume(&name));
    println!("remis : {:?}", platform::set_output_volume(&name, Some(v.volume), Some(v.muted)));
    println!("relu : {:?}", platform::output_volume(&name));
}
