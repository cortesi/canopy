use crate::{
    ViewContext, derive_commands,
    error::Result,
    geom,
    layout::{self, LengthPercentage, Style},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// Defines the set of glyphs used to draw the frame.
pub struct FrameGlyphs {
    /// Top-left corner glyph.
    pub topleft: char,
    /// Top-right corner glyph.
    pub topright: char,
    /// Bottom-left corner glyph.
    pub bottomleft: char,
    /// Bottom-right corner glyph.
    pub bottomright: char,
    /// Horizontal border glyph.
    pub horizontal: char,
    /// Vertical border glyph.
    pub vertical: char,
    /// Active vertical indicator glyph.
    pub vertical_active: char,
    /// Active horizontal indicator glyph.
    pub horizontal_active: char,
}

/// Single line thin Unicode box drawing frame set.
pub const SINGLE: FrameGlyphs = FrameGlyphs {
    topleft: '┌',
    topright: '┐',
    bottomleft: '└',
    bottomright: '┘',
    horizontal: '─',
    vertical: '│',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Double line Unicode box drawing frame set.
pub const DOUBLE: FrameGlyphs = FrameGlyphs {
    topleft: '╔',
    topright: '╗',
    bottomleft: '╚',
    bottomright: '╝',
    horizontal: '═',
    vertical: '║',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Single line thick Unicode box drawing frame set.
pub const SINGLE_THICK: FrameGlyphs = FrameGlyphs {
    topleft: '┏',
    topright: '┓',
    bottomleft: '┗',
    bottomright: '┛',
    horizontal: '━',
    vertical: '┃',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Round corner thin Unicode box drawing frame set.
pub const ROUND: FrameGlyphs = FrameGlyphs {
    topleft: '╭',
    topright: '╮',
    bottomleft: '╰',
    bottomright: '╯',
    horizontal: '─',
    vertical: '│',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Round corner thick Unicode box drawing frame set.
pub const ROUND_THICK: FrameGlyphs = FrameGlyphs {
    topleft: '╭',
    topright: '╮',
    bottomleft: '╰',
    bottomright: '╯',
    horizontal: '━',
    vertical: '┃',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// A frame around an element with optional title and indicators.
pub struct Frame {
    /// Glyph set for rendering.
    glyphs: FrameGlyphs,
    /// Optional title string.
    title: Option<String>,
}

#[derive_commands]
impl Frame {
    /// Construct a frame.
    pub fn new() -> Self {
        Self {
            glyphs: ROUND,
            title: None,
        }
    }

    /// Build a frame with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: FrameGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a frame with a specified title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Return the glyph set used by the frame.
    pub fn glyphs(&self) -> &FrameGlyphs {
        &self.glyphs
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
    fn render(&mut self, rndr: &mut Render, area: geom::Rect, ctx: &dyn ViewContext) -> Result<()> {
        let f = geom::Frame::new(area, 1);
        let style = if ctx.is_on_focus_path() {
            "frame/focused"
        } else {
            "frame"
        };

        rndr.fill(style, f.topleft, self.glyphs.topleft)?;
        rndr.fill(style, f.topright, self.glyphs.topright)?;
        rndr.fill(style, f.bottomleft, self.glyphs.bottomleft)?;
        rndr.fill(style, f.bottomright, self.glyphs.bottomright)?;
        rndr.fill(style, f.left, self.glyphs.vertical)?;

        if let Some(title) = &self.title {
            let title_with_spaces = format!(" {title} ");
            let title_len = title_with_spaces.len();

            rndr.fill(style, f.top, self.glyphs.horizontal)?;

            let title_line = f.top.line(0);
            let title_rect = geom::Rect::new(
                title_line.tl.x,
                title_line.tl.y,
                title_len.min(f.top.w as usize) as u32,
                1,
            );
            rndr.text("frame/title", title_rect.line(0), &title_with_spaces)?;
        } else {
            rndr.fill(style, f.top, self.glyphs.horizontal)?;
        }

        let child = ctx.children(ctx.node_id()).into_iter().next();
        if let Some(child_id) = child {
            if let Some(child_vp) = ctx.node_vp(child_id) {
                if let Some((pre, active, post)) = child_vp.vactive(f.right)? {
                    rndr.fill(style, pre, self.glyphs.vertical)?;
                    rndr.fill(style, post, self.glyphs.vertical)?;
                    rndr.fill("frame/active", active, self.glyphs.vertical_active)?;
                } else {
                    rndr.fill(style, f.right, self.glyphs.vertical)?;
                }

                if let Some((pre, active, post)) = child_vp.hactive(f.bottom)? {
                    rndr.fill(style, pre, self.glyphs.horizontal)?;
                    rndr.fill(style, post, self.glyphs.horizontal)?;
                    rndr.fill("frame/active", active, self.glyphs.horizontal_active)?;
                } else {
                    rndr.fill(style, f.bottom, self.glyphs.horizontal)?;
                }
            }
        } else {
            rndr.fill(style, f.right, self.glyphs.vertical)?;
            rndr.fill(style, f.bottom, self.glyphs.horizontal)?;
        }

        Ok(())
    }

    fn configure_style(&self, style: &mut Style) {
        style.padding = layout::Rect {
            left: LengthPercentage::Points(1.0),
            right: LengthPercentage::Points(1.0),
            top: LengthPercentage::Points(1.0),
            bottom: LengthPercentage::Points(1.0),
        };
    }

    fn name(&self) -> NodeName {
        NodeName::convert("frame")
    }
}
