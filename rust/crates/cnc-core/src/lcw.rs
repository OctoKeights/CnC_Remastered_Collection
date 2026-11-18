//! LCW compression ("Format80"), Westwood's LZ77-style codec.
//!
//! The decoder is a bounds-checked port of `LCW_Uncompress` (`LCWUNCMP.CPP`). Unlike the
//! original it never reads or writes outside its buffers: malformed input is reported as an
//! [`LcwError`]. The encoder is a port of the map editor's `WWCompression.LcwCompress`.
//!
//! Command bytes:
//!
//! | Bits | Meaning |
//! |---|---|
//! | `0cccpppp pppppppp` | copy `c + 3` bytes from `p` bytes back in the output |
//! | `10cccccc` | copy `c` literal bytes from the input (`0x80` ends the stream) |
//! | `11cccccc pppp` | copy `c + 3` bytes from absolute output position `p` |
//! | `11111110 cccc v` | write byte `v`, `c` times |
//! | `11111111 cccc pppp` | copy `c` bytes from absolute output position `p` |
//!
//! A stream that starts with `0x00` is in *relative* mode: the 3- and 5-byte commands then
//! give a distance back from the current position instead of an absolute position. Classic
//! game data never uses it, but the editor emits it for buffers over 64 KiB.

use std::fmt;

/// Errors produced while decoding an LCW stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcwError {
    /// The input ended before the `0x80` end marker.
    Truncated,
    /// A copy command referenced data outside the output written so far.
    BadReference {
        /// Offset in the input of the offending command.
        at: usize,
    },
    /// Decoding would produce more than the permitted number of bytes.
    OutputTooLarge {
        /// The limit that was exceeded.
        limit: usize,
    },
}

impl fmt::Display for LcwError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("LCW stream ended without an end marker"),
            Self::BadReference { at } => {
                write!(
                    f,
                    "LCW copy command at input offset {at} references missing data"
                )
            }
            Self::OutputTooLarge { limit } => {
                write!(f, "LCW stream decodes to more than {limit} bytes")
            }
        }
    }
}

impl std::error::Error for LcwError {}

/// The result of a successful decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The decompressed bytes.
    pub data: Vec<u8>,
    /// How many input bytes were consumed, including the end marker.
    pub consumed: usize,
}

struct Input<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> Result<u8, LcwError> {
        let byte = *self.data.get(self.pos).ok_or(LcwError::Truncated)?;
        self.pos += 1;
        Ok(byte)
    }

    fn word(&mut self) -> Result<usize, LcwError> {
        let lo = self.byte()?;
        let hi = self.byte()?;
        Ok(usize::from(u16::from_le_bytes([lo, hi])))
    }

    fn slice(&mut self, len: usize) -> Result<&[u8], LcwError> {
        let end = self.pos + len;
        let slice = self.data.get(self.pos..end).ok_or(LcwError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }
}

struct Output {
    data: Vec<u8>,
    limit: usize,
}

impl Output {
    fn reserve(&self, count: usize) -> Result<(), LcwError> {
        if self.data.len() + count > self.limit {
            Err(LcwError::OutputTooLarge { limit: self.limit })
        } else {
            Ok(())
        }
    }

    /// Copies `count` bytes starting at output position `from`. The source may overlap the
    /// bytes being written, which is how LCW expresses runs, so this copies byte by byte.
    fn copy_from(&mut self, from: usize, count: usize, at: usize) -> Result<(), LcwError> {
        if from >= self.data.len() && count > 0 {
            return Err(LcwError::BadReference { at });
        }
        self.reserve(count)?;
        for i in 0..count {
            let byte = self.data[from + i];
            self.data.push(byte);
        }
        Ok(())
    }
}

