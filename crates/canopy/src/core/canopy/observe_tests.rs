//! Scroll observation: which scroll changes poll an observer, and when the
//! observation ends.

use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

use super::*;
use crate::{
    Context, ViewContext,
    error::{Error, Result},
    geom::{Point, PointI32, Rect, Size},
    input::{Event, key, mouse},
    layout::{CanvasContext, Layout, RevealAlign, ScrollDirection, ScrollOp},
    testing::ManualClock,
    widget::Widget,
};

/// A one-row widget that counts its polls.
struct Observer {
    /// Polls so far.
    polls: Rc<Cell<usize>>,
}

impl Widget for Observer {
    fn layout(&self) -> Layout {
        Layout::column().fixed_height(1)
    }

    fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
        self.polls.set(self.polls.get() + 1);
        Ok(None)
    }
}

/// A focusable surface that fills its space and scrolls a 100 by 100 canvas.
struct Surface;

impl Widget for Surface {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn canvas(&self, _view: Size, _ctx: &CanvasContext) -> Size {
        Size::new(100, 100)
    }
}

/// A fill-sized column that stacks its children.
struct Column;

impl Widget for Column {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

/// An application with an observer row above a surface it observes.
struct Fixture {
    canopy: Canopy,
    observer: NodeId,
    surface: NodeId,
    polls: Rc<Cell<usize>>,
}

impl Fixture {
    fn new() -> Result<Self> {
        let polls = Rc::new(Cell::new(0));
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.set_clock_for_testing(Arc::new(ManualClock::new()))?;
        let root: NodeId = canopy.replace_root(Column)?.into();
        let observer = canopy.core.create_detached(Observer {
            polls: Rc::clone(&polls),
        })?;
        canopy.core.attach(root, observer)?;
        let surface = canopy.core.create_detached(Surface)?;
        canopy.core.attach(root, surface)?;
        canopy.core.set_focus(surface)?;
        canopy.set_screen_size(Size::new(10, 4))?;
        canopy.turn(TurnInput::Prepare)?;
        canopy.with_context(observer, |ctx| ctx.observe_scroll(surface))?;
        polls.set(0);
        Ok(Self {
            canopy,
            observer,
            surface,
            polls,
        })
    }

    /// Run one turn and return the polls it made.
    fn turn(&mut self, input: TurnInput) -> Result<usize> {
        self.polls.set(0);
        self.canopy.turn(input)?;
        Ok(self.polls.get())
    }

    /// Return the surface's scroll offset as its view reports it.
    fn offset(&self) -> Point {
        self.canopy.core.nodes[self.surface].view.scroll
    }

    /// Return a wheel event over the surface.
    fn wheel(action: mouse::Action) -> Event {
        Event::Mouse(mouse::MouseEvent {
            action,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: PointI32 { x: 0, y: 2 },
        })
    }
}

#[test]
fn a_wheel_default_scroll_polls_the_observer_in_the_same_turn() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let polls = fixture.turn(TurnInput::Events(vec![Fixture::wheel(
        mouse::Action::ScrollDown,
    )]))?;
    assert_eq!(polls, 1);
    assert!(fixture.offset().y > 0, "the wheel scrolled the surface");
    Ok(())
}

#[test]
fn a_key_that_scrolls_through_navigation_polls_the_observer() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture
        .canopy
        .eval_script(r#"canopy.bind("down", {description = "Down"}, "canopy.nav.down")"#)?;
    let down = key::Key::parse_spec("down")?;
    assert_eq!(fixture.turn(TurnInput::Events(vec![Event::Key(down)]))?, 1);
    assert_eq!(fixture.offset().y, 1);
    Ok(())
}

#[test]
fn scroll_node_and_a_reveal_poll_the_observer() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let (observer, surface) = (fixture.observer, fixture.surface);
    fixture.canopy.with_context(observer, |ctx| {
        ctx.scroll_node(surface, ScrollOp::Lines(ScrollDirection::Down, 5))
    })?;
    assert_eq!(fixture.turn(TurnInput::Wake)?, 1);
    assert_eq!(fixture.offset().y, 5);

    // A reveal moves the offset while the frame lays out, so the poll runs on
    // the next turn.
    fixture.canopy.with_context(surface, |ctx| {
        ctx.reveal_area(Rect::new(0, 50, 1, 1), RevealAlign::Nearest);
        Ok(())
    })?;
    assert_eq!(fixture.turn(TurnInput::Wake)?, 0);
    assert_eq!(fixture.turn(TurnInput::Wake)?, 1);
    assert_eq!(fixture.offset().y, 48);
    Ok(())
}

#[test]
fn a_scroll_that_moves_nothing_does_not_poll() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let wheel_up = Fixture::wheel(mouse::Action::ScrollUp);
    assert_eq!(fixture.turn(TurnInput::Events(vec![wheel_up]))?, 0);
    let (observer, surface) = (fixture.observer, fixture.surface);
    let outcome = fixture.canopy.with_context(observer, |ctx| {
        ctx.scroll_node(surface, ScrollOp::To(Point::ZERO))
    })?;
    assert!(!outcome.changed());
    assert_eq!(fixture.turn(TurnInput::Wake)?, 0);
    Ok(())
}

#[test]
fn observing_twice_polls_once() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let (observer, surface) = (fixture.observer, fixture.surface);
    fixture
        .canopy
        .with_context(observer, |ctx| ctx.observe_scroll(surface))?;
    assert_eq!(fixture.canopy.core.nodes[surface].scroll_observers.len(), 1);
    let wheel = Fixture::wheel(mouse::Action::ScrollDown);
    assert_eq!(fixture.turn(TurnInput::Events(vec![wheel]))?, 1);
    Ok(())
}

#[test]
fn removing_the_observer_ends_the_observation() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.canopy.core.remove_subtree(fixture.observer)?;
    let wheel = Fixture::wheel(mouse::Action::ScrollDown);
    fixture.canopy.turn(TurnInput::Events(vec![wheel]))?;
    assert!(fixture.offset().y > 0, "the surface still scrolls");
    assert_eq!(fixture.polls.get(), 0);
    assert!(
        fixture.canopy.core.nodes[fixture.surface]
            .scroll_observers
            .is_empty(),
        "the scroll dropped the expired observer"
    );
    Ok(())
}

#[test]
fn removing_the_observed_node_ends_the_observation() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let (observer, surface) = (fixture.observer, fixture.surface);
    fixture.canopy.core.remove_subtree(surface)?;
    assert_eq!(fixture.turn(TurnInput::Prepare)?, 0);
    assert!(matches!(
        fixture
            .canopy
            .with_context(observer, |ctx| ctx.observe_scroll(surface)),
        Err(Error::NodeNotFound(id)) if id == surface
    ));
    Ok(())
}
