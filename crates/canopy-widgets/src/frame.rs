use canopy::{
    Context, EventOutcome, NodeId, ViewContext, Widget, derive_commands,
    error::Result,
    event::{Event, mouse},
    geom,
    layout::{Edges, Layout},
    render::Render,
    state::NodeName,
    view::View,
};

use super::boxed::{BoxGlyphs, ROUND};

/// Defines the set of glyphs used to draw active scroll indicators.
pub struct ScrollGlyphs {
    /// Active vertical indicator glyph.
    pub vertical_active: char,
    /// Active horizontal indicator glyph.
    pub horizontal_active: char,
}

/// Active scroll indicator glyph set.
pub const SCROLL: ScrollGlyphs = ScrollGlyphs {
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Lines to scroll per mouse wheel tick within a frame.
const WHEEL_SCROLL_LINES: i32 = 3;

/// Scrollbar axis used for drag tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollAxis {
    /// Vertical scrollbar.
    Vertical,
    /// Horizontal scrollbar.
    Horizontal,
}

/// Scrollbar drag tracking state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScrollDrag {
    /// Axis being dragged.
    axis: ScrollAxis,
    /// Pointer offset within the active indicator.
    grab_offset: u32,
}

impl ScrollDrag {
    /// Start a vertical scrollbar drag.
    fn vertical(grab_offset: u32) -> Self {
        Self {
            axis: ScrollAxis::Vertical,
            grab_offset,
        }
    }

    /// Start a horizontal scrollbar drag.
    fn horizontal(grab_offset: u32) -> Self {
        Self {
            axis: ScrollAxis::Horizontal,
            grab_offset,
        }
    }
}

/// A frame around an element with optional title and indicators.
pub struct Frame {
    /// Glyph set for rendering the box border.
    box_glyphs: BoxGlyphs,
    /// Glyph set for rendering scroll indicators.
    scroll_glyphs: ScrollGlyphs,
    /// Optional title string.
    title: Option<String>,
    /// Active scrollbar drag state.
    scroll_drag: Option<ScrollDrag>,
}

#[derive_commands]
impl Frame {
    /// Construct a frame.
    pub fn new() -> Self {
        Self {
            box_glyphs: ROUND,
            scroll_glyphs: SCROLL,
            title: None,
            scroll_drag: None,
        }
    }

