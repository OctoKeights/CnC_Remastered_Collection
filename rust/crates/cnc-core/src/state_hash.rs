//! Platform-independent hashing of simulation state, for desync detection.
//!
//! Lockstep multiplayer only works if every peer's simulation stays bit-identical, so
//! each tick the simulation folds its state into a [`StateHasher`] and peers compare the
//! results. `std::hash::Hash` isn't suitable: its output may change between Rust releases
//! and `usize` differs between platforms. This uses 64-bit FNV-1a over explicit
//! little-endian bytes, so the same state hashes the same everywhere, forever.

use crate::fixed::Fixed;
use crate::geometry::{Coord, Dir};

const OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

/// Accumulates a hash of simulation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHasher {
    /// A hasher with nothing added yet.
    #[must_use]
    pub const fn new() -> Self {
        Self(OFFSET_BASIS)
    }

    /// Adds raw bytes.
    pub fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// Adds a value.
    pub fn add<T: StateHash + ?Sized>(&mut self, value: &T) -> &mut Self {
        value.state_hash(self);
        self
    }

    /// The hash so far.
    #[must_use]
    pub const fn finish(&self) -> u64 {
        self.0
    }
}

/// Simulation state that can be folded into a [`StateHasher`].
///
/// Implementations must depend only on the value (never on addresses, `usize`, or
/// iteration order of unordered containers) so every platform agrees.
pub trait StateHash {
    /// Adds this value to `hasher`.
    fn state_hash(&self, hasher: &mut StateHasher);
}

macro_rules! ints {
    ($($t:ty),*) => {$(
        impl StateHash for $t {
            fn state_hash(&self, hasher: &mut StateHasher) {
                hasher.write(&self.to_le_bytes());
            }
        }
    )*};
}
ints!(u8, u16, u32, u64, i8, i16, i32, i64);

impl StateHash for bool {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.write(&[u8::from(*self)]);
    }
}

impl<T: StateHash> StateHash for Option<T> {
    fn state_hash(&self, hasher: &mut StateHasher) {
        match self {
            None => hasher.write(&[0]),
            Some(value) => {
                hasher.write(&[1]);
                value.state_hash(hasher);
            }
        }
    }
}

impl<T: StateHash> StateHash for [T] {
    fn state_hash(&self, hasher: &mut StateHasher) {
        // Length first, so [a, b] + [c] and [a] + [b, c] differ.
        u64::try_from(self.len())
            .unwrap_or(u64::MAX)
            .state_hash(hasher);
        for value in self {
            value.state_hash(hasher);
        }
    }
}

impl<T: StateHash, const N: usize> StateHash for [T; N] {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.as_slice().state_hash(hasher);
    }
}

impl<T: StateHash> StateHash for Vec<T> {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.as_slice().state_hash(hasher);
    }
}

impl<T: StateHash + ?Sized> StateHash for &T {
    fn state_hash(&self, hasher: &mut StateHasher) {
        (**self).state_hash(hasher);
    }
}

macro_rules! tuples {
    ($(($($name:ident $index:tt),+))*) => {$(
        impl<$($name: StateHash),+> StateHash for ($($name,)+) {
            fn state_hash(&self, hasher: &mut StateHasher) {
                $(self.$index.state_hash(hasher);)+
            }
        }
    )*};
}
tuples! {
    (A 0, B 1)
    (A 0, B 1, C 2)
    (A 0, B 1, C 2, D 3)
    (A 0, B 1, C 2, D 3, E 4)
}

impl StateHash for () {
    fn state_hash(&self, _hasher: &mut StateHasher) {}
}

impl StateHash for str {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.as_bytes().state_hash(hasher);
    }
}

