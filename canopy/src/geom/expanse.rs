use super::{Point, Rect};

/// An `Expanse` is a rectangle that has a width and height but no location.
/// This is useful when we want to deal with `Rect`s abstractly, or when we want
/// to madate that the location of a `Rect` is (0, 0).
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Expanse {
    pub w: u16,
    pub h: u16,
}

impl Default for Expanse {
    /// Constructs a zero-valued size.
    fn default() -> Expanse {
        Expanse { w: 0, h: 0 }
    }
}

impl Expanse {
    pub fn new(w: u16, h: u16) -> Expanse {
        Expanse { w, h }
    }

    /// The area of this expanse.
    pub fn area(&self) -> u32 {
        self.w as u32 * self.h as u32
    }

    /// Return a `Rect` with the same dimensions as the `Expanse`, but a location at (0, 0).
    pub fn rect(&self) -> Rect {
        Rect {
            tl: Point::default(),
            w: self.w,
            h: self.h,
        }
    }
    /// True if this Size can completely enclose the target size in both dimensions.
    pub fn contains(&self, other: &Expanse) -> bool {
        self.w >= other.w && self.h >= other.h
    }
}

impl From<Rect> for Expanse {
    fn from(r: Rect) -> Expanse {
        Expanse { w: r.w, h: r.h }
    }
}
