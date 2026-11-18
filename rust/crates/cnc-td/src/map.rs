//! Tiberian Dawn scenario maps: the `[Map]` section of `SCGxxEA.INI` and the terrain
//! template layer in the matching `.BIN` file (`MapClass::Read_Binary` / `Write_Binary` in
//! `MAP.CPP`).
//!
//! The `.BIN` file holds two bytes per cell for all 64x64 cells, row by row: the template
//! id and the icon within it. A template byte of 255 means no template (clear ground); the
//! game writes those cells as `255, 0`, and so does [`Templates::to_bin`]. (The C# editor
//! wrote `255, 255`; both read back the same.)

use std::fmt;

use cnc_core::ini::Ini;
use cnc_core::map::{MapInfo, Tile, TileGrid};

use crate::{MAP_CELLS, MAP_HEIGHT, MAP_WIDTH, TemplateType, Theater};

/// Size of a `.BIN` file in bytes.
pub const BIN_SIZE: usize = MAP_CELLS * 2;

const NO_TEMPLATE: u8 = 0xFF;

/// The `.BIN` data is shorter than a full map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TruncatedBin {
    /// Bytes actually present.
    pub len: usize,
}

impl fmt::Display for TruncatedBin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "map .BIN is {} bytes, expected {BIN_SIZE}", self.len)
    }
}

impl std::error::Error for TruncatedBin {}

/// The template layer of a Tiberian Dawn map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Templates(TileGrid<u8>);

impl Default for Templates {
    fn default() -> Self {
        Self(TileGrid::new(MAP_WIDTH, MAP_HEIGHT))
    }
}

impl Templates {
    /// Parses a `.BIN` file.
    ///
    /// Unlike the original, which stopped at the end of a short file and left the rest of
    /// the map as it was, a short file is an error.
    ///
    /// # Errors
    ///
    /// Returns [`TruncatedBin`] if `data` is shorter than [`BIN_SIZE`].
    pub fn from_bin(data: &[u8]) -> Result<Self, TruncatedBin> {
        if data.len() < BIN_SIZE {
            return Err(TruncatedBin { len: data.len() });
        }
        let mut grid = TileGrid::new(MAP_WIDTH, MAP_HEIGHT);
        for (index, pair) in data[..BIN_SIZE].chunks_exact(2).enumerate() {
            if pair[0] != NO_TEMPLATE {
                grid.set(
                    index,
                    Some(Tile {
                        template: pair[0],
                        icon: pair[1],
                    }),
                );
            }
        }
        Ok(Self(grid))
    }

    /// Serialises to the `.BIN` format.
    #[must_use]
    pub fn to_bin(&self) -> Vec<u8> {
        self.0
            .tiles()
            .iter()
            .flat_map(|tile| match tile {
                Some(t) => [t.template, t.icon],
                None => [NO_TEMPLATE, 0],
            })
            .collect()
    }

    /// The underlying grid of raw tiles.
    #[must_use]
    pub fn grid(&self) -> &TileGrid<u8> {
        &self.0
    }

    /// The underlying grid, for editing.
    pub fn grid_mut(&mut self) -> &mut TileGrid<u8> {
        &mut self.0
    }

    /// The typed template and icon at a cell, if a known template is placed there.
    #[must_use]
    pub fn template_at(&self, cell: crate::Cell) -> Option<(TemplateType, u8)> {
        let tile = self.0.get(usize::from(cell.index()))?;
        Some((TemplateType::from_id(tile.template)?, tile.icon))
    }

    /// Places a template icon on a cell, or clears it with `None`.
    pub fn place(&mut self, cell: crate::Cell, template: Option<(TemplateType, u8)>) {
        self.0.set(
            usize::from(cell.index()),
            template.map(|(t, icon)| Tile {
                template: t.id(),
                icon,
            }),
        );
    }
}

/// Reads the `[Map]` section with Tiberian Dawn's defaults (desert, 62x62 at 1,1).
#[must_use]
pub fn read_map_info(ini: &Ini) -> MapInfo<Theater> {
    MapInfo::read_ini(ini, Theater::Desert, MAP_WIDTH, MAP_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cell;

    #[test]
    fn bin_roundtrip() {
        let mut templates = Templates::default();
        templates.place(Cell::new(3, 4), Some((TemplateType::Water, 0)));
        templates.place(Cell::new(63, 63), Some((TemplateType::Clear1, 5)));

        let bin = templates.to_bin();
        assert_eq!(bin.len(), BIN_SIZE);
        assert_eq!(&bin[..2], &[0xFF, 0x00]);

        let back = Templates::from_bin(&bin).expect("full file");
        assert_eq!(back, templates);
        assert_eq!(
            back.template_at(Cell::new(3, 4)),
            Some((TemplateType::Water, 0))
        );
        assert_eq!(back.template_at(Cell::new(0, 0)), None);
        assert_eq!(back.grid().placed().count(), 2);
    }

    #[test]
    fn short_bin_is_an_error() {
        assert_eq!(Templates::from_bin(&[0; 10]), Err(TruncatedBin { len: 10 }));
    }

    #[test]
    fn map_info_defaults() {
        let info = read_map_info(&Ini::parse("[MAP]\nTheater=TEMPERATE\nX=2\n"));
        assert_eq!(info.theater, Theater::Temperate);
        assert_eq!((info.bounds.x, info.bounds.width), (2, 62));
        assert_eq!(read_map_info(&Ini::new()).theater, Theater::Desert);
    }
}
