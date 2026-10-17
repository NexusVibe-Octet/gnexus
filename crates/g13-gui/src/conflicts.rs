//! Logitech software that conflicts with G Nexus.
//!
//! Logitech Gaming Software (`LCore.exe`) drives the G13 itself and captures
//! its interrupt reports: the engine then receives no key at all (seen in
//! phase 1). This module lets the window warn about it and close it on
//! request. G HUB does not support the G13 and was checked to coexist
//! without problem, so it is deliberately not listed (users keep it for their
//! other Logitech devices).

use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};

/// Executable name (lower case) → product name.
const KNOWN: &[(&str, &str)] = &[("lcore.exe", "Logitech Gaming Software")];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub product: &'static str,
    pub process: String,
}

fn scan() -> (System, Vec<(sysinfo::Pid, Conflict)>) {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let found = sys
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            let name = p.name().to_string_lossy().to_lowercase();
            KNOWN
                .iter()
                .find(|(exe, _)| *exe == name)
                .map(|(_, product)| (*pid, Conflict { product, process: p.name().to_string_lossy().into_owned() }))
        })
        .collect();
    (sys, found)
}

/// Conflicting programs currently running (one entry per process).
pub fn running() -> Vec<Conflict> {
    scan().1.into_iter().map(|(_, c)| c).collect()
}

/// Ask every conflicting process to stop. Returns what is still running.
pub fn close_all() -> Vec<Conflict> {
    let (sys, found) = scan();
    for (pid, c) in &found {
        if let Some(p) = sys.process(*pid)
            && !p.kill()
        {
            log::warn!("could not stop {}", c.process);
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    running()
}
