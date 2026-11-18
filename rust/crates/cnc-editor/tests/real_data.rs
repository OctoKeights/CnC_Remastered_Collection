//! Checks against a real Remastered Collection install.
//!
//! These tests do nothing unless `CNC_REMASTERED_DIR` points at the game folder, e.g.
//!
//! ```sh
//! CNC_REMASTERED_DIR="C:/Program Files (x86)/Steam/steamapps/common/CnCRemastered" \
//!     cargo test -p cnc-editor --test real_data -- --nocapture
//! ```

use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::path::PathBuf;

use cnc_core::ini::Ini;
use cnc_core::meg::MegArchive;
use cnc_core::mix::MixArchive;
use cnc_editor::{Game, Scenario};

fn root() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("CNC_REMASTERED_DIR")?);
    assert!(
        dir.is_dir(),
        "CNC_REMASTERED_DIR is not a directory: {}",
        dir.display()
    );
    Some(dir)
}

fn cncdata(root: &std::path::Path, rest: &str) -> PathBuf {
    root.join("Data").join("CNCDATA").join(rest)
}

fn open_mix(path: &std::path::Path) -> MixArchive<BufReader<File>> {
    let file = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    MixArchive::open(BufReader::new(file)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every file listed in a MIX index must lie inside the file, after the header.
fn check_bounds(path: &std::path::Path, mix: &MixArchive<BufReader<File>>) {
    let len = fs::metadata(path).map(|m| m.len()).unwrap_or_default();
    let index = mix.index();
    let digest = if index.has_digest() { 20 } else { 0 };
    assert!(
        index.data_start() + u64::from(index.data_size()) + digest <= len,
        "{}: data block runs past end of file",
        path.display()
    );
}

/// A re-saved `.BIN` must match the original byte for byte, except that clear cells are
/// normalised to the game's `255, 0`.
fn assert_bin_matches(what: &str, original: &[u8], saved: Option<&[u8]>) {
    let saved = saved.unwrap_or_else(|| panic!("{what}: no .BIN saved"));
    assert_eq!(saved.len(), cnc_td::map::BIN_SIZE, "{what}: .BIN size");
    let differing = original
        .chunks_exact(2)
        .zip(saved.chunks_exact(2))
        .filter(|(a, b)| a != b && !(a[0] == 0xFF && b == &[0xFF, 0]))
        .count();
    assert_eq!(differing, 0, "{what}: {differing} cells changed on save");
}

#[test]
fn tiberian_dawn_mixes_and_maps() {
    let Some(root) = root() else { return };
    let cd1 = cncdata(&root, "TIBERIAN_DAWN/CD1");
    for name in [
        "CONQUER.MIX",
        "GENERAL.MIX",
        "TEMPERAT.MIX",
        "DESERT.MIX",
        "LOCAL.MIX",
    ] {
        let path = cd1.join(name);
        if path.exists() {
            let mix = open_mix(&path);
            check_bounds(&path, &mix);
            println!("{name}: {} files", mix.index().entries().len());
        }
    }

    // Every campaign map in GENERAL.MIX must load and round-trip.
    let mut general = open_mix(&cd1.join("GENERAL.MIX"));
    let mut loaded = 0;
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
                let text = String::from_utf8_lossy(&ini);
                let scenario =
                    Scenario::load_td(&text, &bin).unwrap_or_else(|e| panic!("{stem}: {e}"));
                assert_eq!(scenario.game(), Game::TiberianDawn);
                let saved = scenario.save();
                assert_eq!(
                    saved.bin.as_deref(),
                    Some(bin.as_slice()),
                    "{stem}: .BIN changed"
                );
                let again = Scenario::load_td(&saved.ini, &bin).expect("reload");
                assert_eq!(
                    again, scenario,
                    "{stem}: INI round trip changed the scenario"
                );
                loaded += 1;
            }
        }
    }
    println!("loaded {loaded} TD campaign maps");
    assert!(loaded >= 40, "only {loaded} campaign maps found");
}

