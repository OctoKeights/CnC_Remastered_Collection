//! Map geometry: leptons, coordinates, cells and facings.
//!
//! The games measure positions in *leptons*, 256 to a cell. A [`Coord`] packs a 16-bit X
//! (low word) and Y (high word); in each, the high byte is the cell and the low byte the
//! lepton within it. A [`Cell`] is a cell index `y * width + x`, where the map width is a
//! power of two that differs per game (64 in Tiberian Dawn, 128 in Red Alert), so `Cell` is
//! generic over the number of X bits.

use std::fmt;

/// Leptons per cell.
pub const CELL_LEPTONS: i32 = 256;

/// Converts whole cells to leptons (`Cell_To_Lepton`).
#[must_use]
pub const fn cell_to_lepton(cell: i32) -> i32 {
    cell << 8
}

/// Converts leptons to the nearest whole cell (`Lepton_To_Cell`).
#[must_use]
pub const fn lepton_to_cell(lepton: i32) -> i32 {
    ((lepton + 0x80) as u32 >> 8) as i32
}

/// A position on the map in leptons (`COORDINATE`).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Coord(u32);

impl Coord {
    /// Builds a coordinate from lepton positions (`XY_Coord`). Values wrap to 16 bits.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self(((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF))
    }

    /// Builds a coordinate from its packed 32-bit form.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The packed 32-bit form.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// X in leptons (`Coord_X`).
    #[must_use]
    pub const fn x(self) -> i16 {
        self.0 as u16 as i16
    }

    /// Y in leptons (`Coord_Y`).
    #[must_use]
    pub const fn y(self) -> i16 {
        (self.0 >> 16) as u16 as i16
    }

    /// The X cell (`Coord_XCell`).
    #[must_use]
    pub const fn x_cell(self) -> u8 {
        (self.0 >> 8) as u8
    }

    /// The Y cell (`Coord_YCell`).
    #[must_use]
    pub const fn y_cell(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// The lepton offset within the X cell (`Coord_XLepton`).
    #[must_use]
    pub const fn x_lepton(self) -> u8 {
        self.0 as u8
    }

    /// The lepton offset within the Y cell (`Coord_YLepton`).
    #[must_use]
    pub const fn y_lepton(self) -> u8 {
        (self.0 >> 16) as u8
    }

    /// Moves to the centre of the containing cell (`Coord_Snap`).
    #[must_use]
    pub const fn snap(self) -> Self {
        Self((self.0 & 0xFF00_FF00) | 0x0080_0080)
    }

    /// Component-wise sum, each half wrapping at 16 bits (`Coord_Add`).
    #[must_use]
    pub const fn add(self, other: Self) -> Self {
        Self::new(
            self.x() as i32 + other.x() as i32,
            self.y() as i32 + other.y() as i32,
        )
    }

    /// Component-wise difference, each half wrapping at 16 bits (`Coord_Sub`).
    #[must_use]
    pub const fn sub(self, other: Self) -> Self {
        Self::new(
            self.x() as i32 - other.x() as i32,
            self.y() as i32 - other.y() as i32,
        )
    }

    /// The midpoint, treating halves as unsigned (`Coord_Mid`).
    #[must_use]
    pub const fn midpoint(self, other: Self) -> Self {
        let x = ((self.0 & 0xFFFF) + (other.0 & 0xFFFF)) >> 1;
        let y = ((self.0 >> 16) + (other.0 >> 16)) >> 1;
        Self((y << 16) | x)
    }

    /// The games' cheap distance estimate in leptons (`Distance_Coord`): the larger axis
    /// difference plus half the smaller one, computed in 16-bit arithmetic as the original
    /// assembly does.
    #[must_use]
    pub const fn distance(self, other: Self) -> u16 {
        let dx = self.x().wrapping_sub(other.x()).wrapping_abs();
        let dy = self.y().wrapping_sub(other.y()).wrapping_abs();
        let (major, minor) = if dy > dx { (dy, dx) } else { (dx, dy) };
        (major as u16).wrapping_add((minor as u16) >> 1)
    }

    /// The direction from this coordinate towards `target`, to 1/256 of a turn
    /// (`Direction256`).
    #[must_use]
    pub fn direction_to(self, target: Self) -> Dir {
        desired_facing256(
            i32::from(self.x()),
            i32::from(self.y()),
            i32::from(target.x()),
            i32::from(target.y()),
        )
    }

    /// The direction from this coordinate towards `target`, to the nearest of the eight
    /// compass points (`Direction8`).
    #[must_use]
    pub fn direction8_to(self, target: Self) -> Dir {
        desired_facing8(
            i32::from(self.x()),
            i32::from(self.y()),
            i32::from(target.x()),
            i32::from(target.y()),
        )
    }

    /// The centre of the neighbouring cell in direction `facing` (`Adjacent_Cell`).
    #[must_use]
    pub const fn adjacent(self, facing: Facing) -> Self {
        let (dx, dy) = facing.offset();
        self.add(Self::new(dx * CELL_LEPTONS, dy * CELL_LEPTONS))
            .snap()
    }
}

