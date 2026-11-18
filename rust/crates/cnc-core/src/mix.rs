//! MIX archives (`MIXFILE.CPP`), the classic Westwood container format.
//!
//! Layout (all little-endian):
//!
//! ```text
//! [u16 0, u16 flags]          Red Alert only; flags: 1 = SHA-1 digest, 2 = encrypted header
//! i16 count, i32 data_size    file header
//! count x { u32 id, u32 offset, u32 size }
//! data_size bytes of data     offsets are relative to the start of this block
//! [20-byte SHA-1 digest]      if flagged
//! ```
//!
//! Files are identified only by [`mix_id`] of their upper-cased name, so names can't be
//! listed from the archive itself.
//!
//! Red Alert headers flagged as encrypted are decrypted with the game's built-in key; see
//! [`crate::crypto`].

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

use crate::bytes::Reader;
use crate::crc::mix_id;
use crate::crypto::{Blowfish, PublicKey, WESTWOOD_KEY};

const FLAG_DIGEST: u16 = 0x0001;
const FLAG_ENCRYPTED: u16 = 0x0002;
const ENTRY_SIZE: u64 = 12;
const HEADER_SIZE: usize = 6;

/// Errors from reading a MIX archive.
#[derive(Debug)]
pub enum MixError {
    /// An I/O error from the underlying reader.
    Io(io::Error),
    /// The header is truncated or inconsistent.
    Corrupt(&'static str),
}

impl fmt::Display for MixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "MIX I/O error: {e}"),
            Self::Corrupt(why) => write!(f, "corrupt MIX archive: {why}"),
        }
    }
}

impl std::error::Error for MixError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Corrupt(_) => None,
        }
    }
}

impl From<io::Error> for MixError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// One file inside a MIX archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixEntry {
    /// [`mix_id`] of the file name.
    pub id: u32,
    /// Offset from the start of the data block.
    pub offset: u32,
    /// Size in bytes.
    pub size: u32,
}

/// The parsed header of a MIX archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixIndex {
    entries: Vec<MixEntry>,
    by_id: HashMap<u32, usize>,
    data_start: u64,
    data_size: u32,
    has_digest: bool,
}

impl MixIndex {
    /// Reads the header from the start of `reader`, leaving it positioned after the index.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors, a malformed header, or an encrypted header.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, MixError> {
        let mut first = [0u8; 4];
        reader.read_exact(&mut first)?;
        let lead = u16::from_le_bytes([first[0], first[1]]);
        let flags = u16::from_le_bytes([first[2], first[3]]);

        let (header, data_start, has_digest) = if lead == 0 {
            let has_digest = flags & FLAG_DIGEST != 0;
            if flags & FLAG_ENCRYPTED != 0 {
                let (header, consumed) = read_encrypted_header(reader, &WESTWOOD_KEY)?;
                (header, 4 + consumed, has_digest)
            } else {
                let header = read_plain_header(reader, Vec::new())?;
                let len = header.len() as u64;
                (header, 4 + len, has_digest)
            }
        } else {
            // Classic header: the four bytes already read are the count and part of the size.
            let header = read_plain_header(reader, first.to_vec())?;
            let len = header.len() as u64;
            (header, len, false)
        };

        let mut h = Reader::new(&header);
        let count = h
            .i16()
            .and_then(|c| usize::try_from(c).ok())
            .ok_or(MixError::Corrupt("negative file count"))?;
        let data_size = h
            .i32()
            .and_then(|s| u32::try_from(s).ok())
            .ok_or(MixError::Corrupt("negative data size"))?;
        let table = h
            .take(count * ENTRY_SIZE as usize)
            .ok_or(MixError::Corrupt("truncated file table"))?;

        let mut t = Reader::new(table);
        let mut entries = Vec::with_capacity(count);
        let mut by_id = HashMap::with_capacity(count);
        for index in 0..count {
            let (Some(id), Some(offset), Some(size)) = (t.u32(), t.u32(), t.u32()) else {
                return Err(MixError::Corrupt("truncated file table"));
            };
            if u64::from(offset) + u64::from(size) > u64::from(data_size) {
                return Err(MixError::Corrupt("file extends past the data block"));
            }
            entries.push(MixEntry { id, offset, size });
            by_id.entry(id).or_insert(index);
        }

