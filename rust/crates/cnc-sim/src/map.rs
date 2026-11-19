//! The map: per-cell terrain, overlays, ground type and occupants (`MapClass` and
//! `CellClass` in Tiberian Dawn's `MAP.CPP` and `CELL.CPP`).
//!
//! The map is one [`Map`] resource rather than an entity per cell: cells are looked up by
//! index constantly and never move, so a flat array is both faster and simpler to keep
//! deterministic.

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use cnc_core::geometry::Facing;
use cnc_core::map::MapBounds;
use cnc_core::random::CrtRand;
use cnc_core::state_hash::{StateHash, StateHasher};
use cnc_td::data::{Land, Locomotion, cell_land};
use cnc_td::{Cell, MAP_CELLS, OverlayType, TemplateType, Theater};

use crate::components::Kind;

/// Credits per step of Tiberium density (`UnitTypeClass::TIBERIUM_STEP`).
pub const TIBERIUM_STEP: i32 = 25;

/// The sides a wall joins along, in the order of its join bits (`Wall_Update`).
const WALL_SIDES: [Facing; 4] = [Facing::N, Facing::E, Facing::S, Facing::W];

/// Tiberium density by number of Tiberium neighbours (`Tiberium_Adjust`'s `_adj`).
const TIBERIUM_DENSITY: [u8; 9] = [0, 1, 3, 4, 6, 7, 8, 10, 11];

/// One cell's state (`CellClass`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellState {
    /// The terrain template icon (`TType`, `TIcon`); `None` is plain clear ground.
    pub template: Option<(TemplateType, u8)>,
    /// The overlay (`Overlay`): a wall, Tiberium, a crate, ...
    pub overlay: Option<OverlayType>,
    /// Overlay detail (`OverlayData`): Tiberium density, a wall's join and damage bits.
    pub overlay_data: u8,
    land: Land,
    land_override: Option<Land>,
    /// Occupation flags (`Flag.Composite`): infantry spots, vehicle reservation, ...
    flags: u8,
    /// The house whose infantry hold spots in the cell (`InfType`).
    pub infantry_house: Option<cnc_td::House>,
    /// Objects occupying the cell, most recently placed first (`OccupierPtr` and the
    /// objects' `Next` links).
    occupiers: Vec<Entity>,
}

impl Default for CellState {
    fn default() -> Self {
        Self {
            template: None,
            overlay: None,
            overlay_data: 0,
            land: cell_land(None, None),
            land_override: None,
            flags: 0,
            infantry_house: None,
            occupiers: Vec::new(),
        }
    }
}

impl CellState {
    /// The ground type, honouring a per-scenario override (`Land_Type`).
    #[must_use]
    pub fn land(&self) -> Land {
        self.land_override.unwrap_or(self.land)
    }

    /// The occupation flags (`Flag.Composite`): bits 0-4 are the infantry spots, then
    /// [`CellState::VEHICLE`], monolith and building.
    #[must_use]
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// Whether a vehicle has reserved the cell (`Flag.Occupy.Vehicle`).
    #[must_use]
    pub fn vehicle_reserved(&self) -> bool {
        self.flags & Self::VEHICLE != 0
    }

    /// The vehicle bit of [`CellState::flags`]: a vehicle is in the cell or driving into it.
    pub const VEHICLE: u8 = 0x20;
    /// The monolith bit: something immovable (a tree, a rock) is in the cell.
    pub const MONOLITH: u8 = 0x40;
    /// The building bit.
    pub const BUILDING: u8 = 0x80;

    /// Objects occupying the cell, most recently placed first.
    #[must_use]
    pub fn occupiers(&self) -> &[Entity] {
        &self.occupiers
    }

    /// Recomputes the ground from the template and overlay (`Recalc_Attributes`).
    fn recalc(&mut self) {
        self.land = cell_land(self.template, self.overlay);
    }

    fn is_wall(&self) -> bool {
        self.overlay.is_some_and(|o| o.data().is_wall)
    }

