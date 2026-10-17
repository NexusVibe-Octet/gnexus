//! Logitech HID++ headsets (PRO X Wireless 046D:0ABA): its USB dongle answers
//! HID++ 2.0 at device index 0xFF on the vendor collection 0xFF43 / 0x0202.
//!
//! What it offers (probed 2026-10-17, see docs/ARCHITECTURE.md):
//! - 0x1F20 ADC measurement: fn 0 = battery voltage (mV, big-endian) + state
//!   (3 = charging); fn 1 / fn 2 = get / set the automatic power-off delay (minutes, 0 = never);
//! - 0x8300 sidetone (the mic heard in the headset): fn 0 / fn 1 = get / set 0..100;
//! - 0x8310 equalizer (in the headset): fn 0 = bands / dB range, fn 1 = frequencies
//!   (7 per call), fn 2 = gains, fn 3 = set gains (`02` + one signed byte per band).
//!   It keeps the settings but does NOT change the sound (listening test): G13 Studio uses
//!   the equalizer of the system instead (`platform::output_eq`).
//!
//! When the headset is off, the dongle answers with an HID++ error.

pub mod eq;
pub mod worker;

pub use worker::{HeadsetCommand, HeadsetStatus, HeadsetWorker};

use hidapi::HidApi;
use serde::{Deserialize, Serialize};

use crate::mouse::Link;

const LOGITECH: u16 = 0x046D;
pub const ADC_MEASUREMENT: u16 = 0x1F20;
pub const SIDETONE: u16 = 0x8300;
pub const EQUALIZER: u16 = 0x8310;
/// Power-off delays the headset accepts (minutes, 0 = never), as G HUB offers them.
pub const SLEEP_CHOICES: [u8; 7] = [0, 5, 10, 15, 30, 60, 90];

/// The headset's built-in equalizer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equalizer {
    /// Band centre frequencies (Hz).
    pub freqs: Vec<u16>,
    /// Gain per band (dB).
    pub gains: Vec<i8>,
    pub min_db: i8,
    pub max_db: i8,
}

/// Battery level from the voltage (piecewise-linear, HeadsetControl's table for the PRO series).
pub fn battery_percent(mv: u16) -> Option<u8> {
    const TABLE: [(u16, u8); 6] = [(4150, 100), (3830, 50), (3780, 30), (3740, 20), (3670, 5), (3320, 0)];
    if mv < TABLE[TABLE.len() - 1].0 {
        return None;
    }
    for w in TABLE.windows(2) {
        let ((hv, hp), (lv, lp)) = (w[0], w[1]);
        if mv >= hv {
            return Some(hp);
        }
        if mv >= lv {
            let frac = f32::from(mv - lv) / f32::from(hv - lv);
            return Some(lp + (frac * f32::from(hp - lp)) as u8);
        }
    }
    Some(0)
}

/// An open headset dongle and the feature indices it has.
pub struct Headset {
    pub link: Link,
    pub product_id: u16,
    pub adc: Option<u8>,
    pub sidetone: Option<u8>,
    pub eq: Option<u8>,
}

impl Headset {
    /// Read the feature table (only possible while the headset is on).
    pub fn resolve(&mut self) -> bool {
        let l = &self.link;
        let ok = l.call(0, 1, &[0, 0, 0x5A]).is_ok_and(|d| d[0] >= 2);
        if ok {
            self.adc = l.feature(ADC_MEASUREMENT);
            self.sidetone = l.feature(SIDETONE);
            self.eq = l.feature(EQUALIZER);
        }
        ok
    }

    pub fn name(&self) -> Option<String> {
        self.link.name()
    }

    /// Voltage (mV) and charging.
    pub fn battery(&self) -> Result<(u16, bool), String> {
        let i = self.adc.ok_or("pas de mesure de batterie")?;
        let d = self.link.call(i, 0, &[])?;
        Ok((u16::from_be_bytes([d[0], d[1]]), d[2] == 3))
    }

