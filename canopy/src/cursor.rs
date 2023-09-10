use std::ops::Add;

use crate::geom;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum CursorShape {
    Underscore,
    Line,
    Block,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Cursor {
    /// Location of the cursor, relative to the node's origin.
    pub location: geom::Point,
    /// Shape of the cursor.
    pub shape: CursorShape,
    /// Should the cursor blink?
    pub blink: bool,
}

impl Add<geom::Point> for Cursor {
    type Output = Self;

    fn add(self, other: geom::Point) -> Self {
        Self {
            location: self.location + other,
            shape: self.shape,
            blink: self.blink,
        }
    }
}
