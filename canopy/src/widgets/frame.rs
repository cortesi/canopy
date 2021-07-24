use duplicate::duplicate;
use std::marker::PhantomData;

use pad::PadStr;

use crate as canopy;
use crate::{
    geom::{Frame as GFrame, Rect},
    state::{NodeState, StatefulNode},
    Canopy, Node, Result,
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
pub struct Frame<S, N>
where
    N: canopy::Node<S>,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: NodeState,
    pub glyphs: FrameGlyphs,
    pub title: Option<String>,
}

impl<S, N> Frame<S, N>
where
    N: canopy::Node<S>,
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

impl<S, N> Node<S> for Frame<S, N>
where
    N: canopy::Node<S>,
{
    fn layout(&mut self, app: &mut Canopy<S>, screen: Rect) -> Result<()> {
        let v = self.fit(app, screen.into())?;
        self.update_view(v, screen);
        self.child.layout(app, screen.inner(1)?)
    }
    fn should_render(&self, app: &Canopy<S>) -> Option<bool> {
        Some(app.should_render(&self.child))
    }
    fn render(&self, app: &mut Canopy<S>) -> Result<()> {
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

        Ok(())
    }
    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<S>])) -> Result<()>,
    ) -> Result<()> {
        f(reference([self.child]))
    }
}
