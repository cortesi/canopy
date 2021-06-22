use crate::geom;

pub enum CursorShape {
    Underscore,
    Line,
    Block,
}

pub struct Cursor {
    /// Location of the cursor, relative to the node's origin.
    pub location: geom::Point,
    /// Shape of the cursor.
    pub shape: CursorShape,
    /// Should the cursor blink?
    pub blink: bool,
}
