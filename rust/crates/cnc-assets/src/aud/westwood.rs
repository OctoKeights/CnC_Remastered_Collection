//! Westwood's 8-bit audio codec (`SCOMP_WESTWOOD`, compression type 1).
//!
//! The decoder isn't part of the source release, so this follows the format's public
//! description. Each chunk restarts at sample 128 (silence). A chunk whose compressed and
//! decoded sizes are equal is stored raw. Otherwise each command byte is `mm cccccc`:
//!
//! | Mode | Meaning |
//! |---|---|
//! | `00` | `c + 1` bytes follow, each holding four 2-bit deltas |
//! | `01` | `c + 1` bytes follow, each holding two 4-bit deltas |
//! | `10` | if `c & 0x20`, one sample changes by the signed 5-bit value in `c`; else `c + 1` raw samples follow |
//! | `11` | the last sample repeats `c + 1` times |
//!
//! Deltas are read low bits first and samples saturate at 0 and 255.

const DELTA2: [i16; 4] = [-2, -1, 0, 1];
const DELTA4: [i16; 16] = [-9, -8, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 8];

/// Decodes one chunk into at most `output_len` unsigned 8-bit samples. Malformed input
/// stops decoding early rather than failing.
#[must_use]
pub fn decode(input: &[u8], output_len: usize) -> Vec<u8> {
    if input.len() == output_len {
        return input.to_vec();
    }

    let mut out = Vec::with_capacity(output_len);
    let mut sample: i16 = 128;
    let mut src = input.iter().copied();

    let push = |out: &mut Vec<u8>, sample: &mut i16, delta: i16| {
        *sample = (*sample + delta).clamp(0, 255);
        out.push(*sample as u8);
    };

    while out.len() < output_len {
        let Some(cmd) = src.next() else { break };
        let count = usize::from(cmd & 0x3F) + 1;
        match cmd >> 6 {
            0 => {
                for byte in src.by_ref().take(count) {
                    for shift in [0, 2, 4, 6] {
                        push(
                            &mut out,
                            &mut sample,
                            DELTA2[usize::from(byte >> shift & 3)],
                        );
                    }
                }
            }
            1 => {
                for byte in src.by_ref().take(count) {
                    push(&mut out, &mut sample, DELTA4[usize::from(byte & 0x0F)]);
                    push(&mut out, &mut sample, DELTA4[usize::from(byte >> 4)]);
                }
            }
            2 if cmd & 0x20 != 0 => {
                // Sign-extend the low five bits.
                let delta = i16::from(((cmd & 0x1F) << 3) as i8 >> 3);
                push(&mut out, &mut sample, delta);
            }
            2 => {
                for byte in src.by_ref().take(count) {
                    sample = i16::from(byte);
                    out.push(byte);
                }
            }
            _ => out.extend(std::iter::repeat_n(sample as u8, count)),
        }
    }
    out.truncate(output_len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_chunks_are_copied() {
        assert_eq!(decode(&[1, 2, 3], 3), vec![1, 2, 3]);
    }

    #[test]
    fn each_mode() {
        // 2-bit: one byte 0b11_10_01_00 -> deltas -2, -1, 0, +1 from 128.
        assert_eq!(decode(&[0x00, 0b1110_0100], 4), vec![126, 125, 125, 126]);
        // 4-bit: 0x0F -> +8 then -9.
        assert_eq!(decode(&[0x40, 0x0F], 3), vec![136, 127]);
        // 5-bit signed delta of -1, then raw samples 7 and 9, then repeat 9 twice.
        assert_eq!(
            decode(&[0x80 | 0x20 | 0x1F, 0x81, 7, 9, 0xC1], 6),
            vec![127, 7, 9, 9, 9]
        );
        // Saturation at 255.
        assert_eq!(decode(&[0x80, 250, 0x40, 0xFF], 3), vec![250, 255, 255]);
    }
}
