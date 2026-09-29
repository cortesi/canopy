//! Tests of the mouse `Leave` event.

use std::cell::RefCell;

use super::*;
use crate::{
    Context,
    error::Result,
    geom::PointI32,
    input::{Event, key, mouse},
    layout::Layout,
    testing::backend::TestRender,
    widget::{EventOutcome, Widget},
};

thread_local! {
    /// Each `Leave` that a probe received: its name and the local location.
    static LEFT: RefCell<Vec<(&'static str, PointI32)>> = const { RefCell::new(Vec::new()) };
}

/// A widget that logs each `Leave` it receives and ignores every event.
struct Probe {
    /// Name in the log.
    name: &'static str,
}

impl Widget for Probe {
    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(m) = event
            && m.action == mouse::Action::Leave
        {
            LEFT.with(|left| left.borrow_mut().push((self.name, m.location)));
        }
        Ok(EventOutcome::Ignore)
    }
}

/// Takes the log.
fn left() -> Vec<(&'static str, PointI32)> {
    LEFT.with(|left| left.take())
}

/// The nodes of the fixture tree.
struct Tree {
    /// Application.
    canopy: Canopy,
    /// Left half of the screen.
    left: NodeId,
    /// The node that fills the right half.
    inner: NodeId,
}

/// Builds `outer` on a 20 by 4 screen, with `left` on the left half and
/// `right` on the right half. `inner` fills `right`.
fn tree() -> Result<Tree> {
    LEFT.with(|left| left.borrow_mut().clear());
    let mut canopy = CanopyBuilder::new().build()?;
    let root = canopy.core.root;
    let outer = canopy
        .core
        .add_child_to_boxed(root, Box::new(Probe { name: "outer" }))?;
    canopy
        .core
        .set_layout_of(outer, Layout::row().flex_horizontal(1).flex_vertical(1))?;
    let left = canopy
        .core
        .add_child_to_boxed(outer, Box::new(Probe { name: "left" }))?;
    canopy.core.set_layout_of(left, Layout::fill())?;
    let right = canopy
        .core
        .add_child_to_boxed(outer, Box::new(Probe { name: "right" }))?;
    canopy.core.set_layout_of(right, Layout::fill())?;
    let inner = canopy
        .core
        .add_child_to_boxed(right, Box::new(Probe { name: "inner" }))?;
    canopy.core.set_layout_of(inner, Layout::fill())?;
    canopy.set_screen_size(Size::new(20, 4))?;
    canopy.render(&mut TestRender::new())?;
    Ok(Tree {
        canopy,
        left,
        inner,
    })
}

/// Moves the pointer to one screen cell.
fn move_to(canopy: &mut Canopy, x: i32, y: i32) -> Result<()> {
    canopy.event(&Event::Mouse(mouse::MouseEvent {
        action: mouse::Action::Moved,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: PointI32 { x, y },
    }))
}

#[test]
fn a_move_to_a_sibling_leaves_only_the_last_node() -> Result<()> {
    let Tree { mut canopy, .. } = tree()?;
    move_to(&mut canopy, 2, 1)?;
    assert!(left().is_empty(), "the first move leaves nothing");
    move_to(&mut canopy, 12, 1)?;
    assert_eq!(
        left(),
        [("left", PointI32 { x: 12, y: 1 })],
        "outer holds the new node, so it does not get Leave"
    );
    move_to(&mut canopy, 13, 2)?;
    assert!(left().is_empty(), "a move within a node leaves nothing");
    Ok(())
}

#[test]
fn a_move_out_of_a_nested_node_leaves_it_and_its_parent() -> Result<()> {
    let Tree { mut canopy, .. } = tree()?;
    move_to(&mut canopy, 12, 1)?;
    move_to(&mut canopy, 2, 1)?;
    assert_eq!(
        left(),
        [
            ("inner", PointI32 { x: -8, y: 1 }),
            ("right", PointI32 { x: -8, y: 1 }),
        ],
        "the location is local to each node"
    );
    Ok(())
}

#[test]
fn a_move_off_the_screen_leaves_every_node() -> Result<()> {
    let Tree { mut canopy, .. } = tree()?;
    move_to(&mut canopy, 12, 1)?;
    move_to(&mut canopy, -1, 1)?;
    let names = left().into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert_eq!(names, ["inner", "right", "outer"]);
    Ok(())
}

#[test]
fn a_focus_loss_leaves_every_node_under_the_pointer_once() -> Result<()> {
    let Tree { mut canopy, .. } = tree()?;
    canopy.event(&Event::FocusLost)?;
    assert!(left().is_empty(), "no pointer, no Leave");
    move_to(&mut canopy, 12, 1)?;
    canopy.event(&Event::FocusLost)?;
    let names = left().into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert_eq!(names, ["inner", "right", "outer"]);
    canopy.event(&Event::FocusLost)?;
    move_to(&mut canopy, 2, 1)?;
    assert!(left().is_empty(), "the pointer came back to a new node");
    Ok(())
}

#[test]
fn an_explicit_leave_clears_the_pointer_without_bubbling() -> Result<()> {
    let Tree { mut canopy, .. } = tree()?;
    move_to(&mut canopy, 12, 1)?;
    canopy.event(&Event::Mouse(mouse::MouseEvent {
        action: mouse::Action::Leave,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: PointI32 { x: 12, y: 1 },
    }))?;
    let names = left().into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert_eq!(names, ["inner", "right", "outer"]);
    move_to(&mut canopy, 2, 1)?;
    assert!(left().is_empty(), "the old pointer path is gone");
    Ok(())
}

#[test]
fn a_removed_node_gets_no_leave() -> Result<()> {
    let Tree {
        mut canopy,
        left: node,
        ..
    } = tree()?;
    move_to(&mut canopy, 2, 1)?;
    canopy.core.remove_subtree(node)?;
    canopy.render(&mut TestRender::new())?;
    move_to(&mut canopy, 12, 1)?;
    assert!(left().is_empty());
    Ok(())
}

#[test]
fn the_ancestors_of_a_removed_node_still_get_leave() -> Result<()> {
    let Tree {
        mut canopy, inner, ..
    } = tree()?;
    move_to(&mut canopy, 12, 1)?;
    canopy.core.remove_subtree(inner)?;
    canopy.render(&mut TestRender::new())?;
    move_to(&mut canopy, 2, 1)?;
    let names = left().into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert_eq!(names, ["right"]);
    Ok(())
}

#[test]
fn leave_prints_as_a_mouse_action_and_is_not_a_binding_spec() {
    let error = mouse::Mouse::parse_spec("Leave").expect_err("no binding");
    assert!(error.to_string().contains("runs no binding"), "{error}");
    let spec = mouse::Mouse {
        action: mouse::Action::Leave,
        button: mouse::Button::None,
        modifiers: key::Empty,
    };
    assert_eq!(spec.to_string(), "Leave", "a trace names it");
    assert_eq!(mouse::Action::Leave.scroll_delta(), None);
}
