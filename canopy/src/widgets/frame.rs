use pad::PadStr;

use crate as canopy;
use crate::{
    derive_commands, fit_frame, geom,
    geom::Expanse,
    state::{NodeState, StatefulNode},
    Core, Node, Render, Result,
};

/// Defines the set of glyphs used to draw the frame
pub struct FrameGlyphs {
    pub topleft: char,
    pub topright: char,
    pub bottomleft: char,
    pub bottomright: char,
    pub horizontal: char,
    pub vertical: char,
    pub vertical_active: char,
    pub horizontal_active: char,
}

/// Single line thin Unicode box drawing frame set
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

/// Double line Unicode box drawing frame set
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

/// Single line thick Unicode box drawing frame set
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

/// A frame around an element.
///
/// Colors:
///     frame:          normal frame border
///     frame/focused   frame border if we hold focus
///     frame/active    color of active area indicator
#[derive(StatefulNode)]
pub struct Frame<N>
where
    N: Node,
{
    pub child: N,
    pub state: NodeState,
    pub glyphs: FrameGlyphs,
    pub title: Option<String>,
    pub frame: geom::Frame,
}

#[derive_commands]
impl<N> Frame<N>
where
    N: Node,
{
    pub fn new(c: N) -> Self {
        Frame {
            child: c,
            state: NodeState::default(),
            glyphs: SINGLE,
            title: None,
            frame: geom::Frame::zero(),
        }
    }

    /// Build a frame with a specified glyph set
    pub fn with_glyphs(mut self, glyphs: FrameGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a frame with a specified title
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }
}

impl<N> Node for Frame<N>
where
    N: Node,
{
    fn force_render(&self, c: &dyn Core) -> bool {
        c.needs_render(&self.child)
    }

    fn fit(&mut self, sz: crate::geom::Expanse) -> Result<()> {
        self.frame = fit_frame!(self, self.child, sz, 1);
        Ok(())
    }

    fn render(&mut self, c: &dyn Core, rndr: &mut Render) -> Result<()> {
        let f = self.frame;
        let style = if c.is_on_focus_path(self) {
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
            title.pad(
                f.top.w as usize,
                self.glyphs.horizontal,
                pad::Alignment::Left,
                true,
            );
            rndr.text(style, f.top.line(0), title)?;
        } else {
            rndr.fill(style, f.top, self.glyphs.horizontal)?;
        }

        if let Some((pre, active, post)) = self.child.state().viewport.vactive(f.right)? {
            rndr.fill(style, pre, self.glyphs.vertical)?;
            rndr.fill(style, post, self.glyphs.vertical)?;
            rndr.fill(style, active, self.glyphs.vertical_active)?;
        } else {
            rndr.fill(style, f.right, self.glyphs.vertical)?;
        }

        if let Some((pre, active, post)) = self.child.state().viewport.hactive(f.bottom)? {
            rndr.fill(style, pre, self.glyphs.horizontal)?;
            rndr.fill(style, post, self.glyphs.horizontal)?;
            rndr.fill(style, active, self.glyphs.horizontal_active)?;
        } else {
            rndr.fill(style, f.bottom, self.glyphs.horizontal)?;
        }

        // Our child is always positioned in our upper-left corner, so negative
        // space is to the right and below.
        for r in self
            .vp()
            .view
            .inner(1)
            .sub(&self.child.vp().canvas.rect().shift(1, 1))
        {
            rndr.fill(style, r, ' ')?;
        }

        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}
