//! Components shared by every game object.
//!
//! The original's class hierarchy (`ObjectClass` → `TechnoClass` → `FootClass` →
//! `UnitClass`, …) becomes plain data here: an object is an entity with the components its
//! class's fields would have held.

use bevy_ecs::component::Component;
use cnc_core::geometry::{Coord, Turning};

/// Which of the original's object heaps an entity belongs to (`RTTIType`, reduced to the
/// classes the simulation keeps in heaps).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// `InfantryClass`.
    Infantry,
    /// `UnitClass` (ground vehicles).
    Unit,
    /// `AircraftClass`.
    Aircraft,
    /// `VesselClass` (Red Alert only).
    Vessel,
    /// `BuildingClass`.
    Building,
    /// `TerrainClass`: trees, rocks and other scenery objects.
    Terrain,
}

/// Creation order, standing in for an object's position in its class's heap.
///
/// The original keeps each class's live objects in a vector that is compacted in order
/// when one is freed (`DynamicVectorClass::Delete`), so "heap order" is creation order of
/// the survivors. Sorting by this index within a [`Kind`] reproduces it. Indices are
/// never reused.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeapIndex(pub u32);

/// The object's centre in leptons (`ObjectClass::Coord`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position(pub Coord);

/// The body's facing and the facing it's turning to (`TechnoClass::PrimaryFacing`;
/// buildings have one too).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PrimaryFacing(pub Turning);

/// A turret's facing (`TurretClass::SecondaryFacing`).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SecondaryFacing(pub Turning);

/// Hit points left (`ObjectClass::Strength`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Strength(pub u16);

/// The owning house, as the game's `HousesType` number.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HouseId(pub u8);
