//! Keyframe shapes (`.SHP`): sprites for units, buildings, animations and the interface
//! (`Build_Frame` in `KEYFRAME.CPP`).
//!
//! ```text
//! u16 frames, u16 x, u16 y, u16 width, u16 height, u16 largest_frame_size, i16 flags
//! (frames + 2) x { u32 offset | format << 24, u32 reference | ref_format << 24 }
//! [768-byte palette if flags & 1]
//! frame data
//! ```
//!
//! Every frame is `width * height` palette indices. A frame is stored as one of:
//!
//! * `0x80` keyframe: LCW-compressed pixels;
//! * `0x40` key delta: an XOR delta ([`apply_xor_delta`]) against the keyframe whose data
//!   starts at `reference`;
//! * `0x20` delta: an XOR delta against the previous frame. Its `reference` names the key
//!   delta frame that starts the chain.

use std::fmt;

use cnc_core::bytes::Reader;
use cnc_core::lcw::{self, LcwError};

const KF_KEYFRAME: u8 = 0x80;
const KF_KEYDELTA: u8 = 0x40;
const KF_DELTA: u8 = 0x20;
const HEADER_SIZE: usize = 14;
const PALETTE_SIZE: usize = 768;

/// Errors from decoding a shape file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShpError {
    /// The header or offset table is cut short.
    Truncated,
    /// A frame's data offset lies outside the file.
    BadOffset {
        /// The frame.
        frame: usize,
    },
    /// A frame has a format byte this decoder doesn't know.
    UnknownFormat {
        /// The frame.
        frame: usize,
        /// The format byte.
        format: u8,
    },
    /// A delta frame refers to a frame that can't be its base.
    BadReference {
        /// The frame.
        frame: usize,
    },
    /// A keyframe failed to decompress.
    Lcw {
        /// The frame.
        frame: usize,
        /// The LCW error.
        error: LcwError,
    },
    /// An XOR delta ran past the end of the frame or its data.
    BadDelta {
        /// The frame.
        frame: usize,
    },
}

impl fmt::Display for ShpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("shape header is truncated"),
            Self::BadOffset { frame } => write!(f, "shape frame {frame} points outside the file"),
            Self::UnknownFormat { frame, format } => {
                write!(f, "shape frame {frame} has unknown format 0x{format:02X}")
            }
            Self::BadReference { frame } => write!(f, "shape frame {frame} has a bad reference"),
            Self::Lcw { frame, error } => write!(f, "shape frame {frame}: {error}"),
            Self::BadDelta { frame } => write!(f, "shape frame {frame} has a malformed delta"),
        }
    }
}

impl std::error::Error for ShpError {}

/// A decoded shape: every frame as `width * height` palette indices, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    /// Frame width in pixels.
    pub width: u16,
    /// Frame height in pixels.
    pub height: u16,
    /// The embedded palette, if the file carries one (rare; used by some cutscene art).
    pub palette: Option<Vec<u8>>,
    /// The frames.
    pub frames: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
struct FrameEntry {
    offset: usize,
    format: u8,
    reference: u32,
}

impl Shape {
    /// Decodes every frame of a shape file.
    ///
    /// # Errors
    ///
    /// Returns a [`ShpError`] describing the first problem found.
    pub fn decode(data: &[u8]) -> Result<Self, ShpError> {
        let mut r = Reader::new(data);
        let (
            Some(count),
            Some(_x),
            Some(_y),
            Some(width),
            Some(height),
            Some(_largest),
            Some(flags),
        ) = (
            r.u16(),
            r.u16(),
            r.u16(),
            r.u16(),
            r.u16(),
            r.u16(),
            r.i16(),
        )
        else {
            return Err(ShpError::Truncated);
        };
        let count = usize::from(count);

        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let (Some(offset), Some(reference)) = (r.u32(), r.u32()) else {
                return Err(ShpError::Truncated);
            };
            entries.push(FrameEntry {
                offset: (offset & 0x00FF_FFFF) as usize,
                format: (offset >> 24) as u8,
                reference,
            });
        }

