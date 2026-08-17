//! Wrapping an existing node in a container widget.

use canopy::{Context, NodeId, TypedId, Widget, error::Result};

/// Wrap `child` in a new `widget` node and return the wrapper's typed id.
///
/// The child keeps its identity: it is detached from its current parent and reattached under
/// the wrapper.
pub fn wrap<W: Widget + 'static>(
    c: &mut dyn Context,
    child: impl Into<NodeId>,
    widget: W,
) -> Result<TypedId<W>> {
    let child = child.into();
    let wrapper = c.create_detached(widget)?;
    c.detach(child)?;
    c.attach(NodeId::from(wrapper), child)?;
    Ok(wrapper)
}
