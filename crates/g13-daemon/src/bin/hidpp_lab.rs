//! Guarded write experiments on a Logitech gaming mouse (G502 X PLUS).
//!
//! ```text
//! hidpp-lab status          # mode, profile, DPI, battery (read only)
//! hidpp-lab test-profile    # write a test profile into a DISABLED slot and switch to it
//! hidpp-lab restore         # put that slot and the previous profile back
//! hidpp-lab dpi-live 1600   # set the current DPI in RAM (not saved in the mouse)
//! ```
//!
//! Writes refuse to run while G HUB is open (it would undo them), only touch a
//! profile slot that the directory marks as disabled, save its original bytes
//! first, and read every written sector back.

use std::path::PathBuf;

use g13_daemon::mouse::{Mouse, find_mice};
use hidapi::HidApi;
use logi_hidpp::features::{self as f, Battery, OnboardInfo};
use logi_hidpp::onboard::{Directory, Led, Profile};

const TEST_COLOR: [u8; 3] = [0x19, 0xE3, 0xCF];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    let api = HidApi::new().unwrap_or_else(|e| fail(&format!("hidapi : {e}")));
    let Some(mut m) = find_mice(&api).into_iter().next() else { fail("aucune souris HID++ avec mémoire interne trouvée") };
    // not G13 Studio's id: both may talk to the mouse at the same time
    m.link.sw_id = 0x0E;
    println!("{} ({:04X}, index 0x{:02X})", m.name, m.product_id, m.link.index);
    let r = match cmd {
        "status" => status(&m),
        "settings" => settings(&m),
        "test-profile" => guard().and_then(|()| test_profile(&m)),
        "restore" => guard().and_then(|()| restore(&m)),
        "select" => guard().and_then(|()| select(&m, args.get(1).and_then(|v| v.parse().ok()))),
        "restore-sector" => guard().and_then(|()| restore_sector(&m, args.get(1).and_then(|v| v.parse().ok()), args.get(2))),
        "dpi-slot" => guard().and_then(|()| dpi_slot(&m, args.get(1).and_then(|v| v.parse().ok()))),
        "rate-live" => guard().and_then(|()| rate_live(&m, args.get(1).and_then(|v| v.parse().ok()))),
        "rgb-info" => rgb_info(&m).map(drop),
        "rgb-live" => guard().and_then(|()| rgb_live(&m, args.get(1).map(String::as_str), args.iter().any(|a| a == "--claim"))),
        "rgb-release" => guard().and_then(|()| rgb_release(&m)),
        "mode" => guard().and_then(|()| set_mode(&m, args.get(1).and_then(|v| v.parse().ok()))),
        "raw" => raw(&m, &args[1..]),
        "spy" => guard().and_then(|()| {
            let mute = args.iter().position(|a| a == "--mute").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok());
            spy(&m, args.get(1).and_then(|v| v.parse().ok()), mute)
        }),
        "gradient" => guard().and_then(|()| gradient(&m, args.get(1), args.get(2))),
        "led" => guard().and_then(|()| one_led(&m, args.get(1).and_then(|v| v.parse().ok()), args.get(2))),
        "dpi-live" => guard().and_then(|()| dpi_live(&m, args.get(1).and_then(|v| v.parse().ok()))),
        other => Err(format!("commande inconnue : {other}")),
    };
    if let Err(e) = r {
        fail(&e);
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("ERREUR : {msg}");
    std::process::exit(1)
}

/// G HUB rewrites the mouse when it runs: refuse to write meanwhile.
fn guard() -> Result<(), String> {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let running: Vec<String> = sys
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_lowercase())
        .filter(|n| n.starts_with("lghub"))
        .collect();
    if running.is_empty() { Ok(()) } else { Err(format!("G HUB tourne encore ({}) : fermez-le d'abord", running.join(", "))) }
}

struct Onboard {
    oi: u8,
    info: OnboardInfo,
}

