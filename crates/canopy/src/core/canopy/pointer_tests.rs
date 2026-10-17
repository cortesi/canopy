//! Tests of widgets that let the pointer pass through to the nodes beneath.

use std::cell::RefCell;

use super::*;
use crate::{
    Context, ViewContext,
    error::Result,
    geom::{Point, PointI32},
    input::{Event, ModalBindings, ModalOptions, key, mouse},
    layout::{Align, Direction, Edges, Layout},
    testing::backend::TestRender,
    widget::{EventOutcome, Widget},
};

thread_local! {
    /// Each mouse event a probe received: its name and the action.
    static SEEN: RefCell<Vec<(&'static str, mouse::Action)>> = const { RefCell::new(Vec::new()) };
}

/// Takes the log.
fn seen() -> Vec<(&'static str, mouse::Action)> {
    SEEN.with(|seen| seen.take())
}

/// Takes the log without the `Leave` events that hover tracking adds.
fn routed() -> Vec<(&'static str, mouse::Action)> {
    seen()
        .into_iter()
        .filter(|(_, action)| *action != mouse::Action::Leave)
        .collect()
}

/// A widget that logs each mouse event, takes focus on a click, and can
/// capture the mouse.
struct Probe {
    /// Name in the log.
    name: &'static str,
    /// Whether a press captures the mouse.
    capture: bool,
}

impl Widget for Probe {
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(m) = event else {
            return Ok(EventOutcome::Ignore);
        };
        SEEN.with(|seen| seen.borrow_mut().push((self.name, m.action)));
        if m.action == mouse::Action::Down {
            ctx.set_focus(ctx.node_id())?;
            if self.capture {
                ctx.capture_mouse()?;
            }
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
}

/// A full-size layer that places its child at the bottom right and lets the
/// pointer through everywhere else.
struct Layer;

impl Widget for Layer {
    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(m) = event {
            SEEN.with(|seen| seen.borrow_mut().push(("layer", m.action)));
        }
        Ok(EventOutcome::Ignore)
    }

    fn accept_pointer(&self, _ctx: &dyn ViewContext) -> bool {
        false
    }
}

/// The nodes of the fixture tree.
struct Tree {
    /// Application.
    canopy: Canopy,
    /// The node that fills the screen beneath the layer.
    app: NodeId,
    /// The pass-through layer over it.
    layer: NodeId,
    /// The layer's child: four by one cells at the bottom right, inset by one.
    badge: NodeId,
}

/// Builds a 20 by 6 screen: `app` fills it, and the pass-through `layer`
/// stacks over it with `badge` at columns 15 to 18 of row 4.
fn tree(capture: bool) -> Result<Tree> {
    SEEN.with(|seen| seen.borrow_mut().clear());
    let mut canopy = CanopyBuilder::new().build()?;
    let root = canopy.core.root;
    canopy
        .core
        .set_layout_of(root, Layout::fill().direction(Direction::Stack))?;
    let app = canopy.core.add_child_to_boxed(
        root,
        Box::new(Probe {
            name: "app",
            capture,
        }),
    )?;
    canopy.core.set_layout_of(app, Layout::fill())?;
    let layer = canopy.core.add_child_to_boxed(root, Box::new(Layer))?;
    canopy.core.set_layout_of(
        layer,
        Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::End)
            .align_vertical(Align::End)
            .padding(Edges::new(0, 1, 1, 0)),
    )?;
    let badge = canopy.core.add_child_to_boxed(
        layer,
        Box::new(Probe {
            name: "badge",
            capture: false,
        }),
    )?;
    canopy
        .core
        .set_layout_of(badge, Layout::fill().fixed_width(4).fixed_height(1))?;
    canopy.set_screen_size(Size::new(20, 6))?;
    canopy.render(&mut TestRender::new())?;
    Ok(Tree {
        canopy,
        app,
        layer,
        badge,
    })
}

/// Sends one mouse event at a screen cell.
fn mouse_at(canopy: &mut Canopy, action: mouse::Action, x: i32, y: i32) -> Result<()> {
    let button = match action {
        mouse::Action::Down | mouse::Action::Up | mouse::Action::Drag => mouse::Button::Left,
        _ => mouse::Button::None,
    };
    canopy.event(&Event::Mouse(mouse::MouseEvent {
        action,
        button,
        modifiers: key::Empty,
        location: PointI32 { x, y },
    }))
}

#[test]
fn locate_node_passes_over_the_layer_but_not_its_child() -> Result<()> {
    let Tree {
        canopy,
        app,
        layer,
        badge,
    } = tree(false)?;
    let at = |x, y| canopy.core.locate_node(canopy.core.root, Point { x, y });
    assert_eq!(at(2, 1)?, Some(app), "the layer's empty area is the app's");
    assert_eq!(at(16, 4)?, Some(badge), "the layer's child is a target");
    assert_eq!(at(16, 5)?, Some(app), "the padding passes through too");
    assert_ne!(at(19, 0)?, Some(layer));
    Ok(())
}

