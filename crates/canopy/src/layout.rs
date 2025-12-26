//! Layout types for configuring node positioning and sizing.
//!
//! This module provides a clean abstraction over the underlying layout engine,
//! hiding implementation details while exposing a fluent API for flexbox layouts.

use taffy::style::LengthPercentage;

/// Display mode for layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    /// Flexbox layout.
    #[default]
    Flex,
    /// CSS Grid layout.
    Grid,
    /// Element is hidden from layout.
    None,
}

impl From<Display> for taffy::style::Display {
    fn from(d: Display) -> Self {
        match d {
            Display::Flex => Self::Flex,
            Display::Grid => Self::Grid,
            Display::None => Self::None,
        }
    }
}

impl From<taffy::style::Display> for Display {
    fn from(d: taffy::style::Display) -> Self {
        match d {
            taffy::style::Display::Flex => Self::Flex,
            taffy::style::Display::Grid => Self::Grid,
            taffy::style::Display::None => Self::None,
        }
    }
}

/// Flex direction for flexbox layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexDirection {
    /// Items laid out in a row (left to right).
    #[default]
    Row,
    /// Items laid out in a column (top to bottom).
    Column,
    /// Items laid out in a reversed row (right to left).
    RowReverse,
    /// Items laid out in a reversed column (bottom to top).
    ColumnReverse,
}

impl From<FlexDirection> for taffy::style::FlexDirection {
    fn from(d: FlexDirection) -> Self {
        match d {
            FlexDirection::Row => Self::Row,
            FlexDirection::Column => Self::Column,
            FlexDirection::RowReverse => Self::RowReverse,
            FlexDirection::ColumnReverse => Self::ColumnReverse,
        }
    }
}

impl From<taffy::style::FlexDirection> for FlexDirection {
    fn from(d: taffy::style::FlexDirection) -> Self {
        match d {
            taffy::style::FlexDirection::Row => Self::Row,
            taffy::style::FlexDirection::Column => Self::Column,
            taffy::style::FlexDirection::RowReverse => Self::RowReverse,
            taffy::style::FlexDirection::ColumnReverse => Self::ColumnReverse,
        }
    }
}

/// A dimension value for width/height sizing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Dimension {
    /// Automatic sizing based on content.
    #[default]
    Auto,
    /// Fixed size in terminal cells.
    Points(f32),
    /// Percentage of parent size.
    Percent(f32),
}

impl From<Dimension> for taffy::style::Dimension {
    fn from(d: Dimension) -> Self {
        match d {
            Dimension::Auto => Self::Auto,
            Dimension::Points(p) => Self::Points(p),
            Dimension::Percent(p) => Self::Percent(p),
        }
    }
}

impl From<taffy::style::Dimension> for Dimension {
    fn from(d: taffy::style::Dimension) -> Self {
        match d {
            taffy::style::Dimension::Auto => Self::Auto,
            taffy::style::Dimension::Points(p) => Self::Points(p),
            taffy::style::Dimension::Percent(p) => Self::Percent(p),
        }
    }
}

/// A length value for edges (padding, margin).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Zero length.
    #[default]
    Zero,
    /// Fixed length in terminal cells.
    Points(f32),
    /// Percentage of parent size.
    Percent(f32),
}

impl From<Length> for LengthPercentage {
    fn from(l: Length) -> Self {
        match l {
            Length::Zero => Self::Points(0.0),
            Length::Points(p) => Self::Points(p),
            Length::Percent(p) => Self::Percent(p),
        }
    }
}

impl From<LengthPercentage> for Length {
    fn from(l: LengthPercentage) -> Self {
        match l {
            LengthPercentage::Points(0.0) => Self::Zero,
            LengthPercentage::Points(p) => Self::Points(p),
            LengthPercentage::Percent(p) => Self::Percent(p),
        }
    }
}

/// Edge insets for padding and margin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    /// Top edge length.
    pub top: Length,
    /// Right edge length.
    pub right: Length,
    /// Bottom edge length.
    pub bottom: Length,
    /// Left edge length.
    pub left: Length,
}

impl Edges {
    /// Create edges with uniform length on all sides.
    pub fn all(l: Length) -> Self {
        Self {
            top: l,
            right: l,
            bottom: l,
            left: l,
        }
    }

