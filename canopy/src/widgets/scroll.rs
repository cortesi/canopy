use std::marker::PhantomData;

use crate as canopy;
use crate::{
    node::{EventOutcome, Node},
    state::{NodeState, StatefulNode},
    widgets::frame::FrameContent,
    Canopy, Rect, Result,
};

/// `Scroll` is an adapter that turns a node with `ConstrainedLayout` into one
/// with `FixedLayout` by managing a scrollable view onto the constrained
/// widget.
#[derive(StatefulNode)]
pub struct Scroll<S, N>
where
    N: Node<S>,
{
    _marker: PhantomData<S>,
    pub child: N,
    pub state: NodeState,
}

impl<S, N> Scroll<S, N>
where
    N: Node<S>,
{
    pub fn new(c: N) -> Self {
        Scroll {
            _marker: PhantomData,
            child: c,
            state: NodeState::default(),
        }
    }

    pub fn scroll_to(&mut self, x: u16, y: u16) -> Result<EventOutcome> {
        self.child.state_mut().viewport.scroll_to(x, y);
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn scroll_by(&mut self, x: i16, y: i16) -> Result<EventOutcome> {
        self.child.state_mut().viewport.scroll_by(x, y);
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn page_up(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().viewport.page_up();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn page_down(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().viewport.page_down();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn up(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().viewport.up();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn down(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().viewport.down();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn left(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().viewport.left();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn right(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().viewport.right();
        Ok(EventOutcome::Handle { skip: false })
    }
}

impl<S, N> FrameContent for Scroll<S, N> where N: Node<S> {}

impl<S, N> Node<S> for Scroll<S, N>
where
    N: Node<S>,
{
    fn should_render(&self, app: &Canopy<S>) -> Option<bool> {
        Some(app.should_render(&self.child))
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<S>) -> Result<()>) -> Result<()> {
        f(&self.child)
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S>) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }

    fn layout(&mut self, app: &mut Canopy<S>, screen: Rect) -> Result<()> {
        self.state_mut().viewport.set_fill(screen);
        self.child.layout(app, screen)?;
        Ok(())
    }
}
