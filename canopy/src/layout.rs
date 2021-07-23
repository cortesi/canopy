use crate::geom::Rect;
use crate::{Canopy, Result, StatefulNode};

/// Implemented by nodes with geometry computed based on a width constraint.
///
/// For instance, imagine laying out a paragraph of text. First we `constrain`
/// the Node by specifying the text width. The component then calculates the
/// height that will result, and returns a calculated virtual component
/// rectangle that encloses all its content. Now, the parent component can make
/// a decision to render some sub-view of the virtual component rectangle onto
/// the screen.
pub trait WidthConstrained<S> {
    /// Constrain the width of the component. This should operate on
    /// `self.state_mut().viewport` to set the appropriate sizes. A best-effort
    /// attempt should be made to scale to within the width, but the view's
    /// outer rectangle may be larger or smaller than the constraint. This
    /// method should be used in the `layout` method of a parent, and should be
    /// followed by a call to the child's `layout` method with the established
    /// geometry.
    fn constrain(&mut self, app: &mut Canopy<S>, width: u16) -> Result<()>;
}
