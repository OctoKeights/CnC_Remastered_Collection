//! Westwood audio files (`.AUD`, and the `.V00`-`.V03` / `.JUV` unit responses).
//!
//! ```text
//! u16 rate, i32 size, i32 uncompressed_size, u8 flags (1 = stereo, 2 = 16-bit), u8 compression
//! then chunks of { u16 size, u16 output_size, u32 0x0000DEAF, size bytes }
//! ```
//!
//! Two codecs occur in the shipped data:
//!
//! * `99`, SOS ADPCM: 16-bit IMA-style ADPCM, two samples per byte, low nibble first
//!   (`sosCODECDecompressData` in `ADPCM.CPP`). Decoder state carries across chunks. Some
//!   shipped files declare one sample more in their last chunk than the data holds; the
//!   decoder produces only what is there.
//! * `1`, Westwood's own 8-bit delta codec ([`westwood`]), used by a handful of Tiberian
//!   Dawn sound effects. Its decoder isn't part of the source release; this one follows the
//!   format's public description.

mod sos_tables;
pub mod westwood;

use std::fmt;

use cnc_core::bytes::Reader;

const CHUNK_MAGIC: u32 = 0x0000_DEAF;
const FLAG_STEREO: u8 = 1;
const FLAG_16BIT: u8 = 2;

/// Errors from decoding an audio file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudError {
    /// The header is cut short.
    Truncated,
    /// The compression type isn't one the games use.
    UnsupportedCompression(u8),
    /// A chunk header is missing its `0xDEAF` marker or runs past the end of the file.
    BadChunk {
        /// Offset of the chunk in the file.
        at: usize,
    },
}

impl fmt::Display for AudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("audio header is truncated"),
            Self::UnsupportedCompression(c) => write!(f, "unsupported audio compression {c}"),
            Self::BadChunk { at } => write!(f, "bad audio chunk at offset {at}"),
        }
    }
}

impl std::error::Error for AudError {}

/// The codec an audio file uses (`SCompressType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compression {
    /// Uncompressed (`SCOMP_NONE`).
    None,
    /// Westwood's 8-bit delta codec (`SCOMP_WESTWOOD`).
    Westwood,
    /// SOS ADPCM (`SCOMP_SOS`).
    Sos,
}

/// The parsed file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudHeader {
    /// Playback rate in hertz.
    pub rate: u16,
    /// Bytes of compressed data following the header.
    pub size: u32,
    /// Bytes of audio once decoded.
    pub uncompressed_size: u32,
    /// Two channels rather than one.
    pub stereo: bool,
    /// 16-bit rather than 8-bit samples.
    pub sixteen_bit: bool,
    /// The codec.
    pub compression: Compression,
}

/// Decoded audio as signed 16-bit PCM (8-bit sources are widened), interleaved if stereo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    /// The file header.
    pub header: AudHeader,
    /// The samples.
    pub samples: Vec<i16>,
}

impl AudHeader {
    /// Size of the header in bytes.
    pub const SIZE: usize = 12;

    /// Parses the header at the start of a file.
    ///
    /// # Errors
    ///
    /// Fails if the data is too short or the compression type is unknown.
    pub fn parse(data: &[u8]) -> Result<Self, AudError> {
        let mut r = Reader::new(data);
        let (Some(rate), Some(size), Some(uncompressed_size), Some(flags), Some(comp)) =
            (r.u16(), r.u32(), r.u32(), r.u8(), r.u8())
        else {
            return Err(AudError::Truncated);
        };
        let compression = match comp {
            0 => Compression::None,
            1 => Compression::Westwood,
            99 => Compression::Sos,
            other => return Err(AudError::UnsupportedCompression(other)),
        };
        Ok(Self {
            rate,
            size,
            uncompressed_size,
            stereo: flags & FLAG_STEREO != 0,
            sixteen_bit: flags & FLAG_16BIT != 0,
            compression,
        })
    }
}

/// Splits the body of a compressed file into its chunks' payloads.
fn chunks(data: &[u8]) -> Result<Vec<(&[u8], usize)>, AudError> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    while r.remaining() > 0 {
        let at = AudHeader::SIZE + r.position();
        let (Some(size), Some(output), Some(magic)) = (r.u16(), r.u16(), r.u32()) else {
            return Err(AudError::BadChunk { at });
        };
        if magic != CHUNK_MAGIC {
            return Err(AudError::BadChunk { at });
        }
        let payload = r.take(usize::from(size)).ok_or(AudError::BadChunk { at })?;
        out.push((payload, usize::from(output)));
    }
    Ok(out)
}