        Ok(Self {
            entries,
            by_id,
            data_start,
            data_size,
            has_digest,
        })
    }

    /// All entries, in archive order.
    #[must_use]
    pub fn entries(&self) -> &[MixEntry] {
        &self.entries
    }

    /// Looks up a file by name (case-insensitive).
    #[must_use]
    pub fn find(&self, name: &str) -> Option<MixEntry> {
        self.find_id(mix_id(name))
    }

    /// Looks up a file by its [`mix_id`].
    #[must_use]
    pub fn find_id(&self, id: u32) -> Option<MixEntry> {
        self.by_id.get(&id).map(|&i| self.entries[i])
    }

    /// Absolute offset of the data block within the archive.
    #[must_use]
    pub fn data_start(&self) -> u64 {
        self.data_start
    }

    /// Total size of the data block.
    #[must_use]
    pub fn data_size(&self) -> u32 {
        self.data_size
    }

    /// Whether a SHA-1 digest follows the data block.
    #[must_use]
    pub fn has_digest(&self) -> bool {
        self.has_digest
    }
}

/// Reads a plaintext file header and table, given any bytes of it already read.
fn read_plain_header<R: Read>(reader: &mut R, mut header: Vec<u8>) -> Result<Vec<u8>, MixError> {
    let have = header.len();
    header.resize(HEADER_SIZE, 0);
    reader.read_exact(&mut header[have..])?;
    let count = i16::from_le_bytes([header[0], header[1]]);
    let count = usize::try_from(count).map_err(|_| MixError::Corrupt("negative file count"))?;
    header.resize(HEADER_SIZE + count * ENTRY_SIZE as usize, 0);
    reader.read_exact(&mut header[HEADER_SIZE..])?;
    Ok(header)
}

/// Reads an encrypted header (`PKStraw` + `BlowStraw`): a public-key wrapped Blowfish key,
/// then the header and table Blowfish-encrypted and padded to whole blocks. Returns the
/// plaintext and the number of bytes consumed.
fn read_encrypted_header<R: Read>(
    reader: &mut R,
    key: &PublicKey,
) -> Result<(Vec<u8>, u64), MixError> {
    let mut wrapped = vec![0u8; key.encrypted_len(Blowfish::MAX_KEY_LENGTH)];
    reader.read_exact(&mut wrapped)?;
    let bf_key = key.decrypt(&wrapped);
    let bf = Blowfish::new(bf_key.get(..Blowfish::MAX_KEY_LENGTH).unwrap_or(&bf_key));

    let mut header = vec![0u8; Blowfish::BLOCK_SIZE];
    reader.read_exact(&mut header)?;
    bf.decrypt(&mut header);
    let count = i16::from_le_bytes([header[0], header[1]]);
    let count = usize::try_from(count).map_err(|_| MixError::Corrupt("negative file count"))?;

    let total = (HEADER_SIZE + count * ENTRY_SIZE as usize).next_multiple_of(Blowfish::BLOCK_SIZE);
    let mut rest = vec![0u8; total - Blowfish::BLOCK_SIZE];
    reader.read_exact(&mut rest)?;
    bf.decrypt(&mut rest);
    header.extend_from_slice(&rest);
    Ok((header, (wrapped.len() + total) as u64))
}

/// A MIX archive over any seekable reader.
#[derive(Debug)]
pub struct MixArchive<R> {
    reader: R,
    index: MixIndex,
}

impl<R: Read + Seek> MixArchive<R> {
    /// Opens an archive whose header starts at the reader's current position 0.
    ///
    /// # Errors
    ///
    /// See [`MixIndex::read_from`].
    pub fn open(mut reader: R) -> Result<Self, MixError> {
        reader.seek(SeekFrom::Start(0))?;
        let index = MixIndex::read_from(&mut reader)?;
        Ok(Self { reader, index })
    }

    /// The archive's index.
    #[must_use]
    pub fn index(&self) -> &MixIndex {
        &self.index
    }

    /// Reads an entry's bytes.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors.
    pub fn read_entry(&mut self, entry: MixEntry) -> io::Result<Vec<u8>> {
        self.reader.seek(SeekFrom::Start(
            self.index.data_start + u64::from(entry.offset),
        ))?;
        let mut data = vec![0; entry.size as usize];
        self.reader.read_exact(&mut data)?;
        Ok(data)
    }

    /// Reads a file by name, returning `None` if it isn't in this archive.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors.
    pub fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        self.index
            .find(name)
            .map(|entry| self.read_entry(entry))
            .transpose()
    }

    /// Returns the underlying reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

