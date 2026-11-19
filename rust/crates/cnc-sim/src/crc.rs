//! The lockstep sync check (`Compute_Game_CRC` and `Add_CRC` in Tiberian Dawn's
//! `QUEUE.CPP`).

use bevy_ecs::system::{Query, ResMut};

use crate::components::{HeapIndex, Kind, Position, PrimaryFacing, SecondaryFacing};
use crate::resources::{CrtRng, GameCrc};

/// Folds `value` into `crc`: a one-bit left rotate through the addition (`Add_CRC`).
#[must_use]
pub const fn add_crc(crc: u32, value: u32) -> u32 {
    let high_bit = crc >> 31;
    (crc << 1).wrapping_add(value).wrapping_add(high_bit)
}

/// The order the original visits its heaps in, or `None` for objects it leaves out.
const fn heap_rank(kind: Kind) -> Option<u8> {
    match kind {
        Kind::Infantry => Some(0),
        Kind::Unit => Some(1),
        Kind::Building => Some(2),
        Kind::Aircraft | Kind::Vessel | Kind::Terrain => None,
    }
}

/// What each object contributes to the CRC.
type CrcTerms = (
    &'static Kind,
    &'static HeapIndex,
    &'static Position,
    Option<&'static PrimaryFacing>,
    Option<&'static SecondaryFacing>,
);

/// Computes this tick's [`GameCrc`].
///
/// Infantry, then units, then buildings, each in heap order, add their coordinate plus
/// their facing(s); units add their turret facing too. Aircraft aren't included, as in the
/// original. Finally one `rand()` draw is mixed in, which also advances [`CrtRng`].
#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are taken by value.
pub fn compute_game_crc(
    objects: Query<'_, '_, CrcTerms>,
    mut crt: ResMut<'_, CrtRng>,
    mut crc: ResMut<'_, GameCrc>,
) {
    let mut terms: Vec<(u8, HeapIndex, u32)> = objects
        .iter()
        .filter_map(|(&kind, &index, position, primary, secondary)| {
            let rank = heap_rank(kind)?;
            let mut value = position.0.raw();
            value = value.wrapping_add(u32::from(primary.map_or(0, |f| f.0.0)));
            if kind == Kind::Unit {
                value = value.wrapping_add(u32::from(secondary.map_or(0, |f| f.0.0)));
            }
            Some((rank, index, value))
        })
        .collect();
    terms.sort_unstable_by_key(|&(rank, index, _)| (rank, index));

    let objects_crc = terms
        .iter()
        .fold(0, |acc, &(_, _, value)| add_crc(acc, value));
    crc.0 = add_crc(objects_crc, crt.0.next_u15());
}

#[cfg(test)]
mod tests {
    use super::add_crc;

    #[test]
    fn add_crc_carries_the_high_bit_round() {
        assert_eq!(add_crc(0, 5), 5);
        assert_eq!(add_crc(1, 0), 2);
        assert_eq!(add_crc(0x8000_0000, 5), 6);
        assert_eq!(add_crc(0xFFFF_FFFF, 1), 0);
    }
}
