/// All 34 discrete buttons and keys on the Logitech G13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum G13Key {
    // Top row G-keys (G1 to G7)
    G1,
    G2,
    G3,
    G4,
    G5,
    G6,
    G7,

    // Row 2 G-keys (G8 to G14)
    G8,
    G9,
    G10,
    G11,
    G12,
    G13,
    G14,

    // Row 3 G-keys (G15 to G19)
    G15,
    G16,
    G17,
    G18,
    G19,

    // Row 4 Bottom G-keys (G20 to G22)
    G20,
    G21,
    G22,

    // Thumb buttons
    /// Thumb lateral key (left of joystick)
    G23,
    /// Thumb bottom key (below joystick / space bar)
    G24,
    /// Joystick physical click (G25)
    StickClick,

    // Profile & Macro Keys
    M1,
    M2,
    M3,
    MR,

    // LCD Utility Keys
    /// Round backlight toggle button
    Light,
    /// Round LCD button 1 (leftmost)
    L1,
    /// Round LCD button 2
    L2,
    /// Round LCD button 3
    L3,
    /// Round LCD button 4 (rightmost)
    L4,
}

impl G13Key {
    /// Total number of discrete keys reported by the device.
    pub const COUNT: usize = 34;

    /// Every key, in [`Self::bit_index`] order. `ALL[i].bit_index() == i`.
    pub const ALL: [G13Key; Self::COUNT] = [
        Self::G1, Self::G2, Self::G3, Self::G4, Self::G5, Self::G6, Self::G7, Self::G8,
        Self::G9, Self::G10, Self::G11, Self::G12, Self::G13, Self::G14, Self::G15, Self::G16,
        Self::G17, Self::G18, Self::G19, Self::G20, Self::G21, Self::G22, Self::G23, Self::G24,
        Self::StickClick, Self::Light, Self::L1, Self::L2, Self::L3, Self::L4, Self::M1, Self::M2,
        Self::M3, Self::MR,
    ];

    /// Bit position of this key inside the 40-bit field formed by bytes 3..=7
    /// of Input Report 0x01, as measured on real hardware (see
    /// `g13-protocol-spec.md` §3.2). Not contiguous: bits 23 and 36..=39 are
    /// status flags, bit 22 is unused.
    pub const fn report_bit(self) -> u8 {
        match self {
            Self::Light => 24,
            Self::L1 => 25,
            Self::L2 => 26,
            Self::L3 => 27,
            Self::L4 => 28,
            Self::M1 => 29,
            Self::M2 => 30,
            Self::M3 => 31,
            Self::MR => 32,
            Self::G23 => 33,
            Self::G24 => 34,
            Self::StickClick => 35,
            // G1..G22 sit at bits 0..=21, in order.
            other => other.bit_index(),
        }
    }

    /// Compact index of this key (0..34) — its bit inside
    /// [`crate::G13InputReport::key_mask`]. G1..G24 + stick click come first
    /// (0..=24), so bindable keys can index a 25-slot table directly.
    pub const fn bit_index(self) -> u8 {
        match self {
            Self::G1 => 0,
            Self::G2 => 1,
            Self::G3 => 2,
            Self::G4 => 3,
            Self::G5 => 4,
            Self::G6 => 5,
            Self::G7 => 6,
            Self::G8 => 7,
            Self::G9 => 8,
            Self::G10 => 9,
            Self::G11 => 10,
            Self::G12 => 11,
            Self::G13 => 12,
            Self::G14 => 13,
            Self::G15 => 14,
            Self::G16 => 15,
            Self::G17 => 16,
            Self::G18 => 17,
            Self::G19 => 18,
            Self::G20 => 19,
            Self::G21 => 20,
            Self::G22 => 21,
            Self::G23 => 22,
            Self::G24 => 23,
            Self::StickClick => 24,
            Self::Light => 25,
            Self::L1 => 26,
            Self::L2 => 27,
            Self::L3 => 28,
            Self::L4 => 29,
            Self::M1 => 30,
            Self::M2 => 31,
            Self::M3 => 32,
            Self::MR => 33,
        }
    }

    /// Single-bit mask of this key inside [`crate::G13InputReport::key_mask`].
    pub const fn mask(self) -> u64 {
        1u64 << self.bit_index()
    }

    /// Inverse of [`Self::bit_index`].
    pub const fn from_bit_index(index: u8) -> Option<Self> {
        if (index as usize) < Self::COUNT {
            Some(Self::ALL[index as usize])
        } else {
            None
        }
    }

    /// Short stable identifier (`"G1"`, `"G25"`, `"M1"`, `"BD"`, `"L1"`…),
    /// suitable for config files and IPC.
    pub const fn id(&self) -> &'static str {
        match self {
            Self::StickClick => "G25",
            Self::Light => "BD",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
            Self::L4 => "L4",
            Self::G23 => "G23",
            Self::G24 => "G24",
            other => other.name(),
        }
    }

    /// Parse a key from its [`Self::id`] (case-insensitive). Also accepts
    /// `"StickClick"` and `"Light"`.
    pub fn from_id(s: &str) -> Option<Self> {
        let mut i = 0;
        while i < Self::COUNT {
            let k = Self::ALL[i];
            if k.id().eq_ignore_ascii_case(s) {
                return Some(k);
            }
            i += 1;
        }
        if s.eq_ignore_ascii_case("StickClick") {
            Some(Self::StickClick)
        } else if s.eq_ignore_ascii_case("Light") {
            Some(Self::Light)
        } else {
            None
        }
    }

    /// True for the 25 user-bindable keys (G1..G24 + stick click).
    pub const fn is_g_key(self) -> bool {
        self.bit_index() <= 24
    }

    /// Return the friendly human-readable name of the key.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::G1 => "G1",
            Self::G2 => "G2",
            Self::G3 => "G3",
            Self::G4 => "G4",
            Self::G5 => "G5",
            Self::G6 => "G6",
            Self::G7 => "G7",
            Self::G8 => "G8",
            Self::G9 => "G9",
            Self::G10 => "G10",
            Self::G11 => "G11",
            Self::G12 => "G12",
            Self::G13 => "G13",
            Self::G14 => "G14",
            Self::G15 => "G15",
            Self::G16 => "G16",
            Self::G17 => "G17",
            Self::G18 => "G18",
            Self::G19 => "G19",
            Self::G20 => "G20",
            Self::G21 => "G21",
            Self::G22 => "G22",
            Self::G23 => "G23 (Thumb Left)",
            Self::G24 => "G24 (Thumb Bottom)",
            Self::StickClick => "Stick Click (G25)",
            Self::M1 => "M1",
            Self::M2 => "M2",
            Self::M3 => "M3",
            Self::MR => "MR",
            Self::Light => "Backlight Key",
            Self::L1 => "LCD 1",
            Self::L2 => "LCD 2",
            Self::L3 => "LCD 3",
            Self::L4 => "LCD 4",
        }
    }
}