    fn is_tiberium(&self) -> bool {
        self.overlay
            .is_some_and(|o| o.data().ground == Land::Tiberium)
    }
}

impl StateHash for CellState {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.add(&self.template.map(|(t, icon)| (t.id(), icon)));
        hasher.add(&self.overlay.map(OverlayType::id));
        hasher.add(&self.overlay_data);
        hasher.add(&self.land.id());
        hasher.add(&self.land_override.map(Land::id));
        hasher.add(&self.flags);
        hasher.add(&self.infantry_house.map(cnc_td::House::id));
        // Occupants are hashed through their own components; here only how many there
        // are, since entity ids are an ECS detail.
        hasher.add(&u32::try_from(self.occupiers.len()).unwrap_or(u32::MAX));
    }
}

/// The occupation flag an object of `kind` sets in its cell (`Occupy_Down`).
const fn occupy_flag(kind: Kind) -> u8 {
    match kind {
        Kind::Unit | Kind::Aircraft => CellState::VEHICLE,
        Kind::Terrain => CellState::MONOLITH,
        Kind::Building => CellState::BUILDING,
        Kind::Infantry | Kind::Vessel => 0,
    }
}

/// The game map (`MapClass`).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct Map {
    theater: Theater,
    bounds: MapBounds,
    cells: Vec<CellState>,
    crates: u32,
}

impl Map {
    /// An empty, all-clear map.
    #[must_use]
    pub fn new(theater: Theater, bounds: MapBounds) -> Self {
        Self {
            theater,
            bounds,
            cells: vec![CellState::default(); MAP_CELLS],
            crates: 0,
        }
    }

    /// The theater (tileset).
    #[must_use]
    pub fn theater(&self) -> Theater {
        self.theater
    }

    /// The playable area.
    #[must_use]
    pub fn bounds(&self) -> MapBounds {
        self.bounds
    }

    /// Whether `cell` is inside the playable area.
    #[must_use]
    pub fn in_bounds(&self, cell: Cell) -> bool {
        let b = self.bounds;
        (b.x..b.x + b.width).contains(&cell.x()) && (b.y..b.y + b.height).contains(&cell.y())
    }

    /// A cell, or `None` past the end of the map (as the original's
    /// `(unsigned)cell >= MAP_CELL_TOTAL` checks).
    #[must_use]
    pub fn get(&self, cell: Cell) -> Option<&CellState> {
        self.cells.get(usize::from(cell.index()))
    }

    /// A cell.
    ///
    /// # Panics
    ///
    /// If `cell` is past the end of the map.
    #[must_use]
    pub fn cell(&self, cell: Cell) -> &CellState {
        &self.cells[usize::from(cell.index())]
    }

    fn cell_mut(&mut self, cell: Cell) -> Option<&mut CellState> {
        self.cells.get_mut(usize::from(cell.index()))
    }

    /// Every cell, in index order.
    #[must_use]
    pub fn cells(&self) -> &[CellState] {
        &self.cells
    }

    /// How many crates are on the map (`CrateCount`).
    #[must_use]
    pub fn crates(&self) -> u32 {
        self.crates
    }

    /// The ground type of `cell` (clear past the end of the map).
    #[must_use]
    pub fn land(&self, cell: Cell) -> Land {
        self.get(cell).map_or(Land::Clear, CellState::land)
    }

    /// Speed across `cell` for `locomotion`, 0 (impassable) to 255.
    #[must_use]
    pub fn speed(&self, cell: Cell, locomotion: Locomotion) -> u8 {
        self.land(cell).speed(locomotion)
    }

    /// Sets a cell's template icon and recomputes its ground.
    pub fn set_template(&mut self, cell: Cell, template: Option<(TemplateType, u8)>) {
        if let Some(c) = self.cell_mut(cell) {
            c.template = template;
            c.recalc();
        }
    }

    /// Forces a cell's ground type whatever is on it (`Override_Land_Type`), as the
    /// Remastered Collection does to patch a few campaign maps.
    pub fn override_land(&mut self, cell: Cell, land: Land) {
        if let Some(c) = self.cell_mut(cell) {
            c.land_override = Some(land);
        }
    }

