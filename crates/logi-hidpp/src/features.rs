//! Feature ids and the decoding of the replies used so far (read-only
//! functions). Function numbers are given next to each decoder.

use crate::be16;

pub const ROOT: u16 = 0x0000;
pub const FEATURE_SET: u16 = 0x0001;
pub const FW_VERSION: u16 = 0x0003;
pub const DEVICE_NAME: u16 = 0x0005;
pub const BATTERY_STATUS: u16 = 0x1000;
pub const UNIFIED_BATTERY: u16 = 0x1004;
pub const REPROG_CONTROLS_V4: u16 = 0x1B04;
pub const ADJUSTABLE_DPI: u16 = 0x2201;
pub const REPORT_RATE: u16 = 0x8060;
pub const EXTENDED_REPORT_RATE: u16 = 0x8061;
pub const RGB_EFFECTS: u16 = 0x8071;
pub const ONBOARD_PROFILES: u16 = 0x8100;

/// Human name of a feature id (for listings).
pub fn name(id: u16) -> &'static str {
    match id {
        0x0000 => "Root",
        0x0001 => "FeatureSet",
        0x0002 => "FeatureInfo",
        0x0003 => "DeviceFwVersion",
        0x0004 => "DeviceUnitId",
        0x0005 => "DeviceName",
        0x0006 => "DeviceGroups",
        0x0007 => "DeviceFriendlyName",
        0x0008 => "KeepAlive",
        0x0020 => "ConfigChange",
        0x0021 => "UniqueRandomId",
        0x00C2 => "DfuControlSigned",
        0x00C3 => "DfuControl",
        0x00D0 => "Dfu",
        0x1000 => "BatteryStatus",
        0x1001 => "BatteryVoltage",
        0x1004 => "UnifiedBattery",
        0x1802 => "DeviceReset",
        0x1803 => "GpioAccess",
        0x1805 => "OobState",
        0x1806 => "ConfigurableDeviceProperties",
        0x1811 => "?(0x1811)",
        0x1830 => "PowerModes",
        0x1891 => "?(0x1891)",
        0x18A1 => "?(0x18A1)",
        0x1B04 => "ReprogControlsV4",
        0x1D4B => "WirelessDeviceStatus",
        0x1E00 => "EnableHiddenFeatures",
        0x1E02 => "ManageDeactivatableFeatures",
        0x1EB0 => "?(0x1EB0)",
        0x1F20 => "AdcMeasurement",
        0x2110 => "SmartShift",
        0x2111 => "SmartShiftEnhanced",
        0x2121 => "HiResWheel",
        0x2201 => "AdjustableDpi",
        0x2202 => "ExtendedAdjustableDpi",
        0x2250 => "AnalysisMode",
        0x8010 => "GKeys",
        0x8020 => "MKeys",
        0x8030 => "MacroRecordKey",
        0x8040 => "BrightnessControl",
        0x8060 => "ReportRate",
        0x8061 => "ExtendedReportRate",
        0x8070 => "ColorLedEffects",
        0x8071 => "RgbEffects",
        0x8080 => "PerKeyLighting",
        0x8081 => "PerKeyLightingV2",
        0x8090 => "ModeStatus",
        0x8100 => "OnboardProfiles",
        0x8110 => "MouseButtonSpy",
        0x8111 => "LatencyMonitoring",
        0x8120 => "GamingAttachments",
        0x8123 => "ForceFeedback",
        _ => "?",
    }
}

/// Root fn 0 (getFeature): where `id` lives. `None` = the device lacks it (index 0).
pub fn root_get_feature(data: &[u8; 16]) -> Option<FeatureRef> {
    (data[0] != 0).then(|| FeatureRef { index: data[0], flags: data[1], version: data[2] })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureRef {
    pub index: u8,
    /// bit 7 obsolete, 6 hidden, 5 engineering.
    pub flags: u8,
    pub version: u8,
}

/// Root fn 1 (ping): protocol version.
pub fn root_ping(data: &[u8; 16]) -> (u8, u8) {
    (data[0], data[1])
}

/// FeatureSet fn 1 (getFeatureID(index)): `(feature id, flags, version)`.
pub fn feature_set_entry(data: &[u8; 16]) -> (u16, u8, u8) {
    (be16(data, 0), data[2], data[3])
}

/// DeviceFwVersion fn 1 (getFwInfo(entity)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FwInfo {
    /// 0 main application, 1 bootloader, 2 hardware, 3 touchpad, 4 optical sensor, 5 softdevice, 6 RF.
    pub kind: u8,
    pub prefix: String,
    pub number: u8,
    pub revision: u8,
    pub build: u16,
}