impl fmt::Debug for Coord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Coord({}, {})", self.x(), self.y())
    }
}

/// A cell index on a map `1 << X_BITS` cells wide (`CELL`).
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell<const X_BITS: u32>(u16);

impl<const X_BITS: u32> Cell<X_BITS> {
    /// Map width in cells.
    pub const WIDTH: i32 = 1 << X_BITS;
    const X_MASK: u16 = (1 << X_BITS) - 1;

    /// Builds a cell from X and Y (`XY_Cell`).
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self((y << X_BITS) | (x & Self::X_MASK))
    }

    /// Builds a cell from its raw index.
    #[must_use]
    pub const fn from_index(index: u16) -> Self {
        Self(index)
    }

    /// The raw index.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }

    /// X in cells (`Cell_X`).
    #[must_use]
    pub const fn x(self) -> u16 {
        self.0 & Self::X_MASK
    }

    /// Y in cells (`Cell_Y`).
    #[must_use]
    pub const fn y(self) -> u16 {
        self.0 >> X_BITS
    }

    /// The coordinate of the cell's centre (`Cell_Coord`).
    #[must_use]
    pub const fn center(self) -> Coord {
        Coord::new(
            ((self.x() as i32) << 8) | 0x80,
            ((self.y() as i32) << 8) | 0x80,
        )
    }

    /// The cell containing a coordinate (`Coord_Cell`).
    #[must_use]
    pub const fn containing(coord: Coord) -> Self {
        Self::new(coord.x_cell() as u16, coord.y_cell() as u16)
    }

    /// The neighbouring cell in direction `facing`, wrapping like the original's
    /// `cell + AdjacentCell[facing]`.
    #[must_use]
    pub const fn adjacent(self, facing: Facing) -> Self {
        let (dx, dy) = facing.offset();
        Self(self.0.wrapping_add((dy * Self::WIDTH + dx) as u16))
    }

    /// The cheap distance estimate in cells (`Distance(CELL, CELL)`).
    #[must_use]
    pub const fn distance(self, other: Self) -> u16 {
        let dx = self.x().abs_diff(other.x());
        let dy = self.y().abs_diff(other.y());
        if dy > dx {
            dy + (dx >> 1)
        } else {
            dx + (dy >> 1)
        }
    }

    /// The compass direction towards another cell (`Direction(CELL, CELL)`).
    #[must_use]
    pub fn direction_to(self, target: Self) -> Dir {
        desired_facing8(
            i32::from(self.x()),
            i32::from(self.y()),
            i32::from(target.x()),
            i32::from(target.y()),
        )
    }
}

impl<const X_BITS: u32> fmt::Debug for Cell<X_BITS> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cell({} @ {},{})", self.0, self.x(), self.y())
    }
}

/// A direction in 1/256ths of a turn, clockwise from north (`DirType`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dir(pub u8);

impl Dir {
    /// North.
    pub const N: Self = Self(0);
    /// East.
    pub const E: Self = Self(64);
    /// South.
    pub const S: Self = Self(128);
    /// West.
    pub const W: Self = Self(192);

    /// The nearest of the eight compass facings (`Dir_Facing`).
    #[must_use]
    pub const fn facing(self) -> Facing {
        Facing::ALL[(self.0.wrapping_add(0x10) >> 5) as usize]
    }

    /// The signed shortest turn from `self` to `to`, in 1/256ths.
    #[must_use]
    pub const fn turn_to(self, to: Self) -> i8 {
        to.0.wrapping_sub(self.0) as i8
    }

