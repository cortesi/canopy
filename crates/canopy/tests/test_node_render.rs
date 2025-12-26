//! Integration tests for node rendering.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Core, Loader, NodeId, ViewContext, buf, derive_commands,
        error::Result,
        geom::{Expanse, Rect},
        layout::Dimension,
        render::Render,
        state::NodeName,
        testing::harness::Harness,
        widget::Widget,
    };

    struct NodeB;

    #[derive_commands]
    impl NodeB {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for NodeB {
        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view(), 'B')?;
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("node_b")
        }
    }

    struct NodeA;

    #[derive_commands]
    impl NodeA {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for NodeA {
        fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("node_a")
        }
    }

    struct Root;

    #[derive_commands]
    impl Root {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for Root {
        fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("root")
        }
    }

    impl Loader for Root {
        fn load(c: &mut Canopy) {
            c.add_commands::<Self>();
            c.add_commands::<NodeA>();
            c.add_commands::<NodeB>();
        }
    }

    fn style_flex_child(core: &mut Core, id: NodeId) -> Result<()> {
        core.with_layout_of(id, |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })
    }

    fn build_split_tree(core: &mut Core, depth: usize, horizontal: bool) -> Result<NodeId> {
        let node = core.add(NodeA::new());
        core.with_layout_of(node, |layout| {
            layout.min_size(Dimension::Points(1.0), Dimension::Points(1.0));
        })?;
        if depth == 0 {
            return Ok(node);
        }

        let left = build_split_tree(core, depth - 1, !horizontal)?;
        let right = build_split_tree(core, depth - 1, !horizontal)?;
        core.set_children(node, vec![left, right])?;
        core.with_layout_of(node, |layout| {
            if horizontal {
                layout.flex_row();
            } else {
                layout.flex_col();
            }
        })?;
        style_flex_child(core, left)?;
        style_flex_child(core, right)?;
        Ok(node)
    }

    #[test]
    fn test_simple_node_fill() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(30, 10).build()?;

        let node_a = h.canopy.core.add(NodeA::new());
        let node_b = h.canopy.core.add(NodeB::new());
        h.canopy.core.set_children(h.root, vec![node_a])?;
        h.canopy.core.set_children(node_a, vec![node_b])?;

        h.canopy.core.with_layout_of(h.root, |layout| {
            layout.flex_col();
        })?;

        h.canopy.core.with_layout_of(node_a, |layout| {
            layout
                .flex_col()
                .size(Dimension::Points(10.0), Dimension::Points(5.0));
        })?;

        h.canopy.core.with_layout_of(node_b, |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })?;

        h.canopy.set_root_size(Expanse::new(30, 10))?;
        h.render()?;
        h.tbuf().assert_matches(buf![
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "                              "
            "                              "
            "                              "
            "                              "
            "                              "
        ]);
        Ok(())
    }

    #[test]
    fn test_zero_size_child_at_boundary_renders() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(10, 10).build()?;

        let container = h.canopy.core.add(NodeA::new());
        let top = h.canopy.core.add(NodeB::new());
        let bottom = h.canopy.core.add(NodeA::new());

        h.canopy.core.set_children(h.root, vec![container])?;
        h.canopy.core.set_children(container, vec![top, bottom])?;

        h.canopy.core.with_layout_of(h.root, |layout| {
            layout.flex_col();
        })?;

        h.canopy.core.with_layout_of(container, |layout| {
            layout.flex_col().flex_item(1.0, 1.0, Dimension::Auto);
        })?;

        h.canopy.core.with_layout_of(top, |layout| {
            layout.size(Dimension::Points(10.0), Dimension::Points(10.0));
        })?;

        h.canopy.core.with_layout_of(bottom, |layout| {
            layout.size(Dimension::Points(10.0), Dimension::Points(0.0));
        })?;

        h.canopy.set_root_size(Expanse::new(10, 10))?;
        h.render()?;

        let bottom_vp = h.canopy.core.nodes[bottom].vp;
        assert!(bottom_vp.view().is_zero());
        assert_eq!(bottom_vp.position().y, 10);

        Ok(())
    }

    #[test]
    fn test_resize_deep_tree_does_not_error() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(123, 31).build()?;

        let tree = build_split_tree(&mut h.canopy.core, 5, true)?;
        h.canopy.core.set_children(h.root, vec![tree])?;
        h.canopy.core.with_layout_of(h.root, |layout| {
            layout.flex_col();
        })?;
        style_flex_child(&mut h.canopy.core, tree)?;

        h.render()?;
        h.canopy.set_root_size(Expanse::new(246, 63))?;
        h.render()?;
        h.canopy.set_root_size(Expanse::new(123, 31))?;
        h.render()?;

        for node in h.canopy.core.nodes.values() {
            let min_width = node.layout.get_min_width();
            let min_height = node.layout.get_min_height();
            if matches!(min_width, Dimension::Points(width) if width >= 1.0) {
                assert!(
                    node.vp.view().w >= 1,
                    "node {:?} width unexpectedly below min size",
                    node.name
                );
            }
            if matches!(min_height, Dimension::Points(height) if height >= 1.0) {
                assert!(
                    node.vp.view().h >= 1,
                    "node {:?} height unexpectedly below min size",
                    node.name
                );
            }
        }

        Ok(())
    }
}
