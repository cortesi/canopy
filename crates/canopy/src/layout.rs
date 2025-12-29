//! Layout types for configuring node positioning and sizing.

use crate::geom::{Expanse, Rect};

/// Stack direction for children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Stack children vertically (column).
    #[default]
    Column,
    /// Stack children horizontally (row).
    Row,
    /// Children overlap in the same space (painter's algorithm - last child on top).
    Stack,
}

/// Alignment along an axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Align to the start of the axis.
    #[default]
    Start,
    /// Align to the center of the axis.
    Center,
    /// Align to the end of the axis.
    End,
}

/// Display mode for layout participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    /// Node participates in layout and rendering.
    Block,
    /// Node is removed from layout and not rendered.
    None,
}

/// Sizing strategy for a single axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sizing {
    /// Size derives from `measure()` or wrapping children.
    Measure,
    /// Weighted share of remaining space along the axis.
    Flex(u32),
}

/// Edge insets for padding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges<T> {
    /// Top edge.
    pub top: T,
    /// Right edge.
    pub right: T,
    /// Bottom edge.
    pub bottom: T,
    /// Left edge.
    pub left: T,
}

impl<T: Copy> Edges<T> {
    /// Create edges with uniform length on all sides.
    pub fn all(v: T) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// Create edges with symmetric vertical and horizontal lengths.
    pub fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Create edges from individual values.
    pub fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

impl Edges<u32> {
    /// Total horizontal padding.
    pub fn horizontal(&self) -> u32 {
        self.left.saturating_add(self.right)
    }

    /// Total vertical padding.
    pub fn vertical(&self) -> u32 {
        self.top.saturating_add(self.bottom)
    }
}

/// Size with width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size<T> {
    /// Width component.
    pub width: T,
    /// Height component.
    pub height: T,
}

impl<T> Size<T> {
    /// Create a new size with the given width and height.
    pub fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl Size<u32> {
    /// Zero size.
    pub const ZERO: Self = Self {
        width: 0,
        height: 0,
    };

    /// Size along the main axis.
    pub fn main(self, direction: Direction) -> u32 {
        match direction {
            Direction::Column | Direction::Stack => self.height,
            Direction::Row => self.width,
        }
    }

    /// Size along the cross axis.
    pub fn cross(self, direction: Direction) -> u32 {
        match direction {
            Direction::Column | Direction::Stack => self.width,
            Direction::Row => self.height,
        }
    }

    /// Construct a size from main and cross axis values.
    pub fn from_main_cross(direction: Direction, main: u32, cross: u32) -> Self {
        match direction {
            Direction::Column | Direction::Stack => Self::new(cross, main),
            Direction::Row => Self::new(main, cross),
        }
    }
}

impl From<Expanse> for Size<u32> {
    fn from(v: Expanse) -> Self {
        Self {
            width: v.w,
            height: v.h,
        }
    }
}

impl From<Size<u32>> for Expanse {
    fn from(v: Size<u32>) -> Self {
        Self::new(v.width, v.height)
    }
}

/// Layout configuration for a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Whether this node participates in layout/render.
    pub display: Display,

    /// Stack direction for children.
    pub direction: Direction,

    /// Width sizing strategy (outer size).
    pub width: Sizing,
    /// Height sizing strategy (outer size).
    pub height: Sizing,

    /// Minimum outer width constraint (cells).
    pub min_width: Option<u32>,
    /// Maximum outer width constraint (cells).
    pub max_width: Option<u32>,

    /// Minimum outer height constraint (cells).
    pub min_height: Option<u32>,
    /// Maximum outer height constraint (cells).
    pub max_height: Option<u32>,

    /// Allow horizontal overflow during measurement.
    pub overflow_x: bool,
    /// Allow vertical overflow during measurement.
    pub overflow_y: bool,

    /// Structural padding inside the widget (cells).
    pub padding: Edges<u32>,

    /// Gap between children along the main axis (cells).
    pub gap: u32,

    /// Horizontal alignment of children within content area.
    pub align_horizontal: Align,

    /// Vertical alignment of children within content area.
    pub align_vertical: Align,
}

impl Default for Layout {
    fn default() -> Self {
        Self::column()
    }
}

impl Layout {
    /// Column layout with measured sizing on both axes.
    pub fn column() -> Self {
        Self {
            display: Display::Block,
            direction: Direction::Column,
            width: Sizing::Measure,
            height: Sizing::Measure,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            overflow_x: false,
            overflow_y: false,
            padding: Edges::all(0),
            gap: 0,
            align_horizontal: Align::Start,
            align_vertical: Align::Start,
        }
    }