fn onboard(m: &Mouse) -> Result<Onboard, String> {
    let oi = m.link.feature(f::ONBOARD_PROFILES).ok_or("pas de mémoire interne")?;
    let info = OnboardInfo::parse(&m.link.call(oi, 0, &[])?);
    Ok(Onboard { oi, info })
}

fn status(m: &Mouse) -> Result<(), String> {
    let o = onboard(m)?;
    let l = &m.link;
    let mode = l.call(o.oi, 2, &[])?[0];
    let cur = l.call(o.oi, 4, &[])?;
    let slot = l.call(o.oi, 11, &[])?[0];
    println!("mode : {} ; profil courant : secteur {} ; palier DPI : {slot}", f::onboard_mode_name(mode), u16::from_be_bytes([cur[0], cur[1]]));
    if let Some(i) = l.feature(f::ADJUSTABLE_DPI) {
        let d = l.call(i, 2, &[0])?;
        println!("DPI actuel : {}", u16::from_be_bytes([d[1], d[2]]));
    }
    if let Some(i) = l.feature(f::UNIFIED_BATTERY) {
        let b = Battery::parse(&l.call(i, 1, &[])?);
        println!("batterie : {} % ({})", b.percent, b.charging_name());
    }
    let dir = Directory::parse(&l.read_sector(o.oi, 0, o.info.sector_size)?)?;
    for e in &dir.entries {
        let p = Profile::parse(&l.read_sector(o.oi, e.sector, o.info.sector_size)?, true)?;
        let dpis: Vec<String> = (0..5).filter_map(|i| p.dpi(i)).map(|d| d.to_string()).collect();
        println!(
            "  profil {} {} : {} Hz, DPI {} (défaut n°{}), LED {:?}, nom {:?}",
            e.sector,
            if e.enabled { "activé   " } else { "désactivé" },
            1000 / u32::from(p.report_interval_ms()),
            dpis.join("/"),
            p.default_dpi_slot() + 1,
            p.led(1),
            p.name()
        );
    }
    Ok(())
}

/// What G13 Studio reads from the active profile (read only).
fn settings(m: &Mouse) -> Result<(), String> {
    let o = onboard(m)?;
    let cur = m.link.call(o.oi, 4, &[])?;
    let sector = u16::from_be_bytes([cur[0], cur[1]]);
    let p = Profile::parse(&m.link.read_sector(o.oi, sector, o.info.sector_size)?, true)?;
    println!("profil {sector} : {:?}", g13_daemon::mouse::settings::from_profile(&p));
    Ok(())
}

fn lab_dir() -> PathBuf {
    g13_daemon::profiles::store::default_dir()
        .and_then(|p| p.parent().map(|p| p.join("backups").join("hidpp-lab")))
        .unwrap_or_else(|| PathBuf::from("hidpp-lab"))
}

