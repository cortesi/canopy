use duplicate::duplicate;
use std::marker::PhantomData;

use crate as canopy;
use crate::{
    geom::{Point, Rect},
    layout::{ConstrainedWidthLayout, FillLayout},
    node::{EventOutcome, Node},
    state::{NodeState, StatefulNode},
    widgets::frame::FrameContent,
    Canopy, Result,
};

struct ScrollState {
    // The rectangle we're painting to on screen
    pub rect: Rect,
    // The total size of the virtual widget
    pub virt: Rect,
    // The part of the virtual widget that we're painting to rect
    pub window: Rect,
}

/// `Scroll` is an adapter that turns a node with `ConstrainedLayout` into one
/// with `FixedLayout` by managing a scrollable view onto the constrained
/// widget.
#[derive(StatefulNode)]
pub struct Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: NodeState,
    scrollstate: Option<ScrollState>,
}

impl<S, N> Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    pub fn new(c: N) -> Self {
        Scroll {
            _marker: PhantomData,
            child: c,
            state: NodeState::default(),
            scrollstate: None,
        }
    }

    pub fn scroll_to(&mut self, app: &mut Canopy<S>, x: u16, y: u16) -> Result<EventOutcome> {
        if let Some(ss) = &mut self.scrollstate {
            ss.window = Rect {
                tl: Point { x, y },
                w: ss.window.w,
                h: ss.window.h,
            }
            .clamp(ss.virt)?;
            self.child.layout(app, ss.window, ss.rect)?;
            app.taint_tree(self)?;
        }
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn scroll_by(&mut self, app: &mut Canopy<S>, x: i16, y: i16) -> Result<EventOutcome> {
        if let Some(ss) = &mut self.scrollstate {
            ss.window = ss.window.shift_within(x, y, ss.virt);
            self.child.layout(app, ss.window, ss.rect)?;
            app.taint_tree(self)?;
        }
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn page_up(&mut self, app: &mut Canopy<S>) -> Result<EventOutcome> {
        let h = if let Some(ss) = &mut self.scrollstate {
            ss.window.h
        } else {
            0
        };
        self.scroll_by(app, 0, -(h as i16))
    }

    pub fn page_down(&mut self, app: &mut Canopy<S>) -> Result<EventOutcome> {
        let h = if let Some(ss) = &mut self.scrollstate {
            ss.window.h
        } else {
            0
        };
        self.scroll_by(app, 0, h as i16)
    }

    pub fn up(&mut self, app: &mut Canopy<S>) -> Result<EventOutcome> {
        self.scroll_by(app, 0, -1)
    }

    pub fn down(&mut self, app: &mut Canopy<S>) -> Result<EventOutcome> {
        self.scroll_by(app, 0, 1)
    }

    pub fn left(&mut self, app: &mut Canopy<S>) -> Result<EventOutcome> {
        self.scroll_by(app, -1, 0)
    }

    pub fn right(&mut self, app: &mut Canopy<S>) -> Result<EventOutcome> {
        self.scroll_by(app, 1, 0)
    }
}

impl<S, N> FillLayout<S> for Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn layout_children(&mut self, app: &mut Canopy<S>, rect: Rect) -> Result<()> {
        let virt = self.child.constrain(app, rect.w)?;
        let view = Rect {
            tl: Point { x: 0, y: 0 },
            w: rect.w,
            h: rect.h,
        };
        self.scrollstate = Some(ScrollState {
            window: view,
            virt,
            rect: rect,
        });
        self.child.layout(app, view, rect)?;
        Ok(())
    }
}

impl<S, N> FrameContent for Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn bounds(&self) -> Option<(Rect, Rect)> {
        self.scrollstate.as_ref().map(|ss| (ss.window, ss.virt))
    }
}

impl<S, N> Node<S> for Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn should_render(&self, app: &Canopy<S>) -> Option<bool> {
        Some(app.should_render(&self.child))
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
