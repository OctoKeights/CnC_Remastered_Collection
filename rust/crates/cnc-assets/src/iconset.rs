//! Icon sets: the terrain templates in each theater's MIX file (`CLEAR1.TEM`, `W1.SNO`, ...),
//! loaded with `Load_Icon_Set` (`ICONSET.CPP`) and laid out as `IControl_Type` (`TILE.H`).
//!
//! A template is a block of cells. Each *logical* icon (one per cell of the template) maps to
//! a *physical* icon (the pixels) or to nothing, which leaves a hole in templates that aren't
//! rectangular. All offsets are from the start of the file.
//!
//! Tiberian Dawn header (32 bytes):
//!
//! ```text
//! i16 width, i16 height, i16 count, i16 allocated, i32 size,
//! i32 icons, i32 palettes, i32 remaps, i32 trans_flag, i32 map
//! ```
//!
//! Red Alert header (40 bytes) adds `i16 map_width, i16 map_height` after `allocated`, and
//! `i32 color_map` before `map`.

use std::fmt;

use cnc_core::bytes::Reader;

/// Errors from parsing an icon set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSetError {
    /// The header is cut short or matches neither game's layout.
    BadHeader,
    /// A table lies outside the file.
    Truncated,
}

impl fmt::Display for IconSetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadHeader => f.write_str("icon set header is not recognised"),
            Self::Truncated => f.write_str("icon set tables run past the end of the file"),
        }
    }
}

impl std::error::Error for IconSetError {}

/// A decoded icon set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconSet {
    /// Icon width in pixels (24 in both games).
    pub width: u16,
    /// Icon height in pixels.
    pub height: u16,
    /// Template size in cells, for Red Alert files (Tiberian Dawn keeps it in `CDATA.CPP`).
    pub map_size: Option<(u16, u16)>,
    /// Pixels of each physical icon, `width * height` palette indices each.
    pub icons: Vec<Vec<u8>>,
    /// For each logical icon (cell of the template), the physical icon drawn there.
    pub map: Vec<Option<u8>>,
    /// Per physical icon: whether it uses colour 0 as transparency.
    pub transparent: Vec<bool>,
    /// Red Alert only: the colour control values (`ColorMap`), one per cell of the
    /// `map_size` block.
    pub color_map: Option<Vec<u8>>,
}

struct Layout {
    count: usize,
    map_size: Option<(u16, u16)>,
    icons: usize,
    trans: usize,
    color_map: Option<usize>,
    map: usize,
}

fn offset(value: Option<i32>) -> Option<usize> {
    value.and_then(|v| usize::try_from(v).ok())
}

fn layout(data: &[u8]) -> Option<Layout> {
    let mut r = Reader::new(data);
    let (width, height, count, _allocated) = (r.i16()?, r.i16()?, r.i16()?, r.i16()?);
    if width <= 0 || height <= 0 || count < 0 {
        return None;
    }
    let count = count as usize;

    // Tiberian Dawn: size, icons, palettes, remaps, trans, map. Icons follow the header.
    let mut td = Reader::new(&data[8..]);
    let (_size, icons, _pal, _remap, trans, map) = (
        td.i32()?,
        offset(td.i32()),
        td.i32()?,
        td.i32()?,
        offset(td.i32()),
        offset(td.i32()),
    );
    if icons == Some(32) {
        return Some(Layout {
            count,
            map_size: None,
            icons: 32,
            trans: trans?,
            color_map: None,
            map: map?,
        });
    }

    // Red Alert: map_width, map_height, size, icons, palettes, remaps, trans, color_map, map.
    let (map_w, map_h) = (r.u16()?, r.u16()?);
    let (_size, icons) = (r.i32()?, offset(r.i32()));
    let (_pal, _remap) = (r.i32()?, r.i32()?);
    let (trans, color_map, map) = (offset(r.i32()), offset(r.i32()), offset(r.i32()));
    (icons == Some(40)).then_some(Layout {
        count,
        map_size: Some((map_w, map_h)),
        icons: 40,
        trans: trans?,
        color_map: Some(color_map?),
        map: map?,
    })
}

