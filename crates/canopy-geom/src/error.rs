use std::result::Result as StdResult;

use thiserror::Error;

use crate::{LineSegment, Rect};

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
    /// A line extent lies outside the requested rectangle axis.
    #[error("extent {extent:?} lies outside rectangle {rect:?}")]
    ExtentOutsideRect {
        /// Rejected extent.
        extent: LineSegment,
        /// Rectangle used as the bound.
        rect: Rect,
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
        let rect = Rect::new(0, 0, 4, 4);
        let extent = LineSegment { off: 2, len: 8 };
        let error = rect
            .hslice(extent)
            .expect_err("extent exceeds the rectangle");

        assert_eq!(error, Error::ExtentOutsideRect { extent, rect });
    }
}
