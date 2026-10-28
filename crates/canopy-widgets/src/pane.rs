//! A column that paints the ground of a pane, in the focus ground while the
//! pane has the keys.

use std::any::TypeId;

use canopy::{
    NodeId, NodeName, ViewContext, Widget,
    error::Result,
    layout::{Direction, Layout},
    render::Render,
    style::WidgetState,
};

use crate::Columns;

/// Which focus gives a pane its keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneKeys {
    /// The pane that its `Columns` marks: the pane on the focus path, or the
    /// home pane while focus is on the columns or above them. Outside
    /// `Columns`, focus within the pane.
    Marked,
    /// Focus within the given node, such as a region inside the pane.
    Within(NodeId),
}

/// A column that paints the ground of a pane.
///
/// The pane pushes `pane`, and `focused` while it has the keys, and fills
/// `background`. The built-in themes give `pane/focused/background` the focus
/// ground, which the box of a framed [`Columns`] shares. Then the pane pushes
/// its name, and `focused` again while it has the keys. Descendants resolve
/// their styles under these layers, so a rule such as
/// `app_pane/focused/editor/text` gives a child that paints its own ground the
/// focus ground.
pub struct Pane {
    /// The node name, and the style layer of the descendants.
    name: &'static str,
    /// Which focus gives the pane its keys.
    keys: PaneKeys,
    /// Whether the pane itself accepts focus.
    focusable: bool,
}

impl Pane {
    /// Construct a pane named `name`, with the keys while its `Columns` marks
    /// it.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            keys: PaneKeys::Marked,
            focusable: false,
        }
    }

    /// Build a pane whose keys follow `keys`.
    #[must_use]
    pub fn with_keys(mut self, keys: PaneKeys) -> Self {
        self.keys = keys;
        self
    }

    /// Hold focus, as `Container::focusable` does, so children can swap
    /// without moving focus.
    #[must_use]
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    /// Return whether this pane has the keys. `ctx` is the context of this
    /// pane.
    pub fn has_keys(&self, ctx: &dyn ViewContext) -> bool {
        pane_has_keys(ctx, ctx.node_id(), &self.keys)
    }
}

/// Return whether `node` has the keys under `keys`, for a widget that is its
/// own pane.
///
/// For [`PaneKeys::Marked`], the pane that holds `node` is `node` itself or
/// its ancestor that is a child of a `Columns`. The node has the keys while
/// those columns mark that pane. A node outside `Columns` has the keys while
/// focus is within it.
pub fn pane_has_keys(ctx: &dyn ViewContext, node: NodeId, keys: &PaneKeys) -> bool {
    match *keys {
        PaneKeys::Within(region) => ctx.is_on_focus_path(region),
        PaneKeys::Marked => match holding_columns(ctx, node) {
            Some((columns, pane)) => Columns::marked_pane(ctx, columns) == Some(pane),
            None => ctx.is_on_focus_path(node),
        },
    }
}

/// Return the nearest `Columns` above `node`, and its child that holds
/// `node`.
fn holding_columns(ctx: &dyn ViewContext, node: NodeId) -> Option<(NodeId, NodeId)> {
    let columns = TypeId::of::<Columns>();
    let mut pane = node;
    loop {
        let parent = ctx.parent_of(pane)?;
        if ctx.type_id_of(parent) == Some(columns) {
            return Some((parent, pane));
        }
        pane = parent;
    }
}

