//! The game simulation, as Bevy ECS components and systems.
//!
//! This crate is the game itself with no rendering, input or audio: a 2D renderer, a later
//! 3D mod, a dedicated server and a replay checker all drive the same [`Simulation`] and
//! read its components. Positions stay in the original's leptons ([`Coord`]) and facings
//! in its 0–255 directions ([`Dir`]); what those are drawn as is the renderer's business.
//!
//! # Determinism
//!
//! Multiplayer lockstep and replays need every peer to compute exactly the same thing each
//! tick, and the systems run on a multithreaded executor. The rules that keep that safe:
//!
//! 1. **Every conflicting pair of systems is ordered.** The tick schedule is built with
//!    ambiguity detection set to error, so two systems that touch the same data without
//!    an explicit order make [`Simulation::step`] fail instead of racing. Systems with
//!    disjoint access run in parallel freely, since their order can't matter.
//! 2. **Order-dependent work iterates in [`HeapIndex`] order.** Bevy's query order depends
//!    on archetype layout, not on anything the original game defines. A system whose
//!    result depends on visiting order (drawing random numbers, picking targets, anything
//!    with side effects on shared state) collects its matches and sorts them by
//!    `HeapIndex`, which reproduces the original's per-class heap order.
//! 3. **No parallel iteration writes order-dependent state.** `par_iter` is only for
//!    per-entity work that touches nothing shared.
//! 4. **Integer maths only.** Floats never enter the simulation; see
//!    [`cnc_core::fixed`] and [`cnc_core::geometry`].
//!
//! The per-tick sync check is the original's (`Compute_Game_CRC` in `QUEUE.CPP`), so a
//! desync shows up on the frame it happens; [`Simulation::check_peer`] compares a peer's
//! CRC against the recorded history. To find *what* diverged, [`Simulation::digest`]
//! hashes every registered component and resource as a separate named section, and
//! [`StateDigest::differences`] lists the sections two peers disagree on.
//!
//! Parallel systems that need to change shared state record the changes in a
//! [`cnc_core::deferred::Deferred`] buffer keyed by heap order, then apply them in a
//! later, serial system.

pub mod components;
pub mod crc;
mod digest;
pub mod drive;
pub mod houses;
pub mod map;
pub mod resources;
pub mod scenario;
mod simulation;

pub use bevy_ecs;
pub use cnc_core::geometry::{Coord, Dir};
pub use cnc_core::sync::{StateDigest, SyncCheck, SyncHistory};
pub use components::{
    HeapIndex, HouseId, Kind, Position, PrimaryFacing, SecondaryFacing, Strength,
};
pub use map::Map;
pub use resources::{CrtRng, Frame, GameCrc, SimRng};
pub use scenario::{
    MapLoadReport, TerrainObject, icon_check_from_art, load_td_scenario, spawn_td_unit,
};
pub use simulation::{DEFAULT_SYNC_HISTORY, ObjectSpawn, Phase, Simulation, Threading, Tick};
