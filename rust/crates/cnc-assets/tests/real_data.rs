//! Decodes assets from a real Remastered Collection install.
//!
//! Does nothing unless `CNC_REMASTERED_DIR` points at the game folder; see
//! `cnc-editor/tests/real_data.rs`.

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;

use cnc_assets::aud::{Compression, Sound};
use cnc_assets::iconset::IconSet;
use cnc_assets::palette::Palette;
use cnc_assets::shp::Shape;
use cnc_core::mix::MixArchive;

fn td_mix(name: &str) -> Option<MixArchive<BufReader<File>>> {
    let root = PathBuf::from(std::env::var_os("CNC_REMASTERED_DIR")?);
    let path = root.join("Data/CNCDATA/TIBERIAN_DAWN/CD1").join(name);
    let file = File::open(&path).ok()?;
    Some(MixArchive::open(BufReader::new(file)).unwrap_or_else(|e| panic!("{name}: {e}")))
}

#[test]
fn td_shapes() {
    let Some(mut conquer) = td_mix("CONQUER.MIX") else {
        return;
    };
    let names = cnc_td::UnitType::ALL
        .iter()
        .map(|t| t.name())
        .chain(cnc_td::InfantryType::ALL.iter().map(|t| t.name()))
        .chain(cnc_td::BuildingType::ALL.iter().map(|t| t.name()))
        .chain(cnc_td::AircraftType::ALL.iter().map(|t| t.name()));
    let (mut decoded, mut frames) = (0, 0);
    for name in names {
        let file = format!("{name}.SHP");
        let Some(data) = conquer.read(&file).expect("io") else {
            continue;
        };
        let shape = Shape::decode(&data).unwrap_or_else(|e| panic!("{file}: {e}"));
        let size = usize::from(shape.width) * usize::from(shape.height);
        assert!(size > 0, "{file}: empty frame size");
        assert!(shape.frames.iter().all(|f| f.len() == size), "{file}");
        assert!(
            shape.frames.iter().any(|f| f.iter().any(|&p| p != 0)),
            "{file}: blank"
        );
        decoded += 1;
        frames += shape.frames.len();
    }
    println!("decoded {decoded} TD object shapes, {frames} frames");
    assert!(decoded > 50);
}

#[test]
fn td_templates_and_palettes() {
    for (mix, ext, pal) in [
        ("TEMPERAT.MIX", "TEM", "TEMPERAT.PAL"),
        ("DESERT.MIX", "DES", "DESERT.PAL"),
        ("WINTER.MIX", "WIN", "WINTER.PAL"),
    ] {
        let Some(mut archive) = td_mix(mix) else {
            continue;
        };
        let palette = archive.read(pal).expect("io").expect("theater palette");
        Palette::from_pal(&palette).unwrap_or_else(|e| panic!("{pal}: {e}"));
        let mut decoded = 0;
        for template in cnc_td::TemplateType::ALL {
            let file = format!("{}.{ext}", template.name());
            let Some(data) = archive.read(&file).expect("io") else {
                continue;
            };
            let set = IconSet::decode(&data).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!((set.width, set.height), (24, 24), "{file}");
            assert!(!set.icons.is_empty() && !set.map.is_empty(), "{file}");
            assert!(
                set.map
                    .iter()
                    .flatten()
                    .all(|&i| usize::from(i) < set.icons.len()),
                "{file}: map refers to a missing icon"
            );
            decoded += 1;
        }
        println!("{mix}: {decoded} templates");
        assert!(decoded > 50, "{mix}: only {decoded} templates");
    }
}

#[test]
fn td_audio_decodes_to_declared_length() {
    let (mut sos, mut westwood) = (0, 0);
    for mix in ["SOUNDS.MIX", "SPEECH.MIX", "ZOUNDS.MIX"] {
        let Some(mut archive) = td_mix(mix) else {
            continue;
        };
        let entries = archive.index().entries().to_vec();
        for entry in entries {
            let data = archive.read_entry(entry).expect("io");
            let Ok(sound) = Sound::decode(&data) else {
                continue;
            };
            let header = sound.header;
            let bytes_per_sample = if header.sixteen_bit { 2 } else { 1 };
            let decoded = sound.samples.len() * bytes_per_sample;
            let declared = header.uncompressed_size as usize;
            // Westwood's encoder sometimes declares one more sample in a final SOS chunk
            // than its data holds; nothing else may differ.
            assert!(
                decoded <= declared && declared - decoded <= 2,
                "{mix} entry {:08X}: decoded {decoded} bytes, header says {declared}",
                entry.id
            );
            match header.compression {
                Compression::Sos => sos += 1,
                Compression::Westwood => westwood += 1,
                Compression::None => {}
            }
        }
    }
    println!("decoded {sos} SOS and {westwood} Westwood-compressed sounds");
    if sos + westwood > 0 {
        assert!(westwood > 0, "expected some Westwood-compressed effects");
    }
}

