//! Global simulation state.

use bevy_ecs::resource::Resource;
use cnc_core::random::{CrtRand, SimRandom};

/// The game frame number (`Frame`), advanced once per tick.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Frame(pub u32);

/// The simulation's random number generator (`Sim_Random`).
///
/// Only one system may hold this mutably at a time, and the schedule's ambiguity check
/// enforces an order between them, so the draw sequence is fixed.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimRng(pub SimRandom);

/// The C runtime's `rand()` state, which the original also folds into the game state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CrtRng(pub CrtRand);

/// The last sync CRC (`GameCRC`), computed at the end of each tick.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameCrc(pub u32);

/// The next [`HeapIndex`](crate::HeapIndex) to hand out.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NextHeapIndex(pub u32);
