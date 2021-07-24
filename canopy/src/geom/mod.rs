mod frame;
mod line;
mod linesegment;
mod point;
mod rect;
mod size;
mod viewport;

pub use frame::Frame;
pub use line::Line;
pub use linesegment::LineSegment;
pub use point::Point;
pub use rect::Rect;
pub use size::Size;
pub use viewport::ViewPort;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
