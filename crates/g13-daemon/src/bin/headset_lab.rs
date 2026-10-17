//! Checks on a Logitech HID++ headset (PRO X Wireless):
//!
//! ```text
//! headset-lab                 # status (read only)
//! headset-lab sidetone 30     # set, then read back
//! headset-lab sleep 15        # power-off delay in minutes (0 = never)
//! headset-lab eq 0,0,2,…      # gains of the built-in equalizer (one per band)
//! ```

use g13_daemon::headset::{battery_percent, find_headsets};
use hidapi::HidApi;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let api = HidApi::new().expect("hidapi");
    // listen to every report of the headset dongle for N seconds (what G HUB sends gets answered
    // there): `headset-lab listen 90`
    // the wheel seen two ways at once: the dongle's consumer reports and the keys Windows makes of
    // them (low-level hook, nothing blocked): `headset-lab wheel-test 20`
    if args.first().map(String::as_str) == Some("wheel-test") {
        let secs: u64 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(20);
        let dev = api
            .device_list()
            .find(|d| d.vendor_id() == 0x046D && d.product_id() == 0x0ABA && d.usage_page() == 0x000C)
            .and_then(|d| d.open_device(&api).ok())
            .expect("collection multimédia du casque");
        let rec = g13_daemon::platform::start_recording().expect("écoute du clavier");
        let start = std::time::Instant::now();
        let mut buf = [0u8; 16];
        let mut reports = Vec::new();
        while start.elapsed().as_secs() < secs {
            if let Ok(n) = dev.read_timeout(&mut buf, 20)
                && n > 0
            {
                reports.push((start.elapsed().as_millis(), buf[1]));
            }
        }
        let keys = rec.stop();
        for (t, b) in reports {
            println!("{t:>7} ms  récepteur : {}", match b { 1 => "Volume +", 2 => "Volume -", 0 => "relâché", _ => "?" });
        }
        for (t, k) in keys {
            println!("{:>7} ms  touche Windows : {k:?}", t.as_millis());
        }
        return;
    }
    if args.first().map(String::as_str) == Some("listen") {
        let secs: u64 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(60);
        let devs: Vec<_> = api
            .device_list()
            .filter(|d| d.vendor_id() == 0x046D && d.product_id() == 0x0ABA)
            .filter_map(|d| {
                let dev = d.open_device(&api).ok()?;
                println!("ouvert : page 0x{:04X} usage 0x{:04X}", d.usage_page(), d.usage());
                Some((d.usage_page(), dev))
            })
            .collect();
        let start = std::time::Instant::now();
        let mut buf = [0u8; 64];
        // the Windows volume of the headset too: a wheel may change it without any HID report
        let mut last_vol: Option<(u8, bool)> = None;
        let mut vol_at = std::time::Instant::now();
        while start.elapsed().as_secs() < secs {
            if vol_at.elapsed().as_millis() >= 50 {
                vol_at = std::time::Instant::now();
                let v = g13_daemon::platform::output_volume("PRO X").ok().flatten().map(|v| (v.volume, v.muted));
                if v != last_vol {
                    println!("{:>7} ms  volume Windows {:?}", start.elapsed().as_millis(), v);
                    last_vol = v;
                }
            }
            for (page, dev) in &devs {
                if let Ok(n) = dev.read_timeout(&mut buf, 5)
                    && n > 0
                {
                    println!("{:>7} ms  page {page:04X}  {:02X?}", start.elapsed().as_millis(), &buf[..n.min(24)]);
                }
            }
        }
        return;
    }
    let Some(mut h) = find_headsets(&api).into_iter().next() else {
        eprintln!("aucun casque HID++ trouvé");
        std::process::exit(1);
    };
    // not G13 Studio's software id: both may talk to the headset at the same time
    h.link.sw_id = 0x0E;
    if !h.resolve() {
        println!("dongle {:04X} présent, casque éteint", h.product_id);
        return;
    }
    let num = |i: usize| args.get(i).and_then(|v| v.parse::<u8>().ok());
    match args.first().map(String::as_str) {
        Some("sidetone") => println!("retour micro → {:?}", h.set_sidetone(num(1).expect("niveau 0..100"))),
        Some("sleep") => println!("veille → {:?}", h.set_sleep_minutes(num(1).expect("minutes"))),
        Some("eq") => {
            let g: Vec<i8> = args.get(1).expect("gains").split(',').filter_map(|v| v.trim().parse().ok()).collect();
            println!("égaliseur → {:?}", h.set_equalizer(&g));
        }
        // listening test of the built-in equalizer: 4 phases of 8 s, then flat again
        Some("eq-test") => {
            let i = h.eq.expect("pas d'égaliseur");
            let phases: [(&str, u8, i8); 4] = [
                ("A : tout à -12 dB (préfixe 02)", 2, -12),
                ("B : tout à -12 dB (préfixe 00)", 0, -12),
                ("C : tout à -12 dB (préfixe 01)", 1, -12),
                ("D : basses +12, aigus -12 (préfixe 02)", 2, 0),
            ];
            for (label, prefix, g) in phases {
                let mut p = vec![prefix];
                if label.starts_with('D') {
                    p.extend([12i8, 12, 12, 6, 0, -6, -12, -12, -12, -12].map(|x| x as u8));
                } else {
                    p.extend([g as u8; 10]);
                }
                println!("{} → {:?}", label, h.link.call(i, 3, &p).map(|d| d[..4].to_vec()));
                std::thread::sleep(std::time::Duration::from_secs(8));
            }
            println!("retour à plat → {:?}", h.set_equalizer(&[0; 10]).map(|e| e.gains));
        }
        // the Logitech audio effect (HX2E): white noise straight to the headset, gains at -12 dB
        // from 4 s to 10 s, then back (G13 Studio not needed)
        #[cfg(windows)]
        Some("apo-test") => {
            use g13_daemon::platform::windows::audio;
            const EQ: &str = "{42d67d07-c2b8-47f7-8783-109668632ec6},0";
            let key = audio::endpoint_key("PRO X").expect("audio").expect("sortie du casque");
            let orig = audio::fx_value(&key, EQ).expect("lecture").expect("réglage absent");
            let mut low = orig.clone();
            for b in 0..10 {
                low[48 + 4 * b..52 + 4 * b].copy_from_slice(&(-12f32).to_le_bytes());
            }
            let mut phase = 0;
            println!("0 s : bruit, égaliseur d'origine");
            let r = audio::play_noise("PRO X", 14.0, 0.03, &mut |t| {
                if phase == 0 && t >= 4.0 {
                    phase = 1;
                    println!("4 s : gains à -12 dB → {:?}", audio::set_fx_value(&key, EQ, &low));
                }
                if phase == 1 && t >= 10.0 {
                    phase = 2;
                    println!("10 s : retour → {:?}", audio::set_fx_value(&key, EQ, &orig));
                }
            });
            if phase == 1 {
                let _ = audio::set_fx_value(&key, EQ, &orig);
            }
            println!("fin : {r:?} ; réglage remis : {}", audio::fx_value(&key, EQ).ok().flatten().as_deref() == Some(&orig[..]));
        }
        _ => {}
    }
    println!("nom : {:?}", h.name());
    match h.battery() {
        Ok((mv, chg)) => println!("batterie : {mv} mV → {:?} %{}", battery_percent(mv), if chg { " (en charge)" } else { "" }),
        Err(e) => println!("batterie : {e}"),
    }
    println!("retour micro : {:?}", h.sidetone());
    println!("veille : {:?} min", h.sleep_minutes());
    println!("égaliseur : {:?}", h.equalizer());
    println!("volume Windows : {:?}", g13_daemon::platform::output_volume("PRO X"));
    let eq = g13_daemon::platform::output_eq("PRO X");
    println!("égaliseur Windows (effet Logitech) : {eq:?}");
    if args.first().map(String::as_str) == Some("soft-eq-same")
        && let Ok(Some(e)) = &eq
    {
        println!("réécriture à l'identique : {:?}", g13_daemon::platform::set_output_eq("PRO X", &e.gains));
        println!("relu : {:?}", g13_daemon::platform::output_eq("PRO X").map(|o| o.map(|o| o.gains)));
    }
}
