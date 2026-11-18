//! Red Alert scenario maps. Unlike Tiberian Dawn, everything lives in the scenario INI:
//! `[Map]` gives theater and bounds, `[MapPack]` holds the template layer and
//! `[OverlayPack]` the overlay layer, both as [packed sections](cnc_core::pack).
//!
//! * `[MapPack]` decodes to a `u16` template id per cell (0xFFFF = none), followed by a
//!   `u8` icon per cell, each covering all 128x128 cells row by row.
//! * `[OverlayPack]` decodes to an `i8` overlay id per cell (-1 = none).

use std::fmt;

use cnc_core::ini::Ini;
use cnc_core::map::{MapInfo, Tile, TileGrid};
use cnc_core::pack::{self, PackError};

use crate::{Cell, MAP_CELLS, MAP_HEIGHT, MAP_WIDTH, OverlayType, TemplateType, Theater};

/// Section holding the template layer.
pub const MAP_PACK: &str = "MapPack";
/// Section holding the overlay layer.
pub const OVERLAY_PACK: &str = "OverlayPack";

const NO_TEMPLATE: u16 = 0xFFFF;
const NO_OVERLAY: u8 = 0xFF;

/// Errors from reading a packed map layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    /// The section's data couldn't be unpacked.
    Pack(PackError),
    /// The section unpacked to too little data for a full map.
    TooShort {
        /// Section name.
        section: &'static str,
        /// Bytes unpacked.
        len: usize,
        /// Bytes required.
        expected: usize,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(e) => e.fmt(f),
            Self::TooShort {
                section,
                len,
                expected,
            } => write!(f, "[{section}] holds {len} bytes, expected {expected}"),
        }
    }
}

impl std::error::Error for MapError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pack(e) => Some(e),
            Self::TooShort { .. } => None,
        }
    }
}

impl From<PackError> for MapError {
    fn from(e: PackError) -> Self {
        Self::Pack(e)
    }
}

fn unpack(ini: &Ini, section: &'static str, expected: usize) -> Result<Option<Vec<u8>>, MapError> {
    let Some(s) = ini.section(section) else {
        return Ok(None);
    };
    let data = pack::decode_section(s)?;
    if data.len() < expected {
        return Err(MapError::TooShort {
            section,
            len: data.len(),
            expected,
        });
    }
    Ok(Some(data))
}

/// The template layer of a Red Alert map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Templates(TileGrid<u16>);

impl Default for Templates {
    fn default() -> Self {
        Self(TileGrid::new(MAP_WIDTH, MAP_HEIGHT))
    }
}

impl Templates {
    /// Reads `[MapPack]`. A missing section gives an all-clear map.
    ///
    /// # Errors
    ///
    /// Fails if the section can't be unpacked or is too short.
    pub fn read_ini(ini: &Ini) -> Result<Self, MapError> {
        let mut grid = TileGrid::new(MAP_WIDTH, MAP_HEIGHT);
        if let Some(data) = unpack(ini, MAP_PACK, MAP_CELLS * 3)? {
            let (ids, icons) = data.split_at(MAP_CELLS * 2);
            for (index, (id, &icon)) in ids.chunks_exact(2).zip(icons).enumerate() {
                let template = u16::from_le_bytes([id[0], id[1]]);
                if template != NO_TEMPLATE {
                    grid.set(index, Some(Tile { template, icon }));
                }
            }
        }
        Ok(Self(grid))
    }

    /// Writes `[MapPack]`, replacing any existing contents.
    pub fn write_ini(&self, ini: &mut Ini) {
        let tiles = self.0.tiles();
        let mut data = Vec::with_capacity(MAP_CELLS * 3);
        for tile in tiles {
            let id = tile.map_or(NO_TEMPLATE, |t| t.template);
            data.extend_from_slice(&id.to_le_bytes());
        }
        data.extend(tiles.iter().map(|t| t.map_or(u8::MAX, |t| t.icon)));
        pack::encode_section(ini.section_or_insert(MAP_PACK), &data);
    }

    /// The underlying grid of raw tiles.
    #[must_use]
    pub fn grid(&self) -> &TileGrid<u16> {
        &self.0
    }

    /// The underlying grid, for editing.
    pub fn grid_mut(&mut self) -> &mut TileGrid<u16> {
        &mut self.0
    }

