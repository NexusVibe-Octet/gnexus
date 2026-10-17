use crate::keys::G13Key;

/// Raw and normalized position of the analog mini-joystick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StickPosition {
    /// Raw X axis value (0 = left, ~128 = center, 255 = right)
    pub raw_x: u8,
    /// Raw Y axis value (0 = up, ~128 = center, 255 = down)
    pub raw_y: u8,
}

impl StickPosition {
    /// Return the normalized position as (x, y) where values range from -1.0 to +1.0.
    /// (0.0, 0.0) represents center.
    pub fn normalized(&self) -> (f32, f32) {
        let nx = (self.raw_x as f32 - 127.5) / 127.5;
        // Invert Y so that +1.0 is UP, -1.0 is DOWN
        let ny = -(self.raw_y as f32 - 127.5) / 127.5;
        (nx, ny)
    }

    /// Return true if the stick is within a given deadzone radius from the center (default ~0.15).
    pub fn in_deadzone(&self, radius: f32) -> bool {
        let (x, y) = self.normalized();
        (x * x + y * y).sqrt() <= radius
    }
}

/// Zero-copy, allocation-free parser for G13 Input Report ID 0x01 (8 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct G13InputReport {
    bytes: [u8; 8],
}

impl G13InputReport {
    pub const REPORT_ID: u8 = 0x01;
    pub const REPORT_LEN: usize = 8;

    /// Parse an 8-byte input report slice without allocation.
    pub fn from_bytes(slice: &[u8]) -> Option<Self> {
        if slice.len() < Self::REPORT_LEN || slice[0] != Self::REPORT_ID {
            return None;
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&slice[..8]);
        Some(Self { bytes })
    }

    /// Access raw underlying bytes.
    pub const fn raw(&self) -> &[u8; 8] {
        &self.bytes
    }

    /// Get analog stick position.
    pub fn stick(&self) -> StickPosition {
        StickPosition {
            raw_x: self.bytes[1],
            raw_y: self.bytes[2],
        }
    }

    /// Bytes 3..=7 as one 40-bit value, unprocessed (keys + status flags).
    pub fn raw_bits(&self) -> u64 {
        let b = &self.bytes;
        (b[3] as u64)
            | ((b[4] as u64) << 8)
            | ((b[5] as u64) << 16)
            | ((b[6] as u64) << 24)
            | ((b[7] as u64) << 32)
    }

    /// All 34 key states packed into a single integer: bit `n` is the key
    /// whose [`G13Key::bit_index`] is `n`. Status flags (backlight state,
    /// toggling bits) and unused bits are dropped.
    pub fn key_mask(&self) -> u64 {
        let raw = self.raw_bits();
        let mut mask = 0u64;
        let mut i = 0;
        while i < G13Key::COUNT {
            if raw & (1u64 << G13Key::ALL[i].report_bit()) != 0 {
                mask |= 1u64 << i;
            }
            i += 1;
        }
        mask
    }

    /// Mask of the 34 bits that correspond to real keys.
    pub const KEY_MASK_VALID: u64 = (1u64 << G13Key::COUNT) - 1;

    /// Check if a specific key is pressed.
    pub fn is_pressed(&self, key: G13Key) -> bool {
        self.key_mask() & key.mask() != 0
    }

    /// Iterate over all currently pressed keys without heap allocation.
    pub fn pressed_keys(&self) -> impl Iterator<Item = G13Key> + '_ {
        let mask = self.key_mask();
        G13Key::ALL.into_iter().filter(move |k| mask & k.mask() != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_index_roundtrip() {
        for (i, k) in G13Key::ALL.iter().enumerate() {
            assert_eq!(k.bit_index() as usize, i);
            assert_eq!(G13Key::from_bit_index(i as u8), Some(*k));
            assert_eq!(G13Key::from_id(k.id()), Some(*k));
        }
    }

    #[test]
    fn spec_bit_positions() {
        // Taken from g13-protocol-spec.md §3.2 (measured on hardware)
        let cases: [(usize, u8, G13Key); 12] = [
            (3, 0x01, G13Key::G1),
            (3, 0x80, G13Key::G8),
            (4, 0x40, G13Key::G15),
            (5, 0x20, G13Key::G22),
            (6, 0x01, G13Key::Light),
            (6, 0x02, G13Key::L1),
            (6, 0x10, G13Key::L4),
            (6, 0x80, G13Key::M3),
            (7, 0x01, G13Key::MR),
            (7, 0x02, G13Key::G23),
            (7, 0x04, G13Key::G24),
            (7, 0x08, G13Key::StickClick),
        ];
        for (byte, bit, key) in cases {
            let mut raw = [0x01, 0x80, 0x80, 0, 0, 0, 0, 0];
            raw[byte] = bit;
            let r = G13InputReport::from_bytes(&raw).unwrap();
            assert!(r.is_pressed(key), "{key:?}");
            assert_eq!(r.pressed_keys().count(), 1);
        }
    }

    #[test]
    fn m3_and_mr_are_listed() {
        let r = G13InputReport::from_bytes(&[0x01, 0x80, 0x80, 0, 0, 0, 0x80, 0x01]).unwrap();
        let mut it = r.pressed_keys();
        assert_eq!(it.next(), Some(G13Key::M3));
        assert_eq!(it.next(), Some(G13Key::MR));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn status_bits_ignored() {
        // Idle report captured on hardware: backlight-state bit (byte 5 bit 7)
        // and toggling bit (byte 7 bit 7) set, no key pressed.
        let r = G13InputReport::from_bytes(&[0x01, 0x7C, 0x6F, 0, 0, 0x80, 0, 0x80]).unwrap();
        assert_eq!(r.key_mask(), 0);
        let r = G13InputReport::from_bytes(&[0x01, 0x80, 0x80, 0, 0, 0xC0, 0, 0xF0]).unwrap();
        assert_eq!(r.key_mask(), 0);
    }

    #[test]
    fn report_bits_are_distinct() {
        for (i, a) in G13Key::ALL.iter().enumerate() {
            assert!(a.report_bit() < 40);
            for b in &G13Key::ALL[i + 1..] {
                assert_ne!(a.report_bit(), b.report_bit(), "{a:?} / {b:?}");
            }
        }
    }
}