/// SOS ADPCM decoder state (`_SOS_COMPRESS_INFO`).
#[derive(Debug, Clone, Copy, Default)]
struct SosDecoder {
    index: usize,
    sample: i32,
}

impl SosDecoder {
    fn nibble(&mut self, token: u8) -> i16 {
        let fast = (self.index + usize::from(token)).min(sos_tables::DIFF.len() - 1);
        self.sample = (self.sample + sos_tables::DIFF[fast]).clamp(-32768, 32767);
        self.index = usize::from(sos_tables::INDEX[fast]);
        self.sample as i16
    }

    fn decode(&mut self, data: &[u8], out: &mut Vec<i16>) {
        for &byte in data {
            out.push(self.nibble(byte & 0x0F));
            out.push(self.nibble(byte >> 4));
        }
    }
}

impl Sound {
    /// Decodes a whole audio file.
    ///
    /// # Errors
    ///
    /// Returns an [`AudError`] if the header or a chunk is malformed.
    pub fn decode(data: &[u8]) -> Result<Self, AudError> {
        let header = AudHeader::parse(data)?;
        let body = data.get(AudHeader::SIZE..).unwrap_or_default();
        let body = body.get(..header.size as usize).unwrap_or(body);
        let mut samples = Vec::with_capacity(header.uncompressed_size as usize / 2);

        match header.compression {
            Compression::None => {
                if header.sixteen_bit {
                    samples.extend(
                        body.chunks_exact(2)
                            .map(|b| i16::from_le_bytes([b[0], b[1]])),
                    );
                } else {
                    samples.extend(body.iter().map(|&b| widen(b)));
                }
            }
            Compression::Sos => {
                let mut decoder = SosDecoder::default();
                for (payload, _) in chunks(body)? {
                    decoder.decode(payload, &mut samples);
                }
            }
            Compression::Westwood => {
                for (payload, output) in chunks(body)? {
                    samples.extend(westwood::decode(payload, output).into_iter().map(widen));
                }
            }
        }
        Ok(Self { header, samples })
    }

    /// Encodes the samples as a 16-bit PCM WAV file, for listening or for tools.
    #[must_use]
    pub fn to_wav(&self) -> Vec<u8> {
        let channels: u16 = if self.header.stereo { 2 } else { 1 };
        let rate = u32::from(self.header.rate);
        let data_len = (self.samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in &self.samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

/// Unsigned 8-bit PCM to signed 16-bit.
fn widen(sample: u8) -> i16 {
    (i16::from(sample) - 128) << 8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(comp: u8, flags: u8, chunks: &[(&[u8], u16)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (payload, output) in chunks {
            body.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            body.extend_from_slice(&output.to_le_bytes());
            body.extend_from_slice(&CHUNK_MAGIC.to_le_bytes());
            body.extend_from_slice(payload);
        }
        let mut out = Vec::new();
        out.extend_from_slice(&22050u16.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&[flags, comp]);
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn sos_matches_reference() {
        // Tokens 0x7 then 0x7: index 0 token 7 adds DIFF[7] = 11, moves to index 8.
        let sound = Sound::decode(&file(99, 2, &[(&[0x77], 4)])).expect("valid");
        assert_eq!(sound.samples[0], 11);
        assert_eq!(
            i32::from(sound.samples[1]),
            11 + sos_tables::DIFF[usize::from(sos_tables::INDEX[7]) + 7]
        );
        // State carries across chunks.
        let split = Sound::decode(&file(99, 2, &[(&[0x77], 4), (&[0x07], 4)])).expect("valid");
        let whole = Sound::decode(&file(99, 2, &[(&[0x77, 0x07], 8)])).expect("valid");
        assert_eq!(split.samples, whole.samples);
    }

    #[test]
    fn header_and_errors() {
        let sound = Sound::decode(&file(99, 2, &[])).expect("valid");
        assert!(sound.header.sixteen_bit && !sound.header.stereo);
        assert_eq!(Sound::decode(&[0; 4]), Err(AudError::Truncated));
        assert_eq!(
            Sound::decode(&file(7, 0, &[])),
            Err(AudError::UnsupportedCompression(7))
        );
        let mut bad = file(99, 2, &[(&[1, 2], 4)]);
        bad[12 + 4] = 0;
        assert_eq!(Sound::decode(&bad), Err(AudError::BadChunk { at: 12 }));
    }

    #[test]
    fn wav_header() {
        let sound = Sound::decode(&file(99, 2, &[(&[0x11], 4)])).expect("valid");
        let wav = sound.to_wav();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 4);
    }
}