fn test_profile(m: &Mouse) -> Result<(), String> {
    let o = onboard(m)?;
    let l = &m.link;
    let size = o.info.sector_size;
    let dir = Directory::parse(&l.read_sector(o.oi, 0, size)?)?;
    let target = dir.entries.iter().find(|e| !e.enabled).ok_or("aucun emplacement de profil désactivé : rien n'est écrit")?.sector;
    let base_sector = dir.entries.iter().find(|e| e.enabled).ok_or("aucun profil activé")?.sector;

    // save what we are about to change; never over a test that was not undone
    let state = lab_dir();
    std::fs::create_dir_all(&state).map_err(|e| e.to_string())?;
    if state.join("state.txt").exists() {
        return Err("un essai précédent n'a pas été annulé : lancez d'abord « restore »".into());
    }
    let original = l.read_sector(o.oi, target, size)?;
    let mode = l.call(o.oi, 2, &[])?[0];
    let cur = l.call(o.oi, 4, &[])?;
    let previous = u16::from_be_bytes([cur[0], cur[1]]);
    std::fs::write(state.join(format!("sector_{target:04X}.orig.bin")), &original).map_err(|e| e.to_string())?;
    let dir_bytes = l.read_sector(o.oi, 0, size)?;
    std::fs::write(state.join("sector_0000.orig.bin"), &dir_bytes).map_err(|e| e.to_string())?;
    std::fs::write(state.join("state.txt"), format!("target={target}\nmode={mode}\nprofile={previous}\nbase={base_sector}\n"))
        .map_err(|e| e.to_string())?;
    println!("sauvegardé : emplacement {target} (désactivé), mode {mode}, profil courant {previous} → {}", state.display());

    // test profile = copy of the first enabled one, turquoise LEDs, a name
    let mut p = Profile::parse(&l.read_sector(o.oi, base_sector, size)?, true)?;
    for k in 0..4 {
        p.set_led(k, Led::Fixed { rgb: TEST_COLOR });
    }
    p.set_name(Some("G13 Studio test"));
    p.set_custom_animation(0); // otherwise the factory gradient keeps playing over the LEDs
    l.write_sector(o.oi, target, &p.to_bytes())?;
    println!("profil d'essai écrit dans l'emplacement {target} et relu à l'identique");

    // the mouse only selects enabled profiles: enable the slot in the directory for the test
    let mut enabled = dir.clone();
    for e in &mut enabled.entries {
        if e.sector == target {
            e.enabled = true;
        }
    }
    l.write_sector(o.oi, 0, &enabled.to_bytes())?;
    println!("emplacement {target} activé temporairement dans l'annuaire (relu à l'identique)");
    l.call(o.oi, 1, &[1])?; // onboard mode
    l.call(o.oi, 3, &[(target >> 8) as u8, target as u8])?;
    let now = l.call(o.oi, 4, &[])?;
    println!(
        "souris en mode mémoire, profil courant : {} → l'éclairage doit être turquoise",
        u16::from_be_bytes([now[0], now[1]])
    );
    Ok(())
}

fn restore(m: &Mouse) -> Result<(), String> {
    let o = onboard(m)?;
    let l = &m.link;
    let state = lab_dir();
    let text = std::fs::read_to_string(state.join("state.txt")).map_err(|e| format!("pas d'essai à annuler : {e}"))?;
    let get = |k: &str| text.lines().find_map(|line| line.strip_prefix(&format!("{k}="))).and_then(|v| v.parse::<u16>().ok());
    let (Some(target), Some(base)) = (get("target"), get("base")) else { return Err("state.txt illisible".into()) };
    let original = std::fs::read(state.join(format!("sector_{target:04X}.orig.bin"))).map_err(|e| e.to_string())?;
    // back to a real profile first, then put the slot back as it was
    let previous = get("profile").filter(|&p| p != 0 && p != target).unwrap_or(base);
    l.call(o.oi, 3, &[(previous >> 8) as u8, previous as u8])?;
    if let Ok(dir) = std::fs::read(state.join("sector_0000.orig.bin")) {
        l.write_sector(o.oi, 0, &dir)?;
        println!("annuaire remis à l'identique");
    }
    l.write_sector(o.oi, target, &original)?;
    println!("emplacement {target} remis à l'identique, profil courant : {previous}");
    let _ = std::fs::rename(state.join("state.txt"), state.join("state.done.txt"));
    Ok(())
}

/// Make profile slot `n` the current one (onboard mode).
fn select(m: &Mouse, n: Option<u16>) -> Result<(), String> {
    let n = n.ok_or("usage : hidpp-lab select <emplacement>")?;
    let o = onboard(m)?;
    match m.link.call(o.oi, 1, &[1]) {
        Ok(_) => println!("mode mémoire : accepté"),
        Err(e) => println!("mode mémoire : {e}"),
    }
    for params in [[(n >> 8) as u8, n as u8], [0, n as u8]] {
        match m.link.call(o.oi, 3, &params) {
            Ok(_) => println!("profil {n} ({params:02X?}) : accepté"),
            Err(e) => println!("profil {n} ({params:02X?}) : {e}"),
        }
    }
    let now = m.link.call(o.oi, 4, &[])?;
    println!("profil courant : {}", u16::from_be_bytes([now[0], now[1]]));
    Ok(())
}

