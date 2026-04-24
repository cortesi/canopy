use super::{Point, Rect};

/// Size with width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size<T = u32> {
    /// Width component.
    pub w: T,
    /// Height component.
    pub h: T,
}

impl<T> Size<T> {
    /// Create a new size with the given width and height.
    pub fn new(w: T, h: T) -> Self {
        Self { w, h }
    }
}

impl Size<u32> {
    /// Zero size.
    pub const ZERO: Self = Self { w: 0, h: 0 };

    /// The area of this expanse.
    pub fn area(&self) -> u32 {
        self.w.saturating_mul(self.h)
    }

    /// Return a `Rect` with the same dimensions as the `Size`, but a location at (0, 0).
    pub fn rect(&self) -> Rect {
        Rect {
            tl: Point::default(),
            w: self.w,
            h: self.h,
        }
    }
    /// True if this Size can completely enclose the target size in both dimensions.
    pub fn contains(&self, other: &Self) -> bool {
        self.w >= other.w && self.h >= other.h
    }
}

impl From<Rect> for Size<u32> {
    fn from(r: Rect) -> Self {
        Self { w: r.w, h: r.h }
    }
}

impl From<(u32, u32)> for Size<u32> {
    fn from(v: (u32, u32)) -> Self {
        Self { w: v.0, h: v.1 }
    }
}
