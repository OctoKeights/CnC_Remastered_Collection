//! The games' deterministic random number generators.
//!
//! Multiplayer and replays depend on every machine drawing exactly the same sequence, so
//! these reproduce the originals bit for bit.

/// Tiberian Dawn's simulation RNG (`Sim_Random` in `RAND.CPP`): a walk through a fixed
/// permutation of 0..=255.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimRandom {
    index: u8,
}

impl SimRandom {
    const TABLE: [u8; 256] = [
        0x47, 0xce, 0xc6, 0x6e, 0xd7, 0x9f, 0x98, 0x29, 0x92, 0x0c, 0x74, 0xa2, 0x65, 0x20, 0x4b,
        0x4f, 0x1e, 0xed, 0x3a, 0xdf, 0xa5, 0x7d, 0xb5, 0xc8, 0x86, 0x01, 0x81, 0xca, 0xf1, 0x17,
        0xd6, 0x23, 0xe1, 0xbd, 0x0e, 0xe4, 0x62, 0xfa, 0xd9, 0x5c, 0x68, 0xf5, 0x7f, 0xdc, 0xe7,
        0xb9, 0xc4, 0xb3, 0x7a, 0xd8, 0x06, 0x3e, 0xeb, 0x09, 0x1a, 0x31, 0x3f, 0x46, 0x28, 0x12,
        0xf0, 0x10, 0x84, 0x76, 0x3b, 0xc5, 0x53, 0x18, 0x14, 0x73, 0x7e, 0x59, 0x48, 0x93, 0xaa,
        0x1d, 0x5d, 0x79, 0x24, 0x61, 0x1b, 0xfd, 0x2b, 0xa8, 0xc2, 0xdb, 0xe8, 0x2a, 0xb0, 0x25,
        0x95, 0xab, 0x96, 0x83, 0xfc, 0x5f, 0x9c, 0x32, 0x78, 0x9a, 0x9e, 0xe2, 0x8e, 0x35, 0x4c,
        0x41, 0xa1, 0x69, 0x5a, 0xfe, 0xa7, 0xa4, 0xf6, 0x6d, 0xc1, 0x58, 0x0a, 0xcf, 0xea, 0xc3,
        0xba, 0x85, 0x99, 0x8d, 0x36, 0xb6, 0xdd, 0xd3, 0x04, 0xe6, 0x45, 0x0d, 0x60, 0xae, 0xa3,
        0x22, 0x4d, 0xe9, 0xc9, 0x9b, 0xb7, 0x0f, 0x02, 0x42, 0xf9, 0x0b, 0x8f, 0x43, 0x44, 0x87,
        0x70, 0xbe, 0xe3, 0xf8, 0xee, 0xa9, 0xbc, 0xc0, 0x67, 0x33, 0x16, 0x37, 0x57, 0xad, 0x5e,
        0x9d, 0x64, 0x40, 0x54, 0x05, 0x2c, 0xe0, 0xb2, 0x97, 0x08, 0xaf, 0x75, 0x8a, 0x5b, 0xfb,
        0x4e, 0xbf, 0x91, 0xf3, 0xcb, 0x7c, 0x63, 0xef, 0x89, 0x52, 0x6c, 0x2f, 0x21, 0x4a, 0xf7,
        0xcd, 0x2e, 0xf4, 0xc7, 0x6f, 0x19, 0xb1, 0x66, 0xcc, 0x90, 0x8c, 0x50, 0x51, 0x26, 0x7b,
        0xda, 0x49, 0x80, 0x30, 0x55, 0x1f, 0xd2, 0xb4, 0xd1, 0xd5, 0x6b, 0xf2, 0x72, 0xbb, 0x13,
        0x3d, 0xff, 0x15, 0x38, 0xe5, 0xd4, 0xde, 0x2d, 0x27, 0x94, 0xa0, 0xd0, 0x39, 0x82, 0x8b,
        0x03, 0xac, 0x3c, 0x34, 0x77, 0xb8, 0xec, 0x00, 0x07, 0x1c, 0x88, 0xa6, 0x56, 0x11, 0x71,
        0x6a,
    ];

    /// Starts the walk at the given table index (`SimRandIndex`).
    #[must_use]
    pub const fn with_index(index: u8) -> Self {
        Self { index }
    }

    /// The current table index, for saving games.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.index
    }

    /// Returns the next value, 0..=255. The index is advanced *before* the lookup.
    pub fn next_u8(&mut self) -> u8 {
        self.index = self.index.wrapping_add(1);
        Self::TABLE[usize::from(self.index)]
    }

    /// A value in `min..=max` (`Sim_IRandom`).
    pub fn range(&mut self, min: i32, max: i32) -> i32 {
        let span = max.wrapping_sub(min) as u32;
        fixed_to_cardinal(span, u32::from(self.next_u8())) as i32 + min
    }
}

