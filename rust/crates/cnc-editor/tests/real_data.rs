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