/// Write user sector `n` back from a backup file (CRC checked, read back).
fn restore_sector(m: &Mouse, n: Option<u16>, file: Option<&String>) -> Result<(), String> {
    let (Some(n), Some(file)) = (n, file) else { return Err("usage : hidpp-lab restore-sector <secteur> <fichier.bin>".into()) };
    let o = onboard(m)?;
    let data = std::fs::read(file).map_err(|e| format!("{file} : {e}"))?;
    if data.len() != o.info.sector_size as usize || !logi_hidpp::onboard::crc_ok(&data) {
        return Err("fichier de secteur invalide (taille ou CRC)".into());
    }
    m.link.write_sector(o.oi, n, &data)?;
    println!("secteur {n} réécrit depuis {file} et relu à l'identique");
    Ok(())
}

/// Current DPI slot (0-based) of the onboard profile.
fn dpi_slot(m: &Mouse, n: Option<u8>) -> Result<(), String> {
    let n = n.filter(|&n| n < 5).ok_or("usage : hidpp-lab dpi-slot <0..4>")?;
    let o = onboard(m)?;
    m.link.call(o.oi, 12, &[n])?;
    println!("palier DPI : {}", m.link.call(o.oi, 11, &[])?[0]);
    Ok(())
}

/// Report rate in RAM (0x8060 fn 2, interval in ms).
fn rate_live(m: &Mouse, hz: Option<u16>) -> Result<(), String> {
    let hz = hz.filter(|h| [125, 250, 500, 1000].contains(h)).ok_or("usage : hidpp-lab rate-live <125|250|500|1000>")?;
    let i = m.link.feature(f::REPORT_RATE).ok_or("pas de réglage de fréquence")?;
    let r = m.link.call(i, 2, &[(1000 / hz) as u8]);
    let now = m.link.call(i, 1, &[])?[0];
    println!("réglage : {r:?} ; fréquence lue : {} Hz", 1000 / u16::from(now.max(1)));
    Ok(())
}

/// Clusters (zones) of 0x8071 and their effects, read only.
type Zones = Vec<(u8, Vec<(u8, u16)>)>;

fn rgb_info(m: &Mouse) -> Result<Zones, String> {
    let i = m.link.feature(f::RGB_EFFECTS).ok_or("pas de 0x8071")?;
    let d = m.link.call(i, 0, &[0xFF, 0xFF, 0x00])?;
    let clusters = d[2];
    println!("0x8071 : {clusters} zones (réponse {:02X?})", &d[..8]);
    let mut out = Vec::new();
    for c in 0..clusters {
        let z = m.link.call(i, 0, &[c, 0xFF, 0x00])?;
        let location = u16::from_be_bytes([z[2], z[3]]);
        let n = z[4];
        let mut effects = Vec::new();
        for e in 0..n {
            let x = m.link.call(i, 0, &[c, e, 0x00])?;
            effects.push((e, u16::from_be_bytes([x[2], x[3]])));
        }
        println!("  zone {c} (emplacement {location}) : {} effets {:02X?}", n, effects.iter().map(|(e, id)| (e, id)).collect::<Vec<_>>());
        out.push((c, effects));
    }
    Ok(out)
}