impl FwInfo {
    pub fn parse(d: &[u8; 16]) -> Self {
        Self {
            kind: d[0],
            prefix: d[1..4].iter().filter(|c| c.is_ascii_graphic()).map(|&c| c as char).collect(),
            number: d[4],
            revision: d[5],
            build: be16(d, 6),
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            0 => "application",
            1 => "bootloader",
            2 => "hardware",
            4 => "optical sensor",
            5 => "softdevice",
            6 => "radio",
            _ => "other",
        }
    }

    /// LGS/G HUB style: `"U1 46.01.B0018"` (number and revision are BCD).
    pub fn version(&self) -> String {
        format!("{} {:02X}.{:02X}.B{:04X}", self.prefix, self.number, self.revision, self.build)
    }
}

/// UnifiedBattery fn 1 (getStatus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    /// State of charge in percent (0 if the device only reports levels).
    pub percent: u8,
    /// 1 critical, 2 low, 4 good, 8 full.
    pub level: u8,
    /// 0 discharging, 1 charging, 2 slow charging, 3 charge complete, 4 error.
    pub charging: u8,
    pub external_power: bool,
}

impl Battery {
    pub fn parse(d: &[u8; 16]) -> Self {
        Self { percent: d[0], level: d[1], charging: d[2], external_power: d[3] != 0 }
    }

    pub fn charging_name(&self) -> &'static str {
        match self.charging {
            0 => "sur batterie",
            1 => "en charge",
            2 => "charge lente",
            3 => "chargée",
            _ => "erreur de charge",
        }
    }
}

/// AdjustableDpi fn 1 (getSensorDpiList(sensor)): values in DPI. A value
/// `0xE000 | step` between two values means "every `step` from the previous
/// to the next one": it is returned as `DpiChoice::Range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpiChoice {
    Value(u16),
    Range { min: u16, max: u16, step: u16 },
}

/// Decode one or more consecutive `getSensorDpiList` replies (data after the
/// sensor byte, concatenated). Stops at the first 0.
pub fn dpi_list(words: &[u16]) -> Vec<DpiChoice> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() && words[i] != 0 {
        let w = words[i];
        if w >= 0xE000 {
            if let (Some(DpiChoice::Value(min)), Some(&max)) = (out.last().cloned(), words.get(i + 1)) {
                out.pop();
                out.push(DpiChoice::Range { min, max, step: w & 0x1FFF });
                i += 2;
                continue;
            }
        } else {
            out.push(DpiChoice::Value(w));
        }
        i += 1;
    }
    out
}

/// ExtendedReportRate (0x8061) rate index → interval in microseconds.
pub fn extended_rate_us(index: u8) -> Option<u32> {
    [8000, 4000, 2000, 1000, 500, 250, 125].get(index as usize).copied()
}

/// OnboardProfiles fn 0 (getOnboardProfilesInfo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnboardInfo {
    pub memory_model: u8,
    pub profile_format: u8,
    pub macro_format: u8,
    /// User profiles (writable).
    pub profile_count: u8,
    /// Factory profiles in ROM (read-only).
    pub profile_count_oob: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
    pub mechanical_layout: u8,
    pub various_info: u8,
}

impl OnboardInfo {
    pub fn parse(d: &[u8; 16]) -> Self {
        Self {
            memory_model: d[0],
            profile_format: d[1],
            macro_format: d[2],
            profile_count: d[3],
            profile_count_oob: d[4],
            button_count: d[5],
            sector_count: d[6],
            sector_size: be16(d, 7),
            mechanical_layout: d[9],
            various_info: d[10],
        }
    }
}

/// Sector of the factory (ROM) profile `n` (0-based).
pub const fn rom_sector(n: u16) -> u16 {
    0x0100 + n
}

/// OnboardProfiles fn 2 (getOnboardMode): 1 = onboard (the mouse uses its
/// memory), 2 = host (a program drives it).
pub fn onboard_mode_name(mode: u8) -> &'static str {
    match mode {
        1 => "mémoire de la souris",
        2 => "piloté par un logiciel",
        _ => "inconnu",
    }
}

