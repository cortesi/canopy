use std::result::Result as StdResult;

use thiserror::Error;

use crate::{LineSegment, Point, Rect};

/// Geometry error type.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum Error {
    /// A zero-length window cannot be projected into a track.
    #[error("window cannot be zero length")]
    ZeroLengthWindow,
    /// A window lies outside the view used to project it.
    #[error("view {view:?} does not contain window {window:?}")]
    WindowOutsideView {
        /// Window extent.
        window: LineSegment,
        /// Enclosing view extent.
        view: LineSegment,
    },
    /// A rectangle cannot fit within a smaller target rectangle.
    #[error("rectangle {rect:?} cannot fit within {target:?}")]
    ClampTargetTooSmall {
        /// Rectangle being moved.
        rect: Rect,
        /// Requested enclosing rectangle.
        target: Rect,
    },
    /// A line extent lies outside the requested rectangle axis.
    #[error("extent {extent:?} lies outside rectangle {rect:?}")]
    ExtentOutsideRect {
        /// Rejected extent.
        extent: LineSegment,
        /// Rectangle used as the bound.
        rect: Rect,
    },
    /// A point cannot be rebased because it lies outside the rectangle.
    #[error("point {point:?} lies outside rectangle {rect:?}")]
    PointOutsideRect {
        /// Rejected point.
        point: Point,
        /// Enclosing rectangle.
        rect: Rect,
    },
    /// A rectangle cannot be rebased because it is not contained.
    #[error("rectangle {inner:?} is not contained by {outer:?}")]
    RectOutsideRect {
        /// Rejected inner rectangle.
        inner: Rect,
        /// Expected outer rectangle.
        outer: Rect,
    },
    /// A pane column count cannot be represented by the geometry model.
    #[error("pane column count {count} exceeds u32")]
    PaneColumnCountOverflow {
        /// Requested number of columns.
        count: usize,
    },
    /// A line offset lies outside a rectangle's height.
    #[error("line offset {offset} exceeds rectangle height {height}")]
    LineOffsetOutside {
        /// Requested line offset.
        offset: u32,
        /// Rectangle height.
        height: u32,
    },
    /// A split requested zero sections.
    #[error("cannot split a length into zero sections")]
    ZeroSections,
}

/// Result type for geometry operations.
pub type Result<T> = StdResult<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_errors_expose_rejected_operands() {
        let rect = Rect::new(1, 2, 3, 4);
        let target = Rect::new(0, 0, 2, 2);
        let error = rect.clamp_within(target).expect_err("target is too small");

        assert_eq!(error, Error::ClampTargetTooSmall { rect, target });
    }
}
