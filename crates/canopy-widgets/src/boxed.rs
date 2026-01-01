use canopy::{
    ViewContext, Widget, derive_commands,
    error::Result,
    geom,
    layout::{Edges, Layout},
    render::Render,
    state::NodeName,
};

/// Defines the set of glyphs used to draw the box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxGlyphs {
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
}

impl BoxGlyphs {
    /// Draw a box border using these glyphs.
    pub(crate) fn draw(&self, rndr: &mut Render, style: &str, frame: geom::Frame) -> Result<()> {
        rndr.fill(style, frame.topleft, self.topleft)?;
        rndr.fill(style, frame.topright, self.topright)?;
        rndr.fill(style, frame.bottomleft, self.bottomleft)?;
        rndr.fill(style, frame.bottomright, self.bottomright)?;
        rndr.fill(style, frame.top, self.horizontal)?;
        rndr.fill(style, frame.bottom, self.horizontal)?;
        rndr.fill(style, frame.left, self.vertical)?;
        rndr.fill(style, frame.right, self.vertical)?;
        Ok(())
    }
}

/// Single line thin Unicode box drawing set.
pub const SINGLE: BoxGlyphs = BoxGlyphs {
    topleft: '┌',
    topright: '┐',
    bottomleft: '└',
    bottomright: '┘',
    horizontal: '─',
    vertical: '│',
};

/// Double line Unicode box drawing set.
pub const DOUBLE: BoxGlyphs = BoxGlyphs {
    topleft: '╔',
    topright: '╗',
    bottomleft: '╚',
    bottomright: '╝',
    horizontal: '═',
    vertical: '║',
};

/// Single line thick Unicode box drawing set.
pub const SINGLE_THICK: BoxGlyphs = BoxGlyphs {
    topleft: '┏',
    topright: '┓',
    bottomleft: '┗',
    bottomright: '┛',
    horizontal: '━',
    vertical: '┃',
};

/// Round corner thin Unicode box drawing set.
pub const ROUND: BoxGlyphs = BoxGlyphs {
    topleft: '╭',
    topright: '╮',
    bottomleft: '╰',
    bottomright: '╯',
    horizontal: '─',
    vertical: '│',
};

/// Round corner thick Unicode box drawing set.
pub const ROUND_THICK: BoxGlyphs = BoxGlyphs {
    topleft: '╭',
    topright: '╮',
    bottomleft: '╰',
    bottomright: '╯',
    horizontal: '━',
    vertical: '┃',
};

/// A simple box container around its children.
pub struct Box {
    /// Glyph set for rendering.
    glyphs: BoxGlyphs,
    /// Style name for the box border.
    border_style: String,
    /// Optional style name for filling the box interior.
    fill_style: Option<String>,
}

#[derive_commands]
impl Box {
    /// Construct a box.
    pub fn new() -> Self {
        Self {
            glyphs: SINGLE,
            border_style: "border".to_string(),
            fill_style: None,
        }
    }

    /// Build a box with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a box with a specified border style name.
    pub fn with_border_style(mut self, style: impl Into<String>) -> Self {
        self.border_style = style.into();
        self
    }

    /// Enable interior fill using the default fill style name.
    pub fn with_fill(mut self) -> Self {
        self.fill_style = Some("fill".to_string());
        self
    }

    /// Enable interior fill using a specified style name.
    pub fn with_fill_style(mut self, style: impl Into<String>) -> Self {
        self.fill_style = Some(style.into());
        self
    }
}

impl Default for Box {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Box {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let outer = ctx.view().outer_rect_local();
        let frame = geom::Frame::new(outer, 1);
        if let Some(style) = &self.fill_style {
            let inner = frame.inner();
            if inner.w > 0 && inner.h > 0 {
                rndr.fill(style, inner, ' ')?;
            }
        }
        self.glyphs.draw(rndr, &self.border_style, frame)?;
        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::all(1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("box")
    }
}
