//! Loading and saving scenarios for either game (the `Load` / `Save` halves of the
//! editor's `TiberianDawn/GamePlugin.cs` and `RedAlert/GamePlugin.cs`).
//!
//! Only the map layers are modelled so far; every other section is kept verbatim in the
//! INI so that a load/save round trip doesn't lose anything.

use std::fmt;

use cnc_core::ini::Ini;
use cnc_core::map::MapInfo;

use crate::grid::CellMetrics;

/// Which game a scenario belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Game {
    /// Tiberian Dawn: an `.INI` plus a `.BIN` template file.
    TiberianDawn,
    /// Red Alert: a single `.INI`/`.MPR` with packed layers.
    RedAlert,
}

impl Game {
    /// Guesses the game from an INI's contents: Red Alert maps carry `[MapPack]`.
    #[must_use]
    pub fn detect(ini: &Ini) -> Self {
        if ini.section(cnc_ra::map::MAP_PACK).is_some() {
            Self::RedAlert
        } else {
            Self::TiberianDawn
        }
    }

    /// The map dimensions for this game.
    #[must_use]
    pub const fn metrics(self) -> CellMetrics {
        match self {
            Self::TiberianDawn => CellMetrics::new(cnc_td::MAP_WIDTH, cnc_td::MAP_HEIGHT),
            Self::RedAlert => CellMetrics::new(cnc_ra::MAP_WIDTH, cnc_ra::MAP_HEIGHT),
        }
    }
}

/// Errors from loading a scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioError {
    /// The Tiberian Dawn `.BIN` file was unusable.
    TdBin(cnc_td::map::TruncatedBin),
    /// A Red Alert packed layer was unusable.
    RaMap(cnc_ra::map::MapError),
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TdBin(e) => e.fmt(f),
            Self::RaMap(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ScenarioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TdBin(e) => Some(e),
            Self::RaMap(e) => Some(e),
        }
    }
}

impl From<cnc_td::map::TruncatedBin> for ScenarioError {
    fn from(e: cnc_td::map::TruncatedBin) -> Self {
        Self::TdBin(e)
    }
}

impl From<cnc_ra::map::MapError> for ScenarioError {
    fn from(e: cnc_ra::map::MapError) -> Self {
        Self::RaMap(e)
    }
}

/// A loaded scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scenario {
    /// A Tiberian Dawn scenario.
    TiberianDawn {
        /// Every section of the scenario INI, including ones not modelled yet.
        ini: Ini,
        /// `[Map]`.
        map: MapInfo<cnc_td::Theater>,
        /// The `.BIN` template layer.
        templates: cnc_td::map::Templates,
    },
    /// A Red Alert scenario.
    RedAlert {
        /// Every section of the scenario INI, including ones not modelled yet.
        ini: Ini,
        /// `[Map]`.
        map: MapInfo<cnc_ra::Theater>,
        /// `[MapPack]`.
        templates: cnc_ra::map::Templates,
        /// `[OverlayPack]`.
        overlays: cnc_ra::map::Overlays,
    },
}

/// A scenario serialised back to its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedScenario {
    /// The scenario INI text.
    pub ini: String,
    /// The `.BIN` file, for Tiberian Dawn.
    pub bin: Option<Vec<u8>>,
}

impl Scenario {
    /// Loads a Tiberian Dawn scenario from its INI text and `.BIN` data.
    ///
    /// # Errors
    ///
    /// Fails if the `.BIN` data is too short.
    pub fn load_td(ini_text: &str, bin: &[u8]) -> Result<Self, ScenarioError> {
        let mut ini = Ini::parse(ini_text);
        let map = cnc_td::map::read_map_info(&ini);
        // Store the effective values (defaults included) so the INI matches the model.
        map.write_ini(&mut ini);
        Ok(Self::TiberianDawn {
            map,
            templates: cnc_td::map::Templates::from_bin(bin)?,
            ini,
        })
    }

    /// Loads a Red Alert scenario from its INI text.
    ///
    /// # Errors
    ///
    /// Fails if `[MapPack]` or `[OverlayPack]` can't be unpacked.
    pub fn load_ra(ini_text: &str) -> Result<Self, ScenarioError> {
        let mut ini = Ini::parse(ini_text);
        let map = cnc_ra::map::read_map_info(&ini);
        map.write_ini(&mut ini);
        let templates = cnc_ra::map::Templates::read_ini(&ini)?;
        let overlays = cnc_ra::map::Overlays::read_ini(&ini)?;
        // The packed layers are regenerated on save.
        ini.remove_section(cnc_ra::map::MAP_PACK);
        ini.remove_section(cnc_ra::map::OVERLAY_PACK);
        Ok(Self::RedAlert {
            ini,
            map,
            templates,
            overlays,
        })
    }

    /// Which game this scenario is for.
    #[must_use]
    pub fn game(&self) -> Game {
        match self {
            Self::TiberianDawn { .. } => Game::TiberianDawn,
            Self::RedAlert { .. } => Game::RedAlert,
        }
    }

    /// The INI with every section not modelled separately.
    #[must_use]
    pub fn ini(&self) -> &Ini {
        match self {
            Self::TiberianDawn { ini, .. } | Self::RedAlert { ini, .. } => ini,
        }
    }

    /// Serialises the scenario back to its files.
    #[must_use]
    pub fn save(&self) -> SavedScenario {
        match self {
            Self::TiberianDawn {
                ini,
                map,
                templates,
            } => {
                let mut ini = ini.clone();
                map.write_ini(&mut ini);
                SavedScenario {
                    ini: ini.to_string(),
                    bin: Some(templates.to_bin()),
                }
            }
            Self::RedAlert {
                ini,
                map,
                templates,
                overlays,
            } => {
                let mut ini = ini.clone();
                map.write_ini(&mut ini);
                templates.write_ini(&mut ini);
                overlays.write_ini(&mut ini);
                SavedScenario {
                    ini: ini.to_string(),
                    bin: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn td_roundtrip() {
        let mut bin: Vec<u8> = [0xFF, 0].repeat(cnc_td::map::BIN_SIZE / 2);
        bin[0] = cnc_td::TemplateType::Water.id();
        let text = "[Basic]\nName=Test\n\n[MAP]\nTheater=WINTER\n";
        let scenario = Scenario::load_td(text, &bin).expect("valid");
        assert_eq!(scenario.game(), Game::TiberianDawn);
        let saved = scenario.save();
        assert_eq!(saved.bin.as_deref(), Some(bin.as_slice()));
        assert!(saved.ini.contains("Theater=WINTER"));
        assert!(saved.ini.contains("Name=Test"));
        let again = Scenario::load_td(&saved.ini, &bin).expect("valid");
        assert_eq!(again, scenario);
    }

    #[test]
    fn ra_roundtrip() {
        let mut first =
            Scenario::load_ra("[Basic]\nName=RA\n\n[Map]\nTheater=SNOW\n").expect("valid");
        if let Scenario::RedAlert { templates, .. } = &mut first {
            templates.place(
                cnc_ra::Cell::new(2, 3),
                Some((cnc_ra::TemplateType::Water, 0)),
            );
        }
        let saved = first.save();
        assert!(saved.bin.is_none());
        let parsed = Ini::parse(&saved.ini);
        assert_eq!(Game::detect(&parsed), Game::RedAlert);
        let second = Scenario::load_ra(&saved.ini).expect("valid");
        assert_eq!(second, first);
    }
}
