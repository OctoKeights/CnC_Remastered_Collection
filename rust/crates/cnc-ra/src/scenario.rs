//! The placed objects of a Red Alert scenario INI: `[UNITS]`, `[SHIPS]`, `[AIRCRAFT]`,
//! `[INFANTRY]`, `[STRUCTURES]`, `[TERRAIN]`, `[SMUDGE]`, `[Waypoints]` and
//! `[CellTriggers]`. (Overlays live in `[OverlayPack]`; see [`crate::map`].)
//!
//! Entries are written back in the format of the game's `Write_INI` functions. Red Alert
//! never wrote aircraft, so `[AIRCRAFT]` uses the line format its reader expects.

use std::collections::BTreeMap;

use cnc_core::ini::{Ini, Section};
use cnc_core::scenario as sc;

use crate::{
    AircraftType, BuildingType, House, InfantryType, SmudgeType, TerrainType, UnitType, VesselType,
};

cnc_core::named_enum! {
    /// An order an object is given (`MissionType`, names from `MissionClass::Missions`).
    pub enum Mission: u8 {
        /// Do nothing.
        Sleep = 0 => "Sleep",
        /// Attack a target.
        Attack = 1 => "Attack",
        /// Move to a destination.
        Move = 2 => "Move",
        /// Move as part of a queued order.
        QMove = 3 => "QMove",
        /// Leave the map.
        Retreat = 4 => "Retreat",
        /// Guard the current position.
        Guard = 5 => "Guard",
        /// Guard without moving.
        Sticky = 6 => "Sticky",
        /// Enter a transport or building.
        Enter = 7 => "Enter",
        /// Capture a building.
        Capture = 8 => "Capture",
        /// Harvest Tiberium.
        Harvest = 9 => "Harvest",
        /// Guard an area.
        AreaGuard = 10 => "Area Guard",
        /// Return to base.
        Return = 11 => "Return",
        /// Stop.
        Stop = 12 => "Stop",
        /// Wait until enemies come near.
        Ambush = 13 => "Ambush",
        /// Seek and destroy.
        Hunt = 14 => "Hunt",
        /// Unload cargo.
        Unload = 15 => "Unload",
        /// Blow up a building.
        Sabotage = 16 => "Sabotage",
        /// Being built.
        Construction = 17 => "Construction",
        /// Being sold.
        Selling = 18 => "Selling",
        /// Repair.
        Repair = 19 => "Repair",
        /// Rescue.
        Rescue = 20 => "Rescue",
        /// Fire a missile.
        Missile = 21 => "Missile",
        /// Do nothing and pose no threat.
        Harmless = 22 => "Harmless",
    }
}

/// Number of waypoint slots: 0-97 are numbered, 98 is home, 99 reinforcements, 100 the
/// special airdrop point.
pub const WAYPOINT_COUNT: u8 = 101;

/// A vehicle.
pub type Unit = sc::Unit<House, UnitType, Mission>;
/// A ship.
pub type Ship = sc::Unit<House, VesselType, Mission>;
/// An aircraft.
pub type Aircraft = sc::Unit<House, AircraftType, Mission>;
/// An infantry soldier.
pub type Infantry = sc::Infantry<House, InfantryType, Mission>;
/// A building.
pub type Structure = sc::Structure<House, BuildingType>;
/// A terrain object.
pub type Terrain = sc::Terrain<TerrainType>;
/// A smudge.
pub type Smudge = sc::Smudge<SmudgeType>;

/// Everything placed on a Red Alert map, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Objects {
    /// `[UNITS]`.
    pub units: Vec<Unit>,
    /// `[SHIPS]`.
    pub ships: Vec<Ship>,
    /// `[AIRCRAFT]`.
    pub aircraft: Vec<Aircraft>,
    /// `[INFANTRY]`.
    pub infantry: Vec<Infantry>,
    /// `[STRUCTURES]`.
    pub structures: Vec<Structure>,
    /// `[TERRAIN]`.
    pub terrain: Vec<Terrain>,
    /// `[SMUDGE]`.
    pub smudge: Vec<Smudge>,
    /// `[Waypoints]`: waypoint to cell.
    pub waypoints: BTreeMap<u8, u16>,
    /// `[CellTriggers]`: cell to trigger name.
    pub cell_triggers: BTreeMap<u16, String>,
}

/// Sections this module reads and writes.
pub const SECTIONS: [&str; 9] = [
    "UNITS",
    "SHIPS",
    "AIRCRAFT",
    "INFANTRY",
    "STRUCTURES",
    "TERRAIN",
    "SMUDGE",
    "Waypoints",
    "CellTriggers",
];

fn values<'a>(ini: &'a Ini, section: &str) -> impl Iterator<Item = (&'a str, &'a str)> {
    ini.section(section).into_iter().flat_map(Section::iter)
}