#[test]
fn red_alert_encrypted_mixes_and_rules() {
    let Some(root) = root() else { return };
    let cd1 = cncdata(&root, "RED_ALERT/CD1");
    for name in ["REDALERT.MIX", "EXPAND2.MIX", "HIRES1.MIX", "LORES1.MIX"] {
        let path = cd1.join(name);
        if path.exists() {
            let mix = open_mix(&path);
            check_bounds(&path, &mix);
        }
    }

    let mut redalert = open_mix(&cd1.join("REDALERT.MIX"));
    let local = redalert
        .read("LOCAL.MIX")
        .expect("io")
        .expect("LOCAL.MIX in REDALERT.MIX");
    let mut local = MixArchive::open(Cursor::new(local)).expect("nested encrypted mix");
    let rules = local
        .read("RULES.INI")
        .expect("io")
        .expect("RULES.INI in LOCAL.MIX");
    let rules = Ini::parse(&String::from_utf8_lossy(&rules));

    // Spot-check values every copy of Red Alert's rules has.
    let general = rules.section("General").expect("[General]");
    assert!(general.get_fixed("BuildSpeed").is_some());
    assert!(rules.section("E1").is_some() && rules.section("MCV").is_some());
    let missing: Vec<_> = cnc_ra::UnitType::ALL
        .iter()
        .filter(|u| rules.section(u.name()).is_none())
        .map(|u| u.name())
        .collect();
    // Aftermath's units and the giant ants are defined in AFTRMATH.INI / the ant missions.
    let expansion = [
        "ANT1", "ANT2", "ANT3", "CTNK", "TTNK", "QTNK", "DTRK", "STNK",
    ];
    assert!(
        missing.iter().all(|m| expansion.contains(m)),
        "RULES.INI lacks base-game units: {missing:?}"
    );
    println!("RULES.INI: {} sections", rules.sections().len());

    // Typed rules: everything the base game builds has stats, and every weapon, warhead
    // and projectile it refers to is defined.
    let mut typed = cnc_ra::rules::Rules::from_ini(&rules);
    assert_eq!(
        typed.global.gold_value,
        rules.get_int("General", "GoldValue").expect("GoldValue")
    );
    for (unit, stats) in &typed.units {
        assert!(stats.techno.strength > 0, "{unit} has no strength");
        for weapon in [stats.techno.primary, stats.techno.secondary]
            .into_iter()
            .flatten()
        {
            let w = typed
                .weapons
                .get(&weapon)
                .unwrap_or_else(|| panic!("{unit}: {weapon} undefined"));
            if let Some(warhead) = w.warhead {
                assert!(
                    typed.warheads.contains_key(&warhead),
                    "{weapon}: {warhead} undefined"
                );
            }
            if let Some(projectile) = w.projectile {
                assert!(
                    typed.projectiles.contains_key(&projectile),
                    "{weapon}: {projectile} undefined"
                );
            }
        }
    }
    assert!(
        typed.buildings.values().any(|b| b.power > 0)
            && typed.buildings.values().any(|b| b.drain > 0)
    );
    println!(
        "typed rules: {} units, {} infantry, {} vessels, {} aircraft, {} buildings, {} weapons",
        typed.units.len(),
        typed.infantry.len(),
        typed.vessels.len(),
        typed.aircraft.len(),
        typed.buildings.len(),
        typed.weapons.len()
    );

    // AFTRMATH.INI layers the expansion units on top.
    let mut expand2 = open_mix(&cd1.join("EXPAND2.MIX"));
    if let Some(aftermath) = expand2.read("AFTRMATH.INI").expect("io") {
        let before = typed.units.len();
        typed.apply(&Ini::parse(&String::from_utf8_lossy(&aftermath)));
        assert!(typed.units.len() > before, "AFTRMATH.INI added no units");
        assert!(typed.aftermath.new_units_enabled || typed.aftermath.mtank_distance > 0);
        println!("after AFTRMATH.INI: {} units", typed.units.len());
    }
}

