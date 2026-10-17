/// Standard 24-bit RGB Color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255 };
    pub const RED: Self = Self { r: 255, g: 0, b: 0 };
    pub const GREEN: Self = Self { r: 0, g: 255, b: 0 };
    pub const BLUE: Self = Self { r: 0, g: 0, b: 255 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const CYAN: Self = Self { r: 0, g: 255, b: 255 };
    pub const MAGENTA: Self = Self { r: 255, g: 0, b: 255 };
    pub const YELLOW: Self = Self { r: 255, g: 255, b: 0 };
    pub const ORANGE: Self = Self { r: 255, g: 128, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Feature Report 0x07 builder for G13 RGB Backlight (5 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BacklightReport {
    bytes: [u8; 5],
}

impl BacklightReport {
    pub const REPORT_ID: u8 = 0x07;

    pub const fn new(color: RgbColor) -> Self {
        Self {
            bytes: [Self::REPORT_ID, color.r, color.g, color.b, 0x00],
        }
    }

    pub const fn as_bytes(&self) -> &[u8; 5] {
        &self.bytes
    }
}

/// Flags to control individual M-Key status LEDs (M1, M2, M3, MR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MKeyLeds {
    pub m1: bool,
    pub m2: bool,
    pub m3: bool,
    pub mr: bool,
}

impl MKeyLeds {
    pub const REPORT_ID: u8 = 0x05;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn m1(mut self, on: bool) -> Self {
        self.m1 = on;
        self
    }

    pub fn m2(mut self, on: bool) -> Self {
        self.m2 = on;
        self
    }

    pub fn m3(mut self, on: bool) -> Self {
        self.m3 = on;
        self
    }

    pub fn mr(mut self, on: bool) -> Self {
        self.mr = on;
        self
    }

    /// Produce the 5-byte Feature Report 0x05.
    pub fn to_report(&self) -> [u8; 5] {
        let mut mask = 0u8;
        if self.m1 { mask |= 0x01; }
        if self.m2 { mask |= 0x02; }
        if self.m3 { mask |= 0x04; }
        if self.mr { mask |= 0x08; }
        [Self::REPORT_ID, mask, 0x00, 0x00, 0x00]
    }
}
