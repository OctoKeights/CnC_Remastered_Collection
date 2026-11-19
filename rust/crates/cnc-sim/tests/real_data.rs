//! Loads every Tiberian Dawn campaign map into the simulation.
//!
//! Does nothing unless `CNC_REMASTERED_DIR` points at the game folder (see
//! `crates/cnc-editor/tests/real_data.rs`).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use cnc_assets::iconset::IconSet;
use cnc_core::ini::Ini;
use cnc_core::mix::MixArchive;
use cnc_core::scenario::atoi;
use cnc_sim::scenario::IconCheck;
use cnc_sim::{Map, Simulation, Threading, icon_check_from_art, load_td_scenario};
use cnc_td::data::Land;
use cnc_td::{Cell, OverlayType, TemplateType, Theater};

fn cd1() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("CNC_REMASTERED_DIR")?);
    Some(root.join("Data/CNCDATA/TIBERIAN_DAWN/CD1"))
}

fn mix(dir: &std::path::Path, name: &str) -> MixArchive<BufReader<File>> {
    let file = File::open(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    MixArchive::open(BufReader::new(file)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every template's icon map in each theater.
fn theater_art(dir: &std::path::Path) -> HashMap<(Theater, TemplateType), Vec<Option<u8>>> {
    let mut art = HashMap::new();
    for (theater, file, ext) in [
        (Theater::Temperate, "TEMPERAT.MIX", "TEM"),
        (Theater::Desert, "DESERT.MIX", "DES"),
        (Theater::Winter, "WINTER.MIX", "WIN"),
    ] {
        let mut archive = mix(dir, file);
        for &template in TemplateType::ALL {
            let name = format!("{}.{ext}", template.name());
            if let Some(data) = archive.read(&name).expect("io") {
                let set = IconSet::decode(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
                art.insert((theater, template), set.map);
            }
        }
    }
    art
}

fn load(name: &str, ini: &Ini, bin: &[u8], check: Option<IconCheck<'_>>) -> Simulation {
    let mut sim = Simulation::new(Threading::Single);
    load_td_scenario(&mut sim, name, ini, bin, check);
    sim
}

#[test]
#[allow(clippy::too_many_lines)]
fn campaign_maps_load() {
    let Some(dir) = cd1() else {
        eprintln!("CNC_REMASTERED_DIR not set; skipping");
        return;
    };
    let art = theater_art(&dir);
    let mut general = mix(&dir, "GENERAL.MIX");
    let (mut maps, mut cleared, mut refused, mut units) = (0, 0, 0, 0);
    let mut land_totals: HashMap<Land, u32> = HashMap::new();
    for side in ["G", "B"] {
        for n in 1..=15 {
            for variant in ["EA", "EB", "EC"] {
                let stem = format!("SC{side}{n:02}{variant}");
                let (Some(ini), Some(bin)) = (
                    general.read(&format!("{stem}.INI")).expect("io"),
                    general.read(&format!("{stem}.BIN")).expect("io"),
                ) else {
                    continue;
                };
                let ini = Ini::parse(&String::from_utf8_lossy(&ini));
                let theater = cnc_td::map::read_map_info(&ini).theater;
                let check = icon_check_from_art(|t| art.get(&(theater, t)).cloned());

                let mut sim = Simulation::new(Threading::Single);
                let report = load_td_scenario(&mut sim, &stem, &ini, &bin, Some(&check));
                maps += 1;
                cleared += report.icons_cleared;
                refused += report.overlays_refused;
                units += report.units;
                let listed_units = ini.section("UNITS").map_or(0, |s| s.iter().count());
                assert_eq!(
                    report.units + report.units_skipped,
                    u32::try_from(listed_units).expect("small"),
                    "{stem}"
                );

                // Loading, and running with every vehicle parked, are deterministic.
                let mut again = load(&stem, &ini, &bin, Some(&check));
                assert_eq!(sim.digest(), again.digest(), "{stem}");
                for _ in 0..30 {
                    assert_eq!(
                        sim.step().expect("schedule"),
                        again.step().expect("schedule")
                    );
                }
                assert_eq!(sim.digest(), again.digest(), "{stem}");

                // Each vehicle sits at its cell's centre, in that cell's occupant list.
                let world = sim.world_mut();
                let placed: Vec<_> = world
                    .query::<(
                        cnc_sim::bevy_ecs::entity::Entity,
                        &cnc_sim::drive::UnitKind,
                        &cnc_sim::Position,
                    )>()
                    .iter(world)
                    .map(|(e, _, p)| (e, p.0))
                    .collect();
                let map = world.resource::<Map>();
                for (entity, coord) in placed {
                    let cell = Cell::containing(coord);
                    assert_eq!(coord, cell.center(), "{stem}");
                    assert!(map.cell(cell).occupiers().contains(&entity), "{stem}");
                }

                let map = sim.world().resource::<Map>();
                // Every overlay the file asks for is either on the map or was refused.
                let listed = ini.section("OVERLAY").map_or(0, |s| s.iter().count());
                assert_eq!(
                    report.overlays + report.overlays_refused,
                    u32::try_from(listed).expect("small"),
                    "{stem}"
                );
                for cell in map.cells() {
                    *land_totals.entry(cell.land()).or_default() += 1;
                    // The overpass re-rolls Tiberium, so every Tiberium cell is one of
                    // the twelve Tiberium overlays with a density from the table.
                    if cell.land() == Land::Tiberium
                        && let Some(overlay) = cell.overlay
                    {
                        assert!(overlay.data().is_tiberium, "{stem}");
                        assert!(cell.overlay_data <= 11, "{stem}");
                    }
                }
                // Walls in the file are on the map with their join bits set.
                for (key, value) in ini
                    .section("OVERLAY")
                    .into_iter()
                    .flat_map(cnc_core::ini::Section::iter)
                {
                    let overlay: OverlayType = value
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .parse()
                        .expect("known overlay");
                    let Ok(index) = u16::try_from(atoi(key)) else {
                        continue;
                    };
                    if overlay.data().is_wall && (64..=4032).contains(&index) {
                        let cell = map.cell(Cell::from_index(index));
                        assert_eq!(cell.overlay, Some(overlay), "{stem} cell {index}");
                        assert_eq!(cell.land(), Land::Wall, "{stem} cell {index}");
                    }
                }
                if stem == "SCB07EA" {
                    assert_eq!(map.land(Cell::from_index(2795)), Land::Rock);
                }
                if report.tiberium_value > 0 {
                    assert!(map.cells().iter().any(|c| c.land() == Land::Tiberium));
                }
            }
        }
    }
    println!(
        "{maps} maps; {units} vehicles; {cleared} template cells cleared for missing icons; \
         {refused} overlays refused; land totals {land_totals:?}"
    );
    assert_eq!(maps, 47);
    assert!(land_totals[&Land::Water] > 0 && land_totals[&Land::Tiberium] > 0);
    // Authentic: every TD road template is defined as clear ground, so no cell is road.
    assert!(!land_totals.contains_key(&Land::Road));
}
