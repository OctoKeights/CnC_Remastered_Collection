//! The placed objects of a Tiberian Dawn scenario INI: `[UNITS]`, `[INFANTRY]`,
//! `[STRUCTURES]`, `[AIRCRAFT]`, `[TERRAIN]`, `[SMUDGE]`, `[OVERLAY]`, `[Waypoints]` and
//! `[CellTriggers]`.
//!
//! Entries are written back in the format and key style (`000`, `001`, ...) of the game's
//! `Write_INI` functions.

use std::collections::BTreeMap;

use cnc_core::ini::{Ini, Section};
use cnc_core::scenario::{self as sc, tokens};

use crate::{
    AircraftType, BuildingType, House, InfantryType, OverlayType, SmudgeType, TerrainType, UnitType,
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
        /// Leave the map.
        Retreat = 3 => "Retreat",
        /// Guard the current position.
        Guard = 4 => "Guard",
        /// Guard without moving.
        Sticky = 5 => "Sticky",
        /// Enter a transport or building.
        Enter = 6 => "Enter",
        /// Capture a building.
        Capture = 7 => "Capture",
        /// Harvest Tiberium.
        Harvest = 8 => "Harvest",
        /// Guard an area.
        AreaGuard = 9 => "Area Guard",
        /// Return to base.
        Return = 10 => "Return",
        /// Stop.
        Stop = 11 => "Stop",
        /// Wait until enemies come near.
        Ambush = 12 => "Ambush",
        /// Seek and destroy.
        Hunt = 13 => "Hunt",
        /// Hunt after a delay.
        TimedHunt = 14 => "Timed Hunt",
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
    }
}

/// Number of waypoint slots: 0-25 are lettered, 26 is home, 27 is reinforcements.
pub const WAYPOINT_COUNT: u8 = 28;

/// A vehicle.
pub type Unit = sc::Unit<House, UnitType, Mission>;
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

/// Everything placed on a Tiberian Dawn map, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Objects {
    /// `[UNITS]`.
    pub units: Vec<Unit>,
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
    /// `[OVERLAY]`: cell to overlay.
    pub overlay: BTreeMap<u16, OverlayType>,
    /// `[Waypoints]`: waypoint to cell.
    pub waypoints: BTreeMap<u8, u16>,
    /// `[CellTriggers]`: cell to trigger name.
    pub cell_triggers: BTreeMap<u16, String>,
}

/// Sections this module reads and writes.
pub const SECTIONS: [&str; 9] = [
    "UNITS",
    "AIRCRAFT",
    "INFANTRY",
    "STRUCTURES",
    "TERRAIN",
    "SMUDGE",
    "OVERLAY",
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
            overlay: values(ini, "OVERLAY")
                .filter_map(|(k, v)| {
                    let kind = tokens(v).next()?.parse().ok()?;
                    Some((sc::atoi(k) as u16, kind))
                })
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
        let key = |i: usize| format!("{i:03}");
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
                .map(|(i, s)| (key(i), s.to_value(false)))
                .collect(),
        );
        put(
            "TERRAIN",
            self.terrain
                .iter()
                .map(|t| {
                    let trigger = t.trigger.as_deref().unwrap_or("None");
                    (t.cell.to_string(), format!("{},{trigger}", t.kind))
                })
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
        put(
            "OVERLAY",
            self.overlay
                .iter()
                .map(|(cell, o)| (cell.to_string(), o.to_string()))
                .collect(),
        );
        // Tiberian Dawn writes every waypoint slot, unset ones as -1.
        put(
            "Waypoints",
            (0..WAYPOINT_COUNT)
                .map(|i| {
                    let cell = self.waypoints.get(&i).map_or(-1, |&c| i32::from(c));
                    (i.to_string(), cell.to_string())
                })
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
000=GoodGuy,MCV,256,2405,0,Guard,None
001=BadGuy,LTNK,128,1000,96,Area Guard,dead
002=Nobody,LTNK,256,1,0,Guard,None

[INFANTRY]
000=BadGuy,E1,256,1500,4,Hunt,128,None

[STRUCTURES]
000=BadGuy,HAND,256,1800,0,None

[AIRCRAFT]
000=BadGuy,HELI,256,900,0,Guard

[TERRAIN]
1300=T01,None

[SMUDGE]
000=CR1,1400,0

[OVERLAY]
1450=TI1

[Waypoints]
0=2405
26=2405
27=-1

[CellTriggers]
1500=dead
";

    #[test]
    fn reads_and_writes() {
        let objects = Objects::read_ini(&Ini::parse(SAMPLE));
        assert_eq!(objects.units.len(), 2, "unknown house skipped");
        assert_eq!(objects.units[1].mission, Some(Mission::AreaGuard));
        assert_eq!(objects.units[1].trigger.as_deref(), Some("dead"));
        assert_eq!(objects.infantry[0].facing, 128);
        assert_eq!(objects.aircraft[0].kind, "HELI".parse().expect("HELI"));
        assert_eq!(
            objects.overlay.get(&1450),
            "TI1".parse::<OverlayType>().ok().as_ref()
        );
        assert_eq!(objects.waypoints.len(), 2);

        let mut ini = Ini::new();
        objects.write_ini(&mut ini);
        assert_eq!(
            ini.get("AIRCRAFT", "000"),
            Some("BadGuy,HELI,256,900,0,Guard")
        );
        assert_eq!(ini.get("Waypoints", "1"), Some("-1"));
        assert_eq!(Objects::read_ini(&ini), objects);
    }
}