    /// Records `entity` (an object of `kind`) as occupying `cell`, in front of earlier
    /// occupants, and sets the cell's flag for its kind: vehicle for units and aircraft,
    /// monolith for terrain, building for buildings (`Occupy_Down`). Adding an entity
    /// already there does nothing at all.
    pub fn occupy(&mut self, cell: Cell, entity: Entity, kind: Kind) {
        if let Some(c) = self.cell_mut(cell)
            && !c.occupiers.contains(&entity)
        {
            c.occupiers.insert(0, entity);
            c.flags |= occupy_flag(kind);
        }
    }

    /// Sets or clears a cell's vehicle reservation (`Flag.Occupy.Vehicle`).
    pub fn set_vehicle_reserved(&mut self, cell: Cell, reserved: bool) {
        if let Some(c) = self.cell_mut(cell) {
            if reserved {
                c.flags |= CellState::VEHICLE;
            } else {
                c.flags &= !CellState::VEHICLE;
            }
        }
    }

    /// Whether `cell` is inside the playable area (`In_Radar`), rejecting indexes past the
    /// map as the original's `cell & 0xF000` test does.
    #[must_use]
    pub fn in_radar(&self, cell: Cell) -> bool {
        cell.index() & 0xF000 == 0 && self.in_bounds(cell)
    }

    /// Removes `entity` from `cell`'s occupants and clears its kind's flag, whether or not
    /// it was there (`Occupy_Up`).
    pub fn vacate(&mut self, cell: Cell, entity: Entity, kind: Kind) {
        if let Some(c) = self.cell_mut(cell) {
            c.occupiers.retain(|&e| e != entity);
            c.flags &= !occupy_flag(kind);
        }
    }

    /// Lays an overlay on a cell, with the original's placement rules
    /// (`OverlayClass::Mark(MARK_DOWN)`). Returns whether it was placed.
    ///
    /// `has_terrain` says whether a terrain object (a tree, a rock) occupies the cell,
    /// which blocks everything but walls. While a scenario loads (`scenario_init`) every
    /// cell counts as clear for walls, as `Is_Generally_Clear` makes it.
    ///
    /// Not yet ported: roads (no shipped scenario has any) lay down as plain overlays,
    /// and concrete doesn't pick its edge icons or spread to the neighbouring cell.
    pub fn place_overlay(
        &mut self,
        cell: Cell,
        overlay: OverlayType,
        has_terrain: bool,
        scenario_init: bool,
    ) -> bool {
        let data = overlay.data();
        if data.is_wall {
            let clear = scenario_init || self.generally_clear(cell, has_terrain);
            let Some(c) = self.cell_mut(cell) else {
                return false;
            };
            if !clear || c.overlay == Some(OverlayType::FlagSpot) {
                return false;
            }
            c.overlay = Some(overlay);
            c.overlay_data = 0;
            c.recalc();
            self.wall_update(cell);
            return true;
        }
        let Some(c) = self.cell_mut(cell) else {
            return false;
        };
        let empty = matches!(c.overlay, None | Some(OverlayType::Squish));
        if !empty || has_terrain || !c.land().ground().buildable {
            return false;
        }
        if overlay != OverlayType::Squish {
            c.overlay = Some(overlay);
            c.overlay_data = 0;
        }
        c.recalc();
        if data.ground == Land::Tiberium {
            c.overlay_data = 1;
        }
        if data.is_crate {
            self.crates += 1;
        }
        if data.ground == Land::Tiberium {
            self.tiberium_adjust(cell, None);
        }
        true
    }

    /// Whether a cell can take a wall or building (`Is_Generally_Clear`), minus the
    /// checks on flags and bibs, which nothing places yet.
    fn generally_clear(&self, cell: Cell, has_terrain: bool) -> bool {
        let Some(c) = self.get(cell) else {
            return false;
        };
        if has_terrain || !c.occupiers.is_empty() {
            return false;
        }
        if c.is_wall() || c.overlay == Some(OverlayType::FlagSpot) {
            return false;
        }
        c.land().ground().buildable
    }

