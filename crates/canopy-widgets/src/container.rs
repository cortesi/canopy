//! A container that supplies a layout and nothing else.

use canopy::{
    NodeName, ViewContext, Widget,
    layout::{Align, Direction, Edges, Layout},
};

/// A widget that lays out its children and has no other behavior.
///
/// Parents adjust how each child sizes through layout overrides. Use
/// [`Container::with_name`] to keep a path segment that scripts or bindings
/// match.
///
/// A [focusable](Container::focusable) container holds focus itself, so its
/// children can show, hide, and swap without moving focus. Bindings with a
/// path through the container then apply the whole time.
pub struct Container {
    /// Layout applied to the children.
    layout: Layout,
    /// Path segment for this node.
    name: NodeName,
    /// Whether the container itself accepts focus.
    focusable: bool,
}

impl Container {
    /// Construct a container with any layout.
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            name: NodeName::convert("container"),
            focusable: false,
        }
    }

    /// Fill the available space and place children in a row.
    pub fn row() -> Self {
        Self::new(Layout::fill().direction(Direction::Row))
    }

    /// Fill the available space and stack children in a column.
    pub fn column() -> Self {
        Self::new(Layout::fill().direction(Direction::Column))
    }

    /// Fill the available space and overlap children, the last on top.
    pub fn stack() -> Self {
        Self::new(Layout::fill().direction(Direction::Stack))
    }

    /// Fill the available space and center each child on both axes, overlapped.
    ///
    /// For a dimmed overlay, push an effect on the background content with
    /// `c.push_effect(background_id, effects::brightness(0.5))`. The centered
    /// content stays at full brightness because it is a sibling of the dimmed
    /// content, not a descendant. Insert it inside a parent that uses `Stack`
    /// layout so it can overlay the existing view.
    pub fn center() -> Self {
        Self::new(
            Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::Center)
                .align_vertical(Align::Center),
        )
        .with_name("center")
    }

    /// Fill the available space and inset the children by `padding`.
    pub fn padded(padding: Edges) -> Self {
        Self::new(Layout::fill().padding(padding)).with_name("pad")
    }

    /// Name this node's path segment.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = NodeName::convert(name);
        self
    }

    /// Accept focus as a node of its own.
    #[must_use]
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    /// Accept focus, or stop accepting it.
    ///
    /// A container that stops while it holds focus keeps it until focus
    /// repair runs at the next layout. Repair walks forward in pre-order, so
    /// focus moves to the container's first focusable descendant when it has
    /// one. A modal that closes over the container restores focus the same
    /// way.
    pub fn set_focusable(&mut self, focusable: bool) {
        self.focusable = focusable;
    }
}

impl Widget for Container {
    fn layout(&self) -> Layout {
        self.layout
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.focusable
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{
        Context, ContextExt, NodeId, ViewContext, Widget,
        error::Result,
        input::{ModalBindings, ModalOptions},
        layout::Layout,
        testing::harness::Harness,
    };

    use super::*;

    /// A leaf that accepts focus.
    struct Pane;

    impl Widget for Pane {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }
    }

    /// A leaf that refuses focus.
    struct Plain;

    impl Widget for Plain {}

    /// The nodes a [`Scene`] mounts.
    #[derive(Clone, Copy)]
    struct Nodes {
        /// The focusable container.
        region: NodeId,
        /// A child that refuses focus.
        plain: NodeId,
        /// A child that accepts focus, hidden at first.
        pane: NodeId,
        /// A dialog outside the container, hidden until a modal opens.
        dialog: NodeId,
    }

    /// Root that mounts a focusable container beside a dialog.
    struct Scene {
        /// The mounted nodes.
        nodes: Rc<RefCell<Option<Nodes>>>,
    }

