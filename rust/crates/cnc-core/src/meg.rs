//! Petroglyph MEG archives, the container the Remastered Collection ships its assets in.
//! Ported from the map editor's `Megafile.cs`.
//!
//! Layout of the version 2/3 format used by the Remastered Collection (little-endian):
//!
//! ```text
//! u32 magic (0xFFFFFFFF or 0x8FFFFFFF), u32 version, u32 header_size
//! u32 file_count, u32 string_count, u32 string_table_size
//! string_count x { u16 len, len bytes }            file names
//! file_count x {                                   20 bytes each, packed
//!     u16 flags, u32 crc, i32 index, u32 size, u32 offset, u16 name_index
//! }
//! ```

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

use crate::bytes::Reader;

const ENTRY_SIZE: usize = 20;

/// Errors from reading a MEG archive.
#[derive(Debug)]
pub enum MegError {
    /// An I/O error from the underlying reader.
    Io(io::Error),
    /// The header is not a supported MEG header.
    UnsupportedFormat,
    /// The header is truncated or inconsistent.
    Corrupt(&'static str),
}

impl fmt::Display for MegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "MEG I/O error: {e}"),
            Self::UnsupportedFormat => f.write_str("not a version 2/3 MEG archive"),
            Self::Corrupt(why) => write!(f, "corrupt MEG archive: {why}"),
        }
    }
}

impl std::error::Error for MegError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for MegError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// One file inside a MEG archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MegEntry {
    /// The stored path, e.g. `DATA\ART\TEXTURES\SRGB\...`.
    pub name: String,
    /// The stored CRC-32 of the name.
    pub crc: u32,
    /// Size in bytes.
    pub size: u32,
    /// Absolute offset in the archive.
    pub offset: u32,
}

/// A MEG archive over any seekable reader.
#[derive(Debug)]
pub struct MegArchive<R> {
    reader: R,
    entries: Vec<MegEntry>,
    by_name: HashMap<String, usize>,
}

impl<R: Read + Seek> MegArchive<R> {
    /// Opens and indexes an archive.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors or an unsupported or malformed header.
    pub fn open(mut reader: R) -> Result<Self, MegError> {
        reader.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 24];
        reader.read_exact(&mut header)?;
        let mut h = Reader::new(&header);
        let magic = h.u32().ok_or(MegError::UnsupportedFormat)?;
        // The editor reads both magics the same way: the Remastered archives that carry the
        // 0x8FFFFFFF marker are laid out identically and are not actually encrypted.
        if magic != 0xFFFF_FFFF && magic != 0x8FFF_FFFF {
            return Err(MegError::UnsupportedFormat);
        }
        let _version = h.u32();
        let _header_size = h.u32();
        let (Some(file_count), Some(string_count), Some(string_table_size)) =
            (h.u32(), h.u32(), h.u32())
        else {
            return Err(MegError::Corrupt("truncated header"));
        };

        let mut strings_raw = vec![0u8; string_table_size as usize];
        reader.read_exact(&mut strings_raw)?;
        let mut s = Reader::new(&strings_raw);
        let mut names = Vec::with_capacity(string_count as usize);
        for _ in 0..string_count {
            let len = s.u16().ok_or(MegError::Corrupt("truncated string table"))?;
            let raw = s
                .take(usize::from(len))
                .ok_or(MegError::Corrupt("truncated string table"))?;
            names.push(String::from_utf8_lossy(raw).into_owned());
        }

        let mut table = vec![0u8; file_count as usize * ENTRY_SIZE];
        reader.read_exact(&mut table)?;
        let mut t = Reader::new(&table);
        let mut entries = Vec::with_capacity(file_count as usize);
        let mut by_name = HashMap::with_capacity(file_count as usize);
        for _ in 0..file_count {
            let (Some(_flags), Some(crc), Some(_index), Some(size), Some(offset), Some(name)) =
                (t.u16(), t.u32(), t.i32(), t.u32(), t.u32(), t.u16())
            else {
                return Err(MegError::Corrupt("truncated file table"));
            };
            let name = names
                .get(usize::from(name))
                .ok_or(MegError::Corrupt("file name index out of range"))?
                .clone();
            by_name.insert(name.to_ascii_uppercase(), entries.len());
            entries.push(MegEntry {
                name,
                crc,
                size,
                offset,
            });
        }

        Ok(Self {
            reader,
            entries,
            by_name,
        })
    }

    /// All entries, in archive order.
    #[must_use]
    pub fn entries(&self) -> &[MegEntry] {
        &self.entries
    }

    /// Looks up a file by its stored path (case-insensitive).
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&MegEntry> {
        self.by_name
            .get(&name.to_ascii_uppercase())
            .map(|&i| &self.entries[i])
    }

    /// Reads a file by its stored path, returning `None` if it isn't present.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors.
    pub fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let Some((offset, size)) = self.find(name).map(|e| (e.offset, e.size)) else {
            return Ok(None);
        };
        self.reader.seek(SeekFrom::Start(u64::from(offset)))?;
        let mut data = vec![0; size as usize];
        self.reader.read_exact(&mut data)?;
        Ok(Some(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample() -> Vec<u8> {
        let names = ["DATA\\A.TXT", "DATA\\B.BIN"];
        let payloads: [&[u8]; 2] = [b"alpha", b"\x01\x02"];
        let mut strings = Vec::new();
        for n in names {
            strings.extend_from_slice(&(n.len() as u16).to_le_bytes());
            strings.extend_from_slice(n.as_bytes());
        }
        let header_len = 24 + strings.len() + ENTRY_SIZE * names.len();
        let mut out = Vec::new();
        for v in [
            0xFFFF_FFFFu32,
            0x3F7D_70A4,
            header_len as u32,
            2,
            2,
            strings.len() as u32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&strings);
        let mut offset = header_len as u32;
        for (i, p) in payloads.iter().enumerate() {
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(i as i32).to_le_bytes());
            out.extend_from_slice(&(p.len() as u32).to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&(i as u16).to_le_bytes());
            offset += p.len() as u32;
        }
        for p in payloads {
            out.extend_from_slice(p);
        }
        out
    }

    #[test]
    fn reads_entries() {
        let mut meg = MegArchive::open(Cursor::new(sample())).expect("valid");
        assert_eq!(meg.entries().len(), 2);
        assert_eq!(
            meg.read("data\\a.txt").expect("io").as_deref(),
            Some(b"alpha".as_slice())
        );
        assert_eq!(
            meg.read("DATA\\B.BIN").expect("io").as_deref(),
            Some(b"\x01\x02".as_slice())
        );
        assert!(meg.read("nope").expect("io").is_none());
    }

    #[test]
    fn rejects_other_formats() {
        let result = MegArchive::open(Cursor::new(vec![1; 32]));
        assert!(matches!(result, Err(MegError::UnsupportedFormat)));
    }
}
