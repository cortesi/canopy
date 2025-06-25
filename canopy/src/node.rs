use std::time::Duration;

use crate::{
    commands::CommandNode,
    cursor,
    event::{key, mouse},
    geom::Expanse,
    state::StatefulNode,
    Context, Layout, Render, Result,
};

/// Was an event handled or ignored?
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EventOutcome {
    Handle,
    Ignore,
}

/// The result of an event handler.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EventResult {
    /// Was the event handled or ignored?
    pub outcome: EventOutcome,
    /// If true, handling the event should not trigger a render sweep.
    pub no_render: bool,
}

impl From<EventOutcome> for EventResult {
    fn from(o: EventOutcome) -> Self {
        EventResult {
            outcome: o,
            no_render: false,
        }
    }
}

impl EventResult {
    /// Convenience to signal that the event was handled and should trigger a render.
    pub fn handled() -> Self {
        EventOutcome::Handle.into()
    }

    /// Convenience to signal that the event was ignored.
    pub fn ignored() -> Self {
        EventOutcome::Ignore.into()
    }

    /// Convenience to mark a handled event that should not trigger rendering.
    pub fn no_render(mut self) -> Self {
        self.no_render = true;
        self
    }
}

#[allow(unused_variables)]
/// Nodes are the basic building-blocks of a Canopy UI. They are composed in a tree, with each node responsible for
/// managing its own children.
pub trait Node: StatefulNode + CommandNode {
    /// Force the node to render in the next sweep. Over-riding this method should only be needed rarely, for instance
    /// when a container node (e.g. a frame) needs to redraw if a child node changes.
    fn force_render(&self, c: &dyn Context) -> bool {
        false
    }

    /// Called for each node on the focus path, after each render sweep. The first node that returns a
    /// ``cursor::Cursor`` specification controls the cursor. If no node returns a cursor, cursor display is disabled.
    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }

    /// Attempt to focus this node. If the node accepts focus, it should return true, and if not return false. The
    /// default implementation returns false.
    fn accept_focus(&mut self) -> bool {
        false
    }

    /// Call a closure on this node's children. If any child handler returns an error, processing terminates without
    /// visiting the remaining children. The order in which nodes are processed should match intuitive next/previous
    /// relationships. The default implementation assumes this node has no children, and just returns.
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        Ok(())
    }

    /// Re-compute the size and view of the node if it had to be displayed in the target area. In practice, nodes will
    /// usually either constrain themselves based on the width or the height of the target area, or neither, but not
    /// both. The resulting size may be smaller or larger than the target. If non-trivial computation is done to compute
    /// the size (e.g. reflowing text), it should be cached for use by future calls. This method may be called multiple
    /// times for a given node during a render sweep, so re-fitting to the same size should be cheap and return
    /// consistent results.
    fn layout(&mut self, l: &Layout, target: Expanse) -> Result<()> {
        Ok(())
    }

    /// Handle a key input event. This event is only called for nodes that are on the focus path. The default
    /// implementation ignores input.
    fn handle_key(&mut self, c: &mut dyn Context, k: key::Key) -> Result<EventResult> {
        Ok(EventResult::ignored())
    }

    /// Handle a mouse input event. The default implementation ignores mouse input.
    fn handle_mouse(&mut self, c: &mut dyn Context, k: mouse::MouseEvent) -> Result<EventResult> {
        Ok(EventResult::ignored())
    }

    /// The scheduled poll endpoint. This function is called for every node the first time it is seen during the
    /// pre-render sweep. Each time the function returns a duration, a subsequent call is scheduled. If the function
    /// returns None, the `poll` function is never called again. The default implementation returns `None`.
    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        None
    }

    /// Render this widget. The render method should:
    ///
    /// - Lay out any child nodes by manipulating their viewports. This will often involve calling the `fit` method on
    ///   the child nodes to get their dimensions.
    /// - Render itself to screen. This node's viewport will already have been set by a parent.
    ///
    /// The default implementation does nothing.
    fn render(&mut self, c: &dyn Context, r: &mut Render) -> Result<()> {
        Ok(())
    }
}