/// Builds a classic (Tiberian Dawn style) MIX archive from named files.
///
/// Entries are sorted by signed id, which is what the original binary search expects.
#[must_use]
pub fn build<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Vec<u8> {
    let mut files: Vec<(u32, &[u8])> = files
        .into_iter()
        .map(|(name, data)| (mix_id(name), data))
        .collect();
    files.sort_by_key(|&(id, _)| id as i32);

    let mut table = Vec::with_capacity(files.len() * ENTRY_SIZE as usize);
    let mut body = Vec::new();
    for (id, data) in &files {
        table.extend_from_slice(&id.to_le_bytes());
        table.extend_from_slice(&(body.len() as u32).to_le_bytes());
        table.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
    }

    let mut out = Vec::with_capacity(6 + table.len() + body.len());
    out.extend_from_slice(&(files.len() as i16).to_le_bytes());
    out.extend_from_slice(&(body.len() as i32).to_le_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn classic_roundtrip() {
        let bytes = build([("RULES.INI", b"[General]".as_slice()), ("A.SHP", b"xyz")]);
        let mut mix = MixArchive::open(Cursor::new(bytes)).expect("valid");
        assert_eq!(mix.index().entries().len(), 2);
        assert!(!mix.index().has_digest());
        assert_eq!(
            mix.read("rules.ini").expect("io").as_deref(),
            Some(b"[General]".as_slice())
        );
        assert_eq!(
            mix.read("a.shp").expect("io").as_deref(),
            Some(b"xyz".as_slice())
        );
        assert_eq!(mix.read("missing").expect("io"), None);
    }

    #[test]
    fn extended_header() {
        let classic = build([("X", b"hello".as_slice())]);
        let mut extended = vec![0, 0, 1, 0];
        extended.extend_from_slice(&classic);
        extended.extend_from_slice(&[0; 20]);
        let mut mix = MixArchive::open(Cursor::new(extended)).expect("valid");
        assert!(mix.index().has_digest());
        assert_eq!(
            mix.read("x").expect("io").as_deref(),
            Some(b"hello".as_slice())
        );
    }

    /// Builds an encrypted archive the way Westwood's tools did, using the private key that
    /// `CONST.CPP` carries under `CHEAT_KEYS`.
    #[test]
    fn encrypted_header() {
        const PRIVATE: &str = "AigKVje8mROcR8QixnxUEF5b29Curkq01DNDWCdOG99XBqH79OaCiTCB";
        let private_der = crate::base64::decode(PRIVATE).expect("base64");
        let public_der =
            crate::base64::decode("AihRvNoIbTn85FZRYNZRcT+i6KpU+maCsEqr3Q5q+LDB5tH7Tz2qQ38V")
                .expect("base64");
        let private =
            PublicKey::from_der_modulus_and_exponent(&public_der, &private_der).expect("der");

        let classic = build([("RULES.INI", b"[General]".as_slice()), ("B", b"bee")]);
        let table_len = 6 + 2 * ENTRY_SIZE as usize;
        let (header, body) = classic.split_at(table_len);

        let bf_key: Vec<u8> = (0u8..56).map(|i| i.wrapping_mul(37) ^ 0x5A).collect();
        let mut plain_key = bf_key.clone();
        plain_key.resize(2 * WESTWOOD_KEY.plain_block_size(), 0);
        let wrapped = private.encrypt(&plain_key);
        assert_eq!(wrapped.len(), 80);

        let mut padded = header.to_vec();
        padded.resize(header.len().next_multiple_of(8), 0);
        Blowfish::new(&bf_key).encrypt(&mut padded);

        let mut file = vec![0, 0, 2, 0];
        file.extend_from_slice(&wrapped);
        file.extend_from_slice(&padded);
        file.extend_from_slice(body);

        let mut mix = MixArchive::open(Cursor::new(file)).expect("decrypts");
        assert_eq!(mix.index().data_start(), 4 + 80 + padded.len() as u64);
        assert_eq!(
            mix.read("b").expect("io").as_deref(),
            Some(b"bee".as_slice())
        );
        assert_eq!(
            mix.read("rules.ini").expect("io").as_deref(),
            Some(b"[General]".as_slice())
        );
    }

    #[test]
    fn rejects_out_of_range_entries() {
        let mut bytes = build([("X", b"hello".as_slice())]);
        bytes[2] = 1; // shrink data_size to 1
        assert!(matches!(
            MixIndex::read_from(&mut Cursor::new(bytes)),
            Err(MixError::Corrupt(_))
        ));
    }
}