/// Scales `base` by an 8.8 fraction, rounding, with the original's overflow rule
/// (`Fixed_To_Cardinal` in `MiscAsm.cpp`): results that don't fit in 16 bits give 0xFFFF.
#[must_use]
pub const fn fixed_to_cardinal(base: u32, fixed: u32) -> u32 {
    let product = base.wrapping_mul(fixed).wrapping_add(0x80);
    if product & 0xFF00_0000 != 0 {
        0xFFFF
    } else {
        product >> 8
    }
}

/// The inverse of [`fixed_to_cardinal`]: `cardinal / base` as an 8.8 fraction
/// (`Cardinal_To_Fixed`). A zero base gives 0xFFFF.
#[must_use]
pub const fn cardinal_to_fixed(base: u32, cardinal: u32) -> u32 {
    match cardinal.wrapping_shl(8).checked_div(base) {
        Some(value) => value,
        None => 0xFFFF,
    }
}

/// Red Alert's linear congruential generator (`RandomClass` in `RANDOM.CPP`), yielding
/// 15-bit values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lcg {
    /// The generator state. Public because the game saves and synchronises it directly.
    pub seed: u32,
}

impl Lcg {
    const MULTIPLIER: u32 = 0x41C6_4E6D;
    const INCREMENT: u32 = 0x0000_3039;
    const DISCARD_BITS: u32 = 10;
    /// Number of meaningful bits in each result.
    pub const SIGNIFICANT_BITS: u32 = 15;

    /// Creates a generator with the given seed.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self { seed }
    }

    /// Returns the next value, 0..=32767.
    pub fn next_u15(&mut self) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::INCREMENT);
        (self.seed >> Self::DISCARD_BITS) & ((1 << Self::SIGNIFICANT_BITS) - 1)
    }

    /// A value in `min..=max` (bounds may be given in either order).
    ///
    /// Uses the original's rejection sampling so the number of draws consumed matches.
    pub fn range(&mut self, min: i32, max: i32) -> i32 {
        if min == max {
            return min;
        }
        let (low, high) = if min > max { (max, min) } else { (min, max) };
        let magnitude = high.wrapping_sub(low) as u32;

        let mut high_bit = Self::SIGNIFICANT_BITS - 1;
        while magnitude & (1 << high_bit) == 0 && high_bit > 0 {
            high_bit -= 1;
        }
        let mask = (1u32 << (high_bit + 1)) - 1;

        loop {
            let pick = self.next_u15() & mask;
            if pick <= magnitude {
                return low.wrapping_add(pick as i32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_table_is_a_permutation() {
        let mut seen = [false; 256];
        for &v in &SimRandom::TABLE {
            assert!(!seen[usize::from(v)]);
            seen[usize::from(v)] = true;
        }
    }

    #[test]
    fn sim_random_advances_first() {
        let mut rng = SimRandom::default();
        assert_eq!(rng.next_u8(), 0xce);
        assert_eq!(rng.index(), 1);
        let mut wrap = SimRandom::with_index(255);
        assert_eq!(wrap.next_u8(), 0x47);
    }

    #[test]
    fn sim_range_stays_in_bounds() {
        let mut rng = SimRandom::default();
        for _ in 0..512 {
            let v = rng.range(10, 20);
            assert!((10..=20).contains(&v), "{v}");
        }
    }

    #[test]
    fn lcg_known_sequence() {
        let mut rng = Lcg::new(0);
        // seed1 = 0x3039 -> 0x3039 >> 10 = 12
        assert_eq!(rng.next_u15(), 12);
        let expected_seed = 0x3039u32.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        assert_eq!(rng.next_u15(), (expected_seed >> 10) & 0x7FFF);
    }

    #[test]
    fn lcg_range() {
        let mut rng = Lcg::new(1234);
        assert_eq!(rng.range(5, 5), 5);
        for _ in 0..1000 {
            let v = rng.range(9, -3);
            assert!((-3..=9).contains(&v));
        }
    }

    #[test]
    fn cardinal_conversions() {
        assert_eq!(fixed_to_cardinal(100, 128), 50);
        assert_eq!(fixed_to_cardinal(0x10_0000, 0x100), 0xFFFF);
        assert_eq!(cardinal_to_fixed(4, 1), 64);
        assert_eq!(cardinal_to_fixed(0, 1), 0xFFFF);
    }
}
