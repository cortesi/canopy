//! A container whose children extend past its viewport.

use canopy::{
    NodeName, Widget,
    geom::Size,
    layout::{CanvasContext, Direction, Layout, MeasureOverflow},
};

/// The axes a [`Scroll`] extends along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axes {
    /// Rows extend downward.
    Vertical,
    /// Columns extend rightward.
    Horizontal,
    /// Both rows and columns extend.
    Both,
}

/// A container that lays its children out past its viewport and scrolls over
/// them.
///
/// Children are ordinary nodes. On a scrolling axis they measure without a
/// bound, and the canvas spans their allocated rectangles. Content that must
/// grow along that axis uses measured or fixed sizing; flex children share
/// only the space that remains. The other axis stays bounded, even inside an
/// unbounded ancestor.
///
/// `Scroll` accepts no focus and installs no keys. Wheel input scrolls it
/// through the runtime's default action, focus changes reveal the focused
/// node, and an enclosing frame draws its position.
pub struct Scroll {
    /// Axes the canvas extends along.
    axes: Axes,
}

impl Scroll {
    /// Stack children in a column that scrolls vertically.
    pub fn vertical() -> Self {
        Self {
            axes: Axes::Vertical,
        }
    }

    /// Place children in a row that scrolls horizontally.
    pub fn horizontal() -> Self {
        Self {
            axes: Axes::Horizontal,
        }
    }

    /// Stack children in a column that scrolls on both axes.
    pub fn both() -> Self {
        Self { axes: Axes::Both }
    }

    /// Return whether the canvas extends horizontally and vertically.
    fn extends(&self) -> (bool, bool) {
        match self.axes {
            Axes::Vertical => (false, true),
            Axes::Horizontal => (true, false),
            Axes::Both => (true, true),
        }
    }
}

impl Widget for Scroll {
    fn layout(&self) -> Layout {
        let (horizontal, vertical) = self.extends();
        let overflow = |extends| {
            if extends {
                MeasureOverflow::Unbounded
            } else {
                MeasureOverflow::Bounded
            }
        };
        let direction = if self.axes == Axes::Horizontal {
            Direction::Row
        } else {
            Direction::Column
        };
        Layout::fill()
            .direction(direction)
            .overflow_x(overflow(horizontal))
            .overflow_y(overflow(vertical))
    }

    fn canvas(&self, view: Size, ctx: &CanvasContext) -> Size {
        let (horizontal, vertical) = self.extends();
        let extent = ctx.children_extent();
        Size::new(
            if horizontal {
                extent.w.max(view.w)
            } else {
                view.w
            },
            if vertical {
                extent.h.max(view.h)
            } else {
                view.h
            },
        )
    }

    fn name(&self) -> NodeName {
        NodeName::convert("scroll")
    }
}