    /// Tiberian Dawn's `Dir_Diff`: the difference of the two directions read as *signed*
    /// bytes, without wrapping. It agrees with [`Dir::turn_to`] except when the turn crosses
    /// south; kept for simulation fidelity.
    #[must_use]
    pub const fn legacy_diff(self, to: Self) -> i32 {
        to.0 as i8 as i32 - self.0 as i8 as i32
    }
}

/// One of the eight compass facings (`FacingType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Facing {
    /// North.
    N,
    /// North-east.
    NE,
    /// East.
    E,
    /// South-east.
    SE,
    /// South.
    S,
    /// South-west.
    SW,
    /// West.
    W,
    /// North-west.
    NW,
}

impl Facing {
    /// All facings, clockwise from north.
    pub const ALL: [Self; 8] = [
        Self::N,
        Self::NE,
        Self::E,
        Self::SE,
        Self::S,
        Self::SW,
        Self::W,
        Self::NW,
    ];

    /// The equivalent 256-step direction (`Facing_Dir`).
    #[must_use]
    pub const fn dir(self) -> Dir {
        Dir((self as u8) << 5)
    }

    /// The cell step `(dx, dy)` this facing points along.
    #[must_use]
    pub const fn offset(self) -> (i32, i32) {
        match self {
            Self::N => (0, -1),
            Self::NE => (1, -1),
            Self::E => (1, 0),
            Self::SE => (1, 1),
            Self::S => (0, 1),
            Self::SW => (-1, 1),
            Self::W => (-1, 0),
            Self::NW => (-1, -1),
        }
    }

    /// The facing turned by `steps` eighths clockwise.
    #[must_use]
    pub const fn rotate(self, steps: i32) -> Self {
        Self::ALL[((self as i32 + steps).rem_euclid(8)) as usize]
    }
}

/// Direction from `(src_x, src_y)` to `(dst_x, dst_y)` at full 256-step resolution
/// (`Desired_Facing256`).
///
/// A bit-exact port of the assembly, quirks included: due east comes out as 63 and due
/// south as 127 (one step short), and identical points give 255.
#[must_use]
pub fn desired_facing256(src_x: i32, src_y: i32, dst_x: i32, dst_y: i32) -> Dir {
    let mut quadrant: u32 = 0;

    let mut dx = dst_x.wrapping_sub(src_x);
    if dx < 0 {
        dx = dx.wrapping_neg();
        quadrant = 0b1100_0000;
    }
    let mut dy = src_y.wrapping_sub(dst_y);
    if dy < 0 {
        quadrant ^= 0b0100_0000;
        dy = dy.wrapping_neg();
    }

    let mut adjust = (quadrant & 0b0100_0000) ^ 0b0100_0000;
    let (mut minor, mut major) = (dy as u32, dx as u32);
    if minor >= major {
        std::mem::swap(&mut minor, &mut major);
        adjust ^= 0b0100_0000;
    }

    // Keep the dividend within range of the divide.
    if minor & 0xFFFF_FF00 == 0 {
        while major & 0xFFFF_FF00 != 0 {
            major >>= 1;
            minor >>= 1;
        }
    }

    let ratio = if major == 0 {
        u32::MAX
    } else {
        ((u64::from(minor) << 8) / u64::from(major)) as u32
    };

    let mut angle = ratio >> 3;
    if adjust != 0 {
        adjust -= 1;
        angle = angle.wrapping_neg();
    }
    Dir(angle.wrapping_add(adjust).wrapping_add(quadrant) as u8)
}