        let has_palette = flags & 1 != 0;
        let palette = if has_palette {
            let start = HEADER_SIZE + (count + 2) * 8;
            Some(
                data.get(start..start + PALETTE_SIZE)
                    .ok_or(ShpError::Truncated)?
                    .to_vec(),
            )
        } else {
            None
        };
        // The original adds the palette size to every data offset.
        let skip = if has_palette { PALETTE_SIZE } else { 0 };
        let frame_data = |frame: usize, offset: usize| {
            data.get(offset + skip..)
                .filter(|d| !d.is_empty())
                .ok_or(ShpError::BadOffset { frame })
        };

        let size = usize::from(width) * usize::from(height);
        let keyframe = |frame: usize, offset: usize| -> Result<Vec<u8>, ShpError> {
            // Some Remastered files are 0x0 stubs; their frame data is meaningless.
            if size == 0 {
                return Ok(Vec::new());
            }
            let decoded = lcw::decompress(frame_data(frame, offset)?, size)
                .map_err(|error| ShpError::Lcw { frame, error })?;
            let mut pixels = decoded.data;
            pixels.resize(size, 0);
            Ok(pixels)
        };

        let mut frames: Vec<Vec<u8>> = Vec::with_capacity(count);
        for (frame, entry) in entries.iter().enumerate() {
            let pixels = match entry.format {
                f if f & KF_KEYFRAME != 0 => keyframe(frame, entry.offset)?,
                f if f & KF_KEYDELTA != 0 => {
                    let mut pixels = keyframe(frame, (entry.reference & 0x00FF_FFFF) as usize)?;
                    if size == 0 {
                        frames.push(pixels);
                        continue;
                    }
                    apply_xor_delta(&mut pixels, frame_data(frame, entry.offset)?)
                        .ok_or(ShpError::BadDelta { frame })?;
                    pixels
                }
                f if f & KF_DELTA != 0 => {
                    let start = (entry.reference & 0xFFFF) as usize;
                    let previous = frames
                        .last()
                        .filter(|_| start < frame)
                        .ok_or(ShpError::BadReference { frame })?;
                    let mut pixels = previous.clone();
                    if size == 0 {
                        frames.push(pixels);
                        continue;
                    }
                    apply_xor_delta(&mut pixels, frame_data(frame, entry.offset)?)
                        .ok_or(ShpError::BadDelta { frame })?;
                    pixels
                }
                format => return Err(ShpError::UnknownFormat { frame, format }),
            };
            frames.push(pixels);
        }

        Ok(Self {
            width,
            height,
            palette,
            frames,
        })
    }
}