    /// Joins a wall to its neighbours (`Wall_Update`): the cell and each orthogonal
    /// neighbour that is a wall get the bits for which of their own neighbours are the
    /// same wall type.
    pub fn wall_update(&mut self, cell: Cell) {
        if !self.get(cell).is_some_and(CellState::is_wall) {
            return;
        }
        // The original's list ends with FACING_NONE, the cell itself.
        let targets = WALL_SIDES
            .map(|f| cell.adjacent(f))
            .into_iter()
            .chain([cell]);
        for target in targets {
            let Some(wall) = self.get(target).filter(|t| t.is_wall()).map(|t| t.overlay) else {
                continue;
            };
            let mut icon = 0_u8;
            for (bit, side) in WALL_SIDES.into_iter().enumerate() {
                if self
                    .get(target.adjacent(side))
                    .is_some_and(|n| n.overlay == wall)
                {
                    icon |= 1 << bit;
                }
            }
            let Some(t) = self.cell_mut(target) else {
                continue;
            };
            t.overlay_data = (t.overlay_data & 0xF0) | icon;
            // Fully destroyed wall stages disappear.
            let gone = matches!(
                (t.overlay, t.overlay_data),
                (Some(OverlayType::BrickWall), 48)
                    | (
                        Some(OverlayType::SandbagWall | OverlayType::BarbwireWall),
                        16
                    )
                    | (Some(OverlayType::CycloneWall), 32)
            );
            if gone {
                t.overlay = None;
                t.overlay_data = 0;
            }
            t.recalc();
        }
    }

    /// Sets a Tiberium cell's density from how many neighbours also hold Tiberium, and
    /// returns its worth in credits (`Tiberium_Adjust`).
    ///
    /// With `pregame`, the overlay is first re-rolled to a random one of the twelve
    /// Tiberium overlays (`Random_Pick`, which uses the C runtime's `rand()`), so fields
    /// don't look uniform.
    pub fn tiberium_adjust(&mut self, cell: Cell, pregame: Option<&mut CrtRand>) -> i32 {
        if !self.get(cell).is_some_and(CellState::is_tiberium) {
            return 0;
        }
        if let Some(rng) = pregame {
            let first = i32::from(OverlayType::Tiberium1.id());
            let last = i32::from(OverlayType::Tiberium12.id());
            let pick = u8::try_from(rng.irandom(first, last))
                .ok()
                .and_then(OverlayType::from_id);
            if let (Some(pick), Some(c)) = (pick, self.cell_mut(cell)) {
                c.overlay = Some(pick);
            }
        }
        let count = Facing::ALL
            .into_iter()
            .filter(|&f| {
                self.get(cell.adjacent(f))
                    .is_some_and(CellState::is_tiberium)
            })
            .count();
        let density = TIBERIUM_DENSITY[count];
        if let Some(c) = self.cell_mut(cell) {
            c.overlay_data = density;
        }
        (i32::from(density) + 1) * TIBERIUM_STEP
    }

    /// The final pass over a freshly loaded map (`MapClass::Overpass`): every Tiberium
    /// cell in the playable area is re-rolled and re-weighed. Returns the map's total
    /// Tiberium worth.
    ///
    /// Quirk kept: the scan stops one row short of the bottom edge.
    pub fn overpass(&mut self, rng: &mut CrtRand) -> i64 {
        let b = self.bounds;
        let mut value = 0_i64;
        for y in 0..b.height.saturating_sub(1) {
            for x in 0..b.width {
                let cell = Cell::new(b.x + x, b.y + y);
                value += i64::from(self.tiberium_adjust(cell, Some(&mut *rng)));
            }
        }
        value
    }
}

impl StateHash for Map {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.add(&self.theater.id());
        hasher.add(&(
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            self.bounds.height,
        ));
        hasher.add(&self.crates);
        hasher.add(&self.cells);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Map {
        Map::new(
            Theater::Temperate,
            MapBounds {
                x: 1,
                y: 1,
                width: 62,
                height: 62,
            },
        )
    }

