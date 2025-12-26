//! Integration tests for layout behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::Result,
        geom::{Expanse, Rect},
        layout::{AvailableSpace, Dimension, Edges, Length, Size},
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
        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view(), ' ')
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
        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view(), 'x')
        }

        fn view_size(
            &self,
            _known_dimensions: Size<Option<f32>>,
            _available_space: Size<AvailableSpace>,
        ) -> Size<f32> {
            Size {
                width: 500.0,
                height: 500.0,
            }
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
        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.view(), ' ')
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
            layout.flex_col();
        })?;

        h.canopy.core.with_layout_of(container, |layout| {
            layout
                .flex_col()
                .flex_item(1.0, 1.0, Dimension::Auto)
                .padding(Edges::all(Length::Points(1.0)));
        })?;

        h.canopy.core.with_layout_of(child, |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })?;

        h.canopy.set_root_size(Expanse::new(20, 20))?;
        h.render()?;

        let container_view = h.canopy.core.nodes[container].vp.view();
        let child_vp = h.canopy.core.nodes[child].vp;
        assert_eq!(child_vp.position().x, container_view.tl.x + 1);
        assert_eq!(child_vp.position().y, container_view.tl.y + 1);
        assert_eq!(child_vp.view().w + 2, container_view.w);
        assert_eq!(child_vp.view().h + 2, container_view.h);

        Ok(())
    }
}
