//! Series and status colors from lite's `theme.rs`, which takes them from the
//! React app's `tokens.css`. With these, graphs read the same in every port.
//! Surfaces, text and accents should come from each toolkit's own theme.

use crate::derived::Level;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `0xRRGGBB`.
    pub const fn hex(self) -> u32 {
        (self.0 as u32) << 16 | (self.1 as u32) << 8 | self.2 as u32
    }
}

pub const CPU: Rgb = Rgb(91, 156, 246);
pub const MEMORY: Rgb = Rgb(167, 139, 250);
pub const DISK: Rgb = Rgb(52, 211, 153);
pub const NETWORK: Rgb = Rgb(245, 158, 11);
pub const GPU: Rgb = Rgb(0xf4, 0x72, 0xb6);
pub const NPU: Rgb = Rgb(13, 148, 136);

pub const NORMAL: Rgb = Rgb(52, 211, 153);
pub const WARN: Rgb = Rgb(245, 158, 11);
pub const CRITICAL: Rgb = Rgb(239, 68, 68);

/// Memory composition bar segments.
pub const MEM_IN_USE: Rgb = CPU;
pub const MEM_MODIFIED: Rgb = WARN;
pub const MEM_CACHED: Rgb = NPU;

pub fn level(l: Level) -> Rgb {
    match l {
        Level::Normal => NORMAL,
        Level::Warn => WARN,
        Level::Critical => CRITICAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_packs_rgb() {
        assert_eq!(CPU.hex(), 0x5b9cf6);
        assert_eq!(level(Level::Critical), CRITICAL);
    }
}
