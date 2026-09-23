//! Widget trait and event outcome types.

use std::{
    any::{Any, type_name},
    time::Duration,
};

use crate::{
    Context, PollLifetime, WidgetSemantics,
    core::context::ViewContext,
    cursor,
    error::Result,
    event::{Event, key::Key},
    geom::{Rect, Size},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement},
    render::Render,
    state::NodeName,
};

/// The result of an event handler.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EventOutcome {
    /// The event was processed and propagation stops.
    Handle,
    /// The event was not handled and will bubble up the tree.
    Ignore,
}

/// The axis a scrollbar measures.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ScrollAxis {
    /// Rows, with a track that runs top to bottom.
    Vertical,
    /// Columns, with a track that runs left to right.
    Horizontal,
}

/// A position indicator drawn on a scrollbar track.
///
/// `start` and `end` bound a half-open region of canvas cells, counted from
/// the start of the target's canvas along the scrollbar's axis, so a mark
/// can cover one row or a whole multi-line match. The scrollbar maps the
/// region onto its track the same proportional way it places the thumb, so
/// a mark sits beside the content it annotates; every nonempty region owns
/// at least one track cell, and an empty region draws nothing. Each covered
/// cell outside the thumb draws with `style` and `glyph`; each cell under
/// the thumb keeps the thumb's glyph and the mark's style, so the position
/// reads through in the mark's color.
///
/// The mark color must read from the style's foreground. Block thumb glyphs
/// hide the background, so a text-highlight style with a dark foreground
/// turns the mark dark just when the thumb slides over it; give marks a
/// style whose foreground is the mark color instead.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ScrollMark {
    /// First canvas cell of the marked region.
    pub start: u32,
    /// One past the last canvas cell of the marked region.
    pub end: u32,
    /// Style path of the mark.
    pub style: &'static str,
    /// Glyph of the mark where it sits outside the thumb.
    pub glyph: char,
}

/// A widget is the behavior a node holds: it lays out, renders, and handles
/// input for its node.
pub trait Widget: Any {
    /// Describe application semantics for one publication through read-only
    /// access. Sensitive values must be omitted; this hook does not
    /// serialize widget state.
    fn semantics(&self, _ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
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
    /// `content` is this node's content size (outer minus padding).
    fn canvas(&self, content: Size, _ctx: &CanvasContext) -> Size {
        content
    }

    /// Render this widget's own content. Does not render children.
    fn render(&mut self, _frame: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    /// Handle events.
    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        Ok(EventOutcome::Ignore)
    }

    /// Predict this widget's result for `key` without changing any state.
    ///
    /// `ctx` is a read-only view bound to this widget's node. Its focus
    /// answers follow the route focus the caller is asking about, which can
    /// differ from the live focus.
    ///
    /// The prediction must equal the next routed
    /// `on_event(Event::Key(key), ...)` result for the same pre-event state.
    /// The default, [`EventOutcome::Ignore`], is right for a widget that
    /// handles no keys; a widget that consumes keys in `on_event` must
    /// predict them here. Dispatch passes the raw event key and checks the
    /// prediction against the actual result: a mismatch records a route trace
    /// entry and fails a debug assertion. Discovery probes the canonical
    /// binding key instead, so a prediction is exact for the raw key and
    /// best-effort for a canonical probe.
    fn key_outcome(&self, _key: Key, _ctx: &dyn ViewContext) -> EventOutcome {
        EventOutcome::Ignore
    }

    /// Return whether this widget consumes `intent` in its current state.
    ///
    /// This is a pure promise. When it returns true, the next routed
    /// [`Widget::on_intent`] call for the same intent and state must return
    /// [`EventOutcome::Handle`]. The default says the widget consumes no
    /// intent, so intent bindings stay dormant on its route.
    ///
    /// `ctx` is a read-only view bound to this widget's node. Its focus
    /// answers follow the route focus the caller is asking about.
    fn accepts_intent(&self, _intent: &str, _ctx: &dyn ViewContext) -> bool {
        false
    }

    /// Perform one intent the route offered to this widget.
    ///
    /// Routing calls this only after [`Widget::accepts_intent`] returned true
    /// for the same intent and state. A widget that does not know the intent
    /// returns [`EventOutcome::Ignore`].
    fn on_intent(&mut self, _intent: &str, _ctx: &mut dyn Context) -> Result<EventOutcome> {
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

    /// Return the rectangle of this widget's canvas that
    /// [`Context::reveal_anchor`] shows, given the final `content` size.
    ///
    /// Layout calls this after it settles geometry, so the anchor reflects
    /// changes made earlier in the turn. The hook reads widget state and cannot
    /// change layout. The default returns `None`, which consumes the request
    /// without scrolling.
    fn reveal_anchor(&self, _content: Size) -> Option<Rect> {
        None
    }

    /// Whether this widget displays scroll positions for the subtree beneath
    /// it.
    ///
    /// Scrollbar owners, such as frames, search their subtree for the node
    /// whose position they draw. The search stops at a widget that returns
    /// true, so no node is drawn by two owners.
    fn owns_scrollbars(&self) -> bool {
        false
    }

    /// Position indicators for a scrollbar on `axis`.
    ///
    /// A scrollbar owner draws the returned marks on its track beside this
    /// widget's canvas, so any scrolling widget can annotate positions such
    /// as search matches without owning a scrollbar itself. `content` is
    /// this node's content size, for widgets whose canvas offsets depend on
    /// the view width, such as soft-wrapped text. The default reports no
    /// marks.
    fn scroll_marks(&self, _axis: ScrollAxis, _content: Size) -> Vec<ScrollMark> {
        Vec::new()
    }

    /// Lifetime of scheduled polling. Hiding never stops polling.
    ///
    /// Node lifetime preserves background work while detached. Attachment
    /// lifetime pauses polling on detach and initializes it again after
    /// reattachment.
    fn poll_lifetime(&self) -> PollLifetime {
        PollLifetime::Node
    }

    /// Scheduled poll endpoint.
    ///
    /// Return `Some(delay)` to replace the pending timer. Delays below one
    /// millisecond round up to one millisecond so the runtime can sleep.
    /// Return `None` to stop scheduled polling, including a timer pending when
    /// an explicit wake triggered this callback. Node wakes can still request
    /// an immediate poll without waiting for a timer.
    ///
    /// A failure that [`Error::is_notice`](crate::error::Error::is_notice)
    /// classifies becomes a notice, and polling continues at the delay the
    /// last successful poll requested. Any other failure is fatal.
    fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
        Ok(None)
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