    /// Build a frame with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.box_glyphs = glyphs;
        self
    }

    /// Build a frame with a specified scroll glyph set.
    pub fn with_scroll_glyphs(mut self, glyphs: ScrollGlyphs) -> Self {
        self.scroll_glyphs = glyphs;
        self
    }

    /// Build a frame with a specified title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Return the glyph set used by the frame.
    pub fn glyphs(&self) -> &BoxGlyphs {
        &self.box_glyphs
    }

    /// Return the optional title string.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Wrap an existing child node in a new frame and return the frame node ID.
    pub fn wrap(c: &mut dyn Context, child: NodeId) -> Result<NodeId> {
        Self::wrap_with(c, child, Self::new())
    }

    /// Wrap an existing child node in a configured frame and return the frame node ID.
    pub fn wrap_with(c: &mut dyn Context, child: NodeId, frame: Self) -> Result<NodeId> {
        let frame_id = c.create_detached(frame);
        c.detach(child)?;
        c.attach(frame_id, child)?;
        Ok(frame_id)
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Frame {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let outer = ctx.view().outer_rect_local();
        let f = geom::Frame::new(outer, 1);
        let style = if ctx.is_on_focus_path() {
            "frame/focused"
        } else {
            "frame"
        };

        self.box_glyphs.draw(rndr, style, f)?;

        if let Some(title) = &self.title {
            let title_with_spaces = format!(" {title} ");
            let title_len = title_with_spaces.len();

            let title_line = f.top.line(0);
            let title_rect = geom::Rect::new(
                title_line.tl.x,
                title_line.tl.y,
                title_len.min(f.top.w as usize) as u32,
                1,
            );
            rndr.text("frame/title", title_rect.line(0), &title_with_spaces)?;
        }

        let child = ctx.children().into_iter().next();
        if let Some(child_id) = child
            && let Some(child_view) = ctx.node_view(child_id)
        {
            if let Some((_, active, _)) = child_view.vactive(f.right)? {
                rndr.fill("frame/active", active, self.scroll_glyphs.vertical_active)?;
            }

            if let Some((_, active, _)) = child_view.hactive(f.bottom)? {
                rndr.fill("frame/active", active, self.scroll_glyphs.horizontal_active)?;
            }
        }

        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        let Event::Mouse(m) = event else {
            return EventOutcome::Ignore;
        };

        let Some(child_id) = ctx.children().into_iter().next() else {
            return EventOutcome::Ignore;
        };
        let Some(child_view) = ctx.node_view(child_id) else {
            return EventOutcome::Ignore;
        };

        let view_size = child_view.content_size();
        let canvas_size = child_view.canvas;
        let outer = ctx.view().outer_rect_local();
        let frame = geom::Frame::new(outer, 1);
        let outer_location = m.location + ctx.view().content_origin();

        if let Some(drag) = self.scroll_drag {
            match m.action {
                mouse::Action::Drag => {
                    if let Some(outcome) =
                        handle_scroll_drag(ctx, child_id, &child_view, &frame, outer_location, drag)
                    {
                        return outcome;
                    }
                    self.scroll_drag = None;
                    ctx.release_mouse();
                    return EventOutcome::Consume;
                }
                mouse::Action::Up if m.button == mouse::Button::Left => {
                    self.scroll_drag = None;
                    ctx.release_mouse();
                    return EventOutcome::Handle;
                }
                _ => {}
            }
        }

        match m.action {
            mouse::Action::ScrollUp => {
                if scrollable(view_size.h, canvas_size.h)
                    && scroll_child_by(ctx, child_id, 0, -WHEEL_SCROLL_LINES)
                {
                    return EventOutcome::Handle;
                }
            }
            mouse::Action::ScrollDown => {
                if scrollable(view_size.h, canvas_size.h)
                    && scroll_child_by(ctx, child_id, 0, WHEEL_SCROLL_LINES)
                {
                    return EventOutcome::Handle;
                }
            }
            mouse::Action::ScrollLeft => {
                if scrollable(view_size.w, canvas_size.w)
                    && scroll_child_by(ctx, child_id, -WHEEL_SCROLL_LINES, 0)
                {
                    return EventOutcome::Handle;
                }
            }
            mouse::Action::ScrollRight => {
                if scrollable(view_size.w, canvas_size.w)
                    && scroll_child_by(ctx, child_id, WHEEL_SCROLL_LINES, 0)
                {
                    return EventOutcome::Handle;
                }
            }
            mouse::Action::Down if m.button == mouse::Button::Left => {
                let mut consumed = false;

                if scrollable(view_size.h, canvas_size.h)
                    && frame.right.contains_point(outer_location)
                {
                    if let Some(active) =
                        scroll_active_rect(&child_view, frame.right, ScrollAxis::Vertical)
                        && active.contains_point(outer_location)
                    {
                        let grab_offset = outer_location.y.saturating_sub(active.tl.y);
                        self.scroll_drag = Some(ScrollDrag::vertical(grab_offset));
                        ctx.capture_mouse();
                        return EventOutcome::Handle;
                    }

                    let pos = outer_location.y.saturating_sub(frame.right.tl.y);
                    let target_y =
                        scroll_offset_for_click(pos, frame.right.h, canvas_size.h, view_size.h);
                    if scroll_child_to(ctx, child_id, child_view.tl.x, target_y) {
                        return EventOutcome::Handle;
                    }
                    consumed = true;
                }

                if scrollable(view_size.w, canvas_size.w)
                    && frame.bottom.contains_point(outer_location)
                {
                    if let Some(active) =
                        scroll_active_rect(&child_view, frame.bottom, ScrollAxis::Horizontal)
                        && active.contains_point(outer_location)
                    {
                        let grab_offset = outer_location.x.saturating_sub(active.tl.x);
                        self.scroll_drag = Some(ScrollDrag::horizontal(grab_offset));
                        ctx.capture_mouse();
                        return EventOutcome::Handle;
                    }

                    let pos = outer_location.x.saturating_sub(frame.bottom.tl.x);
                    let target_x =
                        scroll_offset_for_click(pos, frame.bottom.w, canvas_size.w, view_size.w);
                    if scroll_child_to(ctx, child_id, target_x, child_view.tl.y) {
                        return EventOutcome::Handle;
                    }
                    consumed = true;
                }

                if consumed {
                    return EventOutcome::Consume;
                }
            }
            _ => {}
        }

        EventOutcome::Ignore
    }

    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::all(1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("frame")
    }
}

/// Return true when the canvas is larger than the view.
fn scrollable(view_len: u32, canvas_len: u32) -> bool {
    view_len > 0 && canvas_len > view_len
}

