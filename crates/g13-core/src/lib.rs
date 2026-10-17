//! # g13-core
//!
//! Pure, OS-independent protocol definitions, zero-allocation packet parsers,
//! and monochrome LCD framebuffer renderer for the Logitech G13 Advanced Gameboard.

pub mod input;
pub mod keys;
pub mod lcd;
pub mod output;

// Re-exports for ergonomic use
pub use input::{G13InputReport, StickPosition};
pub use keys::G13Key;
pub use lcd::{G13LcdBuffer, LCD_HEIGHT, LCD_WIDTH};
pub use output::{BacklightReport, MKeyLeds, RgbColor};

pub const G13_VID: u16 = 0x046d;
pub const G13_PID: u16 = 0xc21c;
