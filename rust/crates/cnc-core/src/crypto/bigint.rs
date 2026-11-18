//! Just enough unsigned big-integer arithmetic for the MIX key exchange (`INT.H`, `MP.CPP`):
//! modular exponentiation on numbers of a few hundred bits. Simple rather than fast; the
//! game decrypts one 80-byte block per archive.

use std::cmp::Ordering;

/// An unsigned integer stored as little-endian 32-bit limbs with no trailing zero limbs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct BigUint(Vec<u32>);

impl BigUint {
    fn normalized(mut limbs: Vec<u32>) -> Self {
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Self(limbs)
    }

    pub(super) fn from_u32(value: u32) -> Self {
        Self::normalized(vec![value])
    }

    /// Reads a little-endian byte string (how the game lays out `BigInt` in memory).
    pub(super) fn from_le_bytes(bytes: &[u8]) -> Self {
        let limbs = bytes
            .chunks(4)
            .map(|c| {
                let mut w = [0u8; 4];
                w[..c.len()].copy_from_slice(c);
                u32::from_le_bytes(w)
            })
            .collect();
        Self::normalized(limbs)
    }

    /// Reads a big-endian byte string.
    pub(super) fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut le = bytes.to_vec();
        le.reverse();
        Self::from_le_bytes(&le)
    }

    /// Writes the low `len` bytes, little-endian.
    pub(super) fn to_le_bytes(&self, len: usize) -> Vec<u8> {
        let mut out: Vec<u8> = self.0.iter().flat_map(|l| l.to_le_bytes()).collect();
        out.resize(len, 0);
        out
    }

    /// Number of significant bits (`BitCount`).
    pub(super) fn bit_len(&self) -> u32 {
        self.0.last().map_or(0, |&top| {
            (self.0.len() as u32 - 1) * 32 + (32 - top.leading_zeros())
        })
    }

    fn bit(&self, index: u32) -> bool {
        self.0
            .get((index / 32) as usize)
            .is_some_and(|limb| limb >> (index % 32) & 1 == 1)
    }

    fn cmp_mag(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }

    fn mul(&self, other: &Self) -> Self {
        let mut out = vec![0u32; self.0.len() + other.0.len()];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.0.iter().enumerate() {
                let t = u64::from(a) * u64::from(b) + u64::from(out[i + j]) + carry;
                out[i + j] = t as u32;
                carry = t >> 32;
            }
            out[i + other.0.len()] = carry as u32;
        }
        Self::normalized(out)
    }

    fn sub_assign(&mut self, other: &Self) {
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let t = i64::from(self.0[i]) - i64::from(other.0.get(i).copied().unwrap_or(0)) - borrow;
            borrow = i64::from(t < 0);
            self.0[i] = t.rem_euclid(1 << 32) as u32;
        }
        *self = Self::normalized(std::mem::take(&mut self.0));
    }

    fn shl1_or(&mut self, bit: bool) {
        let mut carry = u32::from(bit);
        for limb in &mut self.0 {
            let next = *limb >> 31;
            *limb = *limb << 1 | carry;
            carry = next;
        }
        if carry != 0 {
            self.0.push(carry);
        }
    }

    /// `self mod modulus` by binary long division. `modulus` must be non-zero.
    fn rem(&self, modulus: &Self) -> Self {
        let mut r = Self::default();
        for i in (0..self.bit_len()).rev() {
            r.shl1_or(self.bit(i));
            if r.cmp_mag(modulus) != Ordering::Less {
                r.sub_assign(modulus);
            }
        }
        r
    }

    /// `self ^ exponent mod modulus` by square-and-multiply. A zero modulus gives zero.
    pub(super) fn mod_pow(&self, exponent: &Self, modulus: &Self) -> Self {
        if modulus.0.is_empty() {
            return Self::default();
        }
        let mut result = Self::from_u32(1).rem(modulus);
        let base = self.rem(modulus);
        for i in (0..exponent.bit_len()).rev() {
            result = result.mul(&result).rem(modulus);
            if exponent.bit(i) {
                result = result.mul(&base).rem(modulus);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_mod_pow() {
        let n = |v| BigUint::from_u32(v);
        assert_eq!(n(4).mod_pow(&n(13), &n(497)), n(445));
        assert_eq!(n(2).mod_pow(&n(10), &n(1000)), n(24));
        assert_eq!(n(7).mod_pow(&n(0), &n(5)), n(1));
    }

    #[test]
    fn multi_limb() {
        // (2^64 + 1)^2 mod (2^61 - 1), checked with Python.
        let a = BigUint::from_le_bytes(&[1, 0, 0, 0, 0, 0, 0, 0, 1]);
        let m = BigUint::from_be_bytes(&[0x1F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let r = a.mod_pow(&BigUint::from_u32(2), &m);
        assert_eq!(r, BigUint::from_u32(81));
        assert_eq!(m.bit_len(), 61);
        assert_eq!(BigUint::from_u32(81).to_le_bytes(2), vec![81, 0]);
    }
}