    /// Row layout with measured sizing on both axes.
    pub fn row() -> Self {
        Self {
            direction: Direction::Row,
            ..Self::column()
        }
    }

    /// Stack layout where children overlap in the same space.
    pub fn stack() -> Self {
        Self {
            direction: Direction::Stack,
            ..Self::column()
        }
    }

    /// Fill available space with flex sizing on both axes.
    pub fn fill() -> Self {
        Self {
            width: Sizing::Flex(1),
            height: Sizing::Flex(1),
            ..Self::column()
        }
    }

    /// Remove this node from layout and rendering.
    pub fn none(mut self) -> Self {
        self.display = Display::None;
        self
    }

    /// Set width to flex with the provided weight (clamped to at least 1).
    pub fn flex_horizontal(mut self, weight: u32) -> Self {
        self.width = Sizing::Flex(weight.max(1));
        self
    }

    /// Set height to flex with the provided weight (clamped to at least 1).
    pub fn flex_vertical(mut self, weight: u32) -> Self {
        self.height = Sizing::Flex(weight.max(1));
        self
    }

    /// Set the minimum outer width.
    pub fn min_width(mut self, n: u32) -> Self {
        self.min_width = Some(n);
        self
    }

    /// Set the maximum outer width.
    pub fn max_width(mut self, n: u32) -> Self {
        self.max_width = Some(n);
        self
    }

    /// Set the minimum outer height.
    pub fn min_height(mut self, n: u32) -> Self {
        self.min_height = Some(n);
        self
    }

    /// Set the maximum outer height.
    pub fn max_height(mut self, n: u32) -> Self {
        self.max_height = Some(n);
        self
    }

    /// Allow horizontal overflow during measurement.
    pub fn overflow_x(mut self) -> Self {
        self.overflow_x = true;
        self
    }

    /// Allow vertical overflow during measurement.
    pub fn overflow_y(mut self) -> Self {
        self.overflow_y = true;
        self
    }

    /// Convenience: fixed outer width without a `Fixed` enum.
    pub fn fixed_width(self, n: u32) -> Self {
        self.min_width(n).max_width(n)
    }

    /// Convenience: fixed outer height without a `Fixed` enum.
    pub fn fixed_height(self, n: u32) -> Self {
        self.min_height(n).max_height(n)
    }

    /// Set padding edges.
    pub fn padding(mut self, edges: Edges<u32>) -> Self {
        self.padding = edges;
        self
    }

    /// Set the main-axis gap between children.
    pub fn gap(mut self, n: u32) -> Self {
        self.gap = n;
        self
    }

    /// Set horizontal alignment of children within content area.
    pub fn align_horizontal(mut self, align: Align) -> Self {
        self.align_horizontal = align;
        self
    }

    /// Set vertical alignment of children within content area.
    pub fn align_vertical(mut self, align: Align) -> Self {
        self.align_vertical = align;
        self
    }

    /// Center children both horizontally and vertically.
    pub fn align_center(self) -> Self {
        self.align_horizontal(Align::Center)
            .align_vertical(Align::Center)
    }

    /// Set the layout direction.
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }
}

/// Content-box measurement constraints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Constraint {
    /// No constraint on this axis.
    Unbounded,
    /// The engine guarantees at most n cells on this axis.
    AtMost(u32),
    /// The engine guarantees exactly n cells on this axis.
    Exact(u32),
}

impl Constraint {
    /// Clamp a value to this constraint.
    fn clamp(self, value: u32) -> u32 {
        match self {
            Self::Unbounded => value,
            Self::AtMost(n) => value.min(n),
            Self::Exact(n) => n,
        }
    }

    /// Return true if the constraint is exact.
    fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Return the maximum bound implied by the constraint.
    fn max_bound(self) -> u32 {
        match self {
            Self::Unbounded => u32::MAX,
            Self::AtMost(n) | Self::Exact(n) => n,
        }
    }
}

/// Constraints for measuring a widget's content box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeasureConstraints {
    /// Width constraint.
    pub width: Constraint,
    /// Height constraint.
    pub height: Constraint,
}

impl MeasureConstraints {
    /// Leaf widgets: clamp a content size to these constraints and return Fixed.
    pub fn clamp(&self, content: Size<u32>) -> Measurement {
        Measurement::Fixed(self.clamp_size(content))
    }

    /// Containers: request wrapping.
    pub fn wrap(&self) -> Measurement {
        Measurement::Wrap
    }

