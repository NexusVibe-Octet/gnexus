use g13_core::{
    BacklightReport, G13InputReport, G13LcdBuffer, MKeyLeds, RgbColor, G13_PID, G13_VID,
    LCD_HEIGHT, LCD_WIDTH,
};
use hidapi::{HidApi, HidDevice};
use std::env;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("Error initializing HIDAPI: {}", e);
            return;
        }
    };

    match command {
        "probe" => cmd_probe(&api),
        "monitor" => cmd_monitor(&api),
        "rgb" => cmd_rgb(&api, &args[2..]),
        "leds" => cmd_leds(&api, &args[2..]),
        "text" => cmd_text(&api, &args[2..]),
        "demo" => cmd_demo(&api),
        _ => print_usage(),
    }
}

fn open_g13(api: &HidApi) -> Option<HidDevice> {
    match api.open(G13_VID, G13_PID) {
        Ok(dev) => Some(dev),
        Err(e) => {
            eprintln!("[!] Failed to open Logitech G13 (VID 0x{:04X}, PID 0x{:04X}): {}", G13_VID, G13_PID, e);
            eprintln!("    Make sure the G13 is plugged into a USB port.");
            None
        }
    }
}

fn cmd_probe(api: &HidApi) {
    println!("========================================================");
    println!("           LOGITECH G13 HARDWARE PROBE                  ");
    println!("========================================================");

    for dev_info in api.device_list() {
        if dev_info.vendor_id() == G13_VID && dev_info.product_id() == G13_PID {
            println!("Device Found:");
            println!("  Path:        {:?}", dev_info.path());
            println!("  Product:     {:?}", dev_info.product_string().unwrap_or(""));
            println!("  Release:     0x{:04X}", dev_info.release_number());
            println!("  Interface:   {}", dev_info.interface_number());
            println!("  Usage Page:  0x{:04X}", dev_info.usage_page());
            println!("  Usage:       0x{:04X}", dev_info.usage());
        }
    }

    if let Some(dev) = open_g13(api) {
        println!("\n[+] Device opened successfully.");

        // Read Report Descriptor
        let mut desc = vec![0u8; 512];
        if let Ok(len) = dev.get_report_descriptor(&mut desc) {
            println!("[+] HID Report Descriptor: {} bytes", len);
        }

        // Query Backlight Color
        let mut rgb_buf = [0u8; 5];
        rgb_buf[0] = 0x07;
        if dev.get_feature_report(&mut rgb_buf).is_ok() {
            println!(
                "[+] Current RGB Backlight: R={}, G={}, B={}",
                rgb_buf[1], rgb_buf[2], rgb_buf[3]
            );
        }

        // Query M-keys LEDs
        let mut led_buf = [0u8; 5];
        led_buf[0] = 0x05;
        if dev.get_feature_report(&mut led_buf).is_ok() {
            let mask = led_buf[1];
            println!(
                "[+] Current M-LEDs: M1={}, M2={}, M3={}, MR={}",
                (mask & 1) != 0,
                (mask & 2) != 0,
                (mask & 4) != 0,
                (mask & 8) != 0
            );
        }
    }
}

