//! Layout integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Loader, NodeId, ViewContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        geom::Size,
        layout::{Edges, Layout, MeasureConstraints, Measurement},
        render::Render,
        state::NodeName,
        testing::harness::Harness,
    };

    struct Container;

    impl Container {
        fn new() -> Self {
            Self
        }
    }

    impl CommandNode for Container {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for Container {
        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view().outer_rect_local(), ' ')
        }

        fn name(&self) -> NodeName {
            NodeName::convert("container")
        }
    }

    struct Huge;

    impl Huge {
        fn new() -> Self {
            Self
        }
    }

    impl CommandNode for Huge {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for Huge {
        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view().outer_rect_local(), 'x')
        }

        fn measure(&self, c: MeasureConstraints) -> Measurement {
            c.clamp(Size::new(500, 500))
        }

        fn name(&self) -> NodeName {
            NodeName::convert("huge")
        }
    }

    struct Root;

    impl Root {
        fn new() -> Self {
            Self
        }
    }

    impl CommandNode for Root {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for Root {
        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view().outer_rect_local(), ' ')
        }

        fn name(&self) -> NodeName {
            NodeName::convert("root")
        }
    }

    impl Loader for Root {
        fn load(_c: &mut Canopy) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn child_respects_parent_padding() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(20, 20).build()?;
        let (container, child) = h.canopy.with_root_context(|context| {
            let container: NodeId = context.create_detached(Container::new())?.into();
            let child: NodeId = context.create_detached(Huge::new())?.into();
            context.set_children_of(h.root, vec![container])?;
            context.set_children_of(container, vec![child])?;
            context.set_layout_of(h.root, Layout::fill())?;
            context.set_layout_of(container, Layout::fill().padding(Edges::all(1)))?;
            context.set_layout_of(child, Layout::fill())?;
            Ok((container, child))
        })?;

        h.canopy.set_root_size(Size::new(20, 20))?;
        h.render()?;

        let (container_view, child_view) = h.canopy.with_root_view(|context| {
            (
                context.node_view(container).expect("missing container"),
                context.node_view(child).expect("missing child"),
            )
        });
        assert_eq!(child_view.outer.tl.x, container_view.content.tl.x);
        assert_eq!(child_view.outer.tl.y, container_view.content.tl.y);
        assert_eq!(child_view.outer.w + 2, container_view.outer.w);
        assert_eq!(child_view.outer.h + 2, container_view.outer.h);

        Ok(())
    }
}
