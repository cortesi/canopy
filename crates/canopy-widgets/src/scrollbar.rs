//! Proportional scrollbars and the nodes they display.
//!
//! A scrollbar owner, such as [`crate::Frame`], draws the scroll position of a
//! node beneath it. [`scroll_target`] finds that node for one axis. A
//! [`Scrollbar`] draws the node's thumb into a track in the owner's outer
//! coordinates and turns pointer input on the track into scrolling. Wheel
//! input on a track scrolls the node. A press on the track centers the thumb on
//! the pointer, and a drag keeps the thumb under the pointer.

use canopy::{
    Context, EventOutcome, NodeId, Render, ScrollAxis, View, ViewContext,
    error::Result,
    event::mouse,
    geom::{Point, PointI32, Rect, RectI32, Size},
};

/// The axis a scrollbar measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Rows, with a track that runs top to bottom.
    Vertical,
    /// Columns, with a track that runs left to right.
    Horizontal,
}

/// The glyphs drawn on scrollbar tracks, following the
/// [`BoxGlyphs`](crate::BoxGlyphs) pattern.
///
/// Owners draw thumbs with the `thumb_*` glyphs and track backgrounds with
/// the `track_*` glyphs. Marks that blend into the track reuse the track
/// glyph of their axis and read through color alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarGlyphs {
    /// Thumb on a vertical track.
    pub thumb_vertical: char,
    /// Thumb on a horizontal track.
    pub thumb_horizontal: char,
    /// Background line of a vertical track around the thumb.
    pub track_vertical: char,
    /// Background line of a horizontal track around the thumb.
    pub track_horizontal: char,
}

/// Thin-line scrollbar glyphs over thin-line chrome.
pub const THIN: ScrollbarGlyphs = ScrollbarGlyphs {
    thumb_vertical: '█',
    thumb_horizontal: '▄',
    track_vertical: '│',
    track_horizontal: '─',
};

/// A node whose canvas overflows its content along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollTarget {
    /// Node whose view scrolls.
    pub node: NodeId,
    /// The node's content viewport in screen coordinates, clipped by the
    /// content of every ancestor and by the screen.
    pub viewport: Rect,
}

/// The nodes a subtree offers for one axis.
enum Resolution {
    /// No node overflows.
    Absent,
    /// Exactly one node overflows.
    Unique(ScrollTarget),
    /// More than one node overflows.
    Ambiguous,
}

impl Resolution {
    /// Merge the results of two sibling subtrees.
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Absent, found) | (found, Self::Absent) => found,
            _ => Self::Ambiguous,
        }
    }
}

/// Return the one node beneath `owner` whose canvas overflows along `axis`.
///
/// The search skips nodes with no visible content, which includes hidden
/// nodes. It does not descend into a node that overflows or into a widget that
/// owns scrollbars. It returns `None` when no node qualifies, when more than
/// one does, or when `owner` is detached. The owner's own widget is never
/// read, so an owner can call this while it renders or handles an event.
pub fn scroll_target(
    ctx: &dyn ViewContext,
    owner: NodeId,
    axis: Axis,
) -> Result<Option<ScrollTarget>> {
    if !ctx.is_attached_of(owner) {
        return Ok(None);
    }
    let Some(clip) = ancestor_clip(ctx, owner) else {
        return Ok(None);
    };
    Ok(match resolve_children(ctx, owner, axis, clip)? {
        Resolution::Unique(target) => Some(target),
        Resolution::Absent | Resolution::Ambiguous => None,
    })
}

/// Return the one node in `pane`, a child of `owner`, whose canvas overflows
/// along `axis`.
///
/// Unlike [`scroll_target`], the pane itself can be the target.
pub(crate) fn pane_target(
    ctx: &dyn ViewContext,
    owner: NodeId,
    pane: NodeId,
    axis: Axis,
) -> Result<Option<ScrollTarget>> {
    if !ctx.is_attached_of(owner) {
        return Ok(None);
    }
    let Some(clip) = ancestor_clip(ctx, owner) else {
        return Ok(None);
    };
    Ok(match resolve(ctx, pane, axis, clip)? {
        Resolution::Unique(target) => Some(target),
        Resolution::Absent | Resolution::Ambiguous => None,
    })
}