/// Decodes an LCW stream, refusing to produce more than `limit` bytes.
///
/// # Errors
///
/// Returns an [`LcwError`] if the stream is truncated, contains an invalid back-reference
/// or would exceed `limit`.
pub fn decompress(input: &[u8], limit: usize) -> Result<Decoded, LcwError> {
    let mut src = Input {
        data: input,
        pos: 0,
    };
    let mut out = Output {
        data: Vec::new(),
        limit,
    };

    let relative = input.first() == Some(&0);
    if relative {
        src.pos = 1;
    }

    loop {
        let at = src.pos;
        let op = src.byte()?;

        if op & 0x80 == 0 {
            // Short relative copy from the output.
            let count = usize::from(op >> 4) + 3;
            let distance = (usize::from(op & 0x0F) << 8) | usize::from(src.byte()?);
            let from = out
                .data
                .len()
                .checked_sub(distance)
                .ok_or(LcwError::BadReference { at })?;
            if distance == 0 {
                return Err(LcwError::BadReference { at });
            }
            out.copy_from(from, count, at)?;
        } else if op & 0x40 == 0 {
            if op == 0x80 {
                return Ok(Decoded {
                    data: out.data,
                    consumed: src.pos,
                });
            }
            // Literal run from the input.
            let count = usize::from(op & 0x3F);
            out.reserve(count)?;
            out.data.extend_from_slice(src.slice(count)?);
        } else if op == 0xFE {
            // Run of a single byte.
            let count = src.word()?;
            let value = src.byte()?;
            out.reserve(count)?;
            out.data.resize(out.data.len() + count, value);
        } else {
            let count = if op == 0xFF {
                src.word()?
            } else {
                usize::from(op & 0x3F) + 3
            };
            let position = src.word()?;
            let from = if relative {
                out.data
                    .len()
                    .checked_sub(position)
                    .ok_or(LcwError::BadReference { at })?
            } else {
                position
            };
            out.copy_from(from, count, at)?;
        }
    }
}

/// Encodes data as an LCW stream that [`decompress`] (and the original game) can read.
///
/// Buffers larger than 64 KiB are written in relative mode, as the editor does.
#[must_use]
pub fn compress(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return vec![0x80];
    }

    let relative = input.len() > usize::from(u16::MAX);
    let mut out = Vec::with_capacity(input.len() + input.len() / 63 + 8);
    if relative {
        out.push(0);
    }

    // Start with a literal command; `literal_at` tracks the open literal command, if any,
    // so following unmatched bytes can be appended to it.
    let mut literal_at = Some(out.len());
    out.push(0x81);
    out.push(input[0]);
    let mut pos = 1;
    let mut matcher = Matcher::new(input.len());

    while pos < input.len() {
        let remaining = input.len() - pos;

        // Long runs of one byte are cheapest as a fill command.
        if remaining > 64 && input[pos] == input[pos + 64] {
            let max = pos + remaining.min(usize::from(u16::MAX));
            let run_end = input[pos..max]
                .iter()
                .position(|&b| b != input[pos])
                .map_or(max, |n| pos + n);
            let run = run_end - pos;
            if run >= 0x41 {
                literal_at = None;
                out.push(0xFE);
                out.extend_from_slice(&(run as u16).to_le_bytes());
                out.push(input[pos]);
                pos = run_end;
                continue;
            }
        }

        let (match_at, match_len) = matcher.longest_match(input, pos, relative);

        if match_len <= 2 {
            match literal_at {
                Some(cmd) if out[cmd] < 0xBF => out[cmd] += 1,
                _ => {
                    literal_at = Some(out.len());
                    out.push(0x81);
                }
            }
            out.push(input[pos]);
            pos += 1;
            continue;
        }

        let distance = pos - match_at;
        if match_len > 0x0A || distance > 0x0FFF {
            if match_len > 0x40 {
                out.push(0xFF);
                out.extend_from_slice(&(match_len as u16).to_le_bytes());
            } else {
                out.push(0xC0 | (match_len - 3) as u8);
            }
            let position = if relative { distance } else { match_at };
            out.extend_from_slice(&(position as u16).to_le_bytes());
        } else {
            out.push((((match_len - 3) << 4) | (distance >> 8)) as u8);
            out.push(distance as u8);
        }
        pos += match_len;
        literal_at = None;
    }

    out.push(0x80);
    out
}

/// How many earlier positions with the same 3-byte prefix are examined per match search.
/// The editor's encoder searched exhaustively, which is quadratic; this bounds the work
/// while still finding the long matches that matter for map data.
const MAX_CHAIN: usize = 512;

