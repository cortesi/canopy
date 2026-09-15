use canopy::{
    Context, EventOutcome, NodeId, NodeName, Render, ViewContext, Widget, derive_commands,
    error::Result,
    event::Event,
    geom,
    layout::{Edges, Layout},
};
use unicode_width::UnicodeWidthStr;

use super::boxed::{BoxGlyphs, ROUND};
use crate::Scrollbar;

/// Active vertical scrollbar indicator.
const SCROLL_VERTICAL: char = '█';
/// Active horizontal scrollbar indicator.
const SCROLL_HORIZONTAL: char = '▄';

/// A frame around an element with optional title and indicators.
///
/// The frame draws scrollbars on its right and bottom edges for its first
/// child, and scrolls that child when the scrollbars are pressed or dragged.
pub struct Frame {
    /// Glyph set for rendering the box border.
    box_glyphs: BoxGlyphs,
    /// Optional title string.
    title: Option<String>,
    /// Scrollbar on the right edge.
    vertical: Scrollbar,
    /// Scrollbar on the bottom edge.
    horizontal: Scrollbar,
}

#[derive_commands]
impl Frame {
    /// Construct a frame.
    pub fn new() -> Self {
        Self {
            box_glyphs: ROUND,
            title: None,
            vertical: Scrollbar::vertical("frame/active", SCROLL_VERTICAL),
            horizontal: Scrollbar::horizontal("frame/active", SCROLL_HORIZONTAL),
        }
    }

    /// Build a frame with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.box_glyphs = glyphs;
        self
    }

    /// Build a frame with a specified title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Replace the title.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
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
        let f = geom::FrameRects::new(outer, 1);
        let style = if ctx.is_on_focus_path() {
            "frame/focused"
        } else {
            "frame"
        };

        self.box_glyphs.draw(rndr, style, f)?;

        if let Some(title) = &self.title
            && f.top.w > 0
            && f.top.h > 0
        {
            let title_with_spaces = format!(" {title} ");
            let title_len = UnicodeWidthStr::width(title_with_spaces.as_str());

            let title_line = f.top.line(0)?;
            let title_rect = geom::Rect::new(
                title_line.tl.x,
                title_line.tl.y,
                title_len.min(f.top.w as usize) as u32,
                1,
            );
            rndr.text("frame/title", title_rect.line(0)?, &title_with_spaces)?;
        }

        let child = ctx.children().into_iter().next();
        if let Some(child_id) = child
            && let Some(child_view) = ctx.view_of(child_id)
        {
            self.vertical.render(rndr, &child_view, f.right)?;
            self.horizontal.render(rndr, &child_view, f.bottom)?;
        }

        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(m) = event else {
            return Ok(EventOutcome::Ignore);
        };

        let Some(child_id) = ctx.children().into_iter().next() else {
            return Ok(EventOutcome::Ignore);
        };
        let Some(child_view) = ctx.view_of(child_id) else {
            return Ok(EventOutcome::Ignore);
        };

        let view_size = child_view.content_size();
        let canvas_size = child_view.canvas;
        if let Some(delta) = m.action.scroll_delta() {
            let scrollable = if delta.y == 0 {
                scrollable(view_size.w, canvas_size.w)
            } else {
                scrollable(view_size.h, canvas_size.h)
            };
            if scrollable && scroll_child_by(ctx, child_id, delta.x, delta.y) {
                return Ok(EventOutcome::Handle);
            }
            return Ok(EventOutcome::Ignore);
        }

        let frame = geom::FrameRects::new(ctx.view().outer_rect_local(), 1);
        let scroll_child = |ctx: &mut dyn Context, x: u32, y: u32| {
            scroll_child_to(ctx, child_id, x, y);
            Ok(())
        };
        if self
            .vertical
            .handle_mouse(ctx, m, &child_view, frame.right, scroll_child)?
            == EventOutcome::Handle
        {
            return Ok(EventOutcome::Handle);
        }
        self.horizontal
            .handle_mouse(ctx, m, &child_view, frame.bottom, scroll_child)
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
        .with_widget_dyn_mut(child, &mut |_widget, child_ctx| {
            changed = child_ctx.scroll_by(dx, dy).changed();
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
        .with_widget_dyn_mut(child, &mut |_widget, child_ctx| {
            changed = child_ctx.scroll_to(x, y).changed();
            Ok(())
        })
        .is_err()
    {
        return false;
    }
    changed
}
