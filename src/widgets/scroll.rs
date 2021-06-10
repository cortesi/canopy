use std::io::Write;
use std::marker::PhantomData;

use anyhow::Result;

use crate as canopy;
use crate::{
    geom::{Point, Rect},
    layout::{ConstrainedLayout, FixedLayout},
    widgets, Canopy, EventResult, Node,
};

pub struct Scroll<S, N: canopy::Node<S> + ConstrainedLayout> {
    _marker: PhantomData<S>,
    pub child: N,
    pub state: canopy::NodeState,
    // The rectangle we're painting to
    pub rect: Option<Rect>,
    // The size of the virtual widget
    pub virt: Option<Rect>,
    // The offset within the virtual widget that we're painting to rect
    pub offset: Point,
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> Scroll<S, N> {
    pub fn new(c: N) -> Self {
        Scroll {
            _marker: PhantomData,
            child: c,
            state: canopy::NodeState::default(),
            rect: None,
            virt: None,
            offset: Point { x: 0, y: 0 },
        }
    }

    pub fn scroll_to(&mut self, app: &mut Canopy, p: Point) -> Result<EventResult> {
        if let Some(r) = self.rect {
            self.offset = p;
            self.child.layout(app, self.offset, r)?;
            app.taint_tree(self)?;
        }
        Ok(EventResult::Handle { skip: false })
    }

    pub fn up(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_to(app, self.offset.scroll(0, -1))
    }
    pub fn down(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_to(app, self.offset.scroll(0, 1))
    }
    pub fn left(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_to(app, self.offset.scroll(-1, 0))
    }
    pub fn right(&mut self, app: &mut Canopy) -> Result<EventResult> {
        self.scroll_to(app, self.offset.scroll(1, 0))
    }
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> FixedLayout for Scroll<S, N> {
    fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
        self.rect = rect;
        if let Some(r) = rect {
            self.virt = Some(self.child.constrain(app, Some(r.w), None)?);
            self.child.layout(app, self.offset, r)?;
        }
        Ok(())
    }
}

impl<S, N: canopy::Node<S> + ConstrainedLayout> widgets::frame::FrameContent for Scroll<S, N> {}

impl<S, N: canopy::Node<S> + ConstrainedLayout> Node<S> for Scroll<S, N> {
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
        Ok(())
    }
    fn children(
        &mut self,
        f: &mut dyn FnMut(&mut dyn canopy::Node<S>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)
    }
}
