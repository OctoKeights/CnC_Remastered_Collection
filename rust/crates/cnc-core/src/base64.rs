//! Standard (RFC 4648) Base64, as used for `[MapPack]` / `[OverlayPack]` sections and
//! Red Alert's encrypted INI blocks (`BASE64.CPP`).

use std::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// A character that is not part of the Base64 alphabet was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidBase64 {
    /// The offending byte.
    pub byte: u8,
    /// Its offset in the input.
    pub at: usize,
}

impl fmt::Display for InvalidBase64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid base64 byte 0x{:02X} at offset {}",
            self.byte, self.at
        )
    }
}

impl std::error::Error for InvalidBase64 {}

/// Encodes bytes as padded Base64.
#[must_use]
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 0x3F]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes Base64. Whitespace is skipped and decoding stops at the first `=`, which is how
/// INI-embedded data (split across many entries) is best handled.
///
/// # Errors
///
/// Returns [`InvalidBase64`] for any other byte outside the alphabet.
pub fn decode(text: &str) -> Result<Vec<u8>, InvalidBase64> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for (at, byte) in text.bytes().enumerate() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let value = sextet(byte).ok_or(InvalidBase64 { byte, at })?;
        acc = acc << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, coded) in cases {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).expect("valid"), plain.as_bytes());
        }
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(decode("Zm9!"), Err(InvalidBase64 { byte: b'!', at: 3 }));
    }
}
