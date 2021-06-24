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
    geom::{Point, Rect},
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
    horizontal_active: '█',
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
    horizontal_active: '█',
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
    horizontal_active: '█',
    vertical_active: '█',
};

/// This trait must be implemented for nodes that are direct children of the
/// frame.
pub trait FrameContent {
    /// The title for this element, if any.
    fn title(&self) -> Option<String> {
        None
    }
    /// Return the bounds of the frame content as a `(view, virtual)` tuple
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

            if let Some((view, virt)) = self.child.bounds() {
                let (pre, active, post) = scroll_parts_vert(view, virt, f.right);
                widgets::block(w, pre, c, self.glyphs.vertical)?;
                widgets::block(w, post, c, self.glyphs.vertical)?;
                widgets::block(w, active, c, self.glyphs.vertical_active)?;

                let (pre, active, post) = scroll_parts_horiz(view, virt, f.bottom);
                widgets::block(w, pre, c, self.glyphs.horizontal)?;
                widgets::block(w, post, c, self.glyphs.horizontal)?;
                widgets::block(w, active, c, self.glyphs.horizontal_active)?;
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

// Takes a `view` onto a `virt` element, and splits up `space` vertically into
// three rectangles: `(pre, active, post)`, where pre and post are space outside
// of the active scrollbar indicator.
fn scroll_parts_vert(view: Rect, virt: Rect, space: Rect) -> (Rect, Rect, Rect) {
    let vdraw = space.h as f32 - 1.0;
    let preh = (vdraw * (view.tl.y as f32 / virt.h as f32)).ceil() as u16;
    let activeh = (vdraw * (view.h as f32 / virt.h as f32)).ceil() as u16;
    let posth = view.h.saturating_sub(preh + activeh);

    if activeh == 0 || preh == 0 && posth == 0 {
        (
            space,
            Rect {
                tl: space.tl,
                w: 0,
                h: 0,
            },
            Rect {
                tl: Point {
                    x: space.tl.x,
                    y: space.tl.y,
                },
                w: 0,
                h: 0,
            },
        )
    } else {
        (
            Rect {
                tl: space.tl,
                w: space.w,
                h: preh,
            },
            Rect {
                tl: Point {
                    x: space.tl.x,
                    y: space.tl.y + preh,
                },
                w: space.w,
                h: activeh,
            },
            Rect {
                tl: Point {
                    x: space.tl.x,
                    y: space.tl.y + preh + activeh,
                },
                w: space.w,
                h: posth,
            },
        )
    }
}

// Takes a `view` onto a `virt` element, and splits up `space` horizontally into
// three rectangles: `(pre, active, post)`, where pre and post are space outside
// of the active scrollbar indicator.
fn scroll_parts_horiz(view: Rect, virt: Rect, space: Rect) -> (Rect, Rect, Rect) {
    let prew = ((space.w as f32) * (view.tl.x as f32 / virt.w as f32)).floor() as u16;
    let activew = ((space.w as f32) * (view.w as f32 / virt.w as f32)).floor() as u16;
    let postw = view.w.saturating_sub(prew + activew);

    if activew == 0 || prew == 0 && postw == 0 {
        (
            space,
            Rect {
                tl: space.tl,
                w: 0,
                h: 0,
            },
            Rect {
                tl: Point {
                    x: space.tl.x,
                    y: space.tl.y,
                },
                w: 0,
                h: 0,
            },
        )
    } else {
        (
            Rect {
                tl: space.tl,
                w: prew,
                h: space.h,
            },
            Rect {
                tl: Point {
                    x: space.tl.x + prew,
                    y: space.tl.y,
                },
                w: activew,
                h: space.h,
            },
            Rect {
                tl: Point {
                    x: space.tl.x + prew + activew,
                    y: space.tl.y,
                },
                w: postw,
                h: space.h,
            },
        )
    }
}
