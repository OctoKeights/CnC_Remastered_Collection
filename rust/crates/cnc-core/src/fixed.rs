//! Unsigned 16.16 fixed-point numbers (`fixed` in `FIXED.H`).
//!
//! Red Alert uses these for every rule value that can be fractional (speeds, costs
//! multipliers, percentages). Arithmetic wraps exactly as the 32-bit original does, so game
//! simulation stays deterministic across platforms.

use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Shl, Shr, Sub, SubAssign};
use std::str::FromStr;

/// An unsigned 16.16 fixed-point number.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(u32);

impl Fixed {
    const PRECISION: u32 = 1 << 16;

    /// Zero.
    pub const ZERO: Self = Self(0);
    /// One.
    pub const ONE: Self = Self(Self::PRECISION);
    /// One half.
    pub const HALF: Self = Self::ratio(1, 2);
    /// One third.
    pub const THIRD: Self = Self::ratio(1, 3);
    /// One quarter.
    pub const QUARTER: Self = Self::ratio(1, 4);
    /// Three quarters.
    pub const THREE_QUARTERS: Self = Self::ratio(3, 4);
    /// Two thirds.
    pub const TWO_THIRDS: Self = Self::ratio(2, 3);

    /// Builds a value from its raw 16.16 bits.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw 16.16 bits.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// `numerator / denominator`, truncated. A zero denominator gives zero, as in the
    /// original.
    #[must_use]
    pub const fn ratio(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            Self(0)
        } else {
            Self((numerator as u64 * Self::PRECISION as u64 / denominator as u64) as u32)
        }
    }

    /// A whole number. Only the low 16 bits are kept, as in the original.
    #[must_use]
    pub const fn from_int(value: u32) -> Self {
        Self((value & 0xFFFF) << 16)
    }

    /// The whole part.
    #[must_use]
    pub const fn whole(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// The fractional part, in 1/65536ths.
    #[must_use]
    pub const fn fraction(self) -> u16 {
        self.0 as u16
    }

    /// Rounds to the nearest integer (`operator unsigned`).
    #[must_use]
    pub const fn to_int(self) -> u32 {
        ((self.0 as u64 + (Self::PRECISION as u64 >> 1)) / Self::PRECISION as u64) as u32
    }

    /// Multiplies an integer by this value, rounding to the nearest integer. This is what
    /// `fixed * int` means in the original (the result is an `int`).
    #[must_use]
    pub const fn scale(self, value: u32) -> u32 {
        Self(self.0.wrapping_mul(value)).to_int()
    }

    /// Rounds up to the next whole number.
    #[must_use]
    pub const fn ceil(self) -> Self {
        Self(self.0.wrapping_add(Self::PRECISION - 1) & !0xFFFF)
    }

    /// Rounds down to a whole number.
    #[must_use]
    pub const fn floor(self) -> Self {
        Self(self.0 & !0xFFFF)
    }

    /// Rounds to the nearest whole number (halves round up).
    #[must_use]
    pub const fn round(self) -> Self {
        if self.fraction() as u32 >= Self::PRECISION >> 1 {
            self.ceil()
        } else {
            self.floor()
        }
    }

    /// Caps the value at `cap` (`Saturate`).
    #[must_use]
    pub fn saturate(self, cap: Self) -> Self {
        self.min(cap)
    }

    /// Keeps the value strictly below `cap` (`Sub_Saturate`).
    #[must_use]
    pub const fn sub_saturate(self, cap: Self) -> Self {
        if self.0 >= cap.0 {
            Self(cap.0.wrapping_sub(1))
        } else {
            self
        }
    }

    /// `1 / self`, with the original's rule that dividing by zero leaves the value
    /// unchanged.
    #[must_use]
    pub fn inverse(self) -> Self {
        Self::ONE / self
    }
}

impl From<u16> for Fixed {
    fn from(value: u16) -> Self {
        Self::from_int(u32::from(value))
    }
}

impl Add for Fixed {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for Fixed {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Mul for Fixed {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self((u64::from(self.0) * u64::from(rhs.0) / u64::from(Self::PRECISION)) as u32)
    }
}

impl Div for Fixed {
    type Output = Self;
    /// Rounded division. Dividing by zero or one leaves the value unchanged.
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 || rhs.0 == Self::PRECISION {
            return self;
        }
        let numerator =
            u64::from(self.0) * u64::from(Self::PRECISION) + u64::from(Self::PRECISION >> 1);
        Self((numerator / u64::from(rhs.0)) as u32)
    }
}

impl Mul<u32> for Fixed {
    type Output = Self;
    fn mul(self, rhs: u32) -> Self {
        Self(self.0.wrapping_mul(rhs))
    }
}

impl Div<u32> for Fixed {
    type Output = Self;
    /// Truncating division. Dividing by zero leaves the value unchanged.
    fn div(self, rhs: u32) -> Self {
        self.0.checked_div(rhs).map_or(self, Self)
    }
}

