//! Tiberian Dawn, ported from `TIBERIANDAWN/` in the Remastered Collection source.
//!
//! So far this covers the game's identifiers and its scenario map format; the simulation
//! itself is still to be ported (see `rust/PORTING.md`).

pub mod map;
pub mod scenario;
pub mod scripting;
pub mod types;

pub use types::{
    AircraftType, BuildingType, InfantryType, OverlayType, SmudgeType, TemplateType, TerrainType,
    UnitType,
};

/// Map width in cells (`MAP_CELL_W`).
pub const MAP_WIDTH: u16 = 64;
/// Map height in cells (`MAP_CELL_H`).
pub const MAP_HEIGHT: u16 = 64;
/// Total cells on the map (`MAP_CELL_TOTAL`).
pub const MAP_CELLS: usize = MAP_WIDTH as usize * MAP_HEIGHT as usize;

/// A cell on Tiberian Dawn's 64x64 map.
pub type Cell = cnc_core::geometry::Cell<6>;

cnc_core::named_enum! {
    /// A tileset (`TheaterType`, names from `Theaters[]` in `CONST.CPP`).
    pub enum Theater: u8 {
        /// Desert.
        Desert = 0 => "DESERT",
        /// Jungle (unused by the shipped campaigns).
        Jungle = 1 => "JUNGLE",
        /// Temperate.
        Temperate = 2 => "TEMPERATE",
        /// Winter.
        Winter = 3 => "WINTER",
    }
}

impl Theater {
    /// The 8.3 file name stem of the theater's data (`TEMPERAT`, ...).
    #[must_use]
    pub const fn root(self) -> &'static str {
        match self {
            Self::Temperate => "TEMPERAT",
            other => other.name(),
        }
    }

    /// The three-letter file extension of theater-specific art (`DES`, `TEM`, ...).
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Desert => "DES",
            Self::Jungle => "JUN",
            Self::Temperate => "TEM",
            Self::Winter => "WIN",
        }
    }
}

cnc_core::named_enum! {
    /// A side (`HousesType`, names from `HDATA.CPP`).
    pub enum House: u8 {
        /// Global Defense Initiative.
        Good = 0 => "GoodGuy",
        /// Brotherhood of Nod.
        Bad = 1 => "BadGuy",
        /// Civilians.
        Neutral = 2 => "Neutral",
        /// Disaster Containment Team, the "special" house.
        Special = 3 => "Special",
        /// Multiplayer house 1.
        Multi1 = 4 => "Multi1",
        /// Multiplayer house 2.
        Multi2 = 5 => "Multi2",
        /// Multiplayer house 3.
        Multi3 = 6 => "Multi3",
        /// Multiplayer house 4.
        Multi4 = 7 => "Multi4",
        /// Multiplayer house 5.
        Multi5 = 8 => "Multi5",
        /// Multiplayer house 6.
        Multi6 = 9 => "Multi6",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip() {
        for &t in Theater::ALL {
            assert_eq!(t.name().parse::<Theater>(), Ok(t));
        }
        assert_eq!("goodguy".parse::<House>(), Ok(House::Good));
        assert_eq!("MTNK".parse::<UnitType>(), Ok(UnitType::Mtank));
        assert_eq!(BuildingType::from_id(0), Some(BuildingType::Weap));
        assert_eq!(TemplateType::Clear1.id(), 0);
        assert!("NOPE".parse::<InfantryType>().is_err());
    }
}
