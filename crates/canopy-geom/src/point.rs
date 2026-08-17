use std::ops::Add;

/// A 2D point in integer cell coordinates.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Point {
    /// X coordinate.
    pub x: u32,
    /// Y coordinate.
    pub y: u32,
}

impl Point {
    /// Return the origin point.
    pub fn zero() -> Self {
        (0, 0).into()
    }
    /// Return true when both coordinates are zero.
    pub fn is_zero(&self) -> bool {
        self.x == 0 && self.y == 0
    }
    /// Shift the point by an offset, avoiding under- or overflow.
    pub fn scroll(&self, x: i32, y: i32) -> Self {
        let nx = if x < 0 {
            self.x.saturating_sub(x.unsigned_abs())
        } else {
            self.x.saturating_add(x.unsigned_abs())
        };
        let ny = if y < 0 {
            self.y.saturating_sub(y.unsigned_abs())
        } else {
            self.y.saturating_add(y.unsigned_abs())
        };
        (nx, ny).into()
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x.saturating_add(other.x),
            y: self.y.saturating_add(other.y),
        }
    }
}

impl From<(u32, u32)> for Point {
    #[inline]
    fn from(v: (u32, u32)) -> Self {
        Self { x: v.0, y: v.1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn add() -> Result<()> {
        assert_eq!(Point::zero() + (1u32, 1u32).into(), (1u32, 1u32).into());
        assert_eq!(Point::zero() + (1u32, 0u32).into(), (1u32, 0u32).into());
        assert_eq!(Point::zero() + (0u32, 1u32).into(), (0u32, 1u32).into());
        Ok(())
    }

    #[test]
    fn addition_saturates() {
        assert_eq!(
            Point {
                x: u32::MAX,
                y: u32::MAX - 1,
            } + Point { x: 1, y: 2 },
            Point {
                x: u32::MAX,
                y: u32::MAX,
            }
        );
    }
}
