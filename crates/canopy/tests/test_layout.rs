//! Integration tests for layout behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::Result,
        geom::Expanse,
        layout::{Edges, Layout, MeasureConstraints, Measurement, Size},
        render::Render,
        state::NodeName,
        testing::harness::Harness,
        widget::Widget,
    };

    struct Container;

    impl Container {
        fn new() -> Self {
            Self
        }
    }

    impl CommandNode for Container {
        fn commands() -> Vec<CommandSpec> {
            vec![]
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
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
        fn commands() -> Vec<CommandSpec> {
            vec![]
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
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
        fn commands() -> Vec<CommandSpec> {
            vec![]
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
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
        fn load(_c: &mut Canopy) {}
    }

    #[test]
    fn child_respects_parent_padding() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(20, 20).build()?;
        let container = h.canopy.core.add(Container::new());
        let child = h.canopy.core.add(Huge::new());
        h.canopy.core.set_children(h.root, vec![container])?;
        h.canopy.core.set_children(container, vec![child])?;

        h.canopy.core.with_layout_of(h.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;

        h.canopy.core.with_layout_of(container, |layout| {
            *layout = Layout::column()
                .flex_horizontal(1)
                .flex_vertical(1)
                .padding(Edges::all(1));
        })?;

        h.canopy.core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;

        h.canopy.set_root_size(Expanse::new(20, 20))?;
        h.render()?;

        let container_view = h.canopy.core.nodes[container].view;
        let child_view = h.canopy.core.nodes[child].view;
        assert_eq!(child_view.outer.tl.x, container_view.content.tl.x);
        assert_eq!(child_view.outer.tl.y, container_view.content.tl.y);
        assert_eq!(child_view.outer.w + 2, container_view.outer.w);
        assert_eq!(child_view.outer.h + 2, container_view.outer.h);

        Ok(())
    }
}
