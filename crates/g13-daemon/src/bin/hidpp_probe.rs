//! Read-only HID++ 2.0 explorer for Logitech mice (G502 X PLUS…).
//!
//! Lists Logitech HID interfaces, finds HID++ 2.0 devices (behind a receiver
//! or wired), prints their features, name, firmware, battery, DPI, report
//! rate, buttons, and saves a copy of the onboard memory (profiles) to
//! `<config>/backups/hidpp-<name>-<date>/` before anything is ever written.
//!
//! Only "get" functions are called: nothing on the device is changed.
//!
//! ```text
//! hidpp-probe              # everything + onboard memory backup
//! hidpp-probe --no-backup  # skip the (slower) memory dump
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use logi_hidpp::features::{self as f, Battery, Control, DpiChoice, FwInfo, OnboardInfo};
use logi_hidpp::{DIRECT, Message, Request};

const LOGITECH: u16 = 0x046D;
const TIMEOUT: Duration = Duration::from_millis(400);

struct Link {
    dev: HidDevice,
    index: u8,
}

impl Link {
    /// Send a request and wait for its own answer (replies of other programs are skipped).
    fn call(&self, feature_index: u8, function: u8, params: &[u8]) -> Result<[u8; 16], String> {
        let req = Request::new(self.index, feature_index, function, params);
        self.dev.write(&req.to_bytes()).map_err(|e| format!("écriture : {e}"))?;
        let deadline = Instant::now() + TIMEOUT;
        let mut buf = [0u8; 64];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("pas de réponse".into());
            }
            let n = self.dev.read_timeout(&mut buf, left.as_millis().max(1) as i32).map_err(|e| format!("lecture : {e}"))?;
            if n == 0 {
                continue;
            }
            let Some(msg) = Message::parse(&buf[..n]) else { continue };
            if !req.matches(&msg) {
                continue;
            }
            return match msg {
                Message::Reply { data, .. } => Ok(data),
                Message::Error { code, .. } => Err(format!("erreur HID++ {code:?}")),
                Message::Error10 { code, .. } => Err(format!("erreur HID++ 1.0 (0x{code:02X})")),
                Message::Other { .. } => continue,
            };
        }
    }

    fn feature(&self, id: u16) -> Option<u8> {
        let d = self.call(0, 0, &id.to_be_bytes()).ok()?;
        f::root_get_feature(&d).map(|r| r.index)
    }
}

fn main() {
    let backup = !std::env::args().any(|a| a == "--no-backup");
    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => return eprintln!("hidapi : {e}"),
    };

    println!("== Interfaces HID Logitech");
    let mut candidates = Vec::new();
    for d in api.device_list().filter(|d| d.vendor_id() == LOGITECH) {
        println!(
            "  {:04X}  if {:>2}  page 0x{:04X} usage 0x{:04X}  {}",
            d.product_id(),
            d.interface_number(),
            d.usage_page(),
            d.usage(),
            d.product_string().unwrap_or("")
        );
        // HID++ long reports: vendor page 0xFF00, usage 2 (Windows shows each collection apart)
        // headsets (PRO X Wireless…): HID++ on the vendor page 0xFF43
        let headset = d.usage_page() == 0xFF43 || (d.product_id() == 0x0ABA && d.usage_page() == 0xFF00);
        if headset || (d.usage_page() == 0xFF00 && (d.usage() == 0x0002 || !cfg!(windows))) {
            candidates.push(d.clone());
        }
    }

    for info in candidates {
        let Ok(mut dev) = info.open_device(&api) else {
            println!("\n{:04X} : ouverture impossible", info.product_id());
            continue;
        };
        println!("\n== {:04X} {} (collection HID++ longue)", info.product_id(), info.product_string().unwrap_or(""));
        for index in [DIRECT, 1, 2, 3, 4, 5, 6] {
            let link = Link { dev, index };
            match link.call(0, 1, &[0, 0, 0x5A]) {
                Ok(d) => {
                    let (major, minor) = f::root_ping(&d);
                    println!("\n-- appareil à l'index 0x{index:02X} : HID++ {major}.{minor}");
                    if major >= 2 {
                        probe(&link, backup);
                    }
                }
                Err(e) => println!("   index 0x{index:02X} : {e}"),
            }
            dev = link.dev;
        }
    }
}

