/// A 2D point in integer cell coordinates.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Point {
    /// X coordinate.
    pub x: u32,
    /// Y coordinate.
    pub y: u32,
}

impl Point {
    /// Origin point.
    pub const ZERO: Self = Self { x: 0, y: 0 };

    /// Construct a point from its coordinates.
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
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

impl From<(u32, u32)> for Point {
    #[inline]
    fn from(v: (u32, u32)) -> Self {
        Self { x: v.0, y: v.1 }
    }
}
