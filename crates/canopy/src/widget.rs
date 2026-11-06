//! Widget trait, event outcome types, and the parts a widget mounts.

use std::{
    any::{Any, type_name},
    time::Duration,
};

use crate::{
    Context, NodeId, NodeName,
    core::context::ViewContext,
    error::{Error, Result},
    geom::{Rect, Size},
    input::{Event, NavIntent, key::Key},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::{PollLifetime, WidgetSemantics},
    status::StatusItem,
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

    /// Return the node whose view `nav` scrolls in place of this node's view,
    /// when this widget declines `nav`.
    ///
    /// `None`, the default, scrolls this node's own view. A pane that holds
    /// the keyboard can name a scrolling view inside it. The intent is a
    /// parameter, so a pane can forward some intents and keep others. When
    /// the target cannot move, the route offers the intent to the next node.
    fn scroll_target(&self, _nav: NavIntent, _ctx: &dyn ViewContext) -> Option<NodeId> {
        None
    }

    /// Attempt to focus this widget.
    ///
    /// Widgets can use the provided context to query their tree state (e.g.,
    /// whether they have children) when deciding whether to accept focus.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        false
    }

    /// Return whether a pointer event stops at this widget.
    ///
    /// Hit testing skips a widget that returns false, but still tests its
    /// children. A pointer event over the widget's own area then reaches the
    /// nodes beneath it, so a layer that only places its children, such as a
    /// full-size overlay, does not take clicks from what it covers. Focus,
    /// keys, and mouse capture do not change.
    fn accept_pointer(&self, _ctx: &dyn ViewContext) -> bool {
        true
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

    /// Status items this widget shows in a status line that takes it as a
    /// source.
    ///
    /// The line asks as it renders, so the items follow the widget's state
    /// with no update calls. The default shows nothing.
    fn status(&self, _ctx: &dyn ViewContext) -> Vec<StatusItem> {
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

/// What a widget builds when it mounts, held with what it builds it from.
///
/// A widget that builds children or other parts in
/// [`Widget::on_mount`] keeps them in one `Mount`. The pending value `P`
/// holds what the mount needs, such as options and callbacks; the mounted
/// value `M` holds what the mount built, such as child node ids. The mount
/// keeps `P`, so a widget mounts again from it after an unmount. Callbacks
/// that a mount used to move out of the widget belong in `P` as shared
/// values (`Rc`), or are rebuilt from it.
///
/// A widget value can mount more than once: a failed tree edit runs
/// [`Widget::on_unmount`] on the widgets it mounted, and a retained widget
/// mounts again when it is next attached. Each widget's `on_unmount` calls
/// [`Mount::unmount`], so the next mount starts from the pending value.
///
/// The value is in one of two states: pending, with only `P`, or mounted,
/// with `P` and `M`.
#[derive(Debug, Clone)]
pub struct Mount<P, M> {
    /// What the mount builds from, kept in both states.
    pending: P,
    /// What the mount built, while mounted.
    parts: Option<M>,
}

impl<P, M> Mount<P, M> {
    /// Hold `pending` until the widget mounts.
    pub fn new(pending: P) -> Self {
        Self {
            pending,
            parts: None,
        }
    }

    /// Build the mounted parts with `f`, from the pending value.
    ///
    /// A failure leaves the value pending, so a later mount can try again. A
    /// value that is already mounted is an error, and stays as it is.
    pub fn mount(&mut self, f: impl FnOnce(&mut P) -> Result<M>) -> Result<()> {
        if self.parts.is_some() {
            return Err(Error::Invalid("widget is already mounted".into()));
        }
        self.parts = Some(f(&mut self.pending)?);
        Ok(())
    }

    /// Drop the mounted parts and return to pending. A pending value stays
    /// as it is.
    pub fn unmount(&mut self) {
        self.parts = None;
    }

    /// Return whether the parts are mounted.
    pub fn is_mounted(&self) -> bool {
        self.parts.is_some()
    }

    /// Return the mounted parts, or an error that the widget is not mounted.
    pub fn mounted(&self) -> Result<&M> {
        self.parts.as_ref().ok_or_else(not_mounted)
    }

    /// Return the mounted parts mutably, or an error that the widget is not
    /// mounted.
    pub fn mounted_mut(&mut self) -> Result<&mut M> {
        self.parts.as_mut().ok_or_else(not_mounted)
    }

    /// Return the pending value and the mounted parts together, or an error
    /// that the widget is not mounted.
    pub fn split_mut(&mut self) -> Result<(&mut P, &mut M)> {
        let parts = self.parts.as_mut().ok_or_else(not_mounted)?;
        Ok((&mut self.pending, parts))
    }

    /// Return the pending value, in either state.
    pub fn pending(&self) -> &P {
        &self.pending
    }

    /// Return the pending value mutably, in either state.
    pub fn pending_mut(&mut self) -> &mut P {
        &mut self.pending
    }
}

/// The error of a widget part read before the widget mounts.
fn not_mounted() -> Error {
    Error::Invalid("widget is not mounted".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply one event to a mount and report the state after it.
    fn step(mount: &mut Mount<u32, u32>, event: &str) -> (bool, bool) {
        let ok = match event {
            "mount" => mount
                .mount(|pending| {
                    *pending += 1;
                    Ok(*pending * 10)
                })
                .is_ok(),
            "fail" => mount
                .mount(|_| Err(Error::Invalid("mount failed".into())))
                .is_ok(),
            "unmount" => {
                mount.unmount();
                true
            }
            other => panic!("unknown event {other}"),
        };
        (ok, mount.is_mounted())
    }

    #[test]
    fn mount_transitions_follow_their_table() {
        // (start mounted, event, succeeds, mounted after)
        let table = [
            (false, "mount", true, true),
            (false, "fail", false, false),
            (false, "unmount", true, false),
            (true, "mount", false, true),
            (true, "fail", false, true),
            (true, "unmount", true, false),
        ];
        for (start, event, ok, mounted) in table {
            let mut mount = Mount::new(0);
            if start {
                step(&mut mount, "mount");
            }
            assert_eq!(
                step(&mut mount, event),
                (ok, mounted),
                "{event} from {start}"
            );
            assert_eq!(mount.mounted().is_ok(), mounted);
            assert_eq!(mount.split_mut().is_ok(), mounted);
        }
    }

    #[test]
    fn a_remount_builds_from_the_kept_pending_value() -> Result<()> {
        let mut mount = Mount::new(0);
        mount.mount(|pending| {
            *pending += 1;
            Ok(*pending)
        })?;
        assert_eq!(*mount.mounted()?, 1);
        mount.unmount();
        let error = mount.mounted().expect_err("a pending value has no parts");
        assert!(error.to_string().contains("not mounted"), "{error}");
        assert_eq!(*mount.pending(), 1, "the pending value survives an unmount");
        mount.mount(|pending| {
            *pending += 1;
            Ok(*pending)
        })?;
        assert_eq!(*mount.mounted()?, 2);
        *mount.mounted_mut()? += 5;
        let (pending, parts) = mount.split_mut()?;
        assert_eq!((*pending, *parts), (2, 7));
        Ok(())
    }
}