fn probe(l: &Link, backup: bool) {
    // feature table
    let Some(fs) = l.feature(f::FEATURE_SET) else { return println!("   (pas de FeatureSet)") };
    let count = l.call(fs, 0, &[]).map(|d| d[0]).unwrap_or(0);
    println!("   {count} features :");
    let mut ids = Vec::new();
    for i in 0..=count {
        if let Ok(d) = l.call(fs, 1, &[i]) {
            let (id, flags, ver) = f::feature_set_entry(&d);
            let tag = match flags & 0xE0 {
                0 => "",
                x if x & 0x40 != 0 => " (cachée)",
                x if x & 0x20 != 0 => " (ingénierie)",
                _ => " (obsolète)",
            };
            println!("     [{i:2}] 0x{id:04X} v{ver} {}{tag}", f::name(id));
            ids.push((id, i));
        }
    }
    let at = |id: u16| ids.iter().find(|(x, _)| *x == id).map(|(_, i)| *i);

    // headset features: getters only (function 0 of each, frequencies of the equalizer)
    for (id, what) in [(0x8300u16, "retour micro (0x8300) fn0"), (0x1F20, "tension (0x1F20) fn0"), (0x8310, "égaliseur (0x8310) fn0")] {
        if let Some(i) = at(id) {
            println!("   {what} : {:02X?}", l.call(i, 0, &[]));
        }
    }
    if let Some(i) = at(0x1F20) {
        println!("   tension fn1 (veille ?) : {:02X?}", l.call(i, 1, &[]));
    }
    if let Some(i) = at(0x8310) {
        println!("   égaliseur fn1 [00] : {:02X?}", l.call(i, 1, &[0]));
        println!("   égaliseur fn1 [07] : {:02X?}", l.call(i, 1, &[7]));
        println!("   égaliseur fn2 [00] (gains) : {:02X?}", l.call(i, 2, &[0]));
    }
    let mut name = String::from("appareil");
    if let Some(i) = at(f::DEVICE_NAME) {
        let len = l.call(i, 0, &[]).map(|d| d[0]).unwrap_or(0);
        let mut bytes = Vec::new();
        while bytes.len() < len as usize {
            match l.call(i, 1, &[bytes.len() as u8]) {
                Ok(d) => bytes.extend_from_slice(&d[..(len as usize - bytes.len()).min(16)]),
                Err(_) => break,
            }
        }
        name = String::from_utf8_lossy(&bytes).trim_end_matches('\0').to_string();
        println!("   nom : {name}");
    }
    if let Some(i) = at(f::FW_VERSION) {
        let n = l.call(i, 0, &[]).map(|d| d[0]).unwrap_or(0);
        for e in 0..n {
            if let Ok(d) = l.call(i, 1, &[e]) {
                let fw = FwInfo::parse(&d);
                println!("   firmware {} : {}", fw.kind_name(), fw.version());
            }
        }
    }
    if let Some(i) = at(f::UNIFIED_BATTERY) {
        if let Ok(d) = l.call(i, 1, &[]) {
            let b = Battery::parse(&d);
            println!("   batterie : {} % ({}{})", b.percent, b.charging_name(), if b.external_power { ", câble branché" } else { "" });
        }
    } else if let Some(i) = at(f::BATTERY_STATUS)
        && let Ok(d) = l.call(i, 0, &[])
    {
        println!("   batterie : {} %", d[0]);
    }
    if let Some(i) = at(f::ADJUSTABLE_DPI) {
        let sensors = l.call(i, 0, &[]).map(|d| d[0]).unwrap_or(0);
        for s in 0..sensors {
            let mut words = Vec::new();
            if let Ok(d) = l.call(i, 1, &[s]) {
                words.extend(d[1..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])));
            }
            let list: Vec<String> = f::dpi_list(&words)
                .into_iter()
                .map(|c| match c {
                    DpiChoice::Value(v) => v.to_string(),
                    DpiChoice::Range { min, max, step } => format!("{min}–{max} par pas de {step}"),
                })
                .collect();
            let cur = l.call(i, 2, &[s]).map(|d| (u16::from_be_bytes([d[1], d[2]]), u16::from_be_bytes([d[3], d[4]])));
            match cur {
                Ok((dpi, def)) => println!("   capteur {s} : {dpi} DPI (défaut {def}) ; possibles : {}", list.join(", ")),
                Err(e) => println!("   capteur {s} : {e} ; possibles : {}", list.join(", ")),
            }
        }
    }
    if let Some(i) = at(f::EXTENDED_REPORT_RATE) {
        let list = l.call(i, 0, &[]).map(|d| u16::from_be_bytes([d[0], d[1]])).unwrap_or(0);
        let rates: Vec<String> =
            (0..7).filter(|b| list & (1 << b) != 0).filter_map(f::extended_rate_us).map(|us| format!("{} Hz", 1_000_000 / us)).collect();
        let cur = l.call(i, 1, &[]).ok().and_then(|d| f::extended_rate_us(d[0]));
        println!("   fréquence : {} (possibles : {})", cur.map_or("?".into(), |us| format!("{} Hz", 1_000_000 / us)), rates.join(", "));
    } else if let Some(i) = at(f::REPORT_RATE) {
        let list = l.call(i, 0, &[]).map(|d| d[0]).unwrap_or(0);
        let cur = l.call(i, 1, &[]).map(|d| d[0]).unwrap_or(0);
        let rates: Vec<String> = (0..8).filter(|b| list & (1 << b) != 0).map(|b| format!("{} Hz", 1000 / (b + 1))).collect();
        println!("   fréquence : {} Hz (possibles : {})", if cur > 0 { 1000 / cur as u32 } else { 0 }, rates.join(", "));
    }
    if let Some(i) = at(f::REPROG_CONTROLS_V4) {
        let n = l.call(i, 0, &[]).map(|d| d[0]).unwrap_or(0);
        println!("   {n} commandes (boutons) :");
        for k in 0..n {
            if let Ok(d) = l.call(i, 1, &[k]) {
                let c = Control::parse(&d);
                println!(
                    "     cid 0x{:04X} tid 0x{:04X} flags 0x{:02X}{} {}",
                    c.cid,
                    c.tid,
                    c.flags,
                    if c.divertable() { " déroutable" } else { "" },
                    f::control_name(c.cid)
                );
            }
        }
    }
    if let Some(i) = at(f::RGB_EFFECTS)
        && let Ok(d) = l.call(i, 0, &[0xFF, 0x00])
    {
        println!("   RGB (0x8071) info : {}", hex(&d));
    }
    if let Some(i) = at(f::ONBOARD_PROFILES) {
        onboard(l, i, &name, backup);
    }
}

