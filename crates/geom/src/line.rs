use super::{Point, Rect};

/// A horizontal line, one character high - essentially a Rect with height 1.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Line {
    pub tl: Point,
    pub w: u32,
}

impl Default for Line {
    /// Constructs a zero-valued size.
    fn default() -> Line {
        Line {
            tl: Point::default(),
            w: 0,
        }
    }
}

impl Line {
    pub fn new(x: u32, y: u32, w: u32) -> Line {
        Line {
            tl: Point { x, y },
            w,
        }
    }
    pub fn rect(&self) -> Rect {
        Rect {
            tl: self.tl,
            w: self.w,
            h: 1,
        }
    }
}
