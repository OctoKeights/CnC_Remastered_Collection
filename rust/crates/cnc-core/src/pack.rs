//! Packed binary blobs stored inside INI sections, such as Red Alert's `[MapPack]` and
//! `[OverlayPack]`.
//!
//! The data is cut into 8 KiB blocks, each LCW-compressed and prefixed with
//! `u16 compressed_len, u16 uncompressed_len` (the `LCWStraw` / `LCWPipe` block header).
//! The result is Base64-encoded and split into lines of 70 characters stored as entries
//! `1=`, `2=`, ... in file order.

use std::fmt;

use crate::ini::Section;
use crate::lcw::{self, LcwError};
use crate::{base64, bytes::Reader};

const BLOCK_SIZE: usize = 8192;
const LINE_LEN: usize = 70;

/// Errors from decoding a packed section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// The concatenated text isn't valid Base64.
    Base64(base64::InvalidBase64),
    /// A block header is cut short.
    TruncatedHeader,
    /// A block's compressed data is cut short.
    TruncatedBlock,
    /// A block failed to decompress.
    Lcw(LcwError),
    /// A block decompressed to a different size than its header claims.
    SizeMismatch {
        /// Size from the header.
        expected: usize,
        /// Size actually produced.
        actual: usize,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base64(e) => write!(f, "packed section: {e}"),
            Self::TruncatedHeader => f.write_str("packed section: truncated block header"),
            Self::TruncatedBlock => f.write_str("packed section: truncated block"),
            Self::Lcw(e) => write!(f, "packed section: {e}"),
            Self::SizeMismatch { expected, actual } => write!(
                f,
                "packed section: block decoded to {actual} bytes, header says {expected}"
            ),
        }
    }
}

impl std::error::Error for PackError {}

impl From<base64::InvalidBase64> for PackError {
    fn from(e: base64::InvalidBase64) -> Self {
        Self::Base64(e)
    }
}

impl From<LcwError> for PackError {
    fn from(e: LcwError) -> Self {
        Self::Lcw(e)
    }
}

/// Decodes the blocks in already Base64-decoded data.
///
/// # Errors
///
/// See [`PackError`].
pub fn decode_blocks(data: &[u8]) -> Result<Vec<u8>, PackError> {
    let mut reader = Reader::new(data);
    let mut out = Vec::new();
    while reader.position() < data.len() {
        let (Some(compressed), Some(uncompressed)) = (reader.u16(), reader.u16()) else {
            return Err(PackError::TruncatedHeader);
        };
        let block = reader
            .take(usize::from(compressed))
            .ok_or(PackError::TruncatedBlock)?;
        let decoded = lcw::decompress(block, usize::from(uncompressed))?;
        if decoded.data.len() != usize::from(uncompressed) {
            return Err(PackError::SizeMismatch {
                expected: usize::from(uncompressed),
                actual: decoded.data.len(),
            });
        }
        out.extend_from_slice(&decoded.data);
    }
    Ok(out)
}

/// Compresses data into blocks, without the Base64 layer.
#[must_use]
pub fn encode_blocks(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for block in data.chunks(BLOCK_SIZE) {
        let packed = lcw::compress(block);
        out.extend_from_slice(&(packed.len() as u16).to_le_bytes());
        out.extend_from_slice(&(block.len() as u16).to_le_bytes());
        out.extend_from_slice(&packed);
    }
    out
}

/// Decodes a packed section: its values are concatenated in file order.
///
/// # Errors
///
/// See [`PackError`].
pub fn decode_section(section: &Section) -> Result<Vec<u8>, PackError> {
    let text: String = section.iter().map(|(_, value)| value).collect();
    decode_blocks(&base64::decode(&text)?)
}

/// Encodes data as the entries of a packed section, replacing whatever it held.
pub fn encode_section(section: &mut Section, data: &[u8]) {
    let name = section.name().to_owned();
    *section = Section::new(name);
    let text = base64::encode(&encode_blocks(data));
    for (i, line) in text.as_bytes().chunks(LINE_LEN).enumerate() {
        // Base64 is ASCII, so any byte split is a valid string split.
        section.set((i + 1).to_string(), String::from_utf8_lossy(line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::Ini;

    #[test]
    fn roundtrips_through_ini_text() {
        // A 128x128 map pack: u16 template per cell, then a u8 icon per cell.
        let cells = 128 * 128;
        let mut data = Vec::with_capacity(cells * 3);
        for i in 0..cells {
            let template: u16 = if i % 97 == 0 { 3 } else { 0xFFFF };
            data.extend_from_slice(&template.to_le_bytes());
        }
        data.extend((0..cells).map(|i| (i % 7) as u8));

        let mut ini = Ini::new();
        encode_section(ini.section_or_insert("MapPack"), &data);
        let text = ini.to_string();
        assert!(text.lines().skip(1).all(|l| l.len() <= LINE_LEN + 6));

        let parsed = Ini::parse(&text);
        let section = parsed.section("MapPack").expect("section");
        assert_eq!(decode_section(section).expect("valid"), data);
    }

    #[test]
    fn detects_truncation() {
        let mut blocks = encode_blocks(&[1, 2, 3, 4, 5]);
        blocks.pop();
        assert_eq!(decode_blocks(&blocks), Err(PackError::TruncatedBlock));
        assert_eq!(decode_blocks(&[1, 0]), Err(PackError::TruncatedHeader));
    }
}