fn onboard(l: &Link, i: u8, name: &str, backup: bool) {
    let Ok(d) = l.call(i, 0, &[]) else { return println!("   mémoire interne : illisible") };
    let info = OnboardInfo::parse(&d);
    println!(
        "   mémoire interne : {} profils (+ {} d'usine), {} boutons, {} secteurs de {} octets (modèle {}, format profil {}, macro {})",
        info.profile_count,
        info.profile_count_oob,
        info.button_count,
        info.sector_count,
        info.sector_size,
        info.memory_model,
        info.profile_format,
        info.macro_format
    );
    if let Ok(d) = l.call(i, 2, &[]) {
        println!("   mode actuel : {}", f::onboard_mode_name(d[0]));
    }
    if let Ok(d) = l.call(i, 4, &[]) {
        println!("   profil courant : {}", u16::from_be_bytes([d[0], d[1]]));
    }
    if let Ok(d) = l.call(i, 11, &[]) {
        println!("   palier DPI courant : {}", d[0]);
    }
    if !backup || info.sector_size < 16 {
        return;
    }

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let dir = g13_daemon::profiles::store::default_dir()
        .and_then(|p| p.parent().map(|p| p.join("backups")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("hidpp-{slug}-{stamp}"));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return println!("   sauvegarde : {}: {e}", dir.display());
    }
    let size = info.sector_size;
    let sectors = (0..u16::from(info.sector_count)).chain((0..info.profile_count_oob as u16).map(f::rom_sector));
    let mut summary = format!("{name}\n{info:?}\n");
    let t0 = Instant::now();
    for sector in sectors {
        let mut data = vec![0u8; size as usize];
        let mut ok = true;
        let mut offset = 0u16;
        while offset < size {
            // the last read may overlap: a read must stay inside the sector
            let at = offset.min(size - 16);
            match l.call(i, 5, &[(sector >> 8) as u8, sector as u8, (at >> 8) as u8, at as u8]) {
                Ok(d) => data[at as usize..at as usize + 16].copy_from_slice(&d),
                Err(e) => {
                    let _ = writeln!(summary, "secteur 0x{sector:04X} : {e} à l'octet {at}");
                    ok = false;
                    break;
                }
            }
            offset += 16;
        }
        if !ok {
            println!("   secteur 0x{sector:04X} : non lu");
            continue;
        }
        let used = data.iter().rposition(|&b| b != 0xFF).map_or(0, |p| p + 1);
        let _ = writeln!(summary, "secteur 0x{sector:04X} : {used} octets utilisés");
        if let Err(e) = std::fs::write(dir.join(format!("sector_{sector:04X}.bin")), &data) {
            println!("   secteur 0x{sector:04X} : {e}");
        }
    }
    let _ = std::fs::write(dir.join("info.txt"), &summary);
    println!("   mémoire sauvegardée dans {} ({:.1} s)", dir.display(), t0.elapsed().as_secs_f32());
    print!("{}", summary.lines().skip(2).map(|s| format!("     {s}\n")).collect::<String>());
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