    impl Widget for Scene {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let region = c.add_child(c.node_id(), Container::column().focusable())?;
            c.set_layout_override(region.into(), Layout::fill().into())?;
            let plain = c.add_child(region, Plain)?;
            c.set_layout_override(plain.into(), Layout::fill().into())?;
            let pane = c.add_child(region, Pane)?;
            c.set_layout_override(pane.into(), Layout::fill().into())?;
            c.set_hidden(pane.into(), true)?;
            let dialog = c.add_child(c.node_id(), Pane)?;
            c.set_hidden(dialog.into(), true)?;
            *self.nodes.borrow_mut() = Some(Nodes {
                region: region.into(),
                plain: plain.into(),
                pane: pane.into(),
                dialog: dialog.into(),
            });
            c.set_focus(region.into()).map(|_| ())
        }
    }

    /// Build a scene and return its harness and nodes.
    fn scene() -> Result<(Harness, Nodes)> {
        let nodes = Rc::new(RefCell::new(None));
        let mut harness = Harness::builder(Scene {
            nodes: Rc::clone(&nodes),
        })
        .size(20, 6)
        .build()?;
        harness.render()?;
        let nodes = nodes.borrow().expect("the scene mounts");
        Ok((harness, nodes))
    }

    /// Return the focused node.
    fn focused(harness: &Harness) -> Option<NodeId> {
        harness.canopy.with_root_view(|c| c.focused_node())
    }

    /// Show `shown` and hide `hidden`.
    fn swap(harness: &mut Harness, shown: NodeId, hidden: NodeId) -> Result<()> {
        harness.canopy.with_root_context(|c| {
            c.set_hidden(hidden, true)?;
            c.set_hidden(shown, false).map(|_| ())
        })?;
        harness.render()
    }

    /// Accept focus on the container, or stop accepting it.
    fn set_focusable(harness: &mut Harness, region: NodeId, focusable: bool) -> Result<()> {
        harness.canopy.with_root_context(|c| {
            c.with_widget_mut(region, |container: &mut Container, _| {
                container.set_focusable(focusable);
                Ok(())
            })
        })
    }

    #[test]
    fn a_focusable_container_keeps_focus_while_its_children_swap() -> Result<()> {
        let (mut harness, nodes) = scene()?;
        assert_eq!(focused(&harness), Some(nodes.region));

        swap(&mut harness, nodes.pane, nodes.plain)?;
        assert_eq!(
            focused(&harness),
            Some(nodes.region),
            "a swap leaves focus alone"
        );
        swap(&mut harness, nodes.plain, nodes.pane)?;
        assert_eq!(focused(&harness), Some(nodes.region));
        Ok(())
    }

    #[test]
    fn a_container_that_stops_accepting_focus_hands_it_to_a_child() -> Result<()> {
        let (mut harness, nodes) = scene()?;
        swap(&mut harness, nodes.pane, nodes.plain)?;

        set_focusable(&mut harness, nodes.region, false)?;
        harness.render()?;
        assert_eq!(
            focused(&harness),
            Some(nodes.pane),
            "repair moves focus to the first focusable descendant"
        );

        set_focusable(&mut harness, nodes.region, true)?;
        harness.render()?;
        assert_eq!(
            focused(&harness),
            Some(nodes.pane),
            "accepting focus again does not take it back"
        );
        Ok(())
    }

    #[test]
    fn a_modal_that_closes_over_a_refusing_container_focuses_its_child() -> Result<()> {
        let (mut harness, nodes) = scene()?;
        swap(&mut harness, nodes.pane, nodes.plain)?;
        let owner = harness.root;
        let token = harness.canopy.with_root_context(|c| {
            c.open_modal(ModalOptions {
                owner,
                modal: nodes.dialog,
                initial_focus: nodes.dialog,
                dim_target: None,
                bindings: ModalBindings::Application,
            })
        })?;
        harness.render()?;
        assert_eq!(focused(&harness), Some(nodes.dialog));

        set_focusable(&mut harness, nodes.region, false)?;
        harness.canopy.with_root_context(|c| c.close_modal(token))?;
        harness.render()?;
        assert_eq!(
            focused(&harness),
            Some(nodes.pane),
            "the restore searches the refusing container's subtree"
        );
        Ok(())
    }
}
