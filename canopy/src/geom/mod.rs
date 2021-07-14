mod extent;
mod point;
mod rect;

pub use extent::Extent;
pub use point::Point;
pub use rect::{Frame, Rect};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
