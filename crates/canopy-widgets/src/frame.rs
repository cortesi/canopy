use canopy::{
    Context, EventOutcome, NodeId, NodeName, Render, ViewContext, Widget, derive_commands,
    error::Result,
    event::Event,
    geom::{self, Rect},
    layout::{Edges, Layout},
};
use unicode_width::UnicodeWidthStr;

use super::boxed::{BoxGlyphs, ROUND};
use crate::scrollbar::{Axis, Scrollbar, edge_track, scroll_target};

/// Vertical scrollbar thumb glyph.
const SCROLL_VERTICAL: char = '█';
/// Horizontal scrollbar thumb glyph.
const SCROLL_HORIZONTAL: char = '▄';

/// A frame around an element with an optional title and scroll positions.
///
/// A frame owns the scrollbars of its subtree. On each axis it finds the one
/// node beneath it whose canvas overflows, and draws that node's thumb on the
/// right or bottom border beside the node's visible rows or columns. The
/// border carries a track only where that node reaches it, so a sidebar,
/// header, or footer keeps a plain border. Wheel input, presses, and drags on
/// a track scroll the node. Thumbs use `frame/thumb`, and
/// `frame/thumb/active` while a drag holds them.
pub struct Frame {
    /// Glyph set for rendering the box border.
    box_glyphs: BoxGlyphs,
    /// Optional title string.
    title: Option<String>,
    /// Scrollbar on the right border.
    vertical: Scrollbar,
    /// Scrollbar on the bottom border.
    horizontal: Scrollbar,
}

#[derive_commands]
impl Frame {
    /// Construct a frame.
    pub fn new() -> Self {
        Self {
            box_glyphs: ROUND,
            title: None,
            vertical: Scrollbar::vertical("frame/thumb", SCROLL_VERTICAL)
                .with_active("frame/thumb/active"),
            horizontal: Scrollbar::horizontal("frame/thumb", SCROLL_HORIZONTAL)
                .with_active("frame/thumb/active"),
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

    /// Return the border track on one axis and the node it scrolls.
    fn track(ctx: &dyn ViewContext, axis: Axis) -> Result<Option<(NodeId, Rect)>> {
        let frame = ctx.node_id();
        let Some(target) = scroll_target(ctx, frame, axis)? else {
            return Ok(None);
        };
        let border = geom::FrameRects::new(ctx.view().outer_rect_local(), 1);
        let edge = match axis {
            Axis::Vertical => border.right,
            Axis::Horizontal => border.bottom,
        };
        Ok(edge_track(ctx, &target, frame, axis, edge).map(|track| (target.node, track)))
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

        let vertical = Self::track(ctx, Axis::Vertical)?;
        self.vertical.render(rndr, ctx, vertical.as_slice())?;
        let horizontal = Self::track(ctx, Axis::Horizontal)?;
        self.horizontal.render(rndr, ctx, horizontal.as_slice())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(mouse) = event else {
            return Ok(EventOutcome::Ignore);
        };
        let vertical = Self::track(ctx, Axis::Vertical)?;
        if self
            .vertical
            .handle_mouse(ctx, mouse, vertical.as_slice())?
            == EventOutcome::Handle
        {
            return Ok(EventOutcome::Handle);
        }
        let horizontal = Self::track(ctx, Axis::Horizontal)?;
        self.horizontal
            .handle_mouse(ctx, mouse, horizontal.as_slice())
    }

    fn owns_scrollbars(&self) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::all(1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("frame")
    }
}
