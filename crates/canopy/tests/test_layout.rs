//! Integration tests for layout behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Loader, ReadContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        geom::Expanse,
        layout::{Edges, Layout, MeasureConstraints, Measurement, Size},
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
        fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
        fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
        fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
        let container = h.canopy.core.create_detached(Container::new());
        let child = h.canopy.core.create_detached(Huge::new());
        h.canopy.core.set_children(h.root, vec![container])?;
        h.canopy.core.set_children(container, vec![child])?;

        h.canopy.core.set_layout_of(h.root, Layout::fill())?;

        h.canopy
            .core
            .set_layout_of(container, Layout::fill().padding(Edges::all(1)))?;

        h.canopy.core.set_layout_of(child, Layout::fill())?;

        h.canopy.set_root_size(Expanse::new(20, 20))?;
        h.render()?;

        let core = &h.canopy.core;
        let container_view = core.node(container).expect("missing container").view();
        let child_view = core.node(child).expect("missing child").view();
        assert_eq!(child_view.outer.tl.x, container_view.content.tl.x);
        assert_eq!(child_view.outer.tl.y, container_view.content.tl.y);
        assert_eq!(child_view.outer.w + 2, container_view.outer.w);
        assert_eq!(child_view.outer.h + 2, container_view.outer.h);

        Ok(())
    }
}