/// ReprogControlsV4 fn 1 (getCtrlIdInfo(index)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    /// Control id (what the button is).
    pub cid: u16,
    /// Task id (what it does by default).
    pub tid: u16,
    /// bit 0 mouse button, 1 F-key, 2 hotkey, 3 fn-toggle, 4 reprogrammable, 5 divertable, 6 persistently divertable, 7 virtual.
    pub flags: u8,
    pub position: u8,
    pub group: u8,
    pub group_mask: u8,
    /// bit 0 raw XY.
    pub flags2: u8,
}

impl Control {
    pub fn parse(d: &[u8; 16]) -> Self {
        Self { cid: be16(d, 0), tid: be16(d, 2), flags: d[4], position: d[5], group: d[6], group_mask: d[7], flags2: d[8] }
    }

    pub fn divertable(&self) -> bool {
        self.flags & 0x20 != 0
    }
}

/// Usual names of control ids (subset).
pub fn control_name(cid: u16) -> &'static str {
    match cid {
        0x0050 => "Clic gauche",
        0x0051 => "Clic droit",
        0x0052 => "Clic molette",
        0x0053 => "Précédent",
        0x0056 => "Suivant",
        0x005B => "Molette à gauche",
        0x005D => "Molette à droite",
        0x00C3 => "Geste",
        0x00C4 => "Mode molette (SmartShift)",
        0x00D7 => "Changer d'appareil",
        0x00ED => "DPI −",
        0x00EE => "DPI +",
        0x00FD => "DPI (cycle)",
        0x0100 => "Profil (cycle)",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_ranges() {
        assert_eq!(dpi_list(&[400, 800, 1600, 0, 3200]), vec![DpiChoice::Value(400), DpiChoice::Value(800), DpiChoice::Value(1600)]);
        assert_eq!(
            dpi_list(&[100, 0xE000 | 50, 25600, 0]),
            vec![DpiChoice::Range { min: 100, max: 25600, step: 50 }]
        );
    }

    #[test]
    fn decoders() {
        let mut d = [0u8; 16];
        // G502 X PLUS: 5 profiles + 2 factory, 11 buttons, 16 sectors of 255 bytes
        d[..11].copy_from_slice(&[0x01, 0x05, 0x01, 0x05, 0x02, 0x0B, 0x10, 0x00, 0xFF, 0x0A, 0x00]);
        let i = OnboardInfo::parse(&d);
        assert_eq!((i.profile_count, i.profile_count_oob, i.button_count, i.sector_count, i.sector_size), (5, 2, 11, 16, 255));

        let mut f = [0u8; 16];
        f[..8].copy_from_slice(&[0, b'U', b'1', b'4', 0x46, 0x01, 0x00, 0x18]);
        assert_eq!(FwInfo::parse(&f).version(), "U14 46.01.B0018");

        let mut e = [0u8; 16];
        e[..4].copy_from_slice(&[0x22, 0x01, 0x10, 0x00]);
        assert_eq!(feature_set_entry(&e).0, 0x2201);
        assert_eq!(extended_rate_us(3), Some(1000));
    }
}

/// RGB effects (0x8071) set live, without writing the onboard memory.
///
/// fn 0 getInfo(cluster | 0xFF, effect | 0xFF, 0) · fn 1 setClusterEffect(cluster,
/// effect *index*, 10 parameter bytes, persistence) · fn 5 software control
/// (`01 03 04` take, `01 00 00` give back: the mouse shows its profile again).
/// Relevé sur la G502 X PLUS : une zone, effets 0 éteint, 1 fixe, 0x0A respiration, 3 cycle.
pub mod rgb {
    pub const EFFECT_OFF: u16 = 0x0000;
    pub const EFFECT_STATIC: u16 = 0x0001;
    pub const EFFECT_CYCLE: u16 = 0x0003;
    pub const EFFECT_BREATHING: u16 = 0x000A;
    pub const SW_CONTROL_TAKE: [u8; 3] = [0x01, 0x03, 0x04];
    pub const SW_CONTROL_RELEASE: [u8; 3] = [0x01, 0x00, 0x00];

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LiveEffect {
        Off,
        Static([u8; 3]),
        Breathing { rgb: [u8; 3], period_ms: u16 },
        Cycle { period_ms: u16 },
    }

    impl LiveEffect {
        pub fn effect_id(&self) -> u16 {
            match self {
                Self::Off => EFFECT_OFF,
                Self::Static(_) => EFFECT_STATIC,
                Self::Breathing { .. } => EFFECT_BREATHING,
                Self::Cycle { .. } => EFFECT_CYCLE,
            }
        }
    }