    /// Clamp a size to these constraints.
    pub fn clamp_size(&self, content: Size<u32>) -> Size<u32> {
        Size::new(
            self.width.clamp(content.width),
            self.height.clamp(content.height),
        )
    }

    /// True if the main axis is exact.
    pub fn main_is_exact(&self, direction: Direction) -> bool {
        match direction {
            Direction::Column | Direction::Stack => self.height.is_exact(),
            Direction::Row => self.width.is_exact(),
        }
    }

    /// True if the cross axis is exact.
    pub fn cross_is_exact(&self, direction: Direction) -> bool {
        match direction {
            Direction::Column | Direction::Stack => self.width.is_exact(),
            Direction::Row => self.height.is_exact(),
        }
    }

    /// Return the main axis constraint.
    pub fn main(&self, direction: Direction) -> Constraint {
        match direction {
            Direction::Column | Direction::Stack => self.height,
            Direction::Row => self.width,
        }
    }

    /// Return the cross axis constraint.
    pub fn cross(&self, direction: Direction) -> Constraint {
        match direction {
            Direction::Column | Direction::Stack => self.width,
            Direction::Row => self.height,
        }
    }
}

/// Result of measuring a widget's content box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measurement {
    /// Fixed content size for leaf widgets.
    Fixed(Size<u32>),
    /// Wrap children: engine computes content size from children.
    Wrap,
}

/// Canvas context for computing scrollable extents.
pub struct CanvasContext<'a> {
    /// Child layout results in this node's content coordinate space.
    children: &'a [CanvasChild],
}

impl<'a> CanvasContext<'a> {
    /// Construct a canvas context from a child slice.
    pub fn new(children: &'a [CanvasChild]) -> Self {
        Self { children }
    }

    /// Child layout results in this node's content coordinate space.
    pub fn children(&self) -> &[CanvasChild] {
        self.children
    }

    /// Extent of children outer rects.
    pub fn children_extent(&self) -> Size<u32> {
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        for child in self.children {
            max_x = max_x.max(child.rect.tl.x.saturating_add(child.rect.w));
            max_y = max_y.max(child.rect.tl.y.saturating_add(child.rect.h));
        }
        Size::new(max_x, max_y)
    }
}

/// Child layout results for canvas computations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasChild {
    /// Child outer rect relative to this node's content origin.
    pub rect: Rect,
    /// Child canvas size in the child's content coordinates.
    pub canvas: Size<u32>,
}

impl CanvasChild {
    /// Construct a new canvas child.
    pub fn new(rect: Rect, canvas: Size<u32>) -> Self {
        Self { rect, canvas }
    }
}

/// Clamp a flex weight to at least 1.
pub fn clamp_weight(weight: u32) -> u32 {
    weight.max(1)
}

/// Return true if a constraint is exact.
pub fn is_exact(c: Constraint) -> bool {
    c.is_exact()
}

/// Return the maximum bound for a constraint.
pub fn max_bound(c: Constraint) -> u32 {
    c.max_bound()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;

    #[test]
    fn clamp_size_unbounded() {
        let c = MeasureConstraints {
            width: Constraint::Unbounded,
            height: Constraint::Unbounded,
        };
        let size = Size::new(12, 7);
        assert_eq!(c.clamp_size(size), size);
    }

    #[test]
    fn clamp_size_at_most() {
        let c = MeasureConstraints {
            width: Constraint::AtMost(3),
            height: Constraint::AtMost(0),
        };
        let size = Size::new(10, 4);
        assert_eq!(c.clamp_size(size), Size::new(3, 0));
    }

    #[test]
    fn clamp_size_exact() {
        let c = MeasureConstraints {
            width: Constraint::Exact(0),
            height: Constraint::Exact(5),
        };
        let size = Size::new(3, 3);
        assert_eq!(c.clamp_size(size), Size::new(0, 5));
    }

    #[test]
    fn edges_saturating_add() {
        let edges = Edges::new(u32::MAX, u32::MAX, 1, 1);
        assert_eq!(edges.horizontal(), u32::MAX);
        assert_eq!(edges.vertical(), u32::MAX);
    }

    #[test]
    fn children_extent_empty() {
        let ctx = CanvasContext::new(&[]);
        assert_eq!(ctx.children_extent(), Size::ZERO);
    }

    #[test]
    fn children_extent_max_corner() {
        let children = [
            CanvasChild::new(Rect::new(0, 0, 5, 3), Size::new(1, 1)),
            CanvasChild::new(Rect::new(6, 2, 4, 2), Size::new(100, 100)),
        ];
        let ctx = CanvasContext::new(&children);
        assert_eq!(ctx.children_extent(), Size::new(10, 4));
    }
}