impl Objects {
    /// Reads every object section. Lines the game would skip are skipped.
    #[must_use]
    pub fn read_ini(ini: &Ini) -> Self {
        Self {
            units: values(ini, "UNITS")
                .filter_map(|(_, v)| Unit::parse(v))
                .collect(),
            ships: values(ini, "SHIPS")
                .filter_map(|(_, v)| Ship::parse(v))
                .collect(),
            aircraft: values(ini, "AIRCRAFT")
                .filter_map(|(_, v)| Aircraft::parse(v))
                .collect(),
            infantry: values(ini, "INFANTRY")
                .filter_map(|(_, v)| Infantry::parse(v))
                .collect(),
            structures: values(ini, "STRUCTURES")
                .filter_map(|(_, v)| Structure::parse(v))
                .collect(),
            terrain: values(ini, "TERRAIN")
                .filter_map(|(k, v)| Terrain::parse(k, v))
                .collect(),
            smudge: values(ini, "SMUDGE")
                .filter_map(|(_, v)| Smudge::parse(v))
                .collect(),
            waypoints: sc::read_waypoints(ini.section("Waypoints"), WAYPOINT_COUNT),
            cell_triggers: sc::read_cell_triggers(ini.section("CellTriggers")),
        }
    }

    /// Replaces the object sections of `ini` with these objects.
    pub fn write_ini(&self, ini: &mut Ini) {
        for name in SECTIONS {
            ini.remove_section(name);
        }
        let key = |i: usize| i.to_string();
        let mut put = |section: &str, lines: Vec<(String, String)>| {
            if !lines.is_empty() {
                let s = ini.section_or_insert(section);
                for (k, v) in lines {
                    s.set(k, v);
                }
            }
        };
        put(
            "UNITS",
            self.units
                .iter()
                .enumerate()
                .map(|(i, u)| (key(i), u.to_value()))
                .collect(),
        );
        put(
            "SHIPS",
            self.ships
                .iter()
                .enumerate()
                .map(|(i, u)| (key(i), u.to_value()))
                .collect(),
        );
        put(
            "AIRCRAFT",
            self.aircraft
                .iter()
                .enumerate()
                .map(|(i, a)| (key(i), a.to_value_without_trigger()))
                .collect(),
        );
        put(
            "INFANTRY",
            self.infantry
                .iter()
                .enumerate()
                .map(|(i, u)| (key(i), u.to_value()))
                .collect(),
        );
        put(
            "STRUCTURES",
            self.structures
                .iter()
                .enumerate()
                .map(|(i, s)| (key(i), s.to_value(true)))
                .collect(),
        );
        put(
            "TERRAIN",
            self.terrain
                .iter()
                .map(|t| (t.cell.to_string(), t.kind.to_string()))
                .collect(),
        );
        put(
            "SMUDGE",
            self.smudge
                .iter()
                .enumerate()
                .map(|(i, s)| (key(i), s.to_value()))
                .collect(),
        );
        // Red Alert writes only the waypoints that are set.
        put(
            "Waypoints",
            self.waypoints
                .iter()
                .map(|(i, cell)| (i.to_string(), cell.to_string()))
                .collect(),
        );
        put(
            "CellTriggers",
            self.cell_triggers
                .iter()
                .map(|(cell, t)| (cell.to_string(), t.clone()))
                .collect(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[UNITS]
0=Greece,2TNK,256,5000,0,Guard,None

[SHIPS]
0=USSR,SS,256,6000,64,Hunt,sub1

[INFANTRY]
0=USSR,E1,256,5100,1,Area Guard,0,None

[STRUCTURES]
0=Greece,POWR,256,5200,0,None,1,1
1=Greece,FAKE,256,5200,0,None

[TERRAIN]
5300=T01

[Waypoints]
98=5000

[CellTriggers]
5400=enter
";

    #[test]
    fn reads_and_writes() {
        let objects = Objects::read_ini(&Ini::parse(SAMPLE));
        assert_eq!(objects.structures.len(), 1, "unknown building skipped");
        assert!(objects.structures[0].sellable && objects.structures[0].rebuild);
        assert_eq!(objects.ships[0].trigger.as_deref(), Some("sub1"));
        assert_eq!(objects.infantry[0].mission, Some(Mission::AreaGuard));

        let mut ini = Ini::new();
        objects.write_ini(&mut ini);
        assert_eq!(
            ini.get("STRUCTURES", "0"),
            Some("Greece,POWR,256,5200,0,None,1,1")
        );
        assert_eq!(ini.get("TERRAIN", "5300"), Some("T01"));
        assert_eq!(ini.section("Waypoints").map(Section::len), Some(1));
        assert_eq!(Objects::read_ini(&ini), objects);
    }
}
