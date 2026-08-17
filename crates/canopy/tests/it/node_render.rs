//! Node render integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, NodeId, ViewContext, Widget, buf, derive_commands,
        error::Result,
        geom::Size,
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
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            c.add_commands::<NodeA>()?;
            c.add_commands::<NodeB>()?;
            Ok(())
        }
    }

    fn style_flex_child(core: &mut dyn Context, id: NodeId) -> Result<()> {
        core.with_layout_of(id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })
    }

    fn build_split_tree(core: &mut dyn Context, depth: usize, horizontal: bool) -> Result<NodeId> {
        let node: NodeId = core.create_detached(NodeA::new())?.into();
        core.with_layout_of(node, &mut |layout| {
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
        core.set_children_of(node, vec![left, right])?;
        style_flex_child(core, left)?;
        style_flex_child(core, right)?;
        Ok(node)
    }

    #[test]
    fn test_simple_node_fill() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(30, 10).build()?;

        h.canopy.with_root_context(|context| {
            let node_a: NodeId = context.create_detached(NodeA::new())?.into();
            let node_b: NodeId = context.create_detached(NodeB::new())?.into();
            context.set_children_of(h.root, vec![node_a])?;
            context.set_children_of(node_a, vec![node_b])?;
            context.set_layout_of(h.root, Layout::fill())?;
            context.set_layout_of(node_a, Layout::column().fixed_width(10).fixed_height(5))?;
            context.set_layout_of(node_b, Layout::fill())
        })?;

        h.canopy.set_root_size(Size::new(30, 10))?;
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

        let bottom = h.canopy.with_root_context(|context| {
            let container: NodeId = context.create_detached(NodeA::new())?.into();
            let top: NodeId = context.create_detached(NodeB::new())?.into();
            let bottom: NodeId = context.create_detached(NodeA::new())?.into();
            context.set_children_of(h.root, vec![container])?;
            context.set_children_of(container, vec![top, bottom])?;
            context.set_layout_of(h.root, Layout::fill())?;
            context.set_layout_of(container, Layout::fill())?;
            context.set_layout_of(top, Layout::column().fixed_width(10).fixed_height(10))?;
            context.set_layout_of(bottom, Layout::column().fixed_width(10).fixed_height(0))?;
            Ok(bottom)
        })?;

        h.canopy.set_root_size(Size::new(10, 10))?;
        h.render()?;

        let bottom_view = h
            .canopy
            .with_root_view(|context| context.node_view(bottom))
            .expect("node missing");
        assert!(bottom_view.outer.is_zero());
        assert_eq!(bottom_view.outer.tl.y, 10);

        Ok(())
    }

    #[test]
    fn test_resize_deep_tree_does_not_error() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(123, 31).build()?;

        h.canopy.with_root_context(|context| {
            let tree = build_split_tree(context, 5, true)?;
            context.set_children_of(h.root, vec![tree])?;
            context.set_layout_of(h.root, Layout::fill())?;
            style_flex_child(context, tree)
        })?;

        h.render()?;
        h.canopy.set_root_size(Size::new(246, 63))?;
        h.render()?;
        h.canopy.set_root_size(Size::new(123, 31))?;
        h.render()?;

        h.canopy.with_root_view(|context| {
            let mut stack = vec![h.root];
            while let Some(node_id) = stack.pop() {
                for child in context.children_of(node_id).into_iter().rev() {
                    stack.push(child);
                }
                let layout = context.node_layout(node_id).expect("node layout");
                let view = context.node_view(node_id).expect("node view");
                let path = context.node_path(h.root, node_id);
                if let Some(min_width) = layout.min_width
                    && min_width >= 1
                {
                    assert!(
                        view.outer.w >= 1,
                        "node {path} width unexpectedly below min size"
                    );
                }
                if let Some(min_height) = layout.min_height
                    && min_height >= 1
                {
                    assert!(
                        view.outer.h >= 1,
                        "node {path} height unexpectedly below min size"
                    );
                }
            }
        });

        Ok(())
    }
}
