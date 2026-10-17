//! Prints the application in the foreground whenever it changes, for some seconds
//! (default 10), then the applications with a window: checks the foreground detection.
//!
//! ```text
//! cargo run -p g13-daemon --example foreground_probe -- 20
//! ```

use std::time::{Duration, Instant};

use g13_daemon::platform::{default_foreground_watcher, running_apps};

fn main() {
    let secs = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let mut w = default_foreground_watcher();
    let end = Instant::now() + Duration::from_secs(secs);
    let mut last = None;
    while Instant::now() < end {
        let now = match w.current() {
            Ok(Some(a)) => format!("{} | app_id {:?} | {:?}", a.process_name, a.app_id.unwrap_or_default(), a.title),
            Ok(None) => "(aucune)".into(),
            Err(e) => format!("erreur : {e}"),
        };
        if last.as_ref() != Some(&now) {
            println!("premier plan : {now}");
            last = Some(now);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    for a in running_apps() {
        println!("fenêtre : {} — {:?}", a.process_name, a.title);
    }
    g13_daemon::platform::shutdown();
}
