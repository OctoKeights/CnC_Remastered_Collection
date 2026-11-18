//! Blowfish (`BlowfishEngine` in `BLOWFISH.CPP`).
//!
//! Westwood's implementation is standard 16-round Blowfish with the usual tables; the
//! 64-bit block is read as two big-endian 32-bit halves. It is used in ECB mode.

use super::blowfish_tables::{P_INIT, S_INIT};

const ROUNDS: usize = 16;

/// A keyed Blowfish cipher.
#[derive(Clone)]
pub struct Blowfish {
    p: [u32; ROUNDS + 2],
    s: [[u32; 256]; 4],
}

impl std::fmt::Debug for Blowfish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print key material.
        f.write_str("Blowfish { .. }")
    }
}

impl Blowfish {
    /// Size of a block in bytes.
    pub const BLOCK_SIZE: usize = 8;
    /// Maximum key length in bytes (`MAX_KEY_LENGTH`).
    pub const MAX_KEY_LENGTH: usize = 56;

    /// Keys the cipher (`Submit_Key`). Keys longer than [`Self::MAX_KEY_LENGTH`] bytes use only
    /// their first 56 bytes; an empty key leaves the tables at their initial values.
    #[must_use]
    pub fn new(key: &[u8]) -> Self {
        let mut bf = Self {
            p: P_INIT,
            s: S_INIT,
        };
        let key = &key[..key.len().min(Self::MAX_KEY_LENGTH)];
        if key.is_empty() {
            return bf;
        }

        let mut bytes = key.iter().cycle();
        for entry in &mut bf.p {
            let word = (0..4).fold(0u32, |acc, _| {
                acc << 8 | u32::from(*bytes.next().unwrap_or(&0))
            });
            *entry ^= word;
        }

        let (mut left, mut right) = (0u32, 0u32);
        for i in (0..ROUNDS + 2).step_by(2) {
            (left, right) = bf.encrypt_halves(left, right);
            bf.p[i] = left;
            bf.p[i + 1] = right;
        }
        for sbox in 0..4 {
            for i in (0..256).step_by(2) {
                (left, right) = bf.encrypt_halves(left, right);
                bf.s[sbox][i] = left;
                bf.s[sbox][i + 1] = right;
            }
        }
        bf
    }

    /// The round function: four S-box lookups on the bytes of `half`, high byte first.
    fn round(&self, half: u32) -> u32 {
        let [b0, b1, b2, b3] = half.to_be_bytes().map(usize::from);
        (self.s[0][b0].wrapping_add(self.s[1][b1]) ^ self.s[2][b2]).wrapping_add(self.s[3][b3])
    }

    fn encrypt_halves(&self, mut left: u32, mut right: u32) -> (u32, u32) {
        for i in (0..ROUNDS).step_by(2) {
            left ^= self.p[i];
            right ^= self.round(left);
            right ^= self.p[i + 1];
            left ^= self.round(right);
        }
        (right ^ self.p[ROUNDS + 1], left ^ self.p[ROUNDS])
    }

    fn decrypt_halves(&self, mut left: u32, mut right: u32) -> (u32, u32) {
        for i in (2..=ROUNDS).rev().step_by(2) {
            left ^= self.p[i + 1];
            right ^= self.round(left);
            right ^= self.p[i];
            left ^= self.round(right);
        }
        (right ^ self.p[0], left ^ self.p[1])
    }

    fn process(block: &mut [u8; 8], op: impl Fn(u32, u32) -> (u32, u32)) {
        let left = u32::from_be_bytes([block[0], block[1], block[2], block[3]]);
        let right = u32::from_be_bytes([block[4], block[5], block[6], block[7]]);
        let (l, r) = op(left, right);
        block[..4].copy_from_slice(&l.to_be_bytes());
        block[4..].copy_from_slice(&r.to_be_bytes());
    }

    /// Encrypts one block in place.
    pub fn encrypt_block(&self, block: &mut [u8; 8]) {
        Self::process(block, |l, r| self.encrypt_halves(l, r));
    }

    /// Decrypts one block in place.
    pub fn decrypt_block(&self, block: &mut [u8; 8]) {
        Self::process(block, |l, r| self.decrypt_halves(l, r));
    }

    /// Decrypts whole blocks in place, ECB mode. A trailing partial block is left as it
    /// is, as `BlowStraw` does.
    pub fn decrypt(&self, data: &mut [u8]) {
        for chunk in data.chunks_exact_mut(Self::BLOCK_SIZE) {
            if let Ok(block) = <&mut [u8; 8]>::try_from(chunk) {
                self.decrypt_block(block);
            }
        }
    }

    /// Encrypts whole blocks in place, ECB mode. A trailing partial block is left as it
    /// is, as `BlowStraw` does.
    pub fn encrypt(&self, data: &mut [u8]) {
        for chunk in data.chunks_exact_mut(Self::BLOCK_SIZE) {
            if let Ok(block) = <&mut [u8; 8]>::try_from(chunk) {
                self.encrypt_block(block);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or_default())
            .collect()
    }

    /// Eric Young's published Blowfish test vectors.
    #[test]
    fn standard_vectors() {
        for (key, plain, cipher) in [
            ("0000000000000000", "0000000000000000", "4EF997456198DD78"),
            ("FFFFFFFFFFFFFFFF", "FFFFFFFFFFFFFFFF", "51866FD5B85ECB8A"),
            ("3000000000000000", "1000000000000001", "7D856F9A613063F2"),
            ("0123456789ABCDEF", "1111111111111111", "61F9C3802281B096"),
            ("FEDCBA9876543210", "0123456789ABCDEF", "0ACEAB0FC6A0A28D"),
        ] {
            let bf = Blowfish::new(&hex(key));
            let mut block: [u8; 8] = hex(plain).try_into().unwrap_or_default();
            bf.encrypt_block(&mut block);
            assert_eq!(block.as_slice(), hex(cipher), "key {key}");
            bf.decrypt_block(&mut block);
            assert_eq!(block.as_slice(), hex(plain));
        }
    }

    #[test]
    fn partial_blocks_pass_through() {
        let bf = Blowfish::new(b"a key");
        let mut data = *b"0123456789";
        bf.encrypt(&mut data);
        assert_eq!(&data[8..], b"89");
        bf.decrypt(&mut data);
        assert_eq!(&data, b"0123456789");
    }
}