/// Applies a Westwood XOR delta ("Format40", `Apply_XOR_Delta` in `XORDELTA.ASM`) to
/// `target`. Returns `None` if the delta runs past the end of either buffer or has no end
/// marker.
///
/// | Command | Meaning |
/// |---|---|
/// | `0x00 n v` | XOR the next `n` bytes with `v` |
/// | `0x01..=0x7F` (`n`) | XOR the next `n` bytes with the following `n` bytes |
/// | `0x81..=0xFF` | skip `cmd & 0x7F` bytes |
/// | `0x80 w` (`w` a `u16`) | `w = 0`: end. `w < 0x8000`: skip `w`. Else with `w & 0x3FFF` as `n`: `0x4000` set, XOR `n` bytes with the next byte; clear, XOR `n` bytes with the following `n` |
pub fn apply_xor_delta(target: &mut [u8], delta: &[u8]) -> Option<()> {
    let mut src = Reader::new(delta);
    let mut pos = 0usize;

    let xor_run = |target: &mut [u8], pos: &mut usize, count: usize, value: u8| {
        let span = target.get_mut(*pos..*pos + count)?;
        for b in span.iter_mut() {
            *b ^= value;
        }
        *pos += count;
        Some(())
    };
    let xor_copy = |target: &mut [u8], pos: &mut usize, bytes: &[u8]| {
        let span = target.get_mut(*pos..*pos + bytes.len())?;
        for (b, s) in span.iter_mut().zip(bytes) {
            *b ^= s;
        }
        *pos += bytes.len();
        Some(())
    };

    loop {
        let cmd = src.u8()?;
        match cmd {
            0x00 => {
                let count = usize::from(src.u8()?);
                let value = src.u8()?;
                xor_run(target, &mut pos, count, value)?;
            }
            0x01..=0x7F => xor_copy(target, &mut pos, src.take(usize::from(cmd))?)?,
            0x80 => {
                let word = src.u16()?;
                if word == 0 {
                    return Some(());
                }
                let count = usize::from(word & 0x3FFF);
                if word & 0x8000 == 0 {
                    pos += usize::from(word);
                } else if word & 0x4000 != 0 {
                    let value = src.u8()?;
                    xor_run(target, &mut pos, count, value)?;
                } else {
                    xor_copy(target, &mut pos, src.take(count)?)?;
                }
            }
            0x81..=0xFF => pos += usize::from(cmd & 0x7F),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_delta_commands() {
        let mut buf = [0u8; 12];
        let delta = [
            0x02, 1, 2, // xor two bytes
            0x00, 3, 0xFF, // run of three
            0x81, // skip one
            0x80, 0x02, 0xC0, 0x07, // long run of two with 7
            0x80, 0x01, 0x80, 9, // long copy of one
            0x80, 0x00, 0x00, // end
        ];
        apply_xor_delta(&mut buf, &delta).expect("valid");
        assert_eq!(buf, [1, 2, 0xFF, 0xFF, 0xFF, 0, 7, 7, 9, 0, 0, 0]);
        assert_eq!(
            apply_xor_delta(&mut [0u8; 1], &[0x02, 1, 2, 0x80, 0, 0]),
            None
        );
        assert_eq!(apply_xor_delta(&mut [0u8; 4], &[0x81]), None);
    }

    fn build(width: u16, height: u16, frames: &[(u8, u32, Vec<u8>)]) -> Vec<u8> {
        let table = HEADER_SIZE + (frames.len() + 2) * 8;
        let mut out = Vec::new();
        for v in [frames.len() as u16, 0, 0, width, height, 0, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut offset = table;
        let mut offsets = Vec::new();
        for (format, reference, data) in frames {
            offsets.push(offset);
            let resolved = if *format == KF_KEYDELTA {
                offsets[*reference as usize] as u32
            } else {
                *reference
            };
            out.extend_from_slice(&((offset as u32) | u32::from(*format) << 24).to_le_bytes());
            out.extend_from_slice(&resolved.to_le_bytes());
            offset += data.len();
        }
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&[0; 12]);
        for (_, _, data) in frames {
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn decodes_all_frame_kinds() {
        let key = lcw::compress(&[1, 2, 3, 4]);
        let file = build(
            2,
            2,
            &[
                (KF_KEYFRAME, 0, key),
                (KF_KEYDELTA, 0, vec![0x81, 0x01, 0x07, 0x80, 0, 0]),
                (KF_DELTA, 1, vec![0x00, 4, 0x10, 0x80, 0, 0]),
            ],
        );
        let shape = Shape::decode(&file).expect("valid");
        assert_eq!((shape.width, shape.height), (2, 2));
        assert_eq!(shape.frames[0], [1, 2, 3, 4]);
        assert_eq!(shape.frames[1], [1, 2 ^ 7, 3, 4]);
        assert_eq!(shape.frames[2], [0x11, (2 ^ 7) ^ 0x10, 0x13, 0x14]);
    }

    #[test]
    fn zero_sized_stubs_give_empty_frames() {
        let file = build(0, 0, &[(KF_KEYFRAME, 0, vec![0x81, 1, 0x80])]);
        let shape = Shape::decode(&file).expect("valid");
        assert_eq!(shape.frames, vec![Vec::<u8>::new()]);
    }

    #[test]
    fn rejects_bad_files() {
        assert_eq!(Shape::decode(&[0; 4]), Err(ShpError::Truncated));
        let file = build(2, 2, &[(KF_DELTA, 0, vec![0x80, 0, 0])]);
        assert_eq!(
            Shape::decode(&file),
            Err(ShpError::BadReference { frame: 0 })
        );
    }
}