impl IconSet {
    /// Parses an icon set file of either game.
    ///
    /// # Errors
    ///
    /// Returns an [`IconSetError`] if the header isn't recognised or a table runs past the
    /// end of the file.
    pub fn decode(data: &[u8]) -> Result<Self, IconSetError> {
        let layout = layout(data).ok_or(IconSetError::BadHeader)?;
        let width = u16::from_le_bytes([data[0], data[1]]);
        let height = u16::from_le_bytes([data[2], data[3]]);
        let icon_size = usize::from(width) * usize::from(height);

        // The icon pixels run up to the next table in the file.
        let icons_end = [Some(layout.trans), layout.color_map, Some(layout.map)]
            .into_iter()
            .flatten()
            .filter(|&o| o > layout.icons)
            .min()
            .unwrap_or(data.len());
        let physical = icons_end.saturating_sub(layout.icons) / icon_size;
        let icons = data
            .get(layout.icons..layout.icons + physical * icon_size)
            .ok_or(IconSetError::Truncated)?
            .chunks_exact(icon_size)
            .map(<[u8]>::to_vec)
            .collect();

        let map = data
            .get(layout.map..layout.map + layout.count)
            .ok_or(IconSetError::Truncated)?
            .iter()
            .map(|&i| (i != 0xFF).then_some(i))
            .collect();
        let transparent = data
            .get(layout.trans..layout.trans + physical)
            .ok_or(IconSetError::Truncated)?
            .iter()
            .map(|&t| t != 0)
            .collect();
        let color_len = layout
            .map_size
            .map_or(0, |(w, h)| usize::from(w) * usize::from(h));
        let color_map = layout
            .color_map
            .map(|at| {
                data.get(at..at + color_len)
                    .map(<[u8]>::to_vec)
                    .ok_or(IconSetError::Truncated)
            })
            .transpose()?;

        Ok(Self {
            width,
            height,
            map_size: layout.map_size,
            icons,
            map,
            transparent,
            color_map,
        })
    }

    /// The pixels drawn in a template cell, if any.
    #[must_use]
    pub fn cell(&self, logical: usize) -> Option<&[u8]> {
        let physical = (*self.map.get(logical)?)?;
        self.icons.get(usize::from(physical)).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn td_file(physical: usize, map: &[u8]) -> Vec<u8> {
        let icons = 32;
        let map_at = icons + physical * 4;
        let trans_at = map_at + map.len();
        let size = trans_at + physical;
        let mut out = Vec::new();
        for v in [2i16, 2, map.len() as i16, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [size, icons, 0, 0xDEAD, trans_at, map_at] {
            out.extend_from_slice(&(v as i32).to_le_bytes());
        }
        for i in 0..physical {
            out.extend_from_slice(&[i as u8; 4]);
        }
        out.extend_from_slice(map);
        out.extend((0..physical).map(|i| u8::from(i == 1)));
        out
    }

    #[test]
    fn decodes_td_layout() {
        let set = IconSet::decode(&td_file(2, &[1, 0xFF, 0])).expect("valid");
        assert_eq!((set.width, set.height, set.map_size), (2, 2, None));
        assert_eq!(set.icons.len(), 2);
        assert_eq!(set.map, [Some(1), None, Some(0)]);
        assert_eq!(set.transparent, [false, true]);
        assert_eq!(set.cell(0), Some([1u8; 4].as_slice()));
        assert_eq!(set.cell(1), None);
    }

    /// Mirrors the layout of Red Alert's `S01.TEM`: icons, map, transparency, colour map.
    #[test]
    fn decodes_ra_layout() {
        let (icons, physical, map) = (40usize, 2usize, [0u8, 1, 0xFF, 1]);
        let map_at = icons + physical * 4;
        let trans_at = map_at + map.len();
        let color_at = trans_at + physical;
        let size = color_at + 4;
        let mut file = Vec::new();
        for v in [2i16, 2, 4, 0, 2, 2] {
            file.extend_from_slice(&v.to_le_bytes());
        }
        for v in [size, icons, 0, 0xBEEF, trans_at, color_at, map_at] {
            file.extend_from_slice(&(v as i32).to_le_bytes());
        }
        file.extend_from_slice(&[7; 4]);
        file.extend_from_slice(&[9; 4]);
        file.extend_from_slice(&map);
        file.extend_from_slice(&[0, 1]);
        file.extend_from_slice(&[1, 2, 3, 4]);

        let set = IconSet::decode(&file).expect("valid");
        assert_eq!(set.map_size, Some((2, 2)));
        assert_eq!(set.icons, [vec![7; 4], vec![9; 4]]);
        assert_eq!(set.map, [Some(0), Some(1), None, Some(1)]);
        assert_eq!(set.color_map.as_deref(), Some([1u8, 2, 3, 4].as_slice()));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(IconSet::decode(&[0; 8]), Err(IconSetError::BadHeader));
        let mut file = td_file(2, &[0, 1]);
        file.truncate(file.len() - 2);
        assert_eq!(IconSet::decode(&file), Err(IconSetError::Truncated));
    }
}