/// Scroll a child node by the provided deltas.
fn scroll_child_by(ctx: &mut dyn Context, child: NodeId, dx: i32, dy: i32) -> bool {
    let mut changed = false;
    if ctx
        .with_widget_mut(child, &mut |_widget, child_ctx| {
            changed = child_ctx.scroll_by(dx, dy);
            Ok(())
        })
        .is_err()
    {
        return false;
    }
    changed
}

/// Scroll a child node to the provided offsets.
fn scroll_child_to(ctx: &mut dyn Context, child: NodeId, x: u32, y: u32) -> bool {
    let mut changed = false;
    if ctx
        .with_widget_mut(child, &mut |_widget, child_ctx| {
            changed = child_ctx.scroll_to(x, y);
            Ok(())
        })
        .is_err()
    {
        return false;
    }
    changed
}

/// Convert a scroll track click into a scroll offset.
fn scroll_offset_for_click(pos: u32, track_len: u32, canvas_len: u32, view_len: u32) -> u32 {
    if track_len == 0 || view_len == 0 || canvas_len <= view_len {
        return 0;
    }

    let max_scroll = canvas_len - view_len;
    let pos = pos.min(track_len.saturating_sub(1));
    let scaled = (u64::from(pos) * u64::from(canvas_len)) / u64::from(track_len.max(1));
    scaled.min(u64::from(max_scroll)) as u32
}

/// Return the active scrollbar indicator rect for the given track and axis.
fn scroll_active_rect(view: &View, track: geom::Rect, axis: ScrollAxis) -> Option<geom::Rect> {
    match axis {
        ScrollAxis::Vertical => view
            .vactive(track)
            .ok()
            .flatten()
            .map(|(_, active, _)| active),
        ScrollAxis::Horizontal => view
            .hactive(track)
            .ok()
            .flatten()
            .map(|(_, active, _)| active),
    }
}

/// Convert a scrollbar drag position into a scroll offset.
fn scroll_offset_for_drag(
    pos: u32,
    track_len: u32,
    thumb_len: u32,
    canvas_len: u32,
    view_len: u32,
) -> u32 {
    if track_len == 0 || thumb_len == 0 || canvas_len <= view_len {
        return 0;
    }

    let scroll_range = canvas_len - view_len;
    let track_range = track_len.saturating_sub(thumb_len);
    if track_range == 0 {
        return 0;
    }

    let pos = pos.min(track_range);
    let scaled = (u64::from(pos) * u64::from(scroll_range)) / u64::from(track_range);
    scaled as u32
}

/// Handle an in-progress scrollbar drag.
fn handle_scroll_drag(
    ctx: &mut dyn Context,
    child_id: NodeId,
    child_view: &View,
    frame: &geom::Frame,
    outer_location: geom::Point,
    drag: ScrollDrag,
) -> Option<EventOutcome> {
    let (track, pointer_pos, view_len, canvas_len) = match drag.axis {
        ScrollAxis::Vertical => (
            frame.right,
            outer_location.y.saturating_sub(frame.right.tl.y),
            child_view.content.h,
            child_view.canvas.h,
        ),
        ScrollAxis::Horizontal => (
            frame.bottom,
            outer_location.x.saturating_sub(frame.bottom.tl.x),
            child_view.content.w,
            child_view.canvas.w,
        ),
    };

    if !scrollable(view_len, canvas_len) {
        return Some(EventOutcome::Consume);
    }

    let active = scroll_active_rect(child_view, track, drag.axis)?;
    let thumb_len = match drag.axis {
        ScrollAxis::Vertical => active.h,
        ScrollAxis::Horizontal => active.w,
    };
    let pos = pointer_pos.saturating_sub(drag.grab_offset);
    let target = scroll_offset_for_drag(
        pos,
        track_len(track, drag.axis),
        thumb_len,
        canvas_len,
        view_len,
    );
    let changed = match drag.axis {
        ScrollAxis::Vertical => scroll_child_to(ctx, child_id, child_view.tl.x, target),
        ScrollAxis::Horizontal => scroll_child_to(ctx, child_id, target, child_view.tl.y),
    };

    Some(if changed {
        EventOutcome::Handle
    } else {
        EventOutcome::Consume
    })
}

/// Return the length of a scrollbar track for the given axis.
fn track_len(track: geom::Rect, axis: ScrollAxis) -> u32 {
    match axis {
        ScrollAxis::Vertical => track.h,
        ScrollAxis::Horizontal => track.w,
    }
}
