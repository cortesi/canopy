use std::ops::Add;

use crate::geom;

/// Cursor glyph shape variants.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum CursorShape {
    /// Underscore cursor.
    Underscore,
    /// Vertical bar cursor.
    Line,
    /// Block cursor.
    Block,
}

/// Cursor position, shape, and blink behavior.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Cursor {
    /// Location of the cursor, relative to (0, 0) in the node view rect.
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
