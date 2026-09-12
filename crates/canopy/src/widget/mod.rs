//! Widget trait and event outcome types.

use std::{
    any::{Any, type_name},
    time::Duration,
};

use crate::{
    Context, WidgetSemantics, WorkLifetime,
    core::context::ViewContext,
    cursor,
    error::Result,
    event::Event,
    geom::Size,
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement},
    render::Render,
    state::NodeName,
};

/// The result of an event handler.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EventOutcome {
    /// The event was processed and propagation stops.
    Handle,
    /// The event was not handled and will bubble up the tree.
    Ignore,
}

/// Widgets are the behavior attached to nodes in the Core arena.
pub trait Widget: Any {
    /// Describe application semantics for one publication through read-only
    /// access. Sensitive values must be omitted; this hook does not
    /// serialize widget state.
    fn semantics(&self, _view: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics::default())
    }

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
    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        view
    }

    /// Render this widget's own content. Does not render children.
    fn render(&mut self, _frame: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    /// Handle events.
    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        Ok(EventOutcome::Ignore)
    }

    /// Attempt to focus this widget.
    ///
    /// Widgets can use the provided context to query their tree state (e.g.,
    /// whether they have children) when deciding whether to accept focus.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        false
    }

    /// Cursor specification for focused widgets.
    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }

    /// Lifetime of scheduled polling. Hiding never stops polling.
    ///
    /// Node lifetime preserves background work while detached. Attachment
    /// lifetime pauses polling on detach and initializes it again after
    /// reattachment.
    fn poll_lifetime(&self) -> WorkLifetime {
        WorkLifetime::Node
    }

    /// Scheduled poll endpoint.
    ///
    /// Return `Some(delay)` to replace the pending timer. Delays below one
    /// millisecond round up to one millisecond so the runtime can sleep.
    /// Return `None` to stop scheduled polling, including a timer pending when
    /// an explicit wake triggered this callback. Node wakes can still request
    /// an immediate poll without waiting for a timer.
    fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
        None
    }

    /// Called when the widget is mounted in the tree, before its first render.
    ///
    /// A failed hook restores the structural state listed by
    /// [`Context::edit_structure`]. Widget state, binding registration, and
    /// external effects survive. Compensate those effects or make them safe
    /// to repeat, because a later mount attempt may call this hook again.
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }

    /// Validation hook before a widget is removed or replaced.
    ///
    /// This hook must be side-effect free or safely repeatable.
    fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }

    /// Called before a successfully mounted widget is removed or replaced.
    ///
    /// This hook cannot veto removal. During failure rollback, structural
    /// context operations are rejected and external cleanup must be safe to
    /// repeat.
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