fn cmd_monitor(api: &HidApi) {
    let dev = match open_g13(api) {
        Some(d) => d,
        None => return,
    };

    println!("========================================================");
    println!("           G13 REAL-TIME INPUT MONITOR                  ");
    println!("========================================================");
    println!("Press any key (G1..G22, M1..MR, LCD keys, stick click)");
    println!("or move the analog stick. Press Ctrl+C to stop.\n");

    let mut last_raw = [0u8; 8];

    loop {
        let mut buf = [0u8; 8];
        buf[0] = G13InputReport::REPORT_ID;
        if dev.get_input_report(&mut buf).is_ok() {
            if buf != last_raw {
                if let Some(report) = G13InputReport::from_bytes(&buf) {
                    let stick = report.stick();
                    let (nx, ny) = stick.normalized();
                    let pressed: Vec<&'static str> =
                        report.pressed_keys().map(|k| k.name()).collect();

                    print!(
                        "\rStick: ({:>5.2}, {:>5.2}) | Raw: ({:>3}, {:>3}) | Keys ({}): [{}]",
                        nx,
                        ny,
                        stick.raw_x,
                        stick.raw_y,
                        pressed.len(),
                        pressed.join(", ")
                    );
                    // Clear trailing line remnants
                    print!("          \r");
                    std::io::Write::flush(&mut std::io::stdout()).ok();

                    last_raw = buf;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn cmd_rgb(api: &HidApi, args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: g13-driver rgb <R> <G> <B> (values 0..255)");
        return;
    }
    let r: u8 = args[0].parse().unwrap_or(255);
    let g: u8 = args[1].parse().unwrap_or(255);
    let b: u8 = args[2].parse().unwrap_or(255);

    let dev = match open_g13(api) {
        Some(d) => d,
        None => return,
    };

    let color = RgbColor::new(r, g, b);
    let report = BacklightReport::new(color);
    match dev.send_feature_report(report.as_bytes()) {
        Ok(_) => println!("[+] Backlight set to RGB({}, {}, {})", r, g, b),
        Err(e) => eprintln!("[-] Failed to set backlight: {}", e),
    }
}

fn cmd_leds(api: &HidApi, args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: g13-driver leds <M1:0/1> <M2:0/1> <M3:0/1> <MR:0/1>");
        eprintln!("Example: g13-driver leds 1 0 0 1");
        return;
    }
    let m1 = args.first().map(|s| s == "1").unwrap_or(false);
    let m2 = args.get(1).map(|s| s == "1").unwrap_or(false);
    let m3 = args.get(2).map(|s| s == "1").unwrap_or(false);
    let mr = args.get(3).map(|s| s == "1").unwrap_or(false);

    let dev = match open_g13(api) {
        Some(d) => d,
        None => return,
    };

    let leds = MKeyLeds::new().m1(m1).m2(m2).m3(m3).mr(mr);
    let report = leds.to_report();
    match dev.send_feature_report(&report) {
        Ok(_) => println!("[+] LEDs set: M1={}, M2={}, M3={}, MR={}", m1, m2, m3, mr),
        Err(e) => eprintln!("[-] Failed to set LEDs: {}", e),
    }
}

fn cmd_text(api: &HidApi, args: &[String]) {
    let dev = match open_g13(api) {
        Some(d) => d,
        None => return,
    };

    let mut fb = G13LcdBuffer::new();
    fb.draw_rect(0, 0, LCD_WIDTH, LCD_HEIGHT);

    let lines = if args.is_empty() {
        vec!["G13 RUST DRIVER".to_string(), "Protocol OK!".to_string()]
    } else {
        args.to_vec()
    };

    for (idx, line) in lines.iter().enumerate().take(4) {
        let y = 4 + idx * 9;
        fb.draw_text(6, y, line);
    }

    match dev.write(fb.as_report()) {
        Ok(_) => println!("[+] Text successfully sent to LCD!"),
        Err(e) => eprintln!("[-] Error sending text to LCD: {}", e),
    }
}

fn cmd_demo(api: &HidApi) {
    let dev = match open_g13(api) {
        Some(d) => d,
        None => return,
    };

    println!("Playing LCD Demo for 3 seconds...");
    let start = std::time::Instant::now();
    let mut frame = 0;

    while start.elapsed().as_secs() < 3 {
        let mut fb = G13LcdBuffer::new();
        // Outer decorative double frame
        fb.draw_rect(0, 0, LCD_WIDTH, LCD_HEIGHT);
        fb.draw_rect(2, 2, LCD_WIDTH - 4, LCD_HEIGHT - 4);

        // Header
        fb.draw_text(6, 6, "=== G13 RUST CORE ===");

        // Dynamic progress bar
        let progress = ((frame * 3) % (LCD_WIDTH - 20)) + 1;
        fb.draw_rect(10, 18, LCD_WIDTH - 20, 9);
        for x in 12..(10 + progress).min(LCD_WIDTH - 12) {
            for y in 20..25 {
                fb.set_pixel(x, y, true);
            }
        }

        // Subtext
        fb.draw_text(6, 30, "Native Driver active");

        dev.write(fb.as_report()).ok();
        frame += 1;
        std::thread::sleep(Duration::from_millis(33)); // ~30 FPS
    }
    println!("[+] Demo finished.");
}

fn print_usage() {
    println!("Logitech G13 Rust Driver & Protocol Explorer");
    println!("Usage:");
    println!("  cargo run -- probe                  # Show G13 hardware and current state");
    println!("  cargo run -- monitor                # Interactive real-time keypress & stick monitor");
    println!("  cargo run -- rgb <R> <G> <B>        # Set backlight color (0..255)");
    println!("  cargo run -- leds <M1> <M2> <M3> <MR> # Set profile LEDs (1/0)");
    println!("  cargo run -- text <L1> [L2] [L3] [L4] # Draw text lines on the LCD");
    println!("  cargo run -- demo                   # Play LCD graphics animation");
}
