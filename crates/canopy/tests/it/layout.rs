//! Layout integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        ContextExt, NodeId, NodeName, Render, ViewContext, Widget,
        error::Result,
        geom::Size,
        layout::{Edges, Layout, MeasureConstraints, Measurement},
        testing::harness::Harness,
    };

    struct Container;

    impl Container {
        fn new() -> Self {
            Self
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

    impl Widget for Root {
        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view().outer_rect_local(), ' ')
        }

        fn name(&self) -> NodeName {
            NodeName::convert("root")
        }
    }

    #[test]
    fn child_respects_parent_padding() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(20, 20).build()?;
        let (container, child) = h.canopy.with_root_context(|context| {
            let container: NodeId = context.create_detached(Container::new())?.into();
            let child: NodeId = context.create_detached(Huge::new())?.into();
            context.set_children(h.root, vec![container])?;
            context.set_children(container, vec![child])?;
            context.set_layout_override(h.root, Layout::fill().into())?;
            context.set_layout_override(container, Layout::fill().padding(Edges::all(1)).into())?;
            context.set_layout_override(child, Layout::fill().into())?;
            Ok((container, child))
        })?;

        h.render()?;

        let (container_view, child_view) = h.canopy.with_root_view(|context| {
            (
                context.view_of(container).expect("missing container"),
                context.view_of(child).expect("missing child"),
            )
        });
        assert_eq!(child_view.outer.tl.x, container_view.content.tl.x);
        assert_eq!(child_view.outer.tl.y, container_view.content.tl.y);
        assert_eq!(child_view.outer.w + 2, container_view.outer.w);
        assert_eq!(child_view.outer.h + 2, container_view.outer.h);

        Ok(())
    }
}
