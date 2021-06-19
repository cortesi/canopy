use std::marker::PhantomData;

use anyhow::Result;

use crate as canopy;
use crate::{
    geom::{Point, Rect},
    layout::{ConstrainedLayout, FixedLayout},
    widgets, Canopy, EventResult, Node,
};

struct ScrollState {
    // The rectangle we're painting to on screen
    pub rect: Rect,
    // The total size of the virtual widget
    pub virt: Rect,
    // The offset within the virtual widget that we're painting to rect
    pub view: Rect,
}

/// `Scroll` is an adapter that turns a node with `ConstrainedLayout` into one
/// with `FixedLayout`, by managing a scrollable view onto the constrained
/// widget.
pub struct Scroll<S, N: canopy::Node<S> + ConstrainedLayout> {
    _marker: PhantomData<S>,
    pub child: N,
    pub state: canopy::NodeState,
    scrollstate: Option<ScrollState>,
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> Scroll<S, N> {
    pub fn new(c: N) -> Self {
        Scroll {
            _marker: PhantomData,
            child: c,
            state: canopy::NodeState::default(),
            scrollstate: None,
        }
    }

    pub fn scroll_to(&mut self, app: &mut Canopy, x: u16, y: u16) -> Result<EventResult> {
        if let Some(ss) = &mut self.scrollstate {
            ss.view = Rect {
                tl: Point { x, y },
                w: ss.view.w,
                h: ss.view.h,
            }
            .clamp(ss.virt)?;
            self.child.layout(app, ss.view.tl, ss.rect)?;
            app.taint_tree(self)?;
        }
        Ok(EventResult::Handle { skip: false })
    }

    pub fn scroll_by(&mut self, app: &mut Canopy, x: i16, y: i16) -> Result<EventResult> {
        if let Some(ss) = &mut self.scrollstate {
            ss.view = ss.view.scroll_within(x, y, ss.virt);
            self.child.layout(app, ss.view.tl, ss.rect)?;
            app.taint_tree(self)?;
        }
        Ok(EventResult::Handle { skip: false })
    }

    pub fn page_up(&mut self, app: &mut Canopy) -> Result<EventResult> {
        let h = if let Some(ss) = &mut self.scrollstate {
            ss.view.h
        } else {
            0
        };
        self.scroll_by(app, 0, -(h as i16))
    }

    pub fn page_down(&mut self, app: &mut Canopy) -> Result<EventResult> {
        let h = if let Some(ss) = &mut self.scrollstate {
            ss.view.h
        } else {
            0
        };
        self.scroll_by(app, 0, h as i16)
    }

    pub fn up(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_by(app, 0, -1)
    }

    pub fn down(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_by(app, 0, 1)
    }

    pub fn left(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_by(app, -1, 0)
    }

    pub fn right(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_by(app, 1, 0)
    }
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> FixedLayout for Scroll<S, N> {
    fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
        if let Some(r) = rect {
            let virt = self.child.constrain(app, Some(r.w), None)?;
            let view = Rect {
                tl: Point { x: 0, y: 0 },
                w: r.w,
                h: r.h,
            };
            self.scrollstate = Some(ScrollState {
                view,
                virt,
                rect: r,
            });
            self.child.layout(app, view.tl, r)?;
        } else {
            self.scrollstate = None
        }
        Ok(())
    }
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> widgets::frame::FrameContent for Scroll<S, N> {
    fn bounds(&self) -> Option<(Rect, Rect)> {
        if let Some(ss) = &self.scrollstate {
            Some((ss.view, ss.virt))
        } else {
            None
        }
    }
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> Node<S> for Scroll<S, N> {
    fn should_render(&mut self, app: &mut Canopy) -> Option<bool> {
        Some(app.should_render(&mut self.child))
    }
    fn rect(&self) -> Option<Rect> {
        if let Some(ss) = &self.scrollstate {
            Some(ss.rect)
        } else {
            None
        }
    }
    fn state(&mut self) -> &mut canopy::NodeState {
        &mut self.state
    }
    fn children(
        &mut self,
        f: &mut dyn FnMut(&mut dyn canopy::Node<S>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)
    }
}
