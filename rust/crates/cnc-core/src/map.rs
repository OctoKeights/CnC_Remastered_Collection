//! The `[Map]` section shared by both games' scenario files (`Read_INI` / `Write_INI` in
//! `DISPLAY.CPP`).

use std::fmt::Display;
use std::str::FromStr;

use crate::ini::Ini;

/// Name of the section, as the games write it.
pub const SECTION: &str = "Map";

/// The playable rectangle within the full map, in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapBounds {
    /// Left edge.
    pub x: u16,
    /// Top edge.
    pub y: u16,
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
}

impl MapBounds {
    /// The default: everything but a one-cell border, as the games assume when the keys are
    /// missing.
    #[must_use]
    pub const fn default_for(map_width: u16, map_height: u16) -> Self {
        Self {
            x: 1,
            y: 1,
            width: map_width - 2,
            height: map_height - 2,
        }
    }

    /// Whether a cell position lies inside the playable area.
    #[must_use]
    pub const fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// A scenario's `[Map]` section: theater and playable bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapInfo<T> {
    /// The theater (tileset).
    pub theater: T,
    /// The playable area.
    pub bounds: MapBounds,
}

impl<T: FromStr + Display> MapInfo<T> {
    /// Reads `[Map]`, falling back to the given defaults for missing or invalid keys, as
    /// the games do.
    #[must_use]
    pub fn read_ini(ini: &Ini, default_theater: T, map_width: u16, map_height: u16) -> Self {
        let defaults = MapBounds::default_for(map_width, map_height);
        let section = ini.section(SECTION);
        let int = |key: &str, default: u16| {
            section
                .and_then(|s| s.get_int(key))
                .and_then(|v| u16::try_from(v).ok())
                .unwrap_or(default)
        };
        Self {
            theater: section
                .and_then(|s| s.get("Theater"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(default_theater),
            bounds: MapBounds {
                x: int("X", defaults.x),
                y: int("Y", defaults.y),
                width: int("Width", defaults.width),
                height: int("Height", defaults.height),
            },
        }
    }

    /// Writes `[Map]`.
    pub fn write_ini(&self, ini: &mut Ini) {
        let section = ini.section_or_insert(SECTION);
        section.set("Theater", &self.theater);
        section.set("X", self.bounds.x);
        section.set("Y", self.bounds.y);
        section.set("Width", self.bounds.width);
        section.set("Height", self.bounds.height);
    }
}

/// The terrain template on one cell: which template, and which icon (sub-tile) of it.
///
/// The template is kept as its raw id so that maps referring to templates this build
/// doesn't know about survive a load/save round trip; the game crates offer typed access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile<Id> {
    /// Raw template id (`TemplateType`).
    pub template: Id,
    /// Icon number within the template.
    pub icon: u8,
}

/// A full map's worth of template tiles, row by row. `None` is the default clear ground
/// (`TEMPLATE_NONE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileGrid<Id> {
    width: u16,
    tiles: Vec<Option<Tile<Id>>>,
}

impl<Id: Copy> TileGrid<Id> {
    /// An all-clear grid.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            tiles: vec![None; usize::from(width) * usize::from(height)],
        }
    }

    /// Map width in cells.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Map height in cells.
    #[must_use]
    pub fn height(&self) -> u16 {
        (self.tiles.len() / usize::from(self.width.max(1))) as u16
    }

    /// All tiles, row by row.
    #[must_use]
    pub fn tiles(&self) -> &[Option<Tile<Id>>] {
        &self.tiles
    }

    /// The tile at a cell index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Tile<Id>> {
        self.tiles.get(index).copied().flatten()
    }

    /// The tile at a position.
    #[must_use]
    pub fn at(&self, x: u16, y: u16) -> Option<Tile<Id>> {
        if x >= self.width {
            return None;
        }
        self.get(usize::from(y) * usize::from(self.width) + usize::from(x))
    }

    /// Sets the tile at a cell index. Out-of-range indices are ignored.
    pub fn set(&mut self, index: usize, tile: Option<Tile<Id>>) {
        if let Some(slot) = self.tiles.get_mut(index) {
            *slot = tile;
        }
    }

    /// Iterates over `(cell index, tile)` for every non-clear cell.
    pub fn placed(&self) -> impl Iterator<Item = (usize, Tile<Id>)> + '_ {
        self.tiles
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.map(|t| (i, t)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_with_defaults() {
        let ini = Ini::parse("[MAP]\nTheater=winter\nX=10\nWidth=bogus\n");
        let info: MapInfo<String> = MapInfo::read_ini(&ini, "DESERT".into(), 64, 64);
        assert_eq!(info.theater, "winter");
        assert_eq!(
            info.bounds,
            MapBounds {
                x: 10,
                y: 1,
                width: 0,
                height: 62
            }
        );
    }
}