impl Widget for Pane {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.focusable
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        // The stack ends in `pane/focused` as the pane fills, so the theme's
        // rule applies under any outer layers. The descendants then resolve
        // under `<name>/focused`.
        let focused = self.has_keys(ctx).then(|| WidgetState::Focused.layer());
        render.push_layer("pane");
        if let Some(layer) = focused {
            render.push_layer(layer);
        }
        render.fill("background", ctx.view().outer_rect_local(), ' ')?;
        render.push_layer(self.name);
        if let Some(layer) = focused {
            render.push_layer(layer);
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert(self.name)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{
        Context, ContextExt,
        geom::{Point, Rect},
        style::Color,
        testing::harness::Harness,
    };

    use super::*;
    use crate::{BoxGlyphs, Container};

    /// Builds the scene under the root and returns the nodes a test uses.
    type Build = Box<dyn FnOnce(&mut dyn Context) -> Result<Vec<NodeId>>>;

    /// Root that renders under a host layer and builds a scene when mounted.
    struct Host {
        /// Scene builder, consumed on mount.
        build: Option<Build>,
        /// Nodes the builder returned.
        nodes: Rc<RefCell<Vec<NodeId>>>,
    }

    impl Widget for Host {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            if let Some(build) = self.build.take() {
                *self.nodes.borrow_mut() = build(c)?;
            }
            Ok(())
        }

        fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            // An outer layer, as a host widget pushes.
            render.push_layer("host");
            Ok(())
        }
    }

    /// A leaf that accepts focus and paints `leaf` in its first cell only.
    struct Leaf;

    impl Widget for Leaf {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            render.fill("leaf", Rect::new(0, 0, 1, 1), 'x')
        }

        fn name(&self) -> NodeName {
            NodeName::convert("leaf")
        }
    }

    /// Render a 13x4 scene and return the nodes its builder chose.
    fn scene(
        build: impl FnOnce(&mut dyn Context) -> Result<Vec<NodeId>> + 'static,
    ) -> Result<(Harness, Vec<NodeId>)> {
        let nodes = Rc::new(RefCell::new(Vec::new()));
        let root = Host {
            build: Some(Box::new(build)),
            nodes: Rc::clone(&nodes),
        };
        let mut harness = Harness::builder(root).size(13, 4).build()?;
        harness.render()?;
        let nodes = nodes.borrow().clone();
        Ok((harness, nodes))
    }

    /// Build a focusable holder, framed columns in it, and two panes of
    /// five cells, each with a leaf. Return the holder, the columns, the
    /// panes, and the leaves.
    fn columns(c: &mut dyn Context) -> Result<Vec<NodeId>> {
        let root = c.node_id();
        let holder: NodeId = c.add_child(root, Container::column().focusable())?.into();
        let columns: NodeId = c
            .add_child(
                holder,
                Columns::new().with_frame(BoxGlyphs::SINGLE, BoxGlyphs::SINGLE_THICK),
            )?
            .into();
        let mut nodes = vec![holder, columns];
        for name in ["left", "right"] {
            let pane: NodeId = c.add_child(columns, Pane::new(name))?.into();
            nodes.push(pane);
        }
        for pane in [nodes[2], nodes[3]] {
            nodes.push(c.add_child(pane, Leaf)?.into());
        }
        Ok(nodes)
    }

    /// Return the background drawn at a cell.
    fn bg(harness: &Harness, x: u32, y: u32) -> Color {
        harness
            .buf()
            .get(Point { x, y })
            .expect("the cell is on the screen")
            .style
            .bg
    }

    /// Focus `node` and render.
    fn focus(harness: &mut Harness, node: NodeId) -> Result<()> {
        harness
            .canopy
            .with_root_context(|c| c.set_focus(node).map(|_| ()))?;
        harness.render()
    }

    /// Return the grounds of the left and right panes, and of the top rules
    /// of their boxes. The panes are five cells wide, at columns 1 and 7.
    fn grounds(harness: &Harness) -> [Color; 4] {
        [
            bg(harness, 2, 2),
            bg(harness, 8, 2),
            bg(harness, 2, 0),
            bg(harness, 8, 0),
        ]
    }

    #[test]
    fn the_marked_pane_paints_the_focus_ground_as_its_box_does() -> Result<()> {
        let (mut harness, nodes) = scene(columns)?;
        focus(&mut harness, nodes[4])?;
        let [left, right, left_box, right_box] = grounds(&harness);
        assert_ne!(left, right, "only the marked pane takes the focus ground");
        assert_eq!(left, left_box, "the pane and its box agree");
        assert_eq!(right, right_box, "the other pane keeps the plain ground");
        Ok(())
    }

    #[test]
    fn the_home_pane_paints_the_focus_ground_while_focus_is_on_the_columns() -> Result<()> {
        let (mut harness, nodes) = scene(columns)?;
        focus(&mut harness, nodes[4])?;
        let [focus_ground, plain, ..] = grounds(&harness);
        let (holder, columns, right) = (nodes[0], nodes[1], nodes[3]);
        focus(&mut harness, holder)?;
        assert_eq!(
            grounds(&harness),
            [plain; 4],
            "focus above the columns marks no pane without a home"
        );
        harness.canopy.with_root_context(|c| {
            c.with_widget_mut(columns, |row: &mut Columns, _| {
                row.set_home(vec![right]);
                Ok(())
            })
        })?;
        harness.render()?;
        assert_eq!(
            grounds(&harness),
            [plain, focus_ground, plain, focus_ground],
            "the home pane and its box take the focus ground"
        );
        Ok(())
    }

    #[test]
    fn a_pane_within_a_region_follows_focus_in_it() -> Result<()> {
        let (mut harness, nodes) = scene(|c| {
            let root = c.node_id();
            let row: NodeId = c.add_child(root, Container::row())?.into();
            let region: NodeId = c.add_child(row, Container::column().focusable())?.into();
            let pane = c.add_child(
                region,
                Pane::new("view").with_keys(PaneKeys::Within(region)),
            )?;
            let other: NodeId = c.add_child(row, Container::column().focusable())?.into();
            Ok(vec![region, pane.into(), other])
        })?;
        let (region, other) = (nodes[0], nodes[2]);
        focus(&mut harness, other)?;
        let plain = bg(&harness, 0, 0);
        focus(&mut harness, region)?;
        let ground = bg(&harness, 0, 0);
        assert_ne!(ground, plain, "focus in the region gives the pane its keys");
        harness.canopy.with_root_context(|c| {
            assert!(pane_has_keys(c, nodes[1], &PaneKeys::Within(region)));
            assert!(!pane_has_keys(c, nodes[1], &PaneKeys::Within(other)));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn a_rule_under_the_pane_name_reaches_its_descendants() -> Result<()> {
        let (mut harness, nodes) = scene(columns)?;
        harness
            .canopy
            .style_mut()
            .rules()
            .bg("left/focused/leaf", Color::Red)
            .bg("right/leaf", Color::Blue)
            .apply();
        focus(&mut harness, nodes[4])?;
        assert_eq!(bg(&harness, 1, 1), Color::Red, "the focused pane's rule");
        assert_eq!(bg(&harness, 7, 1), Color::Blue, "the other pane's rule");
        Ok(())
    }
}