    pub fn sidetone(&self) -> Result<u8, String> {
        let i = self.sidetone.ok_or("pas de retour micro")?;
        Ok(self.link.call(i, 0, &[])?[0])
    }

    /// Set the sidetone and read it back.
    pub fn set_sidetone(&self, level: u8) -> Result<u8, String> {
        let i = self.sidetone.ok_or("pas de retour micro")?;
        self.link.call(i, 1, &[level.min(100)])?;
        self.sidetone()
    }

    pub fn sleep_minutes(&self) -> Result<u8, String> {
        let i = self.adc.ok_or("pas de mise en veille")?;
        Ok(self.link.call(i, 1, &[])?[0])
    }

    /// Set the power-off delay and read it back.
    pub fn set_sleep_minutes(&self, minutes: u8) -> Result<u8, String> {
        let i = self.adc.ok_or("pas de mise en veille")?;
        self.link.call(i, 2, &[minutes])?;
        self.sleep_minutes()
    }

    pub fn equalizer(&self) -> Result<Equalizer, String> {
        let i = self.eq.ok_or("pas d'égaliseur")?;
        let info = self.link.call(i, 0, &[])?;
        let count = usize::from(info[0]).min(16);
        let range = info[1] as i8;
        let min_db = if info[3] == 0 { -range } else { info[3] as i8 };
        let max_db = if info[4] == 0 { range } else { info[4] as i8 };
        let mut freqs = Vec::with_capacity(count);
        let mut start = 0;
        while freqs.len() < count {
            let d = self.link.call(i, 1, &[start as u8])?;
            for b in 0..7 {
                if freqs.len() >= count {
                    break;
                }
                freqs.push(u16::from_be_bytes([d[1 + 2 * b], d[2 + 2 * b]]));
            }
            start += 7;
        }
        let g = self.link.call(i, 2, &[0])?;
        let gains = g[..count].iter().map(|&x| x as i8).collect();
        Ok(Equalizer { freqs, gains, min_db, max_db })
    }

    /// Set every band's gain (clamped to the headset's range), then read the equalizer back.
    pub fn set_equalizer(&self, gains: &[i8]) -> Result<Equalizer, String> {
        let i = self.eq.ok_or("pas d'égaliseur")?;
        let cur = self.equalizer()?;
        if gains.len() != cur.gains.len() {
            return Err(format!("{} bandes attendues", cur.gains.len()));
        }
        let mut params = vec![0x02];
        params.extend(gains.iter().map(|&g| g.clamp(cur.min_db, cur.max_db) as u8));
        self.link.call(i, 3, &params)?;
        self.equalizer()
    }
}

/// Every Logitech headset dongle with an HID++ collection (on or off).
pub fn find_headsets(api: &HidApi) -> Vec<Headset> {
    let mut out = Vec::new();
    for info in api.device_list().filter(|d| d.vendor_id() == LOGITECH) {
        if info.usage_page() != 0xFF43 || info.usage() != 0x0202 {
            continue;
        }
        let Ok(dev) = info.open_device(api) else { continue };
        let link = Link {
            dev,
            index: logi_hidpp::DIRECT,
            timeout: crate::mouse::link::TIMEOUT,
            sw_id: logi_hidpp::SW_ID,
            events: Default::default(),
        };
        let mut h = Headset { link, product_id: info.product_id(), adc: None, sidetone: None, eq: None };
        h.resolve();
        out.push(h);
    }
    out
}

/// Fallback name when the headset is off (the dongle alone does not say).
pub fn product_name(product_id: u16) -> &'static str {
    match product_id {
        0x0ABA => "PRO X Wireless",
        _ => "Casque Logitech",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_curve() {
        assert_eq!(battery_percent(4200), Some(100));
        assert_eq!(battery_percent(4150), Some(100));
        assert_eq!(battery_percent(3957), Some(69));
        assert_eq!(battery_percent(3830), Some(50));
        assert_eq!(battery_percent(3700), Some(11));
        assert_eq!(battery_percent(3320), Some(0));
        assert_eq!(battery_percent(3000), None);
    }
}
