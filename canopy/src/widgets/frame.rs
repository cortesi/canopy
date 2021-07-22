use duplicate::duplicate;
use std::marker::PhantomData;

use pad::PadStr;

use crate as canopy;
use crate::{
    geom::Frame as GFrame,
    layout::Layout,
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

/// This trait must be implemented for nodes that are direct children of the
/// frame.
pub trait FrameContent {
    /// The title for this element, if any.
    fn title(&self) -> Option<String> {
        None
    }
}

/// A frame around an element.
///
/// Colors:
///     frame:          normal frame border
///     frame/focused   frame border if we hold focus
///     frame/active    color of active area indicator
#[derive(StatefulNode)]
pub struct Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + Layout<S>,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: NodeState,
    pub glyphs: FrameGlyphs,
}

impl<S, N> Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + Layout<S>,
{
    pub fn new(c: N) -> Self {
        Frame {
            _marker: PhantomData,
            child: c,
            state: NodeState::default(),
            glyphs: SINGLE,
        }
    }
    /// Build a frame with a specified glyph set
    pub fn with_glyphs(mut self, glyphs: FrameGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<S, N> Layout<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + Layout<S>,
{
    fn layout_children(&mut self, app: &mut Canopy<S>) -> Result<()> {
        self.child.layout(app, self.screen_area().inner(1)?)
    }
}

impl<S, N> Node<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + Layout<S>,
{
    fn should_render(&self, app: &Canopy<S>) -> Option<bool> {
        Some(app.should_render(&self.child))
    }
    fn render(&self, app: &mut Canopy<S>) -> Result<()> {
        let style = if app.on_focus_path(self) {
            "frame/focused"
        } else {
            "frame"
        };

        let f = GFrame::new(self.screen_area(), 1)?;
        app.render.fill(style, f.topleft, self.glyphs.topleft)?;
        app.render.fill(style, f.topright, self.glyphs.topright)?;
        app.render
            .fill(style, f.bottomleft, self.glyphs.bottomleft)?;
        app.render
            .fill(style, f.bottomright, self.glyphs.bottomright)?;
        app.render.fill(style, f.left, self.glyphs.vertical)?;

        let top = if f.top.w < 8 || self.child.title().is_none() {
            self.glyphs.horizontal.to_string().repeat(f.top.w as usize)
        } else {
            let t = format!(" {} ", self.child.title().unwrap());
            t.pad(
                f.top.w as usize,
                self.glyphs.horizontal,
                pad::Alignment::Left,
                true,
            )
        };
        app.render.text(style, f.top, &top)?;

        let view = self.child.state().view;

        // Is window equal to or larger than virt?
        if view.view().vextent().contains(&view.outer().vextent()) {
            app.render.fill(style, f.right, self.glyphs.vertical)?;
        } else {
            let (epre, eactive, epost) = f
                .right
                .vextent()
                .split_active(view.outer().vextent(), view.view().vextent())?;

            app.render
                .fill(style, f.right.vextract(&epre)?, self.glyphs.vertical)?;
            app.render
                .fill(style, f.right.vextract(&epost)?, self.glyphs.vertical)?;
            app.render.fill(
                style,
                f.right.vextract(&eactive)?,
                self.glyphs.vertical_active,
            )?;
        }

        // Is window equal to or larger than virt?
        if view.view().hextent().contains(&view.outer().hextent()) {
            app.render.fill(style, f.bottom, self.glyphs.horizontal)?;
        } else {
            let (epre, eactive, epost) = f
                .bottom
                .hextent()
                .split_active(view.outer().hextent(), view.view().hextent())?;

            app.render
                .fill(style, f.bottom.hextract(&epre)?, self.glyphs.horizontal)?;
            app.render
                .fill(style, f.bottom.hextract(&epost)?, self.glyphs.horizontal)?;
            app.render.fill(
                style,
                f.bottom.hextract(&eactive)?,
                self.glyphs.horizontal_active,
            )?;
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
