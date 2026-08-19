use super::{Point, Rect};

/// Size with width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    /// Width component.
    pub w: u32,
    /// Height component.
    pub h: u32,
}

impl Size {
    /// Zero size.
    pub const ZERO: Self = Self { w: 0, h: 0 };

    /// Create a new size with the given width and height.
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }

    /// Return a `Rect` with the same dimensions as the `Size`, but a location at (0, 0).
    pub fn rect(&self) -> Rect {
        Rect {
            tl: Point::default(),
            w: self.w,
            h: self.h,
        }
    }
}

impl From<(u32, u32)> for Size {
    fn from(v: (u32, u32)) -> Self {
        Self { w: v.0, h: v.1 }
    }
}
