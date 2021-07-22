mod frame;
mod linesegment;
mod point;
mod rect;
mod viewport;

pub use frame::Frame;
pub use linesegment::LineSegment;
pub use point::Point;
pub use viewport::ViewPort;

pub use rect::Rect;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
