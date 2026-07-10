use std::ops::Add;

use super::Point;

/// A signed 2D point in integer cell coordinates.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct PointI32 {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
}

impl PointI32 {
    /// Construct a new signed point.
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Return the origin point.
    pub fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    /// Return true when both coordinates are zero.
    pub fn is_zero(&self) -> bool {
        self.x == 0 && self.y == 0
    }
}

impl Add for PointI32 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x.saturating_add(other.x),
            y: self.y.saturating_add(other.y),
        }
    }
}

impl From<(i32, i32)> for PointI32 {
    #[inline]
    fn from(v: (i32, i32)) -> Self {
        Self { x: v.0, y: v.1 }
    }
}

impl From<Point> for PointI32 {
    fn from(p: Point) -> Self {
        Self {
            x: i32::try_from(p.x).unwrap_or(i32::MAX),
            y: i32::try_from(p.y).unwrap_or(i32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_conversion_and_addition_clamp() {
        assert_eq!(PointI32::from(Point { x: u32::MAX, y: 1 }).x, i32::MAX);
        assert_eq!(
            PointI32::new(i32::MAX, i32::MIN) + PointI32::new(1, -1),
            PointI32::new(i32::MAX, i32::MIN)
        );
    }
}