/// Return the screen area visible through `node` and each of its ancestors.
fn ancestor_clip(ctx: &dyn ViewContext, node: NodeId) -> Option<Rect> {
    let screen = ctx.view_of(ctx.root_id())?.outer;
    let mut clip = visible(screen, Rect::new(0, 0, screen.w, screen.h))?;
    let mut current = Some(node);
    while let Some(id) = current {
        clip = visible(ctx.view_of(id)?.content, clip)?;
        current = ctx.parent_of(id);
    }
    Some(clip)
}

/// Return the nonempty part of `rect` inside `clip`.
fn visible(rect: RectI32, clip: Rect) -> Option<Rect> {
    rect.intersect_rect(clip).filter(|area| !area.is_empty())
}

/// Resolve the children of `node`, visible through `clip`.
fn resolve_children(
    ctx: &dyn ViewContext,
    node: NodeId,
    axis: Axis,
    clip: Rect,
) -> Result<Resolution> {
    let mut found = Resolution::Absent;
    for child in ctx.children_of(node) {
        found = found.combine(resolve(ctx, child, axis, clip)?);
        if matches!(found, Resolution::Ambiguous) {
            break;
        }
    }
    Ok(found)
}

/// Resolve the subtree at `node`, visible through `clip`.
fn resolve(ctx: &dyn ViewContext, node: NodeId, axis: Axis, clip: Rect) -> Result<Resolution> {
    let Some(view) = ctx.view_of(node) else {
        return Ok(Resolution::Absent);
    };
    let Some(viewport) = visible(view.content, clip) else {
        return Ok(Resolution::Absent);
    };
    let mut owns_scrollbars = false;
    ctx.with_widget_dyn(node, &mut |widget| {
        owns_scrollbars = widget.owns_scrollbars();
        Ok(())
    })?;
    if owns_scrollbars {
        return Ok(Resolution::Absent);
    }
    let overflows = match axis {
        Axis::Vertical => view.canvas.h > view.content.h,
        Axis::Horizontal => view.canvas.w > view.content.w,
    };
    if overflows {
        return Ok(Resolution::Unique(ScrollTarget { node, viewport }));
    }
    resolve_children(ctx, node, axis, viewport)
}

/// Return the cells of `edge` beside a target's viewport, in the outer
/// coordinates of the context's node.
///
/// A vertical target projects its rows onto a right edge, and a horizontal
/// target projects its columns onto a bottom edge. The track exists only when
/// the target reaches that side: every node from the target up to, but not
/// including, `boundary` must end at its parent's content edge. Padding between
/// a node and its parent's outer edge does not break the chain.
pub(crate) fn edge_track(
    ctx: &dyn ViewContext,
    target: &ScrollTarget,
    boundary: NodeId,
    axis: Axis,
    edge: Rect,
) -> Option<Rect> {
    let mut node = target.node;
    while node != boundary {
        let parent = ctx.parent_of(node)?;
        let (outer, content) = (ctx.view_of(node)?.outer, ctx.view_of(parent)?.content);
        let flush = match axis {
            Axis::Vertical => outer.right() == content.right(),
            Axis::Horizontal => outer.bottom() == content.bottom(),
        };
        if !flush {
            return None;
        }
        node = parent;
    }

    let owner = ctx.view().outer;
    let viewport = target.viewport;
    let (start, length, edge_start, edge_length) = match axis {
        Axis::Vertical => (
            i64::from(viewport.tl.y) - i64::from(owner.tl.y),
            viewport.h,
            edge.tl.y,
            edge.h,
        ),
        Axis::Horizontal => (
            i64::from(viewport.tl.x) - i64::from(owner.tl.x),
            viewport.w,
            edge.tl.x,
            edge.w,
        ),
    };
    let from = start.max(i64::from(edge_start));
    let to = (start + i64::from(length)).min(i64::from(edge_start) + i64::from(edge_length));
    if from >= to {
        return None;
    }
    let (from, length) = (u32::try_from(from).ok()?, u32::try_from(to - from).ok()?);
    Some(match axis {
        Axis::Vertical => Rect::new(edge.tl.x, from, edge.w, length),
        Axis::Horizontal => Rect::new(from, edge.tl.y, length, edge.h),
    })
}

/// A drag that holds a thumb.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// Node the drag scrolls.
    target: NodeId,
    /// Track the drag started on.
    track: Rect,
    /// Pointer offset within the thumb.
    grip: u32,
    /// Whether rendering found the target or track changed. The next event
    /// releases capture.
    cancelled: bool,
}