    /// Create edges with symmetric vertical and horizontal lengths.
    pub fn symmetric(vertical: Length, horizontal: Length) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Create edges from individual lengths.
    pub fn new(top: Length, right: Length, bottom: Length, left: Length) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

impl From<Edges> for taffy::geometry::Rect<LengthPercentage> {
    fn from(e: Edges) -> Self {
        Self {
            top: e.top.into(),
            right: e.right.into(),
            bottom: e.bottom.into(),
            left: e.left.into(),
        }
    }
}

impl From<taffy::geometry::Rect<LengthPercentage>> for Edges {
    fn from(r: taffy::geometry::Rect<LengthPercentage>) -> Self {
        Self {
            top: r.top.into(),
            right: r.right.into(),
            bottom: r.bottom.into(),
            left: r.left.into(),
        }
    }
}

/// Size with width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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

impl<T, U> From<Size<T>> for taffy::geometry::Size<U>
where
    T: Into<U>,
{
    fn from(s: Size<T>) -> Self {
        Self {
            width: s.width.into(),
            height: s.height.into(),
        }
    }
}

impl<T, U> From<taffy::geometry::Size<T>> for Size<U>
where
    T: Into<U>,
{
    fn from(s: taffy::geometry::Size<T>) -> Self {
        Self {
            width: s.width.into(),
            height: s.height.into(),
        }
    }
}

/// Available space constraint for measuring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AvailableSpace {
    /// Known definite size.
    Definite(f32),
    /// Size based on minimum content.
    MinContent,
    /// Size based on maximum content.
    MaxContent,
}

impl AvailableSpace {
    /// Convert to an optional definite value.
    pub fn into_option(self) -> Option<f32> {
        match self {
            Self::Definite(v) => Some(v),
            _ => None,
        }
    }
}

