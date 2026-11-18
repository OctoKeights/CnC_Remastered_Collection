//! Westwood's public-key cipher (`PKey` in `PK.CPP`): textbook RSA on little-endian
//! blocks, used to wrap the Blowfish key of encrypted MIX headers.

use std::sync::LazyLock;

use super::bigint::BigUint;
use crate::base64;

/// The game's public key modulus, DER-encoded and Base64'd as in `Keys[]` (`CONST.CPP`).
const WESTWOOD_MODULUS: &str = "AihRvNoIbTn85FZRYNZRcT+i6KpU+maCsEqr3Q5q+LDB5tH7Tz2qQ38V";

/// The exponent used with the public key (`PKey::Fast_Exponent`).
const FAST_EXPONENT: u32 = 65537;

/// Red Alert's built-in public key (`FastKey`).
pub static WESTWOOD_KEY: LazyLock<PublicKey> = LazyLock::new(|| {
    let der = base64::decode(WESTWOOD_MODULUS).unwrap_or_default();
    PublicKey::from_der_modulus(&der, FAST_EXPONENT).unwrap_or_else(|| PublicKey {
        modulus: BigUint::default(),
        exponent: BigUint::from_u32(FAST_EXPONENT),
        bits: 0,
    })
});

/// An RSA-style key as the game uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    modulus: BigUint,
    exponent: BigUint,
    /// `BitPrecision`: one less than the modulus bit count.
    bits: u32,
}

impl PublicKey {
    /// Builds a key from a DER `INTEGER` modulus (`Decode_Modulus`) and an exponent.
    /// Returns `None` if `der` isn't a DER integer.
    #[must_use]
    pub fn from_der_modulus(der: &[u8], exponent: u32) -> Option<Self> {
        let modulus = BigUint::from_be_bytes(der_integer(der)?);
        let bits = modulus.bit_len().saturating_sub(1);
        Some(Self {
            modulus,
            exponent: BigUint::from_u32(exponent),
            bits,
        })
    }

    /// Builds a key from DER `INTEGER`s for both the modulus and the exponent, as
    /// `Get_PKey(false)` does with the `[PrivateKey]` section.
    #[must_use]
    pub fn from_der_modulus_and_exponent(modulus: &[u8], exponent: &[u8]) -> Option<Self> {
        let mut key = Self::from_der_modulus(modulus, 0)?;
        key.exponent = BigUint::from_be_bytes(der_integer(exponent)?);
        Some(key)
    }

    /// Bytes of plaintext per block (`Plain_Block_Size`).
    #[must_use]
    pub fn plain_block_size(&self) -> usize {
        (self.bits.saturating_sub(1) / 8) as usize
    }

    /// Bytes of ciphertext per block (`Crypt_Block_Size`).
    #[must_use]
    pub fn crypt_block_size(&self) -> usize {
        self.plain_block_size() + 1
    }

    /// Ciphertext length needed to carry `plain_len` bytes (`Block_Count` x block size).
    #[must_use]
    pub fn encrypted_len(&self, plain_len: usize) -> usize {
        let plain = self.plain_block_size().max(1);
        plain_len.div_ceil(plain) * self.crypt_block_size()
    }

    /// Encrypts whole blocks (`PKey::Encrypt`); a trailing partial block is ignored.
    #[must_use]
    pub fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        let (crypt, plain) = (self.crypt_block_size(), self.plain_block_size());
        let mut out = Vec::with_capacity(data.len() / plain.max(1) * crypt);
        if plain == 0 {
            return out;
        }
        for block in data.chunks_exact(plain) {
            let value = BigUint::from_le_bytes(block).mod_pow(&self.exponent, &self.modulus);
            out.extend_from_slice(&value.to_le_bytes(crypt));
        }
        out
    }

    /// Decrypts whole blocks (`PKey::Decrypt`); a trailing partial block is ignored.
    #[must_use]
    pub fn decrypt(&self, data: &[u8]) -> Vec<u8> {
        let (crypt, plain) = (self.crypt_block_size(), self.plain_block_size());
        let mut out = Vec::with_capacity(data.len() / crypt.max(1) * plain);
        if crypt <= 1 {
            return out;
        }
        for block in data.chunks_exact(crypt) {
            let value = BigUint::from_le_bytes(block).mod_pow(&self.exponent, &self.modulus);
            out.extend_from_slice(&value.to_le_bytes(plain));
        }
        out
    }
}

/// The big-endian magnitude bytes of a DER `INTEGER` (`XMP_DER_Decode`).
fn der_integer(der: &[u8]) -> Option<&[u8]> {
    let (&tag, rest) = der.split_first()?;
    if tag != 0x02 {
        return None;
    }
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first & 0x80 == 0 {
        (usize::from(first), rest)
    } else {
        let n = usize::from(first & 0x7F);
        if n == 0 || n > 2 || rest.len() < n {
            return None;
        }
        let len = rest[..n]
            .iter()
            .fold(0usize, |acc, &b| acc << 8 | usize::from(b));
        (len, &rest[n..])
    };
    rest.get(..len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn westwood_key_shape() {
        let key = &*WESTWOOD_KEY;
        assert_eq!(key.bits, 318);
        assert_eq!(key.plain_block_size(), 39);
        assert_eq!(key.crypt_block_size(), 40);
        // A 56-byte Blowfish key needs two blocks.
        assert_eq!(key.encrypted_len(56), 80);
    }

    #[test]
    fn decrypts_textbook_rsa() {
        // n = 3233 (61 * 53), e = 17, d = 2753: encrypting 65 gives 2790.
        let key = PublicKey::from_der_modulus(&[0x02, 0x02, 0x0C, 0xA1], 2753).expect("der");
        assert_eq!(key.crypt_block_size(), 1 + (10 / 8));
        let plain = key.decrypt(&2790u16.to_le_bytes());
        assert_eq!(plain, vec![65]);
    }
}
