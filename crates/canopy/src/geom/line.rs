use super::{Point, Rect};

/// A horizontal line, one character high - essentially a Rect with height 1.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Line {
    /// Top-left point for the line.
    pub tl: Point,
    /// Width in cells.
    pub w: u32,
}

impl Default for Line {
    /// Constructs a zero-valued size.
    fn default() -> Self {
        Self {
            tl: Point::default(),
            w: 0,
        }
    }
}

impl Line {
    /// Construct a line from coordinates and width.
    pub fn new(x: u32, y: u32, w: u32) -> Self {
        Self {
            tl: Point { x, y },
            w,
        }
    }
    /// Convert the line into a rectangle of height 1.
    pub fn rect(&self) -> Rect {
        Rect {
            tl: self.tl,
            w: self.w,
            h: 1,
        }
    }
}