/// Implements [`StateHash`] for a struct by hashing the listed fields in order.
///
/// ```
/// use cnc_core::impl_state_hash;
/// use cnc_core::state_hash::hash_of;
///
/// struct Tank {
///     strength: u16,
///     ammo: i8,
///     selected: bool, // UI state: deliberately left out
/// }
/// impl_state_hash!(Tank { strength, ammo });
///
/// let a = Tank { strength: 400, ammo: -1, selected: false };
/// let b = Tank { strength: 400, ammo: -1, selected: true };
/// assert_eq!(hash_of(&a), hash_of(&b));
/// ```
#[macro_export]
macro_rules! impl_state_hash {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl $crate::state_hash::StateHash for $ty {
            fn state_hash(&self, hasher: &mut $crate::state_hash::StateHasher) {
                $($crate::state_hash::StateHash::state_hash(&self.$field, hasher);)*
            }
        }
    };
}

impl StateHash for Fixed {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.raw().state_hash(hasher);
    }
}

impl StateHash for Coord {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.raw().state_hash(hasher);
    }
}

impl StateHash for Dir {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.0.state_hash(hasher);
    }
}

impl StateHash for crate::heap::SlotId {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.index().state_hash(hasher);
        self.generation().state_hash(hasher);
    }
}

impl StateHash for crate::random::SimRandom {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.index().state_hash(hasher);
    }
}

impl StateHash for crate::random::CrtRand {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.seed.state_hash(hasher);
    }
}

impl<T: StateHash> StateHash for crate::heap::ObjectHeap<T> {
    /// Hashes the live objects with their ids, in the order the game processes them.
    fn state_hash(&self, hasher: &mut StateHasher) {
        u64::try_from(self.len())
            .unwrap_or(u64::MAX)
            .state_hash(hasher);
        for (id, value) in self.iter() {
            id.state_hash(hasher);
            value.state_hash(hasher);
        }
    }
}

impl StateHash for crate::random::Lcg {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.seed.state_hash(hasher);
    }
}

/// Hashes one value from scratch.
#[must_use]
pub fn hash_of<T: StateHash + ?Sized>(value: &T) -> u64 {
    let mut hasher = StateHasher::new();
    value.state_hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_reference_values() {
        // Published FNV-1a 64 test vectors.
        assert_eq!(StateHasher::new().finish(), 0xCBF2_9CE4_8422_2325);
        let mut h = StateHasher::new();
        h.write(b"a");
        assert_eq!(h.finish(), 0xAF63_DC4C_8601_EC8C);
        let mut h = StateHasher::new();
        h.write(b"foobar");
        assert_eq!(h.finish(), 0x8594_4171_F739_67E8);
    }

    #[test]
    fn integers_are_little_endian() {
        let mut a = StateHasher::new();
        a.add(&0x0102_0304_u32);
        let mut b = StateHasher::new();
        b.write(&[4, 3, 2, 1]);
        assert_eq!(a, b);
    }

    #[test]
    fn structure_matters() {
        let split_a: (&[u8], &[u8]) = (&[1, 2], &[3]);
        let split_b: (&[u8], &[u8]) = (&[1], &[2, 3]);
        assert_ne!(hash_of(&split_a), hash_of(&split_b));
        assert_ne!(hash_of(&None::<u8>), hash_of(&Some(0_u8)));
        assert_eq!(hash_of(&Coord::from_raw(5)), hash_of(&5_u32));
        assert_ne!(hash_of("ab"), hash_of("ba"));
    }

    #[test]
    fn heaps_hash_in_processing_order() {
        use crate::heap::ObjectHeap;
        let mut a = ObjectHeap::new(4);
        let first = a.insert(1_u8).expect("free slot");
        a.insert(2).expect("free slot");
        a.remove(first);
        a.insert(3).expect("free slot");
        // Same values and slots, different creation order.
        let mut b = ObjectHeap::new(4);
        b.insert(3_u8).expect("free slot");
        b.insert(2).expect("free slot");
        assert_ne!(hash_of(&a), hash_of(&b));
        assert_eq!(hash_of(&a), hash_of(&a.clone()));
    }
}
