//! The map editor's model, ported from `CnCTDRAMapEditor/` (C#).
//!
//! The editor's `WinForms` UI isn't ported; this crate holds the game-independent model it is
//! built on ([`CellMetrics`], [`CellGrid`]) and [`Scenario`], which loads and saves maps
//! for either game through `cnc-td` and `cnc-ra`.

pub mod grid;
pub mod scenario;

pub use grid::{CellGrid, CellMetrics, Point};
pub use scenario::{Game, Scenario, ScenarioError};