/// Static colour on every zone through 0x8071 (no flash write). `--claim` takes software control first.
fn rgb_live(m: &Mouse, color: Option<&str>, claim: bool) -> Result<(), String> {
    let hex = color.map(|c| c.trim_start_matches('#')).filter(|c| c.len() == 6).ok_or("usage : hidpp-lab rgb-live RRGGBB [--claim]")?;
    let v = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
    let (r, g, b) = ((v >> 16) as u8, (v >> 8) as u8, v as u8);
    let zones = rgb_info(m)?;
    let i = m.link.feature(f::RGB_EFFECTS).ok_or("pas de 0x8071")?;
    if claim {
        println!("prise de contrôle logiciel : {:?}", m.link.call(i, 5, &[0x01, 0x03, 0x04]).map(|d| d[..4].to_vec()));
    }
    for (c, effects) in zones {
        let Some((idx, _)) = effects.iter().find(|(_, id)| *id == 0x0001) else {
            println!("  zone {c} : pas d'effet fixe");
            continue;
        };
        let r = m.link.call(i, 1, &[c, *idx, r, g, b, 0x02, 0, 0, 0, 0, 0, 0, 0x01]);
        println!("  zone {c} : effet fixe (index {idx}) → {:?}", r.map(|d| d[..4].to_vec()));
    }
    Ok(())
}

fn rgb_release(m: &Mouse) -> Result<(), String> {
    let i = m.link.feature(f::RGB_EFFECTS).ok_or("pas de 0x8071")?;
    println!("rendu du contrôle : {:?}", m.link.call(i, 5, &[0x01, 0x00, 0x00]).map(|d| d[..4].to_vec()));
    Ok(())
}

/// 1 = the mouse's own memory, 2 = host (driven by a program).
fn set_mode(m: &Mouse, mode: Option<u8>) -> Result<(), String> {
    let mode = mode.filter(|m| [1, 2].contains(m)).ok_or("usage : hidpp-lab mode <1|2>")?;
    let o = onboard(m)?;
    m.link.call(o.oi, 1, &[mode])?;
    println!("mode : {}", f::onboard_mode_name(m.link.call(o.oi, 2, &[])?[0]));
    Ok(())
}

/// Host mode + MouseButtonSpy (0x8110) for `secs` seconds: print every button event,
/// then stop spying and give the mouse back to its own memory. With `--mute N`, physical
/// button N (1-based, never 1) sends nothing to Windows during the test (remap 0).
fn spy(m: &Mouse, secs: Option<u64>, mute: Option<u8>) -> Result<(), String> {
    let o = onboard(m)?;
    let si = m.link.feature(0x8110).ok_or("pas de 0x8110")?;
    let l = &m.link;
    let remap = l.call(si, 3, &[])?;
    println!("boutons : {} ; table : {:02X?}", l.call(si, 0, &[])?[0], &remap[..11]);
    l.call(o.oi, 1, &[2])?;
    println!("mode : {}", f::onboard_mode_name(l.call(o.oi, 2, &[])?[0]));
    if let Some(n) = mute.filter(|n| (2..=11).contains(n)) {
        let mut t = remap;
        t[n as usize - 1] = 0;
        println!("muet {n} : {:?}", l.call(si, 4, &t).map(|d| d[..4].to_vec()));
    }
    println!("espion : {:?}", l.call(si, 1, &[]).map(|d| d[..4].to_vec()));
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs.unwrap_or(60));
    let start = std::time::Instant::now();
    let mut buf = [0u8; 64];
    while std::time::Instant::now() < end {
        let Ok(n) = l.dev.read_timeout(&mut buf, 100) else { break };
        if n == 0 || buf[1] != l.index {
            continue;
        }
        println!("{:>6} ms  {:02X?}", start.elapsed().as_millis(), &buf[..n.min(12)]);
    }
    println!("fin espion : {:?}", l.call(si, 2, &[]).map(|d| d[..4].to_vec()));
    println!("table remise : {:?}", l.call(si, 4, &remap).map(|d| d[..4].to_vec()));
    l.call(o.oi, 1, &[1])?;
    println!("mode : {}", f::onboard_mode_name(l.call(o.oi, 2, &[])?[0]));
    Ok(())
}

