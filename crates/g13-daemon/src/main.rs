//! `g13d` — Logitech G13 daemon (headless; the GUI embeds the same runtime).
//!
//! ```text
//! g13d [--dry-run] [--poll] [--profiles <dir>] [--no-auto] [-v|-vv]
//!   --dry-run          log the keys/clicks that would be injected instead of sending them
//!   --poll             read input with GetInputReport polling instead of interrupt reads
//!   --profiles <dir>   profile directory (default: <config dir>/g13d/profiles)
//!   --no-auto          no automatic profile switching on focus change
//!   -v / -vv           debug / trace logging (or set RUST_LOG)
//! ```

use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;

use g13_daemon::runtime::{Runtime, RuntimeOptions};

const USAGE: &str = "usage: g13d [--dry-run] [--poll] [--profiles <dir>] [--no-auto] [-v|-vv]";

fn parse_args() -> Result<(RuntimeOptions, u8), String> {
    let mut o = RuntimeOptions::default();
    let mut verbosity = 0;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry-run" => o.dry_run = true,
            "--poll" => o.poll = true,
            "--profiles" => o.profiles_dir = Some(it.next().ok_or("--profiles needs a directory")?.into()),
            "--no-auto" => o.auto_switch = false,
            "-v" => verbosity = verbosity.max(1),
            "-vv" => verbosity = 2,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok((o, verbosity))
}

fn main() -> ExitCode {
    let (opts, verbosity) = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let default_level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp_millis()
        .init();

    let (mut runtime, ui) = match Runtime::start(opts) {
        Ok(r) => r,
        Err(e) => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    // No UI here: dropping the receiver unsubscribes it (no unbounded queue).
    drop(ui);
    let shutdown = runtime.shutdown_flag();
    if let Err(e) = ctrlc::set_handler(move || {
        log::info!("stopping…");
        shutdown.store(true, Ordering::Relaxed);
    }) {
        log::warn!("could not install Ctrl+C handler: {e}");
    }
    log::info!("g13d started");

    while runtime.is_running() {
        std::thread::sleep(Duration::from_millis(50));
    }
    runtime.stop();
    log::info!("bye");
    ExitCode::SUCCESS
}
