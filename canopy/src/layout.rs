use crate::geom::Rect;
use crate::{Canopy, Result, StatefulNode};

/// A layout for nodes that simply fill the space specified. Examples include
/// frames that fill any region we pass them, and widgets that have one fixed
/// dimension, like a fixed-height status bar.
pub trait FillLayout<S>: StatefulNode {
    fn layout_children(&mut self, _app: &mut Canopy<S>, _screen_rect: Rect) -> Result<()> {
        Ok(())
    }

    fn layout(&mut self, app: &mut Canopy<S>, screen_rect: Rect) -> Result<()> {
        self.set_screen_area(screen_rect);
        self.layout_children(app, screen_rect)
    }
}

/// A layout for nodes with geometry computed based on a width constraint. This
/// defines a two-stage layout process where the node is first constrained, and
/// computes a virtual rectangle, then some sub-view of the virtual rectangle is
/// laid out on the screen.
///
/// For instance, imagine laying out a paragraph of text. First we `constrain`
/// the Node by specifying the text width. The component then calculates the
/// height that will result, and returns a calculated virtual component
/// rectangle that encloses all its content. Now, the parent component can make
/// a decision to render some sub-view of the virtual component rectangle onto
/// the screen.
pub trait ConstrainedWidthLayout<S>: StatefulNode {
    /// Constrain the width of the component. Returns a rectangle at the origin
    /// (0, 0) representing the virtual size of the component. A best-effort
    /// attempt is made to scale to within the width, but the returned rectangle
    /// may be larger or smaller than the given constraints. This method should
    /// be used in the `layout` method of a parent, and should be followed by a
    /// call to layout with the established geometry.
    ///
    /// This method may return None, in which case the component will attempt to
    /// render in whatever size it's laid out to.
    fn constrain(&mut self, app: &mut Canopy<S>, width: u16) -> Result<Rect>;

    /// Lay out a view onto the virtual component. The size of `rect` must be
    /// smaller than or equal to the rect returned by `constrain`, and
    /// `virt_origin` must be a point within the virtual component such that
    /// rect would fall entirely inside it.
    fn layout_children(
        &mut self,
        _app: &mut Canopy<S>,
        _virt_rect: Rect,
        _screen_rect: Rect,
    ) -> Result<()> {
        Ok(())
    }

    fn layout(&mut self, app: &mut Canopy<S>, virt_rect: Rect, screen_rect: Rect) -> Result<()> {
        self.set_screen_area(screen_rect);
        self.set_virt_area(virt_rect);
        self.layout_children(app, virt_rect, screen_rect)
    }
}
