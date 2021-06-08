use std::io::Write;
use std::marker::PhantomData;

use anyhow::Result;

use crate as canopy;
use crate::{geom::Rect, widgets, Canopy, Node};

pub struct Scroll<S, N: canopy::Node<S>> {
    _marker: PhantomData<S>,
    pub child: N,
    pub state: canopy::NodeState,
    pub rect: Option<Rect>,
    pub view: Option<Rect>,
}

impl<S, N: canopy::Node<S>> Scroll<S, N> {
    pub fn new(c: N) -> Self {
        Scroll {
            _marker: PhantomData,
            child: c,
            state: canopy::NodeState::default(),
            rect: None,
            view: None,
        }
    }
}

impl<S, N: canopy::Node<S>> widgets::frame::FrameContent for Scroll<S, N> {}

impl<S, N: canopy::Node<S>> Node<S> for Scroll<S, N> {
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
    fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>, virt: Option<Rect>) -> Result<()> {
        self.rect = rect;
        if let Some(r) = rect {
            let v = self.child.virt_size(app, Some(r.w), None);
            self.child.layout(app, rect, v)?;
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
