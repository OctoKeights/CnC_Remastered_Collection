//! Loading a Tiberian Dawn scenario's map into a [`Simulation`] (the map parts of
//! `Read_Scenario_Ini` in `INI.CPP`).
//!
//! Steps run in the original's order, which matters: terrain objects go down before
//! overlays (Tiberium won't grow under a tree), overlays go down in the order the file
//! lists them, and the final [`Map::overpass`] draws random numbers.
//!
//! Not loaded yet: units, infantry, aircraft, buildings, smudges, triggers and the
//! briefing. The C runtime `rand()` state ([`CrtRng`]) is used as it stands; the caller
//! seeds it as the game would (`srand(Seed)` at start-up).

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use cnc_core::geometry::Coord;
use cnc_core::ini::{Ini, Section};
use cnc_core::scenario::{atoi, tokens};
use cnc_td::data::Land;
use cnc_td::{Cell, MAP_CELLS, MAP_WIDTH, OverlayType, TemplateType, TerrainType};

use crate::components::{HeapIndex, Kind, Position, Strength};
use crate::map::Map;
use crate::resources::{CrtRng, NextHeapIndex};
use crate::simulation::Simulation;

/// Which terrain object (tree, rock, ...) an entity is (`TerrainClass::Class`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainObject(pub TerrainType);

/// Says whether `Read_Binary` keeps a template cell. The original clears a cell whose icon
/// is past the template's `width * height` or blank in the theater's icon set; when it
/// has no art for the template it checks nothing and keeps the cell. Passing no check at
/// all keeps every cell, as a build without art would.
pub type IconCheck<'a> = &'a dyn Fn(TemplateType, u8) -> bool;

/// Builds an [`IconCheck`] from a theater's icon-set maps, as `Read_Binary` applies them:
/// `art` returns a template's icon map (`None` entries are blank icons), or `None` if the
/// theater has no art for it.
pub fn icon_check_from_art<F>(art: F) -> impl Fn(TemplateType, u8) -> bool
where
    F: Fn(TemplateType) -> Option<Vec<Option<u8>>>,
{
    move |template, icon| {
        let Some(map) = art(template) else {
            return true;
        };
        let data = template.data();
        i32::from(icon) < data.width * data.height
            && map.get(usize::from(icon)).is_some_and(Option::is_some)
    }
}

/// What loading did, for tests and diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MapLoadReport {
    /// Template cells cleared because their icon doesn't exist in the theater.
    pub icons_cleared: u32,
    /// Terrain objects placed.
    pub terrain: u32,
    /// Terrain objects skipped (wrong theater, off the map, unknown type).
    pub terrain_skipped: u32,
    /// Overlays placed.
    pub overlays: u32,
    /// Overlays the placement rules refused (blocked cell, unbuildable ground, ...).
    pub overlays_refused: u32,
    /// Total Tiberium worth after the final pass, in credits.
    pub tiberium_value: i64,
}

/// Cells the Remastered Collection forces to rock after loading, to close holes in two
/// Nod missions (`Read_Scenario_Ini`).
const LAND_FIXES: [(&str, u16); 2] = [("scb07ea", 2795), ("scb10eb", 2015)];

/// Loads `ini` and its `.BIN` terrain into `sim`'s [`Map`] and spawns its terrain
/// objects. `name` is the scenario's file stem (e.g. `SCB07EA`), which selects the
/// Remastered Collection's per-mission fixes.
pub fn load_td_map(
    sim: &mut Simulation,
    name: &str,
    ini: &Ini,
    bin: &[u8],
    icon_exists: Option<IconCheck<'_>>,
) -> MapLoadReport {
    let mut report = MapLoadReport::default();
    let info = cnc_td::map::read_map_info(ini);
    let mut map = Map::new(info.theater, info.bounds);
    let world = sim.world_mut();

    read_bin(&mut map, bin, icon_exists, &mut report);
    place_terrain(world, &mut map, ini, &mut report);
    place_overlays(&mut map, ini, &mut report);

    // The final smoothing pass, then the Remastered fixes.
    let mut rng = world.resource::<CrtRng>().0;
    report.tiberium_value = map.overpass(&mut rng);
    world.resource_mut::<CrtRng>().0 = rng;
    for (scenario, cell) in LAND_FIXES {
        if name.eq_ignore_ascii_case(scenario) {
            map.override_land(Cell::from_index(cell), Land::Rock);
        }
    }

    world.insert_resource(map);
    report
}