#[test]
fn a_click_and_a_wheel_over_the_layer_reach_the_app() -> Result<()> {
    let Tree {
        mut canopy, app, ..
    } = tree(false)?;
    mouse_at(&mut canopy, mouse::Action::Down, 2, 1)?;
    mouse_at(&mut canopy, mouse::Action::ScrollDown, 3, 2)?;
    assert_eq!(
        seen(),
        [
            ("app", mouse::Action::Down),
            ("app", mouse::Action::ScrollDown)
        ],
        "the layer sees nothing"
    );
    assert_eq!(canopy.core.focus, Some(app), "the click focused the app");
    Ok(())
}

#[test]
fn a_click_on_the_layer_child_reaches_the_child() -> Result<()> {
    let Tree {
        mut canopy, badge, ..
    } = tree(false)?;
    mouse_at(&mut canopy, mouse::Action::Down, 15, 4)?;
    assert_eq!(seen(), [("badge", mouse::Action::Down)]);
    assert_eq!(canopy.core.focus, Some(badge));
    Ok(())
}

#[test]
fn hover_leaves_the_app_for_the_child_and_back() -> Result<()> {
    let Tree { mut canopy, .. } = tree(false)?;
    mouse_at(&mut canopy, mouse::Action::Moved, 2, 1)?;
    mouse_at(&mut canopy, mouse::Action::Moved, 16, 4)?;
    mouse_at(&mut canopy, mouse::Action::Moved, 2, 1)?;
    let leaves = seen()
        .into_iter()
        .filter(|(_, action)| *action == mouse::Action::Leave)
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    assert_eq!(
        leaves,
        ["app", "badge", "layer"],
        "the pointer leaves the app for the child, then the child and the layer \
         that holds it for the app"
    );
    Ok(())
}

#[test]
fn a_capture_holds_the_pointer_over_the_layer_child() -> Result<()> {
    let Tree { mut canopy, .. } = tree(true)?;
    mouse_at(&mut canopy, mouse::Action::Down, 2, 1)?;
    mouse_at(&mut canopy, mouse::Action::Drag, 16, 4)?;
    assert_eq!(
        routed(),
        [("app", mouse::Action::Down), ("app", mouse::Action::Drag)],
        "the capture takes the drag over the child"
    );
    Ok(())
}

#[test]
fn a_modal_admits_a_click_that_passes_through_the_layer() -> Result<()> {
    let Tree {
        mut canopy, app, ..
    } = tree(false)?;
    let root = canopy.core.root;
    canopy.core.open_modal(ModalOptions {
        owner: root,
        modal: app,
        initial_focus: app,
        dim_target: None,
        bindings: ModalBindings::Application,
    })?;
    mouse_at(&mut canopy, mouse::Action::Down, 2, 1)?;
    mouse_at(&mut canopy, mouse::Action::Down, 16, 4)?;
    assert_eq!(
        routed(),
        [("app", mouse::Action::Down)],
        "the modal admits the app beneath the layer, and not the child outside it"
    );
    Ok(())
}

#[test]
fn the_overlay_modal_opened_last_is_on_top() -> Result<()> {
    let Tree {
        mut canopy, app, ..
    } = tree(false)?;
    let root = canopy.core.root;
    // Mounted in this order, the second would cover the first.
    let mut overlay = |name| -> Result<NodeId> {
        let node = canopy.core.add_child_to_boxed(
            root,
            Box::new(Probe {
                name,
                capture: false,
            }),
        )?;
        canopy.core.set_layout_of(node, Layout::fill())?;
        canopy.core.set_hidden(node, true)?;
        Ok(node)
    };
    let first = overlay("first")?;
    let second = overlay("second")?;
    let open = |canopy: &mut Canopy, owner, modal| {
        canopy.core.open_modal(ModalOptions {
            owner,
            modal,
            initial_focus: modal,
            dim_target: None,
            bindings: ModalBindings::Application,
        })
    };
    open(&mut canopy, app, second)?;
    open(&mut canopy, second, first)?;
    canopy.render(&mut TestRender::new())?;
    let order = canopy.core.paint_order(root).into_owned();
    let position = |node| order.iter().position(|child| *child == node);
    assert!(position(first) > position(second), "{order:?}");
    assert_eq!(
        canopy.core.locate_node(root, Point { x: 2, y: 1 })?,
        Some(first)
    );
    mouse_at(&mut canopy, mouse::Action::Down, 2, 1)?;
    assert_eq!(routed(), [("first", mouse::Action::Down)]);
    Ok(())
}
