use std::io::Write;
use std::marker::PhantomData;

use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    style::{Color, Print, SetForegroundColor},
    QueueableCommand,
};
use pad::PadStr;

use crate as canopy;
use crate::{
    geom::Rect,
    layout::FixedLayout,
    state::{NodeState, StatefulNode},
    widgets, Canopy, Node,
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
    /// Return the bounds of the frame content as a `(window, virtual)` tuple
    /// where virtual is the virtual size of the element, and view is some
    /// sub-rectangle of the element that is currently being viewed.
    fn bounds(&self) -> Option<(Rect, Rect)> {
        None
    }
}

/// A frame around an element.
#[derive(StatefulNode)]
pub struct Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout<S>,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: NodeState,
    pub focus_color: Color,
    pub color: Color,
    pub glyphs: FrameGlyphs,
}

impl<S, N> Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout<S>,
{
    pub fn new(c: N, glyphs: FrameGlyphs, color: Color, focus_color: Color) -> Self {
        Frame {
            _marker: PhantomData,
            child: c,
            state: NodeState::default(),
            color,
            focus_color,
            glyphs,
        }
    }
    pub fn with_glyphs(mut self, glyphs: FrameGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn with_focus_color(mut self, color: Color) -> Self {
        self.focus_color = color;
        self
    }
}

impl<S, N> FixedLayout<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout<S>,
{
    fn layout(&mut self, app: &mut Canopy<S>, rect: Option<Rect>) -> Result<()> {
        self.set_rect(rect);
        if let Some(r) = rect {
            self.child.layout(app, Some(r.inner(1)?))?;
        }
        Ok(())
    }
}

impl<S, N> Node<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout<S>,
{
    fn should_render(&mut self, app: &mut Canopy<S>) -> Option<bool> {
        Some(app.should_render(&mut self.child))
    }
    fn render(&mut self, app: &mut Canopy<S>, w: &mut dyn Write) -> Result<()> {
        if let Some(a) = self.rect() {
            let c = if app.on_focus_path(self) {
                self.focus_color
            } else {
                self.color
            };
            w.queue(SetForegroundColor(c))?;

            let f = a.frame(1)?;
            widgets::block(w, f.topleft, c, self.glyphs.topleft)?;
            widgets::block(w, f.topright, c, self.glyphs.topright)?;
            widgets::block(w, f.bottomleft, c, self.glyphs.bottomleft)?;
            widgets::block(w, f.bottomright, c, self.glyphs.bottomright)?;

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
            w.queue(MoveTo(f.top.tl.x, f.top.tl.y))?;
            w.queue(Print(top))?;

            if let Some((window, virt)) = self.child.bounds() {
                // Is window equal to or larger than virt?
                if window.vextent().contains(virt.vextent()) {
                    widgets::block(w, f.right, c, self.glyphs.vertical)?;
                } else {
                    let (epre, eactive, epost) = f
                        .right
                        .vextent()
                        .split_active(window.vextent(), virt.vextent())?;

                    widgets::block(w, f.right.vextract(epre)?, c, self.glyphs.vertical)?;
                    widgets::block(w, f.right.vextract(epost)?, c, self.glyphs.vertical)?;
                    widgets::block(
                        w,
                        f.right.vextract(eactive)?,
                        c,
                        self.glyphs.vertical_active,
                    )?;
                }

                // Is window equal to or larger than virt?
                if window.hextent().contains(virt.hextent()) {
                    widgets::block(w, f.bottom, c, self.glyphs.horizontal)?;
                } else {
                    let (epre, eactive, epost) = f
                        .bottom
                        .hextent()
                        .split_active(window.hextent(), virt.hextent())?;
                    widgets::block(w, f.bottom.hextract(epre)?, c, self.glyphs.horizontal)?;
                    widgets::block(w, f.bottom.hextract(epost)?, c, self.glyphs.horizontal)?;
                    widgets::block(
                        w,
                        f.bottom.hextract(eactive)?,
                        c,
                        self.glyphs.horizontal_active,
                    )?;
                }
            } else {
                widgets::block(w, f.right, c, self.glyphs.vertical)?;
                widgets::block(w, f.bottom, c, self.glyphs.horizontal)?;
            }
            widgets::block(w, f.left, c, self.glyphs.vertical)?;
        }
        Ok(())
    }
    fn children(
        &mut self,
        f: &mut dyn FnMut(&mut dyn canopy::Node<S>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)
    }
}
