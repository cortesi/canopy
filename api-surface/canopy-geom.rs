// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy-geom, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_geom {
    //! Geometry primitives used across canopy.
    //!
    //! Rectangles and line segments use half-open bounds: their near edge is
    //! included and their far edge is excluded. A rectangle is empty when either
    //! dimension is zero, contains no points, and never intersects another
    //! rectangle. Empty rectangles retain an anchor: one is contained by another
    //! rectangle when all of its (coincident) edges lie within the containing
    //! rectangle's closed edge bounds.
    //!
    //! Unsigned coordinates and sizes use saturating arithmetic unless an
    //! operation is explicitly fallible. Edge calculations widen before adding so
    //! rectangles extending beyond `u32::MAX` retain their full mathematical
    //! extent. Conversions to signed coordinates clamp values that cannot be
    //! represented.

    /// Geometry error type.
    #[derive(Debug, Clone, Error, Display, StructuralPartialEq, PartialEq, Eq)]
    pub enum Error {
        /// A zero-length window cannot be projected into a track.
        ZeroLengthWindow,
        /// A window lies outside the view used to project it.
        WindowOutsideView {
            /// Window extent.
            window: crate::LineSegment,
            /// Enclosing view extent.
            view: crate::LineSegment,
        },
        /// A rectangle cannot fit within a smaller target rectangle.
        ClampTargetTooSmall {
            /// Rectangle being moved.
            rect: crate::Rect,
            /// Requested enclosing rectangle.
            target: crate::Rect,
        },
        /// A line extent lies outside the requested rectangle axis.
        ExtentOutsideRect {
            /// Rejected extent.
            extent: crate::LineSegment,
            /// Rectangle used as the bound.
            rect: crate::Rect,
        },
        /// A point cannot be rebased because it lies outside the rectangle.
        PointOutsideRect {
            /// Rejected point.
            point: crate::Point,
            /// Enclosing rectangle.
            rect: crate::Rect,
        },
        /// A rectangle cannot be rebased because it is not contained.
        RectOutsideRect {
            /// Rejected inner rectangle.
            inner: crate::Rect,
            /// Expected outer rectangle.
            outer: crate::Rect,
        },
        /// A pane column count cannot be represented by the geometry model.
        PaneColumnCountOverflow {
            /// Requested number of columns.
            count: usize,
        },
        /// A line offset lies outside a rectangle's height.
        LineOffsetOutside {
            /// Requested line offset.
            offset: u32,
            /// Rectangle height.
            height: u32,
        },
        /// A split requested zero sections.
        ZeroSections,
    }

    /// Result type for geometry operations.
    pub type Result<T> = std::result::Result<T, Error>;

    /// A frame's border regions extracted from a rectangle.
    ///
    /// This struct represents the decomposition of a rectangle into its border
    /// regions: top, bottom, left, right, and corner rectangles. It's useful for
    /// drawing box borders or frame decorations.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq)]
    pub struct FrameRects {
        /// The top of the frame, not including corners
        pub top: super::Rect,
        /// The bottom of the frame, not including corners
        pub bottom: super::Rect,
        /// The left of the frame, not including corners
        pub left: super::Rect,
        /// The right of the frame, not including corners
        pub right: super::Rect,
        /// The top left corner
        pub topleft: super::Rect,
        /// The top right corner
        pub topright: super::Rect,
        /// The bottom left corner
        pub bottomleft: super::Rect,
        /// The bottom right corner
        pub bottomright: super::Rect,
    }

    impl FrameRects {
        /// Construct a new frame. If the rect is too small to fit the specified
        /// frame, we return a zero FrameRects.
        pub fn new(rect: Rect, border: u32) -> Self {}

        /// Get the inner rect of the frame (the space inside the frame)
        pub fn inner(&self) -> Rect {}

        /// Get the outer rect of the frame (the original rect passed to FrameRects::new())
        pub fn outer(&self) -> Rect {}

        /// Return a zero-sized frame.
        pub fn zero() -> Self {}
    }

    /// A horizontal line, one character high - essentially a Rect with height 1.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct Line {
        /// Top-left point for the line.
        pub tl: super::Point,
        /// Width in cells.
        pub w: u32,
    }

    impl Line {
        /// Construct a line from coordinates and width.
        pub fn new(x: u32, y: u32, w: u32) -> Self {}

        /// Convert the line into a rectangle of height 1.
        pub fn rect(&self) -> Rect {}
    }

    impl From<Line> for Rect {
        fn from(l: Line) -> Self {}
    }

    /// A half-open, directionless one-dimensional line segment.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq)]
    pub struct LineSegment {
        /// The offset of this extent.
        pub off: u32,
        /// The length of this extent.
        pub len: u32,
    }

    impl LineSegment {
        /// The exclusive far edge of the extent using widened arithmetic.
        pub fn end(&self) -> u64 {}

        /// Return a segment starting at the nearer input and extending toward the
        /// farther input.
        ///
        /// The length saturates when the complete span exceeds `u32::MAX`.
        pub fn saturating_enclose(&self, other: &Self) -> Self {}

        /// Carve off a fixed-size portion from the start of this LineSegment,
        /// returning a (head, tail) tuple. If the segment is too short to carve out
        /// the width specified, the length of the head will be zero.
        pub fn carve_start(&self, n: u32) -> (Self, Self) {}

        /// Carve off a fixed-size portion from the end of this LineSegment,
        /// returning a (head, tail) tuple. If the segment is too short to carve out
        /// the width specified, the length of the tail will be zero.
        pub fn carve_end(&self, n: u32) -> (Self, Self) {}

        /// Are these two line segments adjacent but non-overlapping?
        pub fn abuts(&self, other: &Self) -> bool {}

        /// Does other lie completely within this extent.
        pub fn contains(&self, other: &Self) -> bool {}

        /// Return true if the two segments overlap.
        pub fn intersects(&self, other: &Self) -> bool {}

        /// Return the intersection between this line segment and other. The line
        /// segment returned will always have a non-zero length.
        pub fn intersection(&self, other: &Self) -> Option<Self> {}

        /// Split this extent into (pre, active, post) extents, based on the
        /// position of a window within a view. The main use for this function is
        /// computation of the active indicator size and position in a scrollbar.
        pub fn split_active(&self, window: Self, view: Self) -> Result<(Self, Self, Self)> {}
    }

    /// A 2D point in integer cell coordinates.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct Point {
        /// X coordinate.
        pub x: u32,
        /// Y coordinate.
        pub y: u32,
    }

    impl Point {
        /// Return the origin point.
        pub fn zero() -> Self {}

        /// Return true when both coordinates are zero.
        pub fn is_zero(&self) -> bool {}

        /// Shift the point by an offset, avoiding under- or overflow.
        pub fn scroll(&self, x: i32, y: i32) -> Self {}

        /// Clamp a point to the cells in `rect`.
        ///
        /// An empty rectangle contains no point, so its origin is returned as the
        /// canonical clamped value.
        pub fn clamp(&self, rect: Rect) -> Self {}

        /// Like scroll, but constrained within a rectangle.
        pub fn scroll_within(&self, x: i32, y: i32, rect: Rect) -> Self {}
    }

    impl Add for Point {
        type Output = Point;
        fn add(self, other: Self) -> Self {}
    }

    impl From<(u32, u32)> for Point {
        fn from(v: (u32, u32)) -> Self {}
    }

    impl From<Point> for PointI32 {
        fn from(p: Point) -> Self {}
    }

    /// A signed 2D point in integer cell coordinates.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct PointI32 {
        /// X coordinate.
        pub x: i32,
        /// Y coordinate.
        pub y: i32,
    }

    impl PointI32 {
        /// Construct a new signed point.
        pub fn new(x: i32, y: i32) -> Self {}

        /// Return the origin point.
        pub fn zero() -> Self {}

        /// Return true when both coordinates are zero.
        pub fn is_zero(&self) -> bool {}
    }

    impl Add for PointI32 {
        type Output = PointI32;
        fn add(self, other: Self) -> Self {}
    }

    impl From<(i32, i32)> for PointI32 {
        fn from(v: (i32, i32)) -> Self {}
    }

    impl From<Point> for PointI32 {
        fn from(p: Point) -> Self {}
    }

    /// A half-open rectangle with an unsigned origin and size.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct Rect {
        /// Top-left corner
        pub tl: super::Point,
        /// Width
        pub w: u32,
        /// Height
        pub h: u32,
    }

    impl Rect {
        /// Construct a rectangle from coordinates and size.
        pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {}

        /// The width times the height of the rectangle, saturated to `u32::MAX`.
        pub fn area(&self) -> u32 {}

        /// Create a zero-sized `Rect` at the origin.
        pub fn zero() -> Self {}

        /// Return a rect with the same size, with the top left at the given point.
        pub fn at(&self, p: impl Into<Point>) -> Self {}

        /// Carve a rectangle with a fixed width out of the start of the horizontal
        /// extent of this rect. Returns a [left, right] array. Left is either
        /// empty or has the extract width specified.
        pub fn carve_hstart(&self, width: u32) -> (Self, Self) {}

        /// Carve a rectangle with a fixed width out of the end of the horizontal
        /// extent of this rect. Returns a [left, right] array. Right is either
        /// empty or has the exact width specified.
        pub fn carve_hend(&self, width: u32) -> (Self, Self) {}

        /// Carve a rectangle with a fixed height out of the start of the vertical
        /// extent of this rect. Returns a [top, bottom] array. Top is either empty
        /// or has the exact height specified.
        pub fn carve_vstart(&self, height: u32) -> (Self, Self) {}

        /// Carve a rectangle with a fixed height out of the end of the vertical
        /// extent of this rect. Returns a [top, bottom] array. Bottom is either
        /// empty or has the exact height specified.
        pub fn carve_vend(&self, height: u32) -> (Self, Self) {}

        /// Clamp this rectangle, shifting it to lie within another rectangle. The
        /// size of the returned Rect is always equal to that of self. If self is
        /// larger than the enclosing rectangle, return an error.
        pub fn clamp_within(&self, rect: impl Into<Self>) -> Result<Self> {}

        /// Return the exclusive right edge using widened arithmetic.
        pub fn right(&self) -> u64 {}

        /// Return the exclusive bottom edge using widened arithmetic.
        pub fn bottom(&self) -> u64 {}

        /// Does this half-open rectangle contain the point?
        pub fn contains_point(&self, p: impl Into<Point>) -> bool {}

        /// Does this rectangle completely enclose the other's half-open bounds?
        ///
        /// Empty rectangles are treated as anchored bounds. They are contained
        /// when their coincident edges fall within this rectangle's closed edge
        /// bounds, including the far edge.
        pub fn contains_rect(&self, other: &Self) -> bool {}

        /// Extracts an inner rectangle, given a border width. If the border width
        /// would exceed the size of the Rect, we return a zero rect.
        pub fn inner(&self, border: u32) -> Self {}

        /// Extract a horizontal section of this rect based on an extent.
        pub fn hslice(&self, e: &LineSegment) -> Result<Self> {}

        /// The horizontal extent of this rect.
        pub fn hextent(&self) -> LineSegment {}

        /// Calculate the intersection of this rectangle and another.
        pub fn intersect(&self, other: &Self) -> Option<Self> {}

        /// Given a point that falls within this rectangle, shift the point to be
        /// relative to our origin. If the point falls outside the rect, an error is
        /// returned.
        pub fn rebase_point(&self, pt: impl Into<Point>) -> Result<Point> {}

        /// Given a rectangle contained within this rectangle, shift the inner
        /// rectangle to be relative to our origin. If the rect is not entirely
        /// contained, an error is returned.
        pub fn rebase_rect(&self, other: &Self) -> Result<Self> {}

        /// A safe function for shifting the rectangle by an offset, which won't
        /// under- or overflow.
        pub fn shift(&self, x: i32, y: i32) -> Self {}

        /// Shift this rectangle, constrained to be within another rectangle. The
        /// size of the returned Rect is always equal to that of self. If self is
        /// larger than the enclosing rectangle, self unchanged.
        pub fn shift_within(&self, x: i32, y: i32, rect: Self) -> Self {}

        /// Splits the rectangle horizontally into n sections, as close to equally
        /// sized as possible.
        pub fn split_horizontal(&self, n: u32) -> Result<Vec<Self>> {}

        /// Splits the rectangle vertically into n sections, as close to equally
        /// sized as possible.
        pub fn split_vertical(&self, n: u32) -> Result<Vec<Self>> {}

        /// Splits the rectangle into columns, with each column split into rows.
        /// Returns a Vec of rects per column.
        pub fn split_panes(&self, spec: &[u32]) -> Result<Vec<Vec<Self>>> {}

        /// Sweeps upwards from the top of the rectangle. Stops once the closure returns true.
        pub fn search_up(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {}

        /// Sweeps downwards from the bottom of the rectangle. Stops once the closure returns true.
        pub fn search_down(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {}

        /// Sweeps leftwards the left of the rectangle. Stops once the closure returns true.
        pub fn search_left(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {}

        /// Sweeps rightwards from the right of the rectangle. Stops once the closure returns true.
        pub fn search_right(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {}

        /// Searches in a given direction sweeping to and fro. Stops once the closure returns true.
        pub fn search(
            &self,
            dir: Direction,
            f: &mut dyn FnMut(Point) -> Result<bool>,
        ) -> Result<()> {
        }

        /// Extract a slice of this rect based on a vertical extent.
        pub fn vslice(&self, e: &LineSegment) -> Result<Self> {}

        /// The vertical extent of this rect.
        pub fn vextent(&self) -> LineSegment {}

        /// Return a line with a given offset in the rectangle.
        pub fn line(&self, off: u32) -> Result<Line> {}

        /// Does this rect have a zero size?
        pub fn is_zero(&self) -> bool {}

        /// Return the `Size` of this rectangle, which has the same size as the
        /// `Rect` but no location.
        pub fn expanse(&self) -> Size {}

        /// Subtract a rectangle from this one, returning a set of rectangles
        /// describing what remains.
        pub fn sub(&self, other: &Self) -> Vec<Self> {}
    }

    impl From<Size> for Rect {
        fn from(s: Size) -> Self {}
    }

    impl From<Line> for Rect {
        fn from(l: Line) -> Self {}
    }

    impl From<(u32, u32, u32, u32)> for Rect {
        fn from(v: (u32, u32, u32, u32)) -> Self {}
    }

    impl From<Rect> for RectI32 {
        fn from(r: Rect) -> Self {}
    }

    impl From<Rect> for Size<u32> {
        fn from(r: Rect) -> Self {}
    }

    /// A half-open rectangle with a signed origin and unsigned size.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct RectI32 {
        /// Top-left corner.
        pub tl: super::PointI32,
        /// Width.
        pub w: u32,
        /// Height.
        pub h: u32,
    }

    impl RectI32 {
        /// Construct a rectangle from coordinates and size.
        pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {}

        /// Does this rect have a zero size?
        pub fn is_zero(&self) -> bool {}

        /// Check if the rectangle contains a point.
        pub fn contains_point(&self, p: super::Point) -> bool {}

        /// Convert a screen point to local coordinates relative to this rect.
        /// If the point is to the left/top of the rect, the result clamps to 0.
        pub fn to_local_point(&self, p: super::Point) -> super::Point {}

        /// Intersect this signed rect with an unsigned rect in the same coordinate space.
        pub fn intersect_rect(&self, other: Rect) -> Option<Rect> {}

        /// Left edge of the rect.
        pub fn left(&self) -> i64 {}

        /// Top edge of the rect.
        pub fn top(&self) -> i64 {}

        /// Right edge of the rect.
        pub fn right(&self) -> i64 {}

        /// Bottom edge of the rect.
        pub fn bottom(&self) -> i64 {}

        /// Center point of the rect.
        pub fn center(&self) -> (i64, i64) {}

        /// Return true if this rect overlaps another vertically.
        pub fn overlaps_vertical(&self, other: Self) -> bool {}

        /// Return true if this rect overlaps another horizontally.
        pub fn overlaps_horizontal(&self, other: Self) -> bool {}
    }

    impl From<Rect> for RectI32 {
        fn from(r: Rect) -> Self {}
    }

    /// Size with width and height.
    #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq, Hash)]
    pub struct Size<T = u32> {
        /// Width component.
        pub w: T,
        /// Height component.
        pub h: T,
    }

    impl<T> Size<T> {
        /// Create a new size with the given width and height.
        pub fn new(w: T, h: T) -> Self {}
    }

    impl Size<u32> {
        /// The area of this expanse.
        pub fn area(&self) -> u32 {}

        /// Return a `Rect` with the same dimensions as the `Size`, but a location at (0, 0).
        pub fn rect(&self) -> Rect {}

        /// True if this Size can completely enclose the target size in both dimensions.
        pub fn contains(&self, other: &Self) -> bool {}
    }

    impl From<Size> for Rect {
        fn from(s: Size) -> Self {}
    }

    impl From<Rect> for Size<u32> {
        fn from(r: Rect) -> Self {}
    }

    impl From<(u32, u32)> for Size<u32> {
        fn from(v: (u32, u32)) -> Self {}
    }

    /// Cardinal directions.
    #[derive(Debug, Clone, Copy, Hash, StructuralPartialEq, PartialEq, Eq)]
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
}