/// Direction from `(x1, y1)` to `(x2, y2)` rounded to one of the eight compass points
/// (`Desired_Facing8`).
#[must_use]
pub fn desired_facing8(x1: i32, y1: i32, x2: i32, y2: i32) -> Dir {
    const TABLE: [u8; 16] = [1, 2, 1, 0, 7, 6, 7, 0, 3, 2, 3, 4, 5, 6, 5, 4];

    let mut index = 0usize;
    let mut dy = y1.wrapping_sub(y2);
    if dy < 0 {
        index += 1;
        dy = dy.wrapping_neg();
    }
    index <<= 1;
    let mut dx = x2.wrapping_sub(x1);
    if dx < 0 {
        index += 1;
        dx = dx.wrapping_neg();
    }

    let (dx, dy) = (dx as u32, dy as u32);
    let y_major = dx < dy;
    let (minor, major) = if y_major { (dx, dy) } else { (dy, dx) };
    index = index << 1 | usize::from(y_major);
    let half = (major + 1) >> 1;
    index = index << 1 | usize::from(minor < half);

    Dir(TABLE[index] << 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TdCell = Cell<6>;
    type RaCell = Cell<7>;

    #[test]
    fn coord_packing() {
        let c = Coord::new(0x0380, 0x0540);
        assert_eq!(c.raw(), 0x0540_0380);
        assert_eq!((c.x_cell(), c.x_lepton()), (3, 0x80));
        assert_eq!((c.y_cell(), c.y_lepton()), (5, 0x40));
        assert_eq!(c.snap(), Coord::new(0x0380, 0x0580));
        assert_eq!(Coord::new(-1, 2).x(), -1);
    }

    #[test]
    fn cell_roundtrip() {
        let c = TdCell::new(10, 20);
        assert_eq!(c.index(), 20 * 64 + 10);
        assert_eq!(c.center(), Coord::new(10 * 256 + 128, 20 * 256 + 128));
        assert_eq!(TdCell::containing(c.center()), c);
        assert_eq!(RaCell::new(10, 20).index(), 20 * 128 + 10);
    }

    #[test]
    fn adjacency_matches_tables() {
        // AdjacentCell[] from CONST.CPP for a 64-wide map.
        let expected: [i32; 8] = [-64, -63, 1, 65, 64, 63, -1, -65];
        let origin = TdCell::new(10, 10);
        for (facing, delta) in Facing::ALL.into_iter().zip(expected) {
            assert_eq!(
                i32::from(origin.adjacent(facing).index()) - i32::from(origin.index()),
                delta
            );
        }
        // AdjacentCoord[] from CONST.CPP.
        let raw: [u32; 8] = [
            0xFF00_0000,
            0xFF00_0100,
            0x0000_0100,
            0x0100_0100,
            0x0100_0000,
            0x0100_FF00,
            0x0000_FF00,
            0xFF00_FF00,
        ];
        let at = TdCell::new(10, 10).center();
        for (facing, step) in Facing::ALL.into_iter().zip(raw) {
            assert_eq!(at.adjacent(facing), at.add(Coord::from_raw(step)).snap());
        }
    }

    #[test]
    fn distances() {
        let a = Coord::new(0, 0);
        assert_eq!(a.distance(Coord::new(300, 100)), 350);
        assert_eq!(a.distance(Coord::new(-100, -300)), 350);
        assert_eq!(TdCell::new(0, 0).distance(TdCell::new(3, 4)), 5);
    }

    #[test]
    fn facings() {
        // The original lands one step short of due east and due south; that is preserved.
        assert_eq!(desired_facing256(0, 0, 0, -100), Dir::N);
        assert_eq!(desired_facing256(0, 0, 100, 0), Dir(63));
        assert_eq!(desired_facing256(0, 0, 0, 100), Dir(127));
        assert_eq!(desired_facing256(0, 0, -100, 0), Dir::W);
        assert_eq!(desired_facing256(0, 0, 100, -100), Dir(32));
        assert_eq!(desired_facing256(5, 5, 5, 5), Dir(255));

        for (dx, dy, f) in [
            (0, -10, Facing::N),
            (10, -10, Facing::NE),
            (10, 0, Facing::E),
            (10, 10, Facing::SE),
            (0, 10, Facing::S),
            (-10, 10, Facing::SW),
            (-10, 0, Facing::W),
            (-10, -10, Facing::NW),
        ] {
            assert_eq!(desired_facing8(0, 0, dx, dy), f.dir(), "{f:?}");
            assert_eq!(desired_facing256(0, 0, dx, dy).facing(), f, "{f:?}");
        }
    }

    #[test]
    fn dir_helpers() {
        assert_eq!(Dir(250).facing(), Facing::N);
        assert_eq!(Dir(250).turn_to(Dir(5)), 11);
        assert_eq!(Dir(5).turn_to(Dir(250)), -11);
        assert_eq!(Dir(100).legacy_diff(Dir(200)), -156);
        assert_eq!(Facing::NW.rotate(2), Facing::NE);
    }
}
