//! A small bounds-checked little-endian reader used by the binary format parsers.

/// A cursor over a byte slice that never panics: every read returns `None` when the
/// data runs out.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Starts reading at the beginning of `data`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Bytes consumed so far.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The next `len` bytes.
    pub fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    /// The next `N` bytes as an array.
    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    /// A single byte.
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    /// A little-endian `u16`.
    pub fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_le_bytes)
    }

    /// A little-endian `i16`.
    pub fn i16(&mut self) -> Option<i16> {
        self.array().map(i16::from_le_bytes)
    }

    /// A little-endian `u32`.
    pub fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    /// A little-endian `i32`.
    pub fn i32(&mut self) -> Option<i32> {
        self.array().map(i32::from_le_bytes)
    }
}

impl Reader<'_> {
    /// Bytes not yet consumed.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
}
