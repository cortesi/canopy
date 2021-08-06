use super::{Point, Rect};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Size {
    pub w: u16,
    pub h: u16,
}

impl Default for Size {
    /// Constructs a zero-valued size.
    fn default() -> Size {
        Size { w: 0, h: 0 }
    }
}

impl Size {
    pub fn new(w: u16, h: u16) -> Size {
        Size { w, h }
    }
    pub fn rect(&self) -> Rect {
        Rect {
            tl: Point::default(),
            w: self.w,
            h: self.h,
        }
    }
    /// True if this Size can completely enclose the target size in both dimensions.
    pub fn contains(&self, other: &Size) -> bool {
        self.w >= other.w && self.h >= other.h
    }
}

impl From<Rect> for Size {
    fn from(r: Rect) -> Size {
        Size { w: r.w, h: r.h }
    }
}
