//! Edge detection: turns successive 8-byte input reports into
//! `Pressed` / `Released` transitions and filtered stick movements.

use g13_core::{G13InputReport, G13Key, StickPosition};

use super::DeviceEvent;

#[derive(Debug, Clone)]
pub struct InputTracker {
    prev_keys: u64,
    prev_stick: Option<StickPosition>,
    /// Minimum change (raw units, per axis) before a stick event is emitted;
    /// filters potentiometer noise.
    stick_hysteresis: u8,
}

impl InputTracker {
    pub fn new(stick_hysteresis: u8) -> Self {
        Self { prev_keys: 0, prev_stick: None, stick_hysteresis }
    }

    /// Bitmask of the keys currently considered held.
    pub fn held_mask(&self) -> u64 {
        self.prev_keys
    }

    /// Feed one report; `emit` is called for every resulting event.
    ///
    /// Ordering guarantees within one report: all releases first, then all
    /// presses (each group in bit order), then the stick event.
    pub fn update(&mut self, report: &G13InputReport, mut emit: impl FnMut(DeviceEvent)) {
        let now = report.key_mask();
        let changed = now ^ self.prev_keys;
        if changed != 0 {
            Self::emit_edges(changed & self.prev_keys, false, &mut emit);
            Self::emit_edges(changed & now, true, &mut emit);
            self.prev_keys = now;
        }

        let stick = report.stick();
        let moved = match self.prev_stick {
            None => true,
            Some(p) => {
                p.raw_x.abs_diff(stick.raw_x) >= self.stick_hysteresis
                    || p.raw_y.abs_diff(stick.raw_y) >= self.stick_hysteresis
                    // always report reaching the physical end stops
                    || (stick != p && (matches!(stick.raw_x, 0 | 255) || matches!(stick.raw_y, 0 | 255)))
            }
        };
        if moved {
            self.prev_stick = Some(stick);
            emit(DeviceEvent::Stick(stick));
        }
    }

    /// Emit a release for every held key and reset (used on disconnect).
    pub fn release_all(&mut self, mut emit: impl FnMut(DeviceEvent)) {
        Self::emit_edges(self.prev_keys, false, &mut emit);
        self.prev_keys = 0;
        self.prev_stick = None;
    }

    fn emit_edges(mut bits: u64, pressed: bool, emit: &mut impl FnMut(DeviceEvent)) {
        while bits != 0 {
            let idx = bits.trailing_zeros() as u8;
            bits &= bits - 1;
            if let Some(key) = G13Key::from_bit_index(idx) {
                emit(DeviceEvent::Key { key, pressed });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a raw report from a `key_mask()`-style mask (+ the idle status bits).
    fn report(keys: u64, x: u8, y: u8) -> G13InputReport {
        let mut raw = 0x80_0080_0000u64; // byte 5 bit 7, byte 7 bit 7
        for k in G13Key::ALL {
            if keys & k.mask() != 0 {
                raw |= 1 << k.report_bit();
            }
        }
        let mut b = [0u8; 8];
        b[0] = 1;
        b[1] = x;
        b[2] = y;
        b[3..8].copy_from_slice(&raw.to_le_bytes()[..5]);
        G13InputReport::from_bytes(&b).unwrap()
    }

    fn collect(t: &mut InputTracker, r: &G13InputReport) -> Vec<DeviceEvent> {
        let mut v = Vec::new();
        t.update(r, |e| v.push(e));
        v.retain(|e| !matches!(e, DeviceEvent::Stick(_)));
        v
    }

    #[test]
    fn press_and_release_edges() {
        let mut t = InputTracker::new(3);
        let g1 = G13Key::G1.mask();
        let mr = G13Key::MR.mask();
        assert_eq!(collect(&mut t, &report(g1, 128, 128)), vec![DeviceEvent::Key { key: G13Key::G1, pressed: true }]);
        // repeated identical report → nothing
        assert!(collect(&mut t, &report(g1, 128, 128)).is_empty());
        // G1 released and MR pressed in the same report: release first
        assert_eq!(
            collect(&mut t, &report(mr, 128, 128)),
            vec![
                DeviceEvent::Key { key: G13Key::G1, pressed: false },
                DeviceEvent::Key { key: G13Key::MR, pressed: true },
            ]
        );
    }

    #[test]
    fn release_all_on_disconnect() {
        let mut t = InputTracker::new(3);
        collect(&mut t, &report(G13Key::G5.mask() | G13Key::M3.mask(), 128, 128));
        let mut v = Vec::new();
        t.release_all(|e| v.push(e));
        assert_eq!(
            v,
            vec![
                DeviceEvent::Key { key: G13Key::G5, pressed: false },
                DeviceEvent::Key { key: G13Key::M3, pressed: false },
            ]
        );
        assert_eq!(t.held_mask(), 0);
    }

    #[test]
    fn stick_hysteresis() {
        let mut t = InputTracker::new(3);
        let mut n = 0;
        t.update(&report(0, 128, 128), |_| n += 1); // first report always emits stick
        t.update(&report(0, 129, 127), |_| n += 1); // noise
        t.update(&report(0, 131, 128), |_| n += 1); // real move
        t.update(&report(0, 255, 128), |_| n += 1); // end stop
        assert_eq!(n, 3);
    }
}