    /// The typed template and icon at a cell, if a known template is placed there.
    #[must_use]
    pub fn template_at(&self, cell: Cell) -> Option<(TemplateType, u8)> {
        let tile = self.0.get(usize::from(cell.index()))?;
        Some((TemplateType::from_id(tile.template)?, tile.icon))
    }

    /// Places a template icon on a cell, or clears it with `None`.
    pub fn place(&mut self, cell: Cell, template: Option<(TemplateType, u8)>) {
        self.0.set(
            usize::from(cell.index()),
            template.map(|(t, icon)| Tile {
                template: t.id(),
                icon,
            }),
        );
    }
}

/// The overlay layer of a Red Alert map: one raw overlay id per cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlays(Vec<Option<u8>>);

impl Default for Overlays {
    fn default() -> Self {
        Self(vec![None; MAP_CELLS])
    }
}

impl Overlays {
    /// Reads `[OverlayPack]`. A missing section gives an empty layer.
    ///
    /// # Errors
    ///
    /// Fails if the section can't be unpacked or is too short.
    pub fn read_ini(ini: &Ini) -> Result<Self, MapError> {
        let mut layer = Self::default();
        if let Some(data) = unpack(ini, OVERLAY_PACK, MAP_CELLS)? {
            for (slot, &id) in layer.0.iter_mut().zip(&data) {
                // Ids are signed bytes in the original; any negative value means none.
                *slot = (id < 0x80).then_some(id);
            }
        }
        Ok(layer)
    }

    /// Writes `[OverlayPack]`, replacing any existing contents.
    pub fn write_ini(&self, ini: &mut Ini) {
        let data: Vec<u8> = self.0.iter().map(|o| o.unwrap_or(NO_OVERLAY)).collect();
        pack::encode_section(ini.section_or_insert(OVERLAY_PACK), &data);
    }

    /// The typed overlay at a cell.
    #[must_use]
    pub fn get(&self, cell: Cell) -> Option<OverlayType> {
        self.0
            .get(usize::from(cell.index()))
            .copied()
            .flatten()
            .and_then(OverlayType::from_id)
    }

    /// Sets or clears the overlay on a cell.
    pub fn set(&mut self, cell: Cell, overlay: Option<OverlayType>) {
        if let Some(slot) = self.0.get_mut(usize::from(cell.index())) {
            *slot = overlay.map(OverlayType::id);
        }
    }

    /// Raw ids, row by row.
    #[must_use]
    pub fn raw(&self) -> &[Option<u8>] {
        &self.0
    }
}

/// Reads the `[Map]` section with Red Alert's defaults (temperate, 126x126 at 1,1).
#[must_use]
pub fn read_map_info(ini: &Ini) -> MapInfo<Theater> {
    MapInfo::read_ini(ini, Theater::Temperate, MAP_WIDTH, MAP_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_roundtrip_through_ini_text() {
        let mut templates = Templates::default();
        templates.place(Cell::new(5, 6), Some((TemplateType::Water, 1)));
        templates.place(Cell::new(127, 127), Some((TemplateType::Clear1, 0)));
        let mut overlays = Overlays::default();
        let wall = OverlayType::ALL[0];
        overlays.set(Cell::new(9, 9), Some(wall));

        let mut ini = Ini::new();
        read_map_info(&ini).write_ini(&mut ini);
        templates.write_ini(&mut ini);
        overlays.write_ini(&mut ini);

        let parsed = Ini::parse(&ini.to_string());
        assert_eq!(read_map_info(&parsed).theater, Theater::Temperate);
        let t = Templates::read_ini(&parsed).expect("valid");
        assert_eq!(t, templates);
        assert_eq!(
            t.template_at(Cell::new(5, 6)),
            Some((TemplateType::Water, 1))
        );
        let o = Overlays::read_ini(&parsed).expect("valid");
        assert_eq!(o.get(Cell::new(9, 9)), Some(wall));
        assert_eq!(o.get(Cell::new(0, 0)), None);
    }

    #[test]
    fn missing_sections_are_empty() {
        let ini = Ini::new();
        assert_eq!(Templates::read_ini(&ini), Ok(Templates::default()));
        assert_eq!(Overlays::read_ini(&ini), Ok(Overlays::default()));
    }

    #[test]
    fn short_pack_is_an_error() {
        let mut ini = Ini::new();
        pack::encode_section(ini.section_or_insert(MAP_PACK), &[0; 100]);
        assert!(matches!(
            Templates::read_ini(&ini),
            Err(MapError::TooShort { len: 100, .. })
        ));
    }
}