#[test]
fn ra_nested_assets() {
    let Some(root) = std::env::var_os("CNC_REMASTERED_DIR").map(PathBuf::from) else {
        return;
    };
    let Ok(file) = File::open(root.join("Data/CNCDATA/RED_ALERT/CD1/REDALERT.MIX")) else {
        return;
    };
    let mut redalert = MixArchive::open(BufReader::new(file)).expect("REDALERT.MIX");
    let local = redalert.read("LOCAL.MIX").expect("io").expect("LOCAL.MIX");
    let mut local = MixArchive::open(Cursor::new(local)).expect("LOCAL.MIX");
    for pal in ["TEMPERAT.PAL", "SNOW.PAL", "INTERIOR.PAL"] {
        if let Some(data) = local.read(pal).expect("io") {
            Palette::from_pal(&data).unwrap_or_else(|e| panic!("{pal}: {e}"));
        }
    }
    let hires = redalert.read("HIRES.MIX").expect("io").expect("HIRES.MIX");
    let mut hires = MixArchive::open(Cursor::new(hires)).expect("HIRES.MIX");
    let mut decoded = 0;
    for name in [
        "SIDEBAR", "STRIP", "POWER", "REPAIR", "SELL", "MAP", "OPTIONS", "TABS",
    ] {
        if let Some(data) = hires.read(&format!("{name}.SHP")).expect("io") {
            Shape::decode(&data).unwrap_or_else(|e| panic!("{name}.SHP: {e}"));
            decoded += 1;
        }
    }
    println!("decoded {decoded} RA interface shapes");
}

/// Red Alert's art and sound from `MAIN.MIX` (CD1, or the Aftermath copy).
#[test]
fn ra_main_assets() {
    let Some(root) = std::env::var_os("CNC_REMASTERED_DIR").map(PathBuf::from) else {
        return;
    };
    let Some(path) = ["RED_ALERT/CD1/MAIN.MIX", "RED_ALERT/AFTERMATH/MAIN.MIX"]
        .iter()
        .map(|p| root.join("Data/CNCDATA").join(p))
        .find(|p| p.exists())
    else {
        return;
    };
    let mut main =
        MixArchive::open(BufReader::new(File::open(path).expect("open"))).expect("MAIN.MIX");
    let mut nested = |name: &str| {
        let data = main
            .read(name)
            .expect("io")
            .unwrap_or_else(|| panic!("{name} in MAIN.MIX"));
        MixArchive::open(Cursor::new(data)).unwrap_or_else(|e| panic!("{name}: {e}"))
    };

    let mut conquer = nested("CONQUER.MIX");
    let names = cnc_ra::UnitType::ALL
        .iter()
        .map(|t| t.name())
        .chain(cnc_ra::InfantryType::ALL.iter().map(|t| t.name()))
        .chain(cnc_ra::VesselType::ALL.iter().map(|t| t.name()))
        .chain(cnc_ra::AircraftType::ALL.iter().map(|t| t.name()))
        .chain(cnc_ra::BuildingType::ALL.iter().map(|t| t.name()));
    let mut shapes = 0;
    for name in names {
        let file = format!("{name}.SHP");
        if let Some(data) = conquer.read(&file).expect("io") {
            Shape::decode(&data).unwrap_or_else(|e| panic!("{file}: {e}"));
            shapes += 1;
        }
    }

    let mut templates = 0;
    for (mix, ext) in [
        ("TEMPERAT.MIX", "TEM"),
        ("SNOW.MIX", "SNO"),
        ("INTERIOR.MIX", "INT"),
    ] {
        let mut archive = nested(mix);
        for template in cnc_ra::TemplateType::ALL {
            let file = format!("{}.{ext}", template.name());
            let Some(data) = archive.read(&file).expect("io") else {
                continue;
            };
            let set = IconSet::decode(&data).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert!(set.map_size.is_some(), "{file}: expected Red Alert layout");
            assert!(
                set.map
                    .iter()
                    .flatten()
                    .all(|&i| usize::from(i) < set.icons.len()),
                "{file}"
            );
            templates += 1;
        }
    }

    let mut sounds = nested("SOUNDS.MIX");
    let entries = sounds.index().entries().to_vec();
    let mut decoded = 0;
    for entry in entries {
        let data = sounds.read_entry(entry).expect("io");
        if let Ok(sound) = Sound::decode(&data) {
            let bytes = sound.samples.len() * if sound.header.sixteen_bit { 2 } else { 1 };
            let declared = sound.header.uncompressed_size as usize;
            assert!(
                bytes <= declared && declared - bytes <= 2,
                "sound {:08X}",
                entry.id
            );
            decoded += 1;
        }
    }
    println!("RA: {shapes} object shapes, {templates} templates, {decoded} sounds");
    assert!(shapes > 50 && templates > 100 && decoded > 50);
}
