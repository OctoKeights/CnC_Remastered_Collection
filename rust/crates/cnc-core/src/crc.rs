//! Westwood's "CRC".
//!
//! It isn't a true CRC: the data is consumed as little-endian 32-bit words, and for each one
//! the accumulator is rotated left by one bit and the word is added. A trailing partial word
//! is zero-padded. This is the hash used for MIX file name lookup (`Calculate_CRC`), INI
//! section/entry indexing (`CRCEngine`) and the map editor's `CRC.Calculate`. All three
//! produce the same value.

/// Streaming form of the Westwood CRC, equivalent to `CRCEngine` in `CRC.H`.
///
/// Data may be fed in any number of pieces; the result is the same as hashing the
/// concatenation in one go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CrcEngine {
    crc: u32,
    staging: [u8; 4],
    index: usize,
}

impl CrcEngine {
    /// Creates an engine with the given initial accumulator (the original default is 0).
    #[must_use]
    pub const fn with_initial(initial: u32) -> Self {
        Self {
            crc: initial,
            staging: [0; 4],
            index: 0,
        }
    }

    /// Creates an engine with a zero accumulator.
    #[must_use]
    pub const fn new() -> Self {
        Self::with_initial(0)
    }

    /// Submits a single byte.
    pub fn push(&mut self, byte: u8) {
        self.staging[self.index] = byte;
        self.index += 1;
        if self.index == self.staging.len() {
            self.crc = self.value();
            self.staging = [0; 4];
            self.index = 0;
        }
    }

    /// Submits a buffer of bytes.
    pub fn update(&mut self, data: &[u8]) {
        let mut rest = data;

        // Top up any partially filled staging word first.
        while self.index != 0 {
            let Some((&first, tail)) = rest.split_first() else {
                return;
            };
            self.push(first);
            rest = tail;
        }

        let mut words = rest.chunks_exact(4);
        for word in &mut words {
            let word = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            self.crc = self.crc.rotate_left(1).wrapping_add(word);
        }
        for &byte in words.remainder() {
            self.push(byte);
        }
    }

    /// Returns the current CRC value, including any pending partial word.
    #[must_use]
    pub const fn value(&self) -> u32 {
        if self.index == 0 {
            self.crc
        } else {
            self.crc
                .rotate_left(1)
                .wrapping_add(u32::from_le_bytes(self.staging))
        }
    }
}

/// Computes the Westwood CRC of a buffer in one call (`Calculate_CRC`).
#[must_use]
pub fn crc(data: &[u8]) -> u32 {
    let mut engine = CrcEngine::new();
    engine.update(data);
    engine.value()
}

/// Computes the MIX file identifier for a file name: the CRC of the upper-cased name.
///
/// Matches `MixFileClass::Offset`, which `strupr`s the name before hashing.
#[must_use]
pub fn mix_id(filename: &str) -> u32 {
    crc(filename.to_ascii_uppercase().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Straight transliteration of the `Calculate_CRC` assembly in `MiscAsm.cpp`,
    /// used as an oracle.
    fn reference(data: &[u8]) -> u32 {
        let mut ebx: u32 = 0;
        let mut words = data.chunks_exact(4);
        for w in &mut words {
            let eax = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
            ebx = ebx.rotate_left(1).wrapping_add(eax);
        }
        let rem = words.remainder();
        if !rem.is_empty() {
            let mut eax: u32 = 0;
            for &b in rem {
                eax = (eax & !0xFF) | u32::from(b);
                eax = eax.rotate_right(8);
            }
            let shift = (4 - rem.len() as u32) * 8;
            eax = eax.rotate_right(shift);
            ebx = ebx.rotate_left(1).wrapping_add(eax);
        }
        ebx
    }

    #[test]
    fn matches_assembly_reference() {
        let samples: [&[u8]; 6] = [
            b"",
            b"A",
            b"AB",
            b"ABC",
            b"CONQUER.MIX",
            b"TEMPERAT.MIX and some more bytes",
        ];
        for s in samples {
            assert_eq!(crc(s), reference(s), "{s:?}");
        }
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data = b"The quick brown fox jumps over the lazy dog";
        for split in 0..data.len() {
            let mut engine = CrcEngine::new();
            engine.update(&data[..split]);
            engine.update(&data[split..]);
            assert_eq!(engine.value(), crc(data));
        }
    }

    #[test]
    fn mix_id_is_case_insensitive() {
        assert_eq!(mix_id("rules.ini"), mix_id("RULES.INI"));
        assert_eq!(mix_id("RULES.INI"), crc(b"RULES.INI"));
    }
}