/// Hash chains over 3-byte prefixes, used to find match candidates quickly.
struct Matcher {
    head: Vec<Option<usize>>,
    prev: Vec<Option<usize>>,
    inserted: usize,
}

impl Matcher {
    const BITS: u32 = 15;

    fn new(len: usize) -> Self {
        Self {
            head: vec![None; 1 << Self::BITS],
            prev: vec![None; len],
            inserted: 0,
        }
    }

    fn hash(bytes: &[u8]) -> usize {
        let key = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
        (key.wrapping_mul(2_654_435_761) >> (32 - Self::BITS)) as usize
    }

    /// Records every position before `pos` that hasn't been indexed yet.
    fn index_up_to(&mut self, input: &[u8], pos: usize) {
        while self.inserted < pos {
            let at = self.inserted;
            if at + 3 <= input.len() {
                let h = Self::hash(&input[at..]);
                self.prev[at] = self.head[h];
                self.head[h] = Some(at);
            }
            self.inserted += 1;
        }
    }

    /// Finds the longest earlier occurrence of the bytes at `pos`, preferring the most
    /// recent one on ties. Lengths and positions stay within what the command encoding can
    /// express.
    fn longest_match(&mut self, input: &[u8], pos: usize, relative: bool) -> (usize, usize) {
        self.index_up_to(input, pos);
        let max_len = (input.len() - pos).min(usize::from(u16::MAX));
        if max_len < 3 {
            return (pos, 0);
        }
        let window_start = if relative {
            pos.saturating_sub(usize::from(u16::MAX))
        } else {
            0
        };

        let mut best = (pos, 0);
        let mut candidate = self.head[Self::hash(&input[pos..])];
        for _ in 0..MAX_CHAIN {
            let Some(at) = candidate.filter(|&at| at >= window_start) else {
                break;
            };
            let len = (0..max_len)
                .find(|&i| input[at + i] != input[pos + i])
                .unwrap_or(max_len);
            if len > best.1 {
                best = (at, len);
                if len == max_len {
                    break;
                }
            }
            candidate = self.prev[at];
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8]) {
        let packed = compress(data);
        let unpacked = decompress(&packed, data.len()).expect("valid stream");
        assert_eq!(unpacked.data, data);
        assert_eq!(unpacked.consumed, packed.len());
    }

    #[test]
    fn decodes_each_command() {
        // literal "ABC", short copy 3 from 3 back, fill 4 x 'Z', long copy 2 from pos 0,
        // medium copy 3 from pos 1, end.
        let stream = [
            0x83, b'A', b'B', b'C', 0x00, 0x03, 0xFE, 4, 0, b'Z', 0xFF, 2, 0, 0, 0, 0xC0, 1, 0,
            0x80,
        ];
        let out = decompress(&stream, 64).expect("valid");
        assert_eq!(out.data, b"ABCABCZZZZABBCA");
    }

    #[test]
    fn overlapping_copy_makes_a_run() {
        let stream = [0x81, b'x', 0x30, 0x01, 0x80];
        assert_eq!(decompress(&stream, 16).expect("valid").data, b"xxxxxxx");
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(decompress(&[0x83, b'A'], 16), Err(LcwError::Truncated));
        assert_eq!(
            decompress(&[0x81, b'A', 0x00, 0x05, 0x80], 16),
            Err(LcwError::BadReference { at: 2 })
        );
        assert_eq!(
            decompress(&[0xFE, 0xFF, 0x00, 1, 0x80], 16),
            Err(LcwError::OutputTooLarge { limit: 16 })
        );
    }

    #[test]
    fn roundtrips() {
        roundtrip(b"");
        roundtrip(b"a");
        roundtrip(b"abababababababab hello hello hello");
        roundtrip(&[7u8; 1000]);
        let noisy: Vec<u8> = (0u32..5000)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8 % 17)
            .collect();
        roundtrip(&noisy);
    }

    #[test]
    fn roundtrips_relative_mode() {
        let big: Vec<u8> = (0u32..70_000)
            .map(|i| (i % 251) as u8 ^ (i / 997) as u8)
            .collect();
        let packed = compress(&big);
        assert_eq!(packed[0], 0);
        assert_eq!(decompress(&packed, big.len()).expect("valid").data, big);
    }
}
