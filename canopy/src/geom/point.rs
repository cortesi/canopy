use super::Rect;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

impl Point {
    pub fn zero() -> Self {
        Point { x: 0, y: 0 }
    }
    /// Shift the point by an offset, avoiding under- or overflow.
    pub fn scroll(&self, x: i16, y: i16) -> Self {
        let nx = if x < 0 {
            self.x.saturating_sub(x.abs() as u16)
        } else {
            self.x.saturating_add(x.abs() as u16)
        };
        let ny = if y < 0 {
            self.y.saturating_sub(y.abs() as u16)
        } else {
            self.y.saturating_add(y.abs() as u16)
        };
        Point { x: nx, y: ny }
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
            self.x.saturating_sub(x.abs() as u16)
        } else {
            self.x.saturating_add(x.abs() as u16)
        };
        let ny = if y < 0 {
            self.y.saturating_sub(y.abs() as u16)
        } else {
            self.y.saturating_add(y.abs() as u16)
        };
        Point { x: nx, y: ny }.clamp(rect)
    }
}
