//! Cell addressing and per-cell storage (`Model/CellMetrics.cs`, `Model/CellGrid.cs`).
//!
//! The C# grid raised `CellChanged` events on every write; here [`CellGrid::set`] returns
//! the previous value instead, so callers (undo stacks, renderers) decide what to do.

use cnc_core::geometry::Facing;

/// A cell position. Signed so that positions just off the map can be represented and
/// rejected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Point {
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
}

impl Point {
    /// Creates a point.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The neighbouring point in a direction (which may lie off the map).
    #[must_use]
    pub const fn step(self, facing: Facing) -> Self {
        let (dx, dy) = facing.offset();
        Self::new(self.x + dx, self.y + dy)
    }
}

/// The dimensions of a map and conversions between points and cell indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellMetrics {
    width: u16,
    height: u16,
}

impl CellMetrics {
    /// Creates metrics for a `width` x `height` map.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// Width in cells.
    #[must_use]
    pub const fn width(self) -> u16 {
        self.width
    }

    /// Height in cells.
    #[must_use]
    pub const fn height(self) -> u16 {
        self.height
    }

    /// Total number of cells.
    #[must_use]
    pub const fn len(self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Whether the map has no cells.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Whether a point lies on the map.
    #[must_use]
    pub const fn contains(self, p: Point) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width as i32 && p.y < self.height as i32
    }

    /// The cell index of a point, if it lies on the map.
    #[must_use]
    pub const fn cell(self, p: Point) -> Option<usize> {
        if self.contains(p) {
            Some(p.y as usize * self.width as usize + p.x as usize)
        } else {
            None
        }
    }

    /// The point of a cell index, if it lies on the map.
    #[must_use]
    pub const fn location(self, cell: usize) -> Option<Point> {
        if cell < self.len() {
            let w = self.width as usize;
            Some(Point::new((cell % w) as i32, (cell / w) as i32))
        } else {
            None
        }
    }

    /// The neighbouring cell index in a direction, if it lies on the map.
    #[must_use]
    pub fn adjacent(self, cell: usize, facing: Facing) -> Option<usize> {
        self.cell(self.location(cell)?.step(facing))
    }

    /// Clamps a point onto the map, keeping the given margins clear (`Clip`).
    #[must_use]
    pub fn clip(self, p: Point, top_left: Point, bottom_right: Point) -> Point {
        let max_x = i32::from(self.width) - bottom_right.x - 1;
        let max_y = i32::from(self.height) - bottom_right.y - 1;
        Point::new(
            p.x.min(max_x).max(top_left.x),
            p.y.min(max_y).max(top_left.y),
        )
    }
}

/// One optional value per cell of a map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellGrid<T> {
    metrics: CellMetrics,
    cells: Vec<Option<T>>,
}

impl<T> CellGrid<T> {
    /// An empty grid.
    #[must_use]
    pub fn new(metrics: CellMetrics) -> Self {
        Self {
            metrics,
            cells: std::iter::repeat_with(|| None)
                .take(metrics.len())
                .collect(),
        }
    }

    /// The grid's metrics.
    #[must_use]
    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    /// The value at a cell index.
    #[must_use]
    pub fn get(&self, cell: usize) -> Option<&T> {
        self.cells.get(cell)?.as_ref()
    }

    /// The value at a point.
    #[must_use]
    pub fn get_at(&self, p: Point) -> Option<&T> {
        self.get(self.metrics.cell(p)?)
    }

    /// Stores a value (or clears the cell with `None`), returning the previous value.
    /// Writes outside the map are ignored and return `None`.
    pub fn set(&mut self, cell: usize, value: Option<T>) -> Option<T> {
        let slot = self.cells.get_mut(cell)?;
        std::mem::replace(slot, value)
    }

    /// [`CellGrid::set`] by point.
    pub fn set_at(&mut self, p: Point, value: Option<T>) -> Option<T> {
        let cell = self.metrics.cell(p)?;
        self.set(cell, value)
    }

    /// The value in the neighbouring cell.
    #[must_use]
    pub fn adjacent(&self, cell: usize, facing: Facing) -> Option<&T> {
        self.get(self.metrics.adjacent(cell, facing)?)
    }

    /// Empties every cell.
    pub fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = None);
    }

    /// Iterates over occupied cells as `(index, value)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|v| (i, v)))
    }

    /// The occupied cells among `cells` (`IntersectsWith`).
    pub fn intersecting<'a>(
        &'a self,
        cells: impl IntoIterator<Item = usize> + 'a,
    ) -> impl Iterator<Item = (usize, &'a T)> + 'a {
        cells
            .into_iter()
            .filter_map(move |i| self.get(i).map(|v| (i, v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics() {
        let m = CellMetrics::new(64, 64);
        assert_eq!(m.cell(Point::new(3, 2)), Some(131));
        assert_eq!(m.location(131), Some(Point::new(3, 2)));
        assert_eq!(m.cell(Point::new(-1, 0)), None);
        assert_eq!(m.location(4096), None);
        assert_eq!(m.adjacent(0, Facing::N), None);
        assert_eq!(m.adjacent(0, Facing::SE), Some(65));
        assert_eq!(
            m.clip(Point::new(-5, 99), Point::new(1, 1), Point::new(1, 1)),
            Point::new(1, 62)
        );
    }

    #[test]
    fn grid() {
        let mut g = CellGrid::new(CellMetrics::new(4, 4));
        assert_eq!(g.set(5, Some('a')), None);
        assert_eq!(g.set(5, Some('b')), Some('a'));
        assert_eq!(g.set(99, Some('z')), None);
        assert_eq!(g.get_at(Point::new(1, 1)), Some(&'b'));
        assert_eq!(g.adjacent(1, Facing::S), Some(&'b'));
        assert_eq!(g.iter().collect::<Vec<_>>(), vec![(5, &'b')]);
        assert_eq!(g.intersecting([0, 5, 6]).count(), 1);
        g.clear();
        assert_eq!(g.iter().count(), 0);
    }
}
