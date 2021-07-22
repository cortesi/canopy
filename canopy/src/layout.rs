use crate::geom::Rect;
use crate::{Canopy, Result, StatefulNode};

/// A layout for nodes that simply fill the space specified. Examples include
/// frames that fill any region we pass them, and widgets that have one fixed
/// dimension, like a fixed-height status bar.
pub trait Layout<S>: StatefulNode {
    /// Lay out this node's children. Implementers should call `layout` or
    /// `hide` on each child. The screen area for this node has already been set
    /// in the `layout` method, and is available through the `screen_area`
    /// method. The default does nothing, and is appropriate for nodes with no
    /// children.
    fn layout_children(&mut self, _app: &mut Canopy<S>) -> Result<()> {
        Ok(())
    }

    /// Lay out this component and all its children. Implementers should use
    /// `set_screen_area` to save the layout information to the node state, and
    /// then call `self.layout_children`. The default implementation already
    /// does both of these things, so most implementers will only need to
    /// override `layout_children`.
    fn layout(&mut self, app: &mut Canopy<S>, screen_rect: Rect) -> Result<()> {
        self.set_screen_area(screen_rect);
        self.layout_children(app)
    }
}

/// A layout for nodes with geometry computed based on a width constraint. This
/// defines a two-stage layout process where the node is first constrained,
/// computing a virtual rectangle, then some sub-view of the virtual rectangle
/// is laid out on the screen.
///
/// For instance, imagine laying out a paragraph of text. First we `constrain`
/// the Node by specifying the text width. The component then calculates the
/// height that will result, and returns a calculated virtual component
/// rectangle that encloses all its content. Now, the parent component can make
/// a decision to render some sub-view of the virtual component rectangle onto
/// the screen.
pub trait ConstrainedWidthLayout<S>: StatefulNode {
    /// Constrain the width of the component. This should operate on
    /// `self.state_mut().view` to set the appropriate sizes. A best-effort
    /// attempt should be made to scale to within the width, but the view's
    /// outer rectangle may be larger or smaller than the constraint. This
    /// method should be used in the `layout` method of a parent, and should be
    /// followed by a call to the child's `layout` method with the established
    /// geometry.
    fn constrain(&mut self, app: &mut Canopy<S>, width: u16) -> Result<()>;

    /// Lay out this node's children. Implementers should call `layout` or
    /// `hide` on each child. The screen and virtual areas for this node have
    /// already been set in the `layout` method, and are available through the
    /// `virt_area` and `screen_area` methods. The default does nothing, and is
    /// appropriate for nodes with no children.
    fn layout_children(&mut self, _app: &mut Canopy<S>) -> Result<()> {
        Ok(())
    }

    /// Lay out this component and all its children. The `virt_rect` argument
    /// must be a sub-rectcangle of the return value from the previous call to
    /// `constrain`, and its size must be equal to `screen_rect`.
    ///
    /// Implementers should use `set_screen_area` and `set_virt_area` save the
    /// layout information to the node state, and then call
    /// `self.layout_children`. The default implementation already does both of
    /// these things, so most implementers will only need to override
    /// `layout_children`.
    fn layout(&mut self, app: &mut Canopy<S>, screen_rect: Rect) -> Result<()> {
        self.set_screen_area(screen_rect);
        self.state_mut().view.resize_inner(screen_rect)?;
        self.layout_children(app)
    }
}