/// Read-only exploration: `raw <feature id hex> <function> [param bytes hex…]`; only
/// functions known to be "get" may be called this way.
fn raw(m: &Mouse, a: &[String]) -> Result<(), String> {
    let id = a.first().and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok()).ok_or("usage : raw <feature> <fn> [octets]")?;
    let func: u8 = a.get(1).and_then(|v| v.parse().ok()).ok_or("fonction ?")?;
    let params: Vec<u8> = a[2..].iter().filter_map(|v| u8::from_str_radix(v, 16).ok()).collect();
    let i = m.link.feature(id).ok_or("feature absente")?;
    match m.link.call(i, func, &params) {
        Ok(d) => println!("0x{id:04X} fn {func} {params:02X?} → {d:02X?}"),
        Err(e) => println!("0x{id:04X} fn {func} {params:02X?} → {e}"),
    }
    Ok(())
}

fn hex_rgb(s: Option<&String>, default: u32) -> [u8; 3] {
    let v = s.and_then(|v| u32::from_str_radix(v.trim_start_matches('#'), 16).ok()).unwrap_or(default);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Take software control of the lighting and switch the firmware effect off,
/// so that per-LED colours (0x8081) show (sequence used by Solaar on this mouse).
fn per_led_mode(m: &Mouse) -> Result<u8, String> {
    let ri = m.link.feature(f::RGB_EFFECTS).ok_or("pas de 0x8071")?;
    let pk = m.link.feature(0x8081).ok_or("pas de 0x8081")?;
    let n = m.link.call(ri, 0, &[0, 0xFF, 0])?[4];
    m.link.call(ri, 5, &[0x01, 0x03, 0x04])?;
    m.link.call(ri, 1, &[0xFF, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01])?;
    Ok(pk)
}

/// Static gradient over LEDs 0..=8, from `a` to `b` (RAM only; rgb-release ends it).
fn gradient(m: &Mouse, a: Option<&String>, b: Option<&String>) -> Result<(), String> {
    let (ca, cb) = (hex_rgb(a, 0xFF4FB3), hex_rgb(b, 0x3D7BFF));
    let pk = per_led_mode(m)?;
    let leds: Vec<[u8; 4]> = (0..9u8)
        .map(|i| {
            let t = f32::from(i) / 8.0;
            let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
            [i, mix(ca[0], cb[0]), mix(ca[1], cb[1]), mix(ca[2], cb[2])]
        })
        .collect();
    for chunk in leds.chunks(4) {
        let params: Vec<u8> = chunk.iter().flatten().copied().collect();
        m.link.call(pk, 1, &params)?;
    }
    m.link.call(pk, 7, &[0, 0, 0, 0, 0])?;
    println!("dégradé envoyé sur les LED 0 à 8");
    Ok(())
}

/// Light a single LED (the others off), to find where each one is.
fn one_led(m: &Mouse, id: Option<u8>, color: Option<&String>) -> Result<(), String> {
    let id = id.filter(|&i| i < 16).ok_or("usage : hidpp-lab led <0..15> [RRGGBB]")?;
    let c = hex_rgb(color, 0xFFFFFF);
    let pk = per_led_mode(m)?;
    let all: Vec<[u8; 4]> = (0..9u8).map(|i| if i == id { [i, c[0], c[1], c[2]] } else { [i, 0, 0, 0] }).collect();
    for chunk in all.chunks(4) {
        m.link.call(pk, 1, &chunk.iter().flatten().copied().collect::<Vec<u8>>())?;
    }
    m.link.call(pk, 7, &[0, 0, 0, 0, 0])?;
    println!("LED {id} seule allumée");
    Ok(())
}

fn dpi_live(m: &Mouse, dpi: Option<u16>) -> Result<(), String> {
    let dpi = dpi.ok_or("usage : hidpp-lab dpi-live <DPI>")?;
    let i = m.link.feature(f::ADJUSTABLE_DPI).ok_or("pas de réglage DPI")?;
    m.link.call(i, 3, &[0, (dpi >> 8) as u8, dpi as u8])?;
    let d = m.link.call(i, 2, &[0])?;
    println!("DPI actuel : {}", u16::from_be_bytes([d[1], d[2]]));
    Ok(())
}
