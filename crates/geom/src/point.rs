use std::ops::Add;

use super::Rect;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl Point {
    pub fn zero() -> Self {
        (0, 0).into()
    }
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
    /// Clamp a point, constraining it to fall within `rect`.
    pub fn clamp(&self, rect: Rect) -> Self {
        Self {
            x: self.x.clamp(rect.tl.x, rect.tl.x + rect.w),
            y: self.y.clamp(rect.tl.y, rect.tl.y + rect.h),
        }
    }
    /// Like scroll, but constrained within a rectangle.
    pub fn scroll_within(&self, x: i32, y: i32, rect: Rect) -> Self {
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
        Self { x: nx, y: ny }.clamp(rect)
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
}
