//! The classic art and sound formats of Tiberian Dawn and Red Alert.
//!
//! | Module | Format | Ported from |
//! |---|---|---|
//! | [`palette`] | `.PAL` 6-bit VGA palettes | `PALETTEC.CPP` |
//! | [`shp`] | `.SHP` keyframe shapes (units, buildings, animations, UI) | `KEYFRAME.CPP`, `XORDELTA.ASM` |
//! | [`iconset`] | Terrain templates (`.TEM`, `.SNO`, `.INT`, ...) | `ICONSET.CPP`, `TILE.H` |
//! | [`aud`] | `.AUD` / `.V00` / `.JUV` sound | `ADPCM.CPP`, `ITABLE.CPP`, `DTABLE.CPP` |
//!
//! Decoders take whole files as byte slices (as read from a MIX archive) and never panic on
//! malformed input; they return an error instead.

pub mod aud;
pub mod iconset;
pub mod palette;
pub mod shp;

pub use palette::Palette;