impl Shl<u32> for Fixed {
    type Output = Self;
    fn shl(self, rhs: u32) -> Self {
        Self(self.0.wrapping_shl(rhs))
    }
}

impl Shr<u32> for Fixed {
    type Output = Self;
    fn shr(self, rhs: u32) -> Self {
        Self(self.0.wrapping_shr(rhs))
    }
}

macro_rules! assign_ops {
    ($($trait:ident $method:ident $op:tt $rhs:ty),*) => {$(
        impl $trait<$rhs> for Fixed {
            fn $method(&mut self, rhs: $rhs) {
                *self = *self $op rhs;
            }
        }
    )*};
}

assign_ops!(
    AddAssign add_assign + Fixed,
    SubAssign sub_assign - Fixed,
    MulAssign mul_assign * Fixed,
    DivAssign div_assign / Fixed,
    MulAssign mul_assign * u32,
    DivAssign div_assign / u32
);

/// Never fails: like the original's `atoi`-based parser, unparseable text gives zero.
impl FromStr for Fixed {
    type Err = std::convert::Infallible;

    /// Parses `"1.25"`, `".5"`, `"3"` or a percentage such as `"75%"`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim_start();
        let digits_end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());

        if text[digits_end..].starts_with('%') {
            let percent = leading_int(text);
            return Ok(Self::ratio(percent, 100));
        }

        let whole = if text.starts_with('.') {
            0
        } else {
            leading_int(text) & 0xFFFF
        };
        let fraction = text.find('.').map_or(0, |dot| {
            let frac_text = &text[dot + 1..];
            let len = frac_text
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(frac_text.len());
            // Beyond nine digits the original's base overflowed; nobody writes that.
            let digits = &frac_text[..len.min(9)];
            let value: u64 = digits.parse().unwrap_or(0);
            let base = 10u64.pow(digits.len() as u32);
            (value * u64::from(Self::PRECISION) / base) as u32 & 0xFFFF
        });
        Ok(Self(whole << 16 | fraction))
    }
}

/// `atoi` on the leading digits (signs and overflow are not meaningful for rule values).
fn leading_int(text: &str) -> u32 {
    text.bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0u32, |acc, d| {
            acc.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
        })
}

/// Formats like `To_ASCII`: the whole part, then up to three decimal places with trailing
/// zeros removed.
///
/// The original printed thousandths with `%02u`, so 1.0625 came out as `"1.62"`; this
/// prints it correctly as `"1.062"`.
impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.whole();
        let thousandths = u32::from(self.fraction()) * 1000 / Self::PRECISION;
        if thousandths == 0 {
            write!(f, "{whole}")
        } else {
            let frac = format!("{thousandths:03}");
            write!(f, "{whole}.{}", frac.trim_end_matches('0'))
        }
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({self} = 0x{:08X})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Fixed {
        s.parse().unwrap_or_default()
    }

    #[test]
    fn parses_rule_values() {
        assert_eq!(parse("1"), Fixed::ONE);
        assert_eq!(parse(".5"), Fixed::HALF);
        assert_eq!(parse("0.25"), Fixed::QUARTER);
        assert_eq!(parse("50%"), Fixed::HALF);
        assert_eq!(parse("  3.75"), Fixed::from_int(3) + Fixed::THREE_QUARTERS);
        assert_eq!(parse("junk"), Fixed::ZERO);
    }

    #[test]
    fn arithmetic_matches_original() {
        let a = Fixed::from_int(3);
        assert_eq!(a * Fixed::HALF, parse("1.5"));
        assert_eq!(a / Fixed::from_int(2), parse("1.5"));
        assert_eq!(a / Fixed::ZERO, a);
        assert_eq!(a / Fixed::ONE, a);
        assert_eq!(Fixed::HALF.scale(7), 4); // 3.5 rounds up
        assert_eq!(Fixed::THIRD.scale(3), 1);
        assert_eq!(Fixed::ZERO - Fixed::ONE, Fixed::from_raw(0xFFFF_0000));
    }

    #[test]
    fn rounding() {
        let x = parse("2.5");
        assert_eq!(x.floor(), Fixed::from_int(2));
        assert_eq!(x.ceil(), Fixed::from_int(3));
        assert_eq!(x.round(), Fixed::from_int(3));
        assert_eq!(parse("2.25").round(), Fixed::from_int(2));
        assert_eq!(x.to_int(), 3);
        assert_eq!(
            Fixed::from_int(5).sub_saturate(Fixed::from_int(5)).raw(),
            0x4_FFFF
        );
    }

    #[test]
    fn display() {
        assert_eq!(Fixed::from_int(4).to_string(), "4");
        assert_eq!(parse("1.5").to_string(), "1.5");
        assert_eq!(parse("1.125").to_string(), "1.125");
        // The original printed this as "1.62".
        assert_eq!(parse("1.0625").to_string(), "1.062");
        assert_eq!(Fixed::QUARTER.to_string(), "0.25");
    }
}