impl From<AvailableSpace> for taffy::style::AvailableSpace {
    fn from(a: AvailableSpace) -> Self {
        match a {
            AvailableSpace::Definite(v) => Self::Definite(v),
            AvailableSpace::MinContent => Self::MinContent,
            AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

impl From<taffy::style::AvailableSpace> for AvailableSpace {
    fn from(a: taffy::style::AvailableSpace) -> Self {
        match a {
            taffy::style::AvailableSpace::Definite(v) => Self::Definite(v),
            taffy::style::AvailableSpace::MinContent => Self::MinContent,
            taffy::style::AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

/// Layout configuration for a node.
///
/// Wraps the underlying layout engine, providing a clean API
/// for configuring flexbox and grid layouts.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub(crate) inner: taffy::style::Style,
}

impl Layout {
    /// Create a new layout with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a layout from a taffy style (internal use).
    pub(crate) fn from_taffy(style: taffy::style::Style) -> Self {
        Self { inner: style }
    }

    /// Get a mutable reference to the inner taffy style.
    ///
    /// Use this for advanced layout features not yet exposed through
    /// the Layout wrapper API (e.g., absolute positioning, grid layout).
    pub fn as_taffy_mut(&mut self) -> &mut taffy::style::Style {
        &mut self.inner
    }

    // === Display Mode ===

    /// Set the display mode.
    pub fn display(&mut self, d: Display) -> &mut Self {
        self.inner.display = d.into();
        self
    }

    /// Set display to flex with column direction.
    pub fn flex_col(&mut self) -> &mut Self {
        self.inner.display = taffy::style::Display::Flex;
        self.inner.flex_direction = taffy::style::FlexDirection::Column;
        self
    }

    /// Set display to flex with row direction.
    pub fn flex_row(&mut self) -> &mut Self {
        self.inner.display = taffy::style::Display::Flex;
        self.inner.flex_direction = taffy::style::FlexDirection::Row;
        self
    }

    /// Set the flex direction.
    pub fn flex_direction(&mut self, d: FlexDirection) -> &mut Self {
        self.inner.flex_direction = d.into();
        self
    }

    // === Flex Item Properties ===

    /// Configure as a flex item with grow, shrink, and basis.
    pub fn flex_item(&mut self, grow: f32, shrink: f32, basis: Dimension) -> &mut Self {
        self.inner.flex_grow = grow;
        self.inner.flex_shrink = shrink;
        self.inner.flex_basis = basis.into();
        self
    }

    /// Set the flex grow factor.
    pub fn flex_grow(&mut self, v: f32) -> &mut Self {
        self.inner.flex_grow = v;
        self
    }

    /// Set the flex shrink factor.
    pub fn flex_shrink(&mut self, v: f32) -> &mut Self {
        self.inner.flex_shrink = v;
        self
    }

    /// Set the flex basis.
    pub fn flex_basis(&mut self, d: Dimension) -> &mut Self {
        self.inner.flex_basis = d.into();
        self
    }

    // === Sizing ===

    /// Set the width.
    pub fn width(&mut self, d: Dimension) -> &mut Self {
        self.inner.size.width = d.into();
        self
    }

    /// Set the height.
    pub fn height(&mut self, d: Dimension) -> &mut Self {
        self.inner.size.height = d.into();
        self
    }

    /// Set both width and height.
    pub fn size(&mut self, width: Dimension, height: Dimension) -> &mut Self {
        self.inner.size.width = width.into();
        self.inner.size.height = height.into();
        self
    }

    /// Set width to 100%.
    pub fn w_full(&mut self) -> &mut Self {
        self.inner.size.width = taffy::style::Dimension::Percent(1.0);
        self
    }

    /// Set height to 100%.
    pub fn h_full(&mut self) -> &mut Self {
        self.inner.size.height = taffy::style::Dimension::Percent(1.0);
        self
    }

    /// Set both width and height to 100%.
    pub fn fill(&mut self) -> &mut Self {
        self.w_full().h_full()
    }

    /// Set the minimum width.
    pub fn min_width(&mut self, d: Dimension) -> &mut Self {
        self.inner.min_size.width = d.into();
        self
    }

    /// Set the minimum height.
    pub fn min_height(&mut self, d: Dimension) -> &mut Self {
        self.inner.min_size.height = d.into();
        self
    }

    /// Set both minimum width and height.
    pub fn min_size(&mut self, width: Dimension, height: Dimension) -> &mut Self {
        self.inner.min_size.width = width.into();
        self.inner.min_size.height = height.into();
        self
    }

    /// Set the maximum width.
    pub fn max_width(&mut self, d: Dimension) -> &mut Self {
        self.inner.max_size.width = d.into();
        self
    }

    /// Set the maximum height.
    pub fn max_height(&mut self, d: Dimension) -> &mut Self {
        self.inner.max_size.height = d.into();
        self
    }

    // === Spacing ===

    /// Set padding on all edges.
    pub fn padding(&mut self, edges: Edges) -> &mut Self {
        self.inner.padding = edges.into();
        self
    }

    /// Set uniform padding on all edges.
    pub fn padding_all(&mut self, l: Length) -> &mut Self {
        self.padding(Edges::all(l))
    }

    /// Set margin on all edges.
    pub fn margin(&mut self, edges: Edges) -> &mut Self {
        let rect = taffy::geometry::Rect {
            top: length_to_auto(edges.top),
            right: length_to_auto(edges.right),
            bottom: length_to_auto(edges.bottom),
            left: length_to_auto(edges.left),
        };
        self.inner.margin = rect;
        self
    }

    /// Set uniform margin on all edges.
    pub fn margin_all(&mut self, l: Length) -> &mut Self {
        self.margin(Edges::all(l))
    }

    // === Getters ===

    /// Get the current display mode.
    pub fn get_display(&self) -> Display {
        self.inner.display.into()
    }

    /// Get the current flex direction.
    pub fn get_flex_direction(&self) -> FlexDirection {
        self.inner.flex_direction.into()
    }

    /// Get the current flex grow factor.
    pub fn get_flex_grow(&self) -> f32 {
        self.inner.flex_grow
    }

    /// Get the current flex shrink factor.
    pub fn get_flex_shrink(&self) -> f32 {
        self.inner.flex_shrink
    }

    /// Get the current flex basis.
    pub fn get_flex_basis(&self) -> Dimension {
        self.inner.flex_basis.into()
    }

    /// Get the current width.
    pub fn get_width(&self) -> Dimension {
        self.inner.size.width.into()
    }

    /// Get the current height.
    pub fn get_height(&self) -> Dimension {
        self.inner.size.height.into()
    }

    /// Get the current padding.
    pub fn get_padding(&self) -> Edges {
        self.inner.padding.into()
    }

    /// Get the current minimum width.
    pub fn get_min_width(&self) -> Dimension {
        self.inner.min_size.width.into()
    }

    /// Get the current minimum height.
    pub fn get_min_height(&self) -> Dimension {
        self.inner.min_size.height.into()
    }
}

/// Convert a Length to a LengthPercentageAuto for margin.
fn length_to_auto(l: Length) -> taffy::style::LengthPercentageAuto {
    match l {
        Length::Zero => taffy::style::LengthPercentageAuto::Points(0.0),
        Length::Points(p) => taffy::style::LengthPercentageAuto::Points(p),
        Length::Percent(p) => taffy::style::LengthPercentageAuto::Percent(p),
    }
}

// Re-export taffy types that are used internally but not in public API
pub(crate) use taffy::style_helpers::{FromFlex, line};
// Re-export grid-related types for internal use (panes widget)
pub(crate) use taffy::{
    geometry::Line,
    style::{GridPlacement, TrackSizingFunction},
};
