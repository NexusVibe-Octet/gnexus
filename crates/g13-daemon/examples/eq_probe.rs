//! Linux: drives G Nexus's PipeWire equalizer as the GUI does.
//!
//! ```text
//! eq_probe install "<output name>"   # in front of that output (flat at first: current settings)
//! eq_probe gains 0 0 0 0 0 12 0 0 0 0
//! eq_probe uninstall
//! ```

#[cfg(target_os = "linux")]
fn main() {
    use g13_daemon::headset::eq;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("install") => eq::install(args.get(1).map_or("PRO X Wireless", String::as_str)),
        Some("uninstall") => eq::uninstall(),
        Some("gains") => {
            let g: Vec<i8> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
            eq::save_gains(&g).map(|s| println!("{:?}", eq::view(&s).gains))
        }
        _ => Err("install <sortie> | gains <10 valeurs> | uninstall".into()),
    };
    println!("{r:?} ; installé : {}", eq::installed(""));
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Linux seulement");
}
