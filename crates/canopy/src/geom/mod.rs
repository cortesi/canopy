//! Geometry primitives used across canopy.

/// Error types for geometry operations.
mod error;
/// Width/height size type.
mod expanse;
/// Frame and padding helpers.
mod frame;
/// Horizontal line helpers.
mod line;
/// Line segment operations.
mod linesegment;
/// Point helpers.
mod point;
/// Rectangle operations.
mod rect;

pub use error::{Error, Result};
pub use expanse::Expanse;
pub use frame::Frame;
pub use line::Line;
pub use linesegment::LineSegment;
pub use point::Point;
pub use rect::Rect;

/// Cardinal directions.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    /// Upward direction.
    Up,
    /// Downward direction.
    Down,
    /// Leftward direction.
    Left,
    /// Rightward direction.
    Right,
}
