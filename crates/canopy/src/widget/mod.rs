//! Widget trait and event outcome types.

use std::{
    any::{Any, type_name},
    time::Duration,
};

use crate::{
    Context,
    core::context::ReadContext,
    cursor,
    error::Result,
    event::Event,
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
};

/// The result of an event handler.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EventOutcome {
    /// The event was processed and propagation stops.
    Handle,
    /// The event was processed without a state change and propagation stops.
    Consume,
    /// The event was not handled and will bubble up the tree.
    Ignore,
}

/// Widgets are the behavior attached to nodes in the Core arena.
pub trait Widget: Any + Send {
    /// Layout configuration for this widget.
    fn layout(&self) -> Layout {
        Layout::column()
    }

    /// Measure intrinsic content size (content box, excludes Layout padding).
    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.wrap()
    }

    /// Canvas size in content coordinates (for scrolling).
    ///
    /// `view` is this node's content size (outer minus padding).
    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        view
    }

    /// Render this widget's own content. Does not render children.
    fn render(&mut self, _frame: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    /// Handle events.
    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        Ok(EventOutcome::Ignore)
    }

    /// Attempt to focus this widget.
    ///
    /// Widgets can use the provided context to query their tree state (e.g., whether they have
    /// children) when deciding whether to accept focus.
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        false
    }

    /// Cursor specification for focused widgets.
    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }

    /// Scheduled poll endpoint.
    fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
        None
    }

    /// Called exactly once when the widget is first mounted in the tree, before the first render.
    ///
    /// The framework guarantees single invocation via an internal `mounted` flag on each node.
    /// There is no need to guard against multiple calls within this method.
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }

    /// Validation hook before a node is removed from the arena.
    ///
    /// This hook must be side-effect free or safely repeatable.
    fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }

    /// Called exactly once immediately before the node is removed from the arena.
    fn on_unmount(&mut self, _ctx: &mut dyn Context) {}

    /// Name used for commands and paths.
    fn name(&self) -> NodeName {
        let name = type_name::<Self>();
        let short = name.rsplit("::").next().unwrap_or(name);
        NodeName::convert(short)
    }
}

/// Convert widgets into boxed trait objects.
impl<W> From<W> for Box<dyn Widget>
where
    W: Widget + 'static,
{
    fn from(widget: W) -> Self {
        Box::new(widget)
    }
}