/// Terrain icons (`Read_Binary`). A short file leaves the remaining cells clear.
fn read_bin(
    map: &mut Map,
    bin: &[u8],
    icon_exists: Option<IconCheck<'_>>,
    report: &mut MapLoadReport,
) {
    for (index, pair) in (0_u16..).zip(bin.chunks_exact(2).take(MAP_CELLS)) {
        let cell = Cell::from_index(index);
        // 255 is TEMPLATE_NONE; ids past the table would be out of bounds in the
        // original, so they count as none too.
        let Some(template) = TemplateType::from_id(pair[0]) else {
            continue;
        };
        let icon = pair[1];
        let valid = template == TemplateType::Clear1
            || icon_exists.is_none_or(|exists| exists(template, icon));
        if valid {
            map.set_template(cell, Some((template, icon)));
        } else {
            report.icons_cleared += 1;
        }
    }
}

/// Terrain objects (`TerrainClass::Read_INI`), in file order.
fn place_terrain(world: &mut World, map: &mut Map, ini: &Ini, report: &mut MapLoadReport) {
    for (key, value) in ini.section("TERRAIN").into_iter().flat_map(Section::iter) {
        let cell = atoi(key);
        let kind = tokens(value)
            .next()
            .and_then(|n| n.parse::<TerrainType>().ok());
        let (Some(kind), Ok(index)) = (kind, u16::try_from(cell)) else {
            report.terrain_skipped += 1;
            continue;
        };
        let data = kind.data();
        if usize::from(index) >= MAP_CELLS || !data.in_theater(map.theater()) {
            report.terrain_skipped += 1;
            continue;
        }
        let cell = Cell::from_index(index);
        let mut next = world.resource_mut::<NextHeapIndex>();
        let heap = HeapIndex(next.0);
        next.0 += 1;
        // `Coord_Fixup`: terrain sits at its cell's top-left corner.
        let corner = Coord::new(i32::from(cell.x()) << 8, i32::from(cell.y()) << 8);
        let entity = world
            .spawn((
                Kind::Terrain,
                heap,
                Position(corner),
                Strength(data.strength),
                TerrainObject(kind),
            ))
            .id();
        for &offset in data.occupy {
            let target = i32::from(index) + i32::from(offset);
            if let Ok(target) = u16::try_from(target)
                && usize::from(target) < MAP_CELLS
            {
                map.occupy(Cell::from_index(target), entity);
            }
        }
        report.terrain += 1;
    }
}

/// Overlays (`OverlayClass::Read_INI`), in file order. Only terrain occupies cells at
/// this point, so any occupant means a terrain object.
fn place_overlays(map: &mut Map, ini: &Ini, report: &mut MapLoadReport) {
    let last = MAP_CELLS as i32 - i32::from(MAP_WIDTH);
    for (key, value) in ini.section("OVERLAY").into_iter().flat_map(Section::iter) {
        let Some(overlay) = tokens(value)
            .next()
            .and_then(|n| n.parse::<OverlayType>().ok())
        else {
            continue;
        };
        // The top and bottom rows are refused, bar the bottom row's first cell (the
        // original compares `cell <= MAP_CELL_TOTAL - MAP_CELL_W`).
        let cell = match u16::try_from(atoi(key)) {
            Ok(cell) if (i32::from(MAP_WIDTH)..=last).contains(&i32::from(cell)) => {
                Cell::from_index(cell)
            }
            _ => {
                report.overlays_refused += 1;
                continue;
            }
        };
        let has_terrain = !map.cell(cell).occupiers().is_empty();
        if map.place_overlay(cell, overlay, has_terrain, true) {
            report.overlays += 1;
        } else {
            report.overlays_refused += 1;
        }
    }
}
