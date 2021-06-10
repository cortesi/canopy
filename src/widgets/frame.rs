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
use crate::{geom::Rect, layout::FixedLayout, widgets, Canopy, Node};

/// Defines the set of glyphs used to draw the frame
pub struct FrameGlyphs {
    pub topleft: char,
    pub topright: char,
    pub bottomleft: char,
    pub bottomright: char,
    pub horizontal: char,
    pub vertical: char,
}

/// Single line thin Unicode box drawing frame set
pub const SINGLE: FrameGlyphs = FrameGlyphs {
    topleft: '\u{250C}',
    topright: '\u{2510}',
    bottomleft: '\u{2514}',
    bottomright: '\u{2518}',
    horizontal: '\u{2500}',
    vertical: '\u{2502}',
};

/// Double line Unicode box drawing frame set
pub const DOUBLE: FrameGlyphs = FrameGlyphs {
    topleft: '\u{2554}',
    topright: '\u{2557}',
    bottomleft: '\u{255A}',
    bottomright: '\u{255D}',
    horizontal: '\u{2550}',
    vertical: '\u{2551}',
};

/// Single line thick Unicode box drawing frame set
pub const SINGLE_THICK: FrameGlyphs = FrameGlyphs {
    topleft: '\u{250F}',
    topright: '\u{2513}',
    bottomleft: '\u{2517}',
    bottomright: '\u{251B}',
    horizontal: '\u{2501}',
    vertical: '\u{2503}',
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
pub struct Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: canopy::NodeState,
    pub rect: Option<Rect>,
    pub focus_color: Color,
    pub color: Color,
    pub glyphs: FrameGlyphs,
}

impl<S, N> Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout,
{
    pub fn new(c: N, glyphs: FrameGlyphs, color: Color, focus_color: Color) -> Self {
        Frame {
            _marker: PhantomData,
            child: c,
            state: canopy::NodeState::default(),
            rect: None,
            color,
            focus_color,
            glyphs,
        }
    }
}

impl<S, N> FixedLayout for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout,
{
    fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
        self.rect = rect;
        if let Some(r) = rect {
            self.child.layout(app, Some(r.inner(1)?))?;
        }
        Ok(())
    }
}

impl<S, N> Node<S> for Frame<S, N>
where
    N: canopy::Node<S> + FrameContent + FixedLayout,
{
    fn should_render(&mut self, app: &mut Canopy) -> Option<bool> {
        Some(app.should_render(&mut self.child))
    }
    fn rect(&self) -> Option<Rect> {
        self.rect
    }
    fn state(&mut self) -> &mut canopy::NodeState {
        &mut self.state
    }
    fn render(&mut self, app: &mut Canopy, w: &mut dyn Write) -> Result<()> {
        if let Some(a) = self.rect {
            let c = if app.on_focus_path(self) {
                self.focus_color
            } else {
                self.color
            };
            let f = a.frame(1)?;

            let twidth = (f.top.w - 2) as usize;
            let top = if twidth < 8 || self.child.title().is_none() {
                self.glyphs.horizontal.to_string().repeat(twidth)
            } else {
                let t = format!(" {} ", self.child.title().unwrap());
                t.pad(twidth, self.glyphs.horizontal, pad::Alignment::Left, true)
            };

            w.queue(SetForegroundColor(c))?;
            w.queue(MoveTo(f.top.tl.x, f.top.tl.y))?;
            w.queue(Print(format!(
                "{}{}{}",
                self.glyphs.topleft, top, self.glyphs.topright
            )))?;

            w.queue(MoveTo(f.bottom.tl.x, f.bottom.tl.y))?;
            w.queue(Print(format!(
                "{}{}{}",
                self.glyphs.bottomleft,
                self.glyphs
                    .horizontal
                    .to_string()
                    .repeat((f.bottom.w - 2) as usize),
                self.glyphs.bottomright
            )))?;

            widgets::block(w, f.left, c, self.glyphs.vertical)?;
            widgets::block(w, f.right, c, self.glyphs.vertical)?;
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
