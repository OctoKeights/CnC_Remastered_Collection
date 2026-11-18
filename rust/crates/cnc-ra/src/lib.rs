//! Red Alert, ported from `REDALERT/` in the Remastered Collection source.
//!
//! So far this covers the game's identifiers and its scenario map format; the simulation
//! itself is still to be ported (see `rust/PORTING.md`).

pub mod map;
pub mod rules;
pub mod scenario;
pub mod scripting;
pub mod types;

pub use types::{
    AircraftType, BuildingType, InfantryType, OverlayType, SmudgeType, TemplateType, TerrainType,
    UnitType, VesselType,
};

/// Map width in cells (`MAP_CELL_W`).
pub const MAP_WIDTH: u16 = 128;
/// Map height in cells (`MAP_CELL_H`).
pub const MAP_HEIGHT: u16 = 128;
/// Total cells on the map (`MAP_CELL_TOTAL`).
pub const MAP_CELLS: usize = MAP_WIDTH as usize * MAP_HEIGHT as usize;

/// A cell on Red Alert's 128x128 map.
pub type Cell = cnc_core::geometry::Cell<7>;

cnc_core::named_enum! {
    /// A tileset (`TheaterType`, names from `Theaters[]` in `CONST.CPP`).
    pub enum Theater: u8 {
        /// Temperate.
        Temperate = 0 => "TEMPERATE",
        /// Snow.
        Snow = 1 => "SNOW",
        /// Interior (indoor missions).
        Interior = 2 => "INTERIOR",
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

    /// The three-letter file extension of theater-specific art (`TEM`, `SNO`, `INT`).
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Temperate => "TEM",
            Self::Snow => "SNO",
            Self::Interior => "INT",
        }
    }
}

cnc_core::named_enum! {
    /// A side (`HousesType`, names from `HDATA.CPP`).
    pub enum House: u8 {
        /// Spain (gold, unremapped).
        Spain = 0 => "Spain",
        /// Greece (light blue).
        Greece = 1 => "Greece",
        /// Soviet Union (red).
        Ussr = 2 => "USSR",
        /// England (green).
        England = 3 => "England",
        /// Ukraine (orange).
        Ukraine = 4 => "Ukraine",
        /// Germany (grey).
        Germany = 5 => "Germany",
        /// France (blue).
        France = 6 => "France",
        /// Turkey (brown).
        Turkey = 7 => "Turkey",
        /// "GoodGuy", the generic Allied house.
        Good = 8 => "GoodGuy",
        /// "BadGuy", the generic Soviet house.
        Bad = 9 => "BadGuy",
        /// Civilians.
        Neutral = 10 => "Neutral",
        /// The "special" house.
        Special = 11 => "Special",
        /// Multiplayer house 1.
        Multi1 = 12 => "Multi1",
        /// Multiplayer house 2.
        Multi2 = 13 => "Multi2",
        /// Multiplayer house 3.
        Multi3 = 14 => "Multi3",
        /// Multiplayer house 4.
        Multi4 = 15 => "Multi4",
        /// Multiplayer house 5.
        Multi5 = 16 => "Multi5",
        /// Multiplayer house 6.
        Multi6 = 17 => "Multi6",
        /// Multiplayer house 7.
        Multi7 = 18 => "Multi7",
        /// Multiplayer house 8.
        Multi8 = 19 => "Multi8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip() {
        for &h in House::ALL {
            assert_eq!(h.name().parse::<House>(), Ok(h));
        }
        assert_eq!("snow".parse::<Theater>(), Ok(Theater::Snow));
        assert_eq!("DD".parse::<VesselType>(), Ok(VesselType::Dd));
        assert_eq!("WEAF".parse::<BuildingType>().map(BuildingType::id), Ok(32));
    }
}
