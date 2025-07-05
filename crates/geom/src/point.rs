use std::ops::Add;

use super::Rect;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

impl Point {
    pub fn zero() -> Self {
        (0, 0).into()
    }
    pub fn is_zero(&self) -> bool {
        self.x == 0 && self.y == 0
    }
    /// Shift the point by an offset, avoiding under- or overflow.
    pub fn scroll(&self, x: i16, y: i16) -> Self {
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
    /// Clamp a point, constraining it to fall within `rect`.
    pub fn clamp(&self, rect: Rect) -> Self {
        Point {
            x: self.x.clamp(rect.tl.x, rect.tl.x + rect.w),
            y: self.y.clamp(rect.tl.y, rect.tl.y + rect.h),
        }
    }
    /// Like scroll, but constrained within a rectangle.
    pub fn scroll_within(&self, x: i16, y: i16, rect: Rect) -> Self {
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
        Point { x: nx, y: ny }.clamp(rect)
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl From<(u16, u16)> for Point {
    #[inline]
    fn from(v: (u16, u16)) -> Point {
        Point { x: v.0, y: v.1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn add() -> Result<()> {
        assert_eq!(Point::zero() + (1, 1).into(), (1, 1).into());
        assert_eq!(Point::zero() + (1, 0).into(), (1, 0).into());
        assert_eq!(Point::zero() + (0, 1).into(), (0, 1).into());
        Ok(())
    }
}
