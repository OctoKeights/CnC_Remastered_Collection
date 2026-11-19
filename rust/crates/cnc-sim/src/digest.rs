//! Full-state digests for diagnosing desyncs.
//!
//! The sync CRC ([`crate::crc`]) only covers positions and facings, as the original's
//! did. When it disagrees between peers, a [`StateDigest`] of everything else (every
//! registered component, in heap order, and every registered resource) names the part of
//! the state that went wrong.
//!
//! Each new component or resource the game adds should be registered with
//! [`Simulation::hash_component`](crate::Simulation::hash_component) or
//! [`Simulation::hash_resource`](crate::Simulation::hash_resource), or the digest won't see
//! it diverge.

use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use cnc_core::state_hash::{StateHash, StateHasher};

use crate::components::{
    HeapIndex, HouseId, Kind, Position, PrimaryFacing, SecondaryFacing, Strength,
};
use crate::drive::{Drive, Foot, MissionState, Placement, UnitKind};
use crate::houses::Houses;
use crate::map::Map;
use crate::resources::{CrtRng, Frame, SimRng};
use crate::scenario::TerrainObject;

/// Writes one section of the digest.
pub(crate) type SectionFn = fn(&mut World, &mut StateHasher);

impl StateHash for Kind {
    fn state_hash(&self, hasher: &mut StateHasher) {
        (*self as u8).state_hash(hasher);
    }
}

macro_rules! newtypes {
    ($($ty:ty),*) => {$(
        impl StateHash for $ty {
            fn state_hash(&self, hasher: &mut StateHasher) {
                self.0.state_hash(hasher);
            }
        }
    )*};
}
newtypes!(
    HeapIndex,
    Position,
    PrimaryFacing,
    SecondaryFacing,
    Strength,
    HouseId,
    Frame,
    SimRng,
    CrtRng
);

impl StateHash for TerrainObject {
    fn state_hash(&self, hasher: &mut StateHasher) {
        self.0.id().state_hash(hasher);
    }
}

/// Hashes which objects exist: every `(Kind, HeapIndex)` in heap order.
pub(crate) fn hash_objects(world: &mut World, hasher: &mut StateHasher) {
    let mut objects: Vec<(Kind, HeapIndex)> = world
        .query::<(&Kind, &HeapIndex)>()
        .iter(world)
        .map(|(&kind, &index)| (kind, index))
        .collect();
    objects.sort_unstable();
    hasher.add(&objects);
}

/// Hashes component `C` on every object that has one, in heap order.
pub(crate) fn hash_component<C: Component + StateHash>(
    world: &mut World,
    hasher: &mut StateHasher,
) {
    let mut query = world.query::<(&Kind, &HeapIndex, &C)>();
    let mut rows: Vec<(Kind, HeapIndex, &C)> = query
        .iter(world)
        .map(|(&kind, &index, value)| (kind, index, value))
        .collect();
    rows.sort_unstable_by_key(|&(kind, index, _)| (kind, index));
    hasher.add(&rows);
}

/// Hashes resource `R`, or a marker if it's missing.
pub(crate) fn hash_resource<R: Resource + StateHash>(world: &mut World, hasher: &mut StateHasher) {
    hasher.add(&world.get_resource::<R>());
}

/// The sections every simulation starts with.
pub(crate) fn core_sections() -> Vec<(&'static str, SectionFn)> {
    vec![
        ("frame", hash_resource::<Frame>),
        ("sim_rng", hash_resource::<SimRng>),
        ("crt_rng", hash_resource::<CrtRng>),
        ("map", hash_resource::<Map>),
        ("houses", hash_resource::<Houses>),
        ("objects", hash_objects),
        ("house", hash_component::<HouseId>),
        ("position", hash_component::<Position>),
        ("primary_facing", hash_component::<PrimaryFacing>),
        ("secondary_facing", hash_component::<SecondaryFacing>),
        ("strength", hash_component::<Strength>),
        ("terrain_object", hash_component::<TerrainObject>),
        ("unit_kind", hash_component::<UnitKind>),
        ("mission", hash_component::<MissionState>),
        ("foot", hash_component::<Foot>),
        ("drive", hash_component::<Drive>),
        ("placement", hash_component::<Placement>),
    ]
}
