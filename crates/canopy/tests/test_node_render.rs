//! Integration tests for node rendering.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Core, Loader, NodeId, ViewContext, Widget, buf, derive_commands,
        error::Result,
        geom::Expanse,
        layout::{Layout, Sizing},
        render::Render,
        state::NodeName,
        testing::harness::Harness,
    };

    struct NodeB;

    #[derive_commands]
    impl NodeB {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for NodeB {
        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view().outer_rect_local(), 'B')?;
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
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })
    }

    fn build_split_tree(core: &mut Core, depth: usize, horizontal: bool) -> Result<NodeId> {
        let node = core.create_detached(NodeA::new());
        core.with_layout_of(node, |layout| {
            let base = if horizontal {
                Layout::row()
            } else {
                Layout::column()
            };
            *layout = base.min_width(1).min_height(1);
        })?;
        if depth == 0 {
            return Ok(node);
        }

        let left = build_split_tree(core, depth - 1, !horizontal)?;
        let right = build_split_tree(core, depth - 1, !horizontal)?;
        core.set_children(node, vec![left, right])?;
        style_flex_child(core, left)?;
        style_flex_child(core, right)?;
        Ok(node)
    }

    #[test]
    fn test_simple_node_fill() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(30, 10).build()?;

        let node_a = h.canopy.core.create_detached(NodeA::new());
        let node_b = h.canopy.core.create_detached(NodeB::new());
        h.canopy.core.set_children(h.root, vec![node_a])?;
        h.canopy.core.set_children(node_a, vec![node_b])?;

        h.canopy.core.with_layout_of(h.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;

        h.canopy.core.with_layout_of(node_a, |layout| {
            *layout = Layout::column().fixed_width(10).fixed_height(5);
        })?;

        h.canopy.core.with_layout_of(node_b, |layout| {
            *layout = Layout::fill();
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

        let container = h.canopy.core.create_detached(NodeA::new());
        let top = h.canopy.core.create_detached(NodeB::new());
        let bottom = h.canopy.core.create_detached(NodeA::new());

        h.canopy.core.set_children(h.root, vec![container])?;
        h.canopy.core.set_children(container, vec![top, bottom])?;

        h.canopy.core.with_layout_of(h.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;

        h.canopy.core.with_layout_of(container, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;

        h.canopy.core.with_layout_of(top, |layout| {
            *layout = Layout::column().fixed_width(10).fixed_height(10);
        })?;

        h.canopy.core.with_layout_of(bottom, |layout| {
            *layout = Layout::column().fixed_width(10).fixed_height(0);
        })?;

        h.canopy.set_root_size(Expanse::new(10, 10))?;
        h.render()?;

        let bottom_view = h.canopy.core.node(bottom).expect("node missing").view();
        assert!(bottom_view.outer.is_zero());
        assert_eq!(
            h.canopy
                .core
                .node(bottom)
                .expect("node missing")
                .rect()
                .tl
                .y,
            10
        );

        Ok(())
    }

    #[test]
    fn test_resize_deep_tree_does_not_error() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(123, 31).build()?;

        let tree = build_split_tree(&mut h.canopy.core, 5, true)?;
        h.canopy.core.set_children(h.root, vec![tree])?;
        h.canopy.core.with_layout_of(h.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        style_flex_child(&mut h.canopy.core, tree)?;

        h.render()?;
        h.canopy.set_root_size(Expanse::new(246, 63))?;
        h.render()?;
        h.canopy.set_root_size(Expanse::new(123, 31))?;
        h.render()?;

        let mut stack = vec![h.root];
        while let Some(node_id) = stack.pop() {
            let node = h.canopy.core.node(node_id).expect("node missing");
            for child in node.children().iter().rev() {
                stack.push(*child);
            }
            let layout = node.layout();
            let view = node.view();
            if let Some(min_width) = layout.min_width
                && min_width >= 1
            {
                assert!(
                    view.outer.w >= 1,
                    "node {:?} width unexpectedly below min size",
                    node.name()
                );
            }
            if let Some(min_height) = layout.min_height
                && min_height >= 1
            {
                assert!(
                    view.outer.h >= 1,
                    "node {:?} height unexpectedly below min size",
                    node.name()
                );
            }
        }

        Ok(())
    }
}