    /// Parameters of setClusterEffect: `effect_index` is the position of the
    /// effect in the cluster's list (getInfo), not its id.
    pub fn cluster_effect(cluster: u8, effect_index: u8, effect: &LiveEffect) -> [u8; 13] {
        let mut p = [0u8; 13];
        p[0] = cluster;
        p[1] = effect_index;
        let q = &mut p[2..12];
        match *effect {
            LiveEffect::Off => {}
            LiveEffect::Static(rgb) => {
                q[..3].copy_from_slice(&rgb);
                q[3] = 0x02; // no ramp: switch to the colour at once
            }
            LiveEffect::Breathing { rgb, period_ms } => {
                q[..3].copy_from_slice(&rgb);
                q[3..5].copy_from_slice(&period_ms.to_be_bytes());
                q[6] = 100; // intensity %
            }
            LiveEffect::Cycle { period_ms } => {
                q[5..7].copy_from_slice(&period_ms.to_be_bytes());
                q[7] = 100;
            }
        }
        p[12] = 0x01;
        p
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn layouts() {
            // the request that turned the G502 X PLUS red
            assert_eq!(cluster_effect(0, 1, &LiveEffect::Static([0xFF, 0, 0])), [0, 1, 0xFF, 0, 0, 0x02, 0, 0, 0, 0, 0, 0, 0x01]);
            let b = cluster_effect(0, 2, &LiveEffect::Breathing { rgb: [1, 2, 3], period_ms: 4000 });
            assert_eq!(&b[2..9], &[1, 2, 3, 0x0F, 0xA0, 0, 100]);
            let c = cluster_effect(0, 3, &LiveEffect::Cycle { period_ms: 5000 });
            assert_eq!(&c[7..10], &[0x13, 0x88, 100]);
            assert_eq!(cluster_effect(0, 0, &LiveEffect::Off)[2..12], [0; 10]);
        }
    }
}

/// Per-LED lighting (PerKeyLightingV2, 0x8081), driven live like G HUB does.
///
/// fn 0 getInfo(0, 0) → LED ids present as a bitmap from byte 2 (bit n of byte
/// 2 + k = LED 8k + n) · fn 1 up to 4 × [led, R, G, B] · fn 7 frame end.
/// Shown only while the firmware effect is off: take software control of
/// 0x8071 (fn 5 `01 03 04`), then 0x8071 fn 1 `FF, effect count, 0…, 01`.
/// Relevé sur la G502 X PLUS : LED 0 à 8.
pub mod per_led {
    pub const FEATURE: u16 = 0x8081;
    pub const FN_INFO: u8 = 0;
    pub const FN_SET: u8 = 1;
    pub const FN_FRAME_END: u8 = 7;
    pub const FRAME_END: [u8; 5] = [0; 5];

    /// LED ids announced by getInfo(0, 0).
    pub fn led_ids(info: &[u8; 16]) -> Vec<u8> {
        let mut ids = Vec::new();
        for (k, byte) in info[2..].iter().enumerate() {
            for bit in 0..8 {
                if byte & (1 << bit) != 0 {
                    ids.push((8 * k + bit) as u8);
                }
            }
        }
        ids
    }

    /// The fn 1 requests that set `leds` (at most 4 per request).
    pub fn set_requests(leds: &[(u8, [u8; 3])]) -> Vec<Vec<u8>> {
        leds.chunks(4).map(|c| c.iter().flat_map(|(id, rgb)| [*id, rgb[0], rgb[1], rgb[2]]).collect()).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn g502_x_plus_leds() {
            let mut info = [0u8; 16];
            info[..4].copy_from_slice(&[0x00, 0x00, 0xFF, 0x01]); // what the mouse answered
            assert_eq!(led_ids(&info), (0..=8).collect::<Vec<u8>>());
            let reqs = set_requests(&[(0, [1, 2, 3]), (1, [4, 5, 6]), (2, [0; 3]), (3, [0; 3]), (8, [9, 9, 9])]);
            assert_eq!(reqs.len(), 2);
            assert_eq!(&reqs[0][..8], &[0, 1, 2, 3, 1, 4, 5, 6]);
            assert_eq!(reqs[1], vec![8, 9, 9, 9]);
        }
    }
}
