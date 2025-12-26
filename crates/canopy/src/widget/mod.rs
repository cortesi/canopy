//! Widget trait and event outcome types.

use std::{
    any::{Any, type_name},
    time::Duration,
};

use crate::{
    Context,
    commands::CommandNode,
    core::context::ViewContext,
    cursor,
    error::Result,
    event::Event,
    geom::Rect,
    layout::{AvailableSpace, Layout, Size},
    render::Render,
    state::NodeName,
};

/// The result of an event handler.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EventOutcome {
    /// The event was processed and the node should be rendered.
    Handle,
    /// The event was processed, but nothing changed so rendering is skipped and propagation stops.
    Consume,
    /// The event was not handled and will bubble up the tree.
    Ignore,
}

/// Widgets are the behavior attached to nodes in the Core arena.
pub trait Widget: Any + Send + CommandNode {
    /// Render the widget into the buffer for the visible area.
    fn render(&mut self, frame: &mut Render, area: Rect, ctx: &dyn ViewContext) -> Result<()>;

    /// Returns the size this widget requests for its view (visible area).
    ///
    /// Called by the layout engine during layout computation.
    ///
    /// # Parameters
    ///
    /// - `known_dimensions`: Dimensions already fixed by the layout algorithm (e.g., from explicit
    ///   width/height in the node's style). When `Some`, the widget **must** return that exact
    ///   value for that dimension - it's a constraint, not a suggestion.
    /// - `available_space`: Space offered by the parent container. `Definite` provides a concrete
    ///   pixel value; `MinContent`/`MaxContent` request intrinsic sizing.
    ///
    /// # Return value
    ///
    /// For widgets relying entirely on flex layout (flex_grow/flex_shrink), return `(0, 0)` to
    /// indicate no size preference. Otherwise, return the widget's preferred size, respecting
    /// any constraints from `known_dimensions`.
    fn view_size(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = known_dimensions
            .width
            .or_else(|| available_space.width.into_option())
            .unwrap_or(0.0);
        let height = known_dimensions
            .height
            .or_else(|| available_space.height.into_option())
            .unwrap_or(0.0);
        Size { width, height }
    }

    /// Returns the total canvas size (scrollable content area).
    ///
    /// Called after layout completes with the computed view size. For scrollable widgets, the
    /// canvas may exceed the view to enable scrolling. For example, a list with 100 items in a
    /// 10-row view returns height 100 here, while the view is only 10 rows.
    ///
    /// The default returns the view size unchanged (no scrolling). Override when content can
    /// exceed view bounds.
    fn canvas_size(&self, view: Size<f32>) -> Size<f32> {
        view
    }

    /// Handle events.
    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    /// Attempt to focus this widget.
    ///
    /// Widgets can use the provided context to query their tree state (e.g., whether they have
    /// children) when deciding whether to accept focus.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
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

    /// Configure default layout for this widget.
    fn layout(&self, _layout: &mut Layout) {}

    /// Called exactly once when the widget is first mounted in the tree, before the first render.
    ///
    /// The framework guarantees single invocation via an internal `mounted` flag on each node.
    /// There is no need to guard against multiple calls within this method.
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }

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
