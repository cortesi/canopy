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
    layout::{AvailableSpace, Size, Style},
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

    /// Calculate intrinsic size for leaf nodes.
    fn measure(
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

    /// Calculate the canvas size for this node.
    fn canvas_size(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        self.measure(known_dimensions, available_space)
    }

    /// Handle events.
    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    /// Attempt to focus this widget.
    fn accept_focus(&mut self) -> bool {
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

    /// Configure default layout style for this widget.
    fn configure_style(&self, _style: &mut Style) {}

    /// Called once when the widget is first mounted in the tree, before the first render.
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
