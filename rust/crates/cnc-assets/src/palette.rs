//! 256-colour palettes (`.PAL`): 768 bytes of red, green and blue, each 0..=63 as the VGA
//! DAC expected.

use std::fmt;

/// One palette entry with 8-bit channels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

/// The file is not a 768-byte palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadPalette {
    /// Length of the data given.
    pub len: usize,
}

impl fmt::Display for BadPalette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "palette is {} bytes, expected 768", self.len)
    }
}

impl std::error::Error for BadPalette {}

/// A 256-colour palette.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Palette {
    /// The original 6-bit values, three per colour.
    vga: [[u8; 3]; 256],
}

impl Palette {
    /// Size of a `.PAL` file.
    pub const FILE_SIZE: usize = 768;

    /// Parses a `.PAL` file. Values above 63 are masked to six bits, as the VGA DAC did.
    ///
    /// # Errors
    ///
    /// Returns [`BadPalette`] unless `data` is exactly 768 bytes.
    pub fn from_pal(data: &[u8]) -> Result<Self, BadPalette> {
        if data.len() != Self::FILE_SIZE {
            return Err(BadPalette { len: data.len() });
        }
        let mut vga = [[0u8; 3]; 256];
        for (entry, rgb) in vga.iter_mut().zip(data.chunks_exact(3)) {
            *entry = [rgb[0] & 0x3F, rgb[1] & 0x3F, rgb[2] & 0x3F];
        }
        Ok(Self { vga })
    }

    /// Serialises back to the 768-byte `.PAL` layout.
    #[must_use]
    pub fn to_pal(&self) -> Vec<u8> {
        self.vga.iter().flatten().copied().collect()
    }

    /// The 6-bit values of a colour.
    #[must_use]
    pub fn vga(&self, index: u8) -> [u8; 3] {
        self.vga[usize::from(index)]
    }

    /// A colour scaled to 8 bits per channel, replicating the top bits so 63 maps to 255.
    #[must_use]
    pub fn rgb(&self, index: u8) -> Rgb {
        let scale = |v: u8| v << 2 | v >> 4;
        let [r, g, b] = self.vga(index);
        Rgb {
            r: scale(r),
            g: scale(g),
            b: scale(b),
        }
    }

    /// All 256 colours at 8 bits per channel.
    #[must_use]
    pub fn colors(&self) -> [Rgb; 256] {
        std::array::from_fn(|i| self.rgb(i as u8))
    }

    /// Converts indexed pixels to RGBA, treating `transparent` (usually index 0) as fully
    /// transparent.
    #[must_use]
    pub fn to_rgba(&self, pixels: &[u8], transparent: Option<u8>) -> Vec<u8> {
        let colors = self.colors();
        pixels
            .iter()
            .flat_map(|&p| {
                let c = colors[usize::from(p)];
                let alpha = if Some(p) == transparent { 0 } else { 255 };
                [c.r, c.g, c.b, alpha]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_scales() {
        let mut data = vec![0u8; 768];
        data[3..6].copy_from_slice(&[63, 32, 0x7F]);
        let pal = Palette::from_pal(&data).expect("valid");
        assert_eq!(pal.vga(1), [63, 32, 63]);
        assert_eq!(
            pal.rgb(1),
            Rgb {
                r: 255,
                g: 130,
                b: 255
            }
        );
        assert_eq!(
            pal.to_rgba(&[0, 1], Some(0)),
            vec![0, 0, 0, 0, 255, 130, 255, 255]
        );
        assert_eq!(Palette::from_pal(&[0; 3]), Err(BadPalette { len: 3 }));
        assert_eq!(pal.to_pal().len(), 768);
    }
}
