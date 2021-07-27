use duplicate::duplicate;
use std::marker::PhantomData;

use pad::PadStr;

use crate as canopy;
use crate::{
    fit_and_update,
    geom::{Frame as GFrame, Rect},
    state::{NodeState, StatefulNode},
    Actions, Canopy, Node, Result,
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
pub struct Frame<S, A: Actions, N>
where
    N: Node<S, A>,
{
    _marker: PhantomData<(S, A)>,
    pub child: N,
    pub state: NodeState,
    pub glyphs: FrameGlyphs,
    pub title: Option<String>,
}

impl<S, A: Actions, N> Frame<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new(c: N) -> Self {
        Frame {
            _marker: PhantomData,
            child: c,
            state: NodeState::default(),
            glyphs: SINGLE,
            title: None,
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

impl<S, A: Actions, N> Node<S, A> for Frame<S, A, N>
where
    N: Node<S, A>,
{
    fn layout(&mut self, app: &mut Canopy<S, A>, screen: Rect) -> Result<()> {
        fit_and_update(app, screen.inner(1)?, &mut self.child)
    }
    fn should_render(&self, app: &Canopy<S, A>) -> Option<bool> {
        Some(app.should_render(&self.child))
    }
    fn render(&self, app: &mut Canopy<S, A>) -> Result<()> {
        let style = if app.on_focus_path(self) {
            "frame/focused"
        } else {
            "frame"
        };

        let f = GFrame::new(self.view(), 1)?;
        app.render.fill(style, f.topleft, self.glyphs.topleft)?;
        app.render.fill(style, f.topright, self.glyphs.topright)?;
        app.render
            .fill(style, f.bottomleft, self.glyphs.bottomleft)?;
        app.render
            .fill(style, f.bottomright, self.glyphs.bottomright)?;
        app.render.fill(style, f.left, self.glyphs.vertical)?;

        if let Some(title) = &self.title {
            title.pad(
                f.top.w as usize,
                self.glyphs.horizontal,
                pad::Alignment::Left,
                true,
            );
            app.render.text(style, f.top.first_line(), &title)?;
        } else {
            app.render.fill(style, f.top, self.glyphs.horizontal)?;
        }

        if let Some((pre, active, post)) = self.child.state().viewport.vactive(f.right)? {
            app.render.fill(style, pre, self.glyphs.vertical)?;
            app.render.fill(style, post, self.glyphs.vertical)?;
            app.render
                .fill(style, active, self.glyphs.vertical_active)?;
        } else {
            app.render.fill(style, f.right, self.glyphs.vertical)?;
        }

        if let Some((pre, active, post)) = self.child.state().viewport.hactive(f.bottom)? {
            app.render.fill(style, pre, self.glyphs.horizontal)?;
            app.render.fill(style, post, self.glyphs.horizontal)?;
            app.render
                .fill(style, active, self.glyphs.horizontal_active)?;
        } else {
            app.render.fill(style, f.bottom, self.glyphs.horizontal)?;
        }

        // Our child is always positioned in our upper-left corner, so negative
        // space is to the right and below.
        for r in self
            .view()
            .inner(1)?
            .sub(&self.child.outer().rect().shift(1, 1))
        {
            app.render.fill(style, r, ' ')?;
        }

        Ok(())
    }
    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<S, A>])) -> Result<()>,
    ) -> Result<()> {
        f(reference([self.child]))
    }
}
