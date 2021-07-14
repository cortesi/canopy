mod extent;
mod frame;
mod point;
mod rect;

pub use extent::Extent;
pub use frame::Frame;
pub use point::Point;
pub use rect::Rect;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