#[test]
fn red_alert_community_maps() {
    let Some(root) = root() else { return };
    let dir = cncdata(&root, "RED_ALERT/COMMUNITY");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let mut loaded = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
        {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read");
        let scenario =
            Scenario::load_ra(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let again = Scenario::load_ra(&scenario.save().ini).expect("reload");
        assert_eq!(
            again,
            scenario,
            "{}: round trip changed the scenario",
            path.display()
        );
        if let Scenario::RedAlert { templates, .. } = &scenario {
            assert!(
                templates.grid().placed().count() > 0,
                "{}: empty map",
                path.display()
            );
        }
        loaded += 1;
    }
    println!("loaded {loaded} RA community maps");
}

#[test]
fn tiberian_dawn_loose_maps() {
    let Some(root) = root() else { return };
    let mut loaded = 0;
    for dir in [
        "TIBERIAN_DAWN/COMMUNITY",
        "TIBERIAN_DAWN/CONSOLE_1",
        "TIBERIAN_DAWN/CONSOLE_2",
    ] {
        let Ok(entries) = fs::read_dir(cncdata(&root, dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
            {
                continue;
            }
            let Ok(bin) = fs::read(path.with_extension("BIN")) else {
                continue;
            };
            let text = String::from_utf8_lossy(&fs::read(&path).expect("read")).into_owned();
            let scenario = Scenario::load_td(&text, &bin)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_bin_matches(
                &path.display().to_string(),
                &bin,
                scenario.save().bin.as_deref(),
            );
            loaded += 1;
        }
    }
    println!("loaded {loaded} loose TD maps");
}

#[test]
fn megafiles() {
    let Some(root) = root() else { return };
    for name in [
        "CONFIG.MEG",
        "MAPS.MEG",
        "SFX2D_ALL.MEG",
        "TEXTURES_COMMON_SRGB.MEG",
    ] {
        let path = root.join("Data").join(name);
        let Ok(file) = File::open(&path) else {
            continue;
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or_default();
        let mut meg =
            MegArchive::open(BufReader::new(file)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!meg.entries().is_empty(), "{name}: no entries");
        for entry in meg.entries() {
            assert!(
                u64::from(entry.offset) + u64::from(entry.size) <= len,
                "{name}: {} runs past end of file",
                entry.name
            );
        }
        // Read the first file back by name.
        let first = meg.entries()[0].name.clone();
        assert!(meg.read(&first).expect("io").is_some());
        println!("{name}: {} files", meg.entries().len());
    }
}

/// Red Alert's main archive: the CD1 copy if present, else the Aftermath one.
fn ra_main(root: &std::path::Path) -> Option<MixArchive<BufReader<File>>> {
    ["RED_ALERT/CD1/MAIN.MIX", "RED_ALERT/AFTERMATH/MAIN.MIX"]
        .iter()
        .map(|p| cncdata(root, p))
        .find(|p| p.exists())
        .map(|p| open_mix(&p))
}

fn entry_count(ini: &Ini, sections: &[&str]) -> usize {
    sections
        .iter()
        .filter_map(|s| ini.section(s))
        .map(cnc_core::ini::Section::len)
        .sum()
}

#[test]
fn tiberian_dawn_scenario_objects_and_scripting() {
    let Some(root) = root() else { return };
    let mut general = open_mix(&cncdata(&root, "TIBERIAN_DAWN/CD1/GENERAL.MIX"));
    let (mut maps, mut objects, mut skipped, mut triggers, mut teams) = (0, 0, 0, 0, 0);
    for side in ["G", "B"] {
        for n in 1..=15 {
            for variant in ["EA", "EB", "EC"] {
                let stem = format!("SC{side}{n:02}{variant}.INI");
                let Some(text) = general.read(&stem).expect("io") else {
                    continue;
                };
                let ini = Ini::parse(&String::from_utf8_lossy(&text));

                let parsed = cnc_td::scenario::Objects::read_ini(&ini);
                let count = parsed.units.len()
                    + parsed.aircraft.len()
                    + parsed.infantry.len()
                    + parsed.structures.len()
                    + parsed.terrain.len()
                    + parsed.smudge.len()
                    + parsed.overlay.len();
                let raw = entry_count(
                    &ini,
                    &[
                        "UNITS",
                        "AIRCRAFT",
                        "INFANTRY",
                        "STRUCTURES",
                        "TERRAIN",
                        "SMUDGE",
                        "OVERLAY",
                    ],
                );
                skipped += raw - count;
                objects += count;
                let mut out = Ini::new();
                parsed.write_ini(&mut out);
                assert_eq!(
                    cnc_td::scenario::Objects::read_ini(&out),
                    parsed,
                    "{stem}: objects"
                );

                let scripting = cnc_td::scripting::Scripting::read_ini(&ini);
                triggers += scripting.triggers.len();
                teams += scripting.team_types.len();
                let mut out = Ini::new();
                scripting.write_ini(&mut out);
                assert_eq!(
                    cnc_td::scripting::Scripting::read_ini(&out),
                    scripting,
                    "{stem}: scripting"
                );
                maps += 1;
            }
        }
    }
    println!(
        "TD: {maps} maps, {objects} objects ({skipped} lines skipped), {triggers} triggers, {teams} teams"
    );
    assert!(maps >= 40 && objects > 1000 && triggers > 100 && teams > 50);
    assert!(
        skipped * 100 < objects,
        "too many object lines skipped: {skipped}"
    );
}

#[test]
fn red_alert_campaign_scenarios() {
    let Some(root) = root() else { return };
    let Some(mut main) = ra_main(&root) else {
        return;
    };
    let general = main
        .read("GENERAL.MIX")
        .expect("io")
        .expect("GENERAL.MIX in MAIN.MIX");
    let mut general = MixArchive::open(Cursor::new(general)).expect("GENERAL.MIX");
    let (mut maps, mut objects, mut skipped, mut triggers, mut teams) = (0, 0, 0, 0, 0);
    for prefix in ["SCG", "SCU", "SCA"] {
        for n in 1..=45 {
            for variant in ["EA", "EB", "EC", "ED"] {
                let stem = format!("{prefix}{n:02}{variant}.INI");
                let Some(text) = general.read(&stem).expect("io") else {
                    continue;
                };
                let text = String::from_utf8_lossy(&text);
                let scenario = Scenario::load_ra(&text).unwrap_or_else(|e| panic!("{stem}: {e}"));
                let again = Scenario::load_ra(&scenario.save().ini).expect("reload");
                assert_eq!(again, scenario, "{stem}: map round trip");

                let ini = Ini::parse(&text);
                let parsed = cnc_ra::scenario::Objects::read_ini(&ini);
                let count = parsed.units.len()
                    + parsed.ships.len()
                    + parsed.aircraft.len()
                    + parsed.infantry.len()
                    + parsed.structures.len()
                    + parsed.terrain.len()
                    + parsed.smudge.len();
                let raw = entry_count(
                    &ini,
                    &[
                        "UNITS",
                        "SHIPS",
                        "AIRCRAFT",
                        "INFANTRY",
                        "STRUCTURES",
                        "TERRAIN",
                        "SMUDGE",
                    ],
                );
                skipped += raw - count;
                objects += count;
                let mut out = Ini::new();
                parsed.write_ini(&mut out);
                assert_eq!(
                    cnc_ra::scenario::Objects::read_ini(&out),
                    parsed,
                    "{stem}: objects"
                );

                let scripting = cnc_ra::scripting::Scripting::read_ini(&ini)
                    .unwrap_or_else(|| panic!("{stem}: old INI format"));
                for team in &scripting.team_types {
                    if let Some(t) = team.trigger {
                        assert!(
                            t < scripting.triggers.len(),
                            "{stem}: team {} trigger",
                            team.name
                        );
                    }
                }
                for trigger in &scripting.triggers {
                    for team in trigger
                        .events
                        .iter()
                        .map(|e| e.team)
                        .chain(trigger.actions.iter().map(|a| a.team))
                        .flatten()
                    {
                        assert!(
                            team < scripting.team_types.len(),
                            "{stem}: {} team ref",
                            trigger.name
                        );
                    }
                }
                triggers += scripting.triggers.len();
                teams += scripting.team_types.len();
                let mut out = Ini::new();
                out.set("Basic", "NewINIFormat", 3);
                scripting.write_ini(&mut out);
                assert_eq!(
                    cnc_ra::scripting::Scripting::read_ini(&out).as_ref(),
                    Some(&scripting),
                    "{stem}: scripting"
                );
                maps += 1;
            }
        }
    }
    println!(
        "RA: {maps} maps, {objects} objects ({skipped} lines skipped), {triggers} triggers, {teams} teams"
    );
    // Names can't be listed from a MIX, so this finds the standard campaign names only
    // (14 in the Aftermath disc's copy).
    assert!(maps >= 10 && objects > 1000 && triggers > 100 && teams > 50);
    assert!(
        skipped * 100 < objects,
        "too many object lines skipped: {skipped}"
    );
}
