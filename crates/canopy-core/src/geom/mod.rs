mod expanse;
mod frame;
mod line;
mod linesegment;
mod point;
mod rect;

pub use expanse::Expanse;
pub use frame::Frame;
pub use line::Line;
pub use linesegment::LineSegment;
pub use point::Point;
pub use rect::Rect;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