/// A proportional scrollbar for one axis.
///
/// The owner resolves its tracks before each render and event, and passes each
/// track with the node it scrolls. A drag stays bound to the node and track it
/// started on. It ends when either changes, or when another node takes mouse
/// capture.
#[derive(Clone, Copy, Debug)]
pub struct Scrollbar {
    /// Axis the scrollbar measures.
    axis: Axis,
    /// Style path and glyph of the thumb.
    thumb: (&'static str, char),
    /// Style path of the thumb while a drag holds it.
    active: Option<&'static str>,
    /// Style path and glyph of the track around the thumb, when drawn.
    track: Option<(&'static str, char)>,
    /// The drag in progress.
    drag: Option<Drag>,
}

impl Scrollbar {
    /// Construct a vertical scrollbar whose thumb uses `style` and `glyph`.
    pub const fn vertical(style: &'static str, glyph: char) -> Self {
        Self::new(Axis::Vertical, style, glyph)
    }

    /// Construct a horizontal scrollbar whose thumb uses `style` and `glyph`.
    pub const fn horizontal(style: &'static str, glyph: char) -> Self {
        Self::new(Axis::Horizontal, style, glyph)
    }

    /// Construct a scrollbar for `axis`.
    const fn new(axis: Axis, style: &'static str, glyph: char) -> Self {
        Self {
            axis,
            thumb: (style, glyph),
            active: None,
            track: None,
            drag: None,
        }
    }

    /// Draw the thumb with `style` while a drag holds it.
    #[must_use]
    pub const fn with_active(mut self, style: &'static str) -> Self {
        self.active = Some(style);
        self
    }

    /// Also draw the track around the thumb with `style` and `glyph`.
    #[must_use]
    pub const fn with_track(mut self, style: &'static str, glyph: char) -> Self {
        self.track = Some((style, glyph));
        self
    }

    /// Return the parts of `track` before, under, and after the thumb, or
    /// `None` when `view` shows its whole canvas along this axis.
    fn parts(&self, view: &View, track: Rect) -> Option<(Rect, Rect, Rect)> {
        let parts = match self.axis {
            Axis::Vertical => view.vactive(track),
            Axis::Horizontal => view.hactive(track),
        };
        parts.ok().flatten()
    }

    /// Return the length of `track` along this axis.
    fn length(&self, track: Rect) -> u32 {
        match self.axis {
            Axis::Vertical => track.h,
            Axis::Horizontal => track.w,
        }
    }

    /// Draw the thumb of each track's node.
    ///
    /// `tracks` pairs each node with its track, in the outer coordinates of
    /// the context's node. A node that shows its whole canvas draws nothing.
    /// Rendering cannot release mouse capture, so a drag whose node or track
    /// has changed draws no active thumb and ends at the next event. After
    /// the thumb, each target's [`ScrollMark`](canopy::ScrollMark)s draw over
    /// the track, so scrolling the thumb across a mark keeps the mark's
    /// color under the thumb's glyph.
    pub fn render(
        &mut self,
        render: &mut Render,
        ctx: &dyn ViewContext,
        tracks: &[(NodeId, Rect)],
    ) -> Result<()> {
        if !ctx.has_mouse_capture() {
            self.drag = None;
        }
        if let Some(drag) = &mut self.drag
            && !tracks.contains(&(drag.target, drag.track))
        {
            drag.cancelled = true;
        }
        for &(target, track) in tracks {
            let Some(view) = ctx.view_of(target) else {
                continue;
            };
            let Some((before, thumb, after)) = self.parts(&view, track) else {
                continue;
            };
            if let Some((style, glyph)) = self.track {
                render.fill(style, before, glyph)?;
                render.fill(style, after, glyph)?;
            }
            let held = self.drag.is_some_and(|drag| {
                !drag.cancelled && drag.target == target && drag.track == track
            });
            let style = match self.active {
                Some(active) if held => active,
                _ => self.thumb.0,
            };
            render.fill(style, thumb, self.thumb.1)?;
            self.render_marks(render, ctx, target, &view, track, thumb)?;
        }
        Ok(())
    }

    /// Draw the scroll marks a target reports onto its track.
    ///
    /// A marked region covers every track cell from its first to its last
    /// mapped cell. A covered cell outside the thumb draws with the mark's
    /// style and glyph; a covered cell under the thumb draws with the mark's
    /// style and the thumb's glyph, so the thumb reads solid while the
    /// mark's color shows through.
    fn render_marks(
        &self,
        render: &mut Render,
        ctx: &dyn ViewContext,
        target: NodeId,
        view: &View,
        track: Rect,
        thumb: Rect,
    ) -> Result<()> {
        let mut marks = Vec::new();
        ctx.with_widget_dyn(target, &mut |widget| {
            marks =
                widget.scroll_marks(self.core_axis(), Size::new(view.content.w, view.content.h));
            Ok(())
        })?;
        if marks.is_empty() {
            return Ok(());
        }
        let canvas_len = match self.axis {
            Axis::Vertical => view.canvas.h,
            Axis::Horizontal => view.canvas.w,
        };
        let track_len = self.length(track);
        for mark in marks {
            if mark.end <= mark.start {
                continue;
            }
            let (Some(first), Some(last)) = (
                Self::mark_cell(mark.start, canvas_len, track_len),
                Self::mark_cell(mark.end.saturating_sub(1), canvas_len, track_len),
            ) else {
                continue;
            };
            for pos in first..=last {
                let cell = match self.axis {
                    Axis::Vertical => Rect::new(track.tl.x, track.tl.y + pos, track.w, 1),
                    Axis::Horizontal => Rect::new(track.tl.x + pos, track.tl.y, 1, track.h),
                };
                if thumb.contains_point(cell.tl) {
                    render.fill(mark.style, cell, self.thumb.1)?;
                } else {
                    render.fill(mark.style, cell, mark.glyph)?;
                }
            }
        }
        Ok(())
    }

    /// Return the track-cell index for a canvas offset.
    ///
    /// The offset maps proportionally, the way [`View::vactive`] places the
    /// thumb, and clamps to the last cell, so the final canvas row owns the
    /// end of the track.
    fn mark_cell(offset: u32, canvas_len: u32, track_len: u32) -> Option<u32> {
        if track_len == 0 || canvas_len == 0 {
            return None;
        }
        let cell = u64::from(offset) * u64::from(track_len) / u64::from(canvas_len);
        u32::try_from(cell.min(u64::from(track_len) - 1)).ok()
    }

    /// Convert this scrollbar's axis to the core axis marks are reported on.
    fn core_axis(&self) -> ScrollAxis {
        match self.axis {
            Axis::Vertical => ScrollAxis::Vertical,
            Axis::Horizontal => ScrollAxis::Horizontal,
        }
    }

    /// Handle a mouse event for the scrollbars drawn in `tracks`.
    ///
    /// `tracks` holds the tracks resolved for this event, as passed to
    /// [`Scrollbar::render`]. A press on a track captures the mouse for the
    /// context's node and the release frees it. Returns
    /// [`EventOutcome::Handle`] for events the scrollbar uses.
    pub fn handle_mouse(
        &mut self,
        ctx: &mut dyn Context,
        event: &mouse::MouseEvent,
        tracks: &[(NodeId, Rect)],
    ) -> Result<EventOutcome> {
        if let Some(outcome) = self.settle_drag(ctx, event, tracks)? {
            return Ok(outcome);
        }
        let view = ctx.view();
        if let Some(delta) = event.action.scroll_delta() {
            let pointer = view.outer_point(event.location);
            return self.wheel(ctx, pointer, delta, tracks);
        }
        match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                match view.outer_point(event.location) {
                    Some(pointer) => self.press(ctx, pointer, tracks),
                    None => Ok(EventOutcome::Ignore),
                }
            }
            mouse::Action::Drag => {
                let Some(drag) = self.drag else {
                    return Ok(EventOutcome::Ignore);
                };
                // A captured drag can leave the widget, so clamp it to the
                // widget's edges.
                let pointer = view.viewport_to_outer(event.location)?.clamped_point();
                self.drag_to(ctx, drag, pointer)?;
                Ok(EventOutcome::Handle)
            }
            mouse::Action::Up if event.button == mouse::Button::Left && self.drag.is_some() => {
                self.drag = None;
                ctx.release_mouse()?;
                Ok(EventOutcome::Handle)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }

    /// Confirm a held drag before an event. Returns an outcome when the event
    /// ends with the drag.
    fn settle_drag(
        &mut self,
        ctx: &mut dyn Context,
        event: &mouse::MouseEvent,
        tracks: &[(NodeId, Rect)],
    ) -> Result<Option<EventOutcome>> {
        let Some(drag) = self.drag else {
            return Ok(None);
        };
        if !ctx.has_mouse_capture() {
            // Another node took capture, which is not ours to release.
            self.drag = None;
            return Ok(None);
        }
        if !drag.cancelled && tracks.contains(&(drag.target, drag.track)) {
            return Ok(None);
        }
        self.drag = None;
        ctx.release_mouse()?;
        let ends = matches!(event.action, mouse::Action::Drag | mouse::Action::Up);
        Ok(ends.then_some(EventOutcome::Handle))
    }

    /// Scroll the node under the pointer by one wheel step along this axis.
    fn wheel(
        &self,
        ctx: &mut dyn Context,
        pointer: Option<Point>,
        delta: PointI32,
        tracks: &[(NodeId, Rect)],
    ) -> Result<EventOutcome> {
        let (dx, dy) = match self.axis {
            Axis::Vertical => (0, delta.y),
            Axis::Horizontal => (delta.x, 0),
        };
        let target = pointer.and_then(|pointer| {
            tracks
                .iter()
                .find(|(_, track)| track.contains_point(pointer))
        });
        let Some((target, view)) =
            target.and_then(|&(target, _)| Some((target, ctx.view_of(target)?)))
        else {
            return Ok(EventOutcome::Ignore);
        };
        let offset = clamp_offset(&view, view.scroll.scroll(dx, dy));
        if offset == view.scroll {
            return Ok(EventOutcome::Ignore);
        }
        ctx.scroll_to_of(target, offset.x, offset.y)?;
        Ok(EventOutcome::Handle)
    }

    /// Start a drag from a press on a track.
    fn press(
        &mut self,
        ctx: &mut dyn Context,
        pointer: Point,
        tracks: &[(NodeId, Rect)],
    ) -> Result<EventOutcome> {
        let Some(&(target, track)) = tracks
            .iter()
            .find(|(_, track)| track.contains_point(pointer))
        else {
            return Ok(EventOutcome::Ignore);
        };
        let Some(view) = ctx.view_of(target) else {
            return Ok(EventOutcome::Ignore);
        };
        let Some((_, thumb, _)) = self.parts(&view, track) else {
            return Ok(EventOutcome::Ignore);
        };
        let (position, thumb_start, thumb_len) = self.along(pointer, track, thumb);
        // A thumb that fills its track cannot travel, and a node that already
        // holds capture is running another drag.
        if thumb_len >= self.length(track) || ctx.has_mouse_capture() {
            return Ok(EventOutcome::Handle);
        }
        let on_thumb = thumb.contains_point(pointer);
        let grip = if on_thumb {
            position.saturating_sub(thumb_start)
        } else {
            thumb_len / 2
        };
        ctx.capture_mouse()?;
        self.drag = Some(Drag {
            target,
            track,
            grip,
            cancelled: false,
        });
        if !on_thumb {
            let start = position.saturating_sub(grip);
            self.scroll(ctx, target, &view, track, thumb_len, start)?;
        }
        Ok(EventOutcome::Handle)
    }

    /// Move a held thumb under the pointer, against the node's current range.
    fn drag_to(&self, ctx: &mut dyn Context, drag: Drag, pointer: Point) -> Result<()> {
        let Some(view) = ctx.view_of(drag.target) else {
            return Ok(());
        };
        let Some((_, thumb, _)) = self.parts(&view, drag.track) else {
            return Ok(());
        };
        let (position, _, thumb_len) = self.along(pointer, drag.track, thumb);
        let start = position.saturating_sub(drag.grip);
        self.scroll(ctx, drag.target, &view, drag.track, thumb_len, start)
    }

    /// Return the pointer position, thumb start, and thumb length along this
    /// axis, measured from the start of `track`.
    fn along(&self, pointer: Point, track: Rect, thumb: Rect) -> (u32, u32, u32) {
        match self.axis {
            Axis::Vertical => (
                pointer.y.saturating_sub(track.tl.y),
                thumb.tl.y.saturating_sub(track.tl.y),
                thumb.h,
            ),
            Axis::Horizontal => (
                pointer.x.saturating_sub(track.tl.x),
                thumb.tl.x.saturating_sub(track.tl.x),
                thumb.w,
            ),
        }
    }

    /// Scroll `target` so its thumb starts `thumb_start` cells into `track`.
    fn scroll(
        &self,
        ctx: &mut dyn Context,
        target: NodeId,
        view: &View,
        track: Rect,
        thumb_len: u32,
        thumb_start: u32,
    ) -> Result<()> {
        let (x, y) = match self.axis {
            Axis::Vertical => (
                view.scroll.x,
                offset_for_thumb_start(
                    thumb_start,
                    track.h,
                    thumb_len,
                    view.canvas.h,
                    view.content.h,
                ),
            ),
            Axis::Horizontal => (
                offset_for_thumb_start(
                    thumb_start,
                    track.w,
                    thumb_len,
                    view.canvas.w,
                    view.content.w,
                ),
                view.scroll.y,
            ),
        };
        ctx.scroll_to_of(target, x, y)?;
        Ok(())
    }
}

/// Clamp a scroll offset to the range of `view`.
fn clamp_offset(view: &View, offset: Point) -> Point {
    let limit = |canvas: u32, content: u32| {
        if content == 0 {
            0
        } else {
            canvas.saturating_sub(content)
        }
    };
    Point {
        x: offset.x.min(limit(view.canvas.w, view.content.w)),
        y: offset.y.min(limit(view.canvas.h, view.content.h)),
    }
}

/// Return the scroll offset whose thumb starts `thumb_start` cells into the
/// track.
///
/// This inverts the thumb placement of [`View::vactive`], so a dragged thumb
/// stays under the pointer. A thumb at the end of the track reaches the last
/// offset, which the rounded-up thumb length could otherwise leave out.
fn offset_for_thumb_start(
    thumb_start: u32,
    track_len: u32,
    thumb_len: u32,
    canvas_len: u32,
    view_len: u32,
) -> u32 {
    let max_offset = canvas_len.saturating_sub(view_len);
    if track_len == 0 || max_offset == 0 {
        return 0;
    }
    if thumb_start >= track_len.saturating_sub(thumb_len) {
        return max_offset;
    }
    let offset = (u64::from(thumb_start) * u64::from(canvas_len)).div_ceil(u64::from(track_len));
    u32::try_from(offset).unwrap_or(u32::MAX).min(max_offset)
}

#[cfg(test)]
mod tests {
    use canopy::geom::{RectI32, Size};
    use proptest::prelude::*;

    use super::*;

    /// Return the thumb of a vertical track for a view scrolled to `offset`.
    fn thumb(track_len: u32, canvas_len: u32, view_len: u32, offset: u32) -> Rect {
        let view = View::new(
            RectI32::new(0, 0, 1, view_len),
            RectI32::new(0, 0, 1, view_len),
            Point { x: 0, y: offset },
            Size::new(1, canvas_len),
        );
        Scrollbar::vertical("thumb", '#')
            .parts(&view, Rect::new(0, 0, 1, track_len))
            .expect("the canvas is taller than the view")
            .1
    }

    proptest! {
        #[test]
        fn a_dragged_thumb_stays_under_the_pointer_and_reaches_the_end(
            track_len in 1u32..40,
            view_len in 1u32..40,
            extra in 1u32..200,
            start in 0u32..40,
        ) {
            // Once the canvas is at least as long as the track, each thumb
            // start has an offset that places the thumb exactly there.
            let canvas_len = (view_len + extra).max(track_len);
            let thumb_len = thumb(track_len, canvas_len, view_len, 0).h;
            let last_start = track_len - thumb_len;
            let start = start.min(last_start);

            let offset = offset_for_thumb_start(start, track_len, thumb_len, canvas_len, view_len);
            prop_assert!(offset <= canvas_len - view_len);
            prop_assert_eq!(thumb(track_len, canvas_len, view_len, offset).tl.y, start);
            if start == last_start {
                prop_assert_eq!(offset, canvas_len - view_len);
            }
        }
    }

    #[test]
    fn marks_map_proportionally_and_clamp_to_the_track() {
        assert_eq!(Scrollbar::mark_cell(0, 100, 10), Some(0));
        assert_eq!(Scrollbar::mark_cell(50, 100, 10), Some(5));
        assert_eq!(Scrollbar::mark_cell(99, 100, 10), Some(9));
        // An offset past the canvas still lands on the last cell.
        assert_eq!(Scrollbar::mark_cell(1_000, 100, 10), Some(9));
        assert_eq!(Scrollbar::mark_cell(0, 0, 10), None);
        assert_eq!(Scrollbar::mark_cell(0, 100, 0), None);
    }

    #[test]
    fn a_view_that_fits_has_no_scrollbar_and_no_offset() {
        let view = View::new(
            RectI32::new(0, 0, 1, 5),
            RectI32::new(0, 0, 1, 5),
            Point::ZERO,
            Size::new(1, 5),
        );
        assert!(
            Scrollbar::vertical("thumb", '#')
                .parts(&view, Rect::new(0, 0, 1, 5))
                .is_none()
        );
        assert_eq!(offset_for_thumb_start(3, 5, 5, 5, 5), 0);
        assert_eq!(offset_for_thumb_start(0, 0, 0, 30, 5), 0);
    }
}
