use duplicate::duplicate;
use std::io::Write;
use std::marker::PhantomData;

use crossterm::{cursor::MoveTo, style::Print, QueueableCommand};
use pad::PadStr;

use crate as canopy;
use crate::{
    geom::{Frame as GFrame, Rect},
    layout::FillLayout,
    state::{NodeState, StatefulNode},
    style::Style,
    widgets, Canopy, Node, Result,
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
///
/// Colors:
///     frame:          normal frame border
///     frame/focused   frame border if we hold focus
///     frame/active    color of active area indicator
#[derive(StatefulNode)]
pub struct Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FillLayout<S>,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: NodeState,
    pub glyphs: FrameGlyphs,
}

impl<S, N> Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FillLayout<S>,
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

impl<S, N> FillLayout<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FillLayout<S>,
{
    fn layout(&mut self, app: &mut Canopy<S>, rect: Rect) -> Result<()> {
        self.set_screen_area(rect);
        self.child.layout(app, rect.inner(1)?)?;
        Ok(())
    }
}

impl<S, N> Node<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FillLayout<S>,
{
    fn should_render(&self, app: &Canopy<S>) -> Option<bool> {
        Some(app.should_render(&self.child))
    }
    fn render(&self, app: &Canopy<S>, style: &mut Style, r: Rect, w: &mut dyn Write) -> Result<()> {
        if app.on_focus_path(self) {
            style.set("frame/focused", w)?;
        } else {
            style.set("frame", w)?;
        };

        let f = GFrame::new(r, 1)?;
        widgets::block(w, f.topleft, self.glyphs.topleft)?;
        widgets::block(w, f.topright, self.glyphs.topright)?;
        widgets::block(w, f.bottomleft, self.glyphs.bottomleft)?;
        widgets::block(w, f.bottomright, self.glyphs.bottomright)?;

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

        widgets::block(w, f.left, self.glyphs.vertical)?;
        if let Some((window, virt)) = self.child.bounds() {
            let mut vertactive = None;
            let mut horizactive = None;

            // Is window equal to or larger than virt?
            if window.vextent().contains(&virt.vextent()) {
                widgets::block(w, f.right, self.glyphs.vertical)?;
            } else {
                let (epre, eactive, epost) = f
                    .right
                    .vextent()
                    .split_active(window.vextent(), virt.vextent())?;

                widgets::block(w, f.right.vextract(&epre)?, self.glyphs.vertical)?;
                widgets::block(w, f.right.vextract(&epost)?, self.glyphs.vertical)?;

                vertactive = Some(f.right.vextract(&eactive)?);
                // colors.set("frame/active", w)?;
                // widgets::block(w, f.right.vextract(eactive)?, self.glyphs.vertical_active)?;
            }

            // Is window equal to or larger than virt?
            if window.hextent().contains(&virt.hextent()) {
                widgets::block(w, f.bottom, self.glyphs.horizontal)?;
            } else {
                let (epre, eactive, epost) = f
                    .bottom
                    .hextent()
                    .split_active(window.hextent(), virt.hextent())?;
                widgets::block(w, f.bottom.hextract(&epre)?, self.glyphs.horizontal)?;
                widgets::block(w, f.bottom.hextract(&epost)?, self.glyphs.horizontal)?;
                horizactive = Some(f.bottom.hextract(&eactive)?);
            }
            if vertactive.is_none() || horizactive.is_none() {
                style.set("frame/active", w)?;
            }
            if let Some(vc) = vertactive {
                widgets::block(w, vc, self.glyphs.vertical_active)?;
            }
            if let Some(hc) = horizactive {
                widgets::block(w, hc, self.glyphs.horizontal_active)?;
            }
        } else {
            widgets::block(w, f.right, self.glyphs.vertical)?;
            widgets::block(w, f.bottom, self.glyphs.horizontal)?;
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