    #[test]
    fn ground_follows_template_and_overlay() {
        let mut m = map();
        let c = Cell::new(10, 10);
        assert_eq!(m.land(c), Land::Clear);
        m.set_template(c, Some((TemplateType::Water, 0)));
        assert_eq!(m.land(c), Land::Water);
        assert_eq!(m.speed(c, Locomotion::Track), 0);
        // Tiberium won't go on water.
        assert!(!m.place_overlay(c, OverlayType::Tiberium3, false, true));
        // A wall will, during scenario load.
        assert!(m.place_overlay(c, OverlayType::SandbagWall, false, true));
        assert_eq!(m.land(c), Land::Wall);
        m.override_land(c, Land::Rock);
        assert_eq!(m.land(c), Land::Rock);
    }

    #[test]
    fn walls_join_their_neighbours() {
        let mut m = map();
        let (a, b, c) = (Cell::new(5, 5), Cell::new(6, 5), Cell::new(6, 6));
        for cell in [a, b, c] {
            assert!(m.place_overlay(cell, OverlayType::BrickWall, false, true));
        }
        // Bits: N=1, E=2, S=4, W=8.
        assert_eq!(m.cell(a).overlay_data, 2);
        assert_eq!(m.cell(b).overlay_data, 8 | 4);
        assert_eq!(m.cell(c).overlay_data, 1);
        // Different wall types don't join.
        assert!(m.place_overlay(Cell::new(4, 5), OverlayType::SandbagWall, false, true));
        assert_eq!(m.cell(a).overlay_data, 2);
        // Outside scenario load a wall needs a clear cell.
        assert!(!m.place_overlay(a, OverlayType::BrickWall, false, false));
    }

    #[test]
    fn tiberium_density_and_overpass() {
        let mut m = map();
        let centre = Cell::new(20, 20);
        assert!(m.place_overlay(centre, OverlayType::Tiberium1, false, true));
        assert_eq!(m.cell(centre).overlay_data, 0);
        for f in Facing::ALL {
            assert!(m.place_overlay(centre.adjacent(f), OverlayType::Tiberium1, false, true));
        }
        // Placing neighbours doesn't revisit the centre; the overpass does.
        assert_eq!(m.cell(centre).overlay_data, 0);
        assert!(!m.place_overlay(Cell::new(30, 30), OverlayType::Tiberium1, true, true));

        let mut rng = CrtRand::default();
        let total = m.overpass(&mut rng);
        assert_eq!(m.cell(centre).overlay_data, 11);
        // Worth is (density + 1) steps: the centre has 8 neighbours (density 11), edge
        // middles 5 (7), corners 3 (4).
        assert_eq!(total, i64::from((12 + 4 * 8 + 4 * 5) * TIBERIUM_STEP));
        let kinds: Vec<_> = Facing::ALL
            .iter()
            .map(|&f| m.cell(centre.adjacent(f)).overlay)
            .collect();
        assert!(kinds.iter().any(|&k| k != Some(OverlayType::Tiberium1)));
        assert!(
            kinds
                .iter()
                .all(|k| k.is_some_and(|o| o.data().is_tiberium))
        );
    }

    #[test]
    fn occupiers_newest_first() {
        let mut m = map();
        let c = Cell::new(3, 3);
        let mut world = bevy_ecs::world::World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        m.occupy(c, a, Kind::Terrain);
        m.occupy(c, b, Kind::Unit);
        m.occupy(c, a, Kind::Terrain);
        assert_eq!(m.cell(c).occupiers(), [b, a]);
        assert_eq!(m.cell(c).flags(), CellState::VEHICLE | CellState::MONOLITH);
        m.vacate(c, b, Kind::Unit);
        assert_eq!(m.cell(c).occupiers(), [a]);
        assert_eq!(m.cell(c).flags(), CellState::MONOLITH);
    }
}
