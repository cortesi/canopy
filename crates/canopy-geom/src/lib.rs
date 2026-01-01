//! Geometry primitives used across canopy.

#![warn(missing_docs)]

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
/// Signed point helpers.
mod point_i32;
/// Rectangle operations.
mod rect;
/// Signed rectangle operations.
mod rect_i32;

pub use error::{Error, Result};
pub use expanse::Expanse;
pub use frame::FrameRects;
pub use line::Line;
pub use linesegment::LineSegment;
pub use point::Point;
pub use point_i32::PointI32;
pub use rect::Rect;
pub use rect_i32::RectI32;

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
