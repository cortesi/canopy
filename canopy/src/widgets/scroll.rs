use std::marker::PhantomData;

use crate as canopy;
use crate::{
    layout::{ConstrainedWidthLayout, Layout},
    node::{EventOutcome, Node},
    state::{NodeState, StatefulNode},
    widgets::frame::FrameContent,
    Canopy, Result,
};

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
        }
    }

    pub fn scroll_to(&mut self, x: u16, y: u16) -> Result<EventOutcome> {
        self.child.state_mut().view.scroll_to(x, y);
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn scroll_by(&mut self, x: i16, y: i16) -> Result<EventOutcome> {
        self.child.state_mut().view.scroll_by(x, y);
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn page_up(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().view.page_up();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn page_down(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().view.page_down();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn up(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().view.up();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn down(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().view.down();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn left(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().view.left();
        Ok(EventOutcome::Handle { skip: false })
    }

    pub fn right(&mut self) -> Result<EventOutcome> {
        self.child.state_mut().view.right();
        Ok(EventOutcome::Handle { skip: false })
    }
}

impl<S, N> Layout<S> for Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn layout_children(&mut self, app: &mut Canopy<S>) -> Result<()> {
        // $r.state().view.screen() ==>> $r.screen()
        let rect = self.state().view.screen();
        self.child.constrain(app, rect.w)?;
        self.child.layout(app, rect)?;
        Ok(())
    }
}

impl<S, N> FrameContent for Scroll<S, N> where N: Node<S> + ConstrainedWidthLayout<S> {}

impl<S, N> Node<S> for Scroll<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
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
}
