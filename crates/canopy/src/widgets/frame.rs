use super::boxed::{BoxGlyphs, ROUND};
use crate::{
    ViewContext, derive_commands,
    error::Result,
    geom,
    layout::{Edges, Layout},
    render::Render,
    state::NodeName,
    widget::Widget,
};

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

/// A frame around an element with optional title and indicators.
pub struct Frame {
    /// Glyph set for rendering the box border.
    box_glyphs: BoxGlyphs,
    /// Glyph set for rendering scroll indicators.
    scroll_glyphs: ScrollGlyphs,
    /// Optional title string.
    title: Option<String>,
}

#[derive_commands]
impl Frame {
    /// Construct a frame.
    pub fn new() -> Self {
        Self {
            box_glyphs: ROUND,
            scroll_glyphs: SCROLL,
            title: None,
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

    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::all(1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("frame")
    }
}
