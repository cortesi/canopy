use crate::geom::{Point, Rect};
use crate::{Canopy, Result};

/// A layout for nodes that provide no feedback on their internal geometry -
/// they just fill the space specified. Examples include frames that fill any
/// region we pass them, and widgets that have one fixed dimension, like a
/// fixed-height status bar.
pub trait FillLayout<S> {
    fn layout(&mut self, app: &mut Canopy<S>, rect: Option<Rect>) -> Result<()>;
}

/// A layout for nodes with geometry computed based on constraints. This defines
/// a two-stage layout process where the node is first constrained, and computes
/// a virtual rectangle, then some sub-view of the virtual rectangle is laid out
/// on the screen.
///
/// For instance, imagine laying out a paragraph of text. First we `constrain`
/// the Node by specifying the text width. The component then calculates the
/// height that will result, and returns a calculated virtual component
/// rectangle that encloses all its content. Now, the parent component can make
/// a decision to render some sub-view of the virtual component rectangle onto
/// the screen.
pub trait ConstrainedLayout<S> {
    /// Constrain size of the component along a dimension. Returns a rectangle
    /// at origin (0, 0) representing the virtual size of the component. A
    /// best-effort attempt is made to scale to within the constraints, but the
    /// returned rectangle may be larger or smaller than the given constraints.
    /// This method should be used in the `layout` method of a parent, and
    /// should be followed by a call to layout with the established geometry.
    ///
    /// This method may return None, in which case the component will attempt to
    /// render in whatever size it's laid out to.
    fn constrain(
        &mut self,
        app: &mut Canopy<S>,
        width: Option<u16>,
        height: Option<u16>,
    ) -> Result<Rect>;
    /// Lay out a view onto the virtual component. The size of `rect` must be
    /// smaller than or equal to the rect `constrain`, and `virt_origin` must be
    /// a point within the virtual component such that rect would fall entirely
    /// inside it.
    fn layout(&mut self, app: &mut Canopy<S>, virt_origin: Point, rect: Rect) -> Result<()>;
}
