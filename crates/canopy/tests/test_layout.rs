//! Integration tests for layout behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::Result,
        geom::{Expanse, Point, Rect},
        layout,
        layout::{
            AvailableSpace, Dimension, Display, FlexDirection, LengthPercentage,
            LengthPercentageAuto, Position, Size,
        },
        render::Render,
        state::NodeName,
        testing::harness::Harness,
        widget::Widget,
    };

    struct Big;

    impl Big {
        fn new() -> Self {
            Self
        }
    }

    impl CommandNode for Big {
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

    impl Widget for Big {
        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            r.fill("", ctx.canvas().rect(), 'x')
        }

        fn measure(
            &self,
            known_dimensions: Size<Option<f32>>,
            available_space: Size<AvailableSpace>,
        ) -> Size<f32> {
            let width = known_dimensions
                .width
                .or_else(|| available_space.width.into_option())
                .unwrap_or(0.0);
            let height = known_dimensions
                .height
                .or_else(|| available_space.height.into_option())
                .unwrap_or(0.0);
            Size {
                width: width * 2.0,
                height: height * 2.0,
            }
        }

        fn name(&self) -> NodeName {
            NodeName::convert("big")
        }
    }

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

        fn measure(
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
    fn child_clamped_to_parent() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(4, 4).build()?;
        let child = h.canopy.core.add(Big::new());
        h.canopy.core.set_children(h.root, vec![child])?;

        h.canopy.core.build(child).style(|style| {
            style.size.width = Dimension::Points(4.0);
            style.size.height = Dimension::Points(4.0);
            style.position = Position::Absolute;
            style.inset = layout::Rect {
                left: LengthPercentageAuto::Points(3.0),
                right: LengthPercentageAuto::Auto,
                top: LengthPercentageAuto::Points(3.0),
                bottom: LengthPercentageAuto::Auto,
            };
        });

        h.canopy.set_root_size(Expanse::new(4, 4))?;
        h.render()?;

        let buf = h.buf();
        for y in 0..4 {
            for x in 0..4 {
                let cell = buf.get(Point { x, y }).unwrap();
                if x == 3 && y == 3 {
                    assert_eq!(cell.ch, 'x', "Expected 'x' at ({x}, {y})");
                } else {
                    assert_eq!(cell.ch, ' ', "Expected ' ' at ({x}, {y})");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn child_respects_parent_padding() -> Result<()> {
        let mut h = Harness::builder(Root::new()).size(20, 20).build()?;
        let container = h.canopy.core.add(Container::new());
        let child = h.canopy.core.add(Huge::new());
        h.canopy.core.set_children(h.root, vec![container])?;
        h.canopy.core.set_children(container, vec![child])?;

        h.canopy.core.build(h.root).style(|style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        });

        h.canopy.core.build(container).style(|style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
            style.padding = layout::Rect {
                left: LengthPercentage::Points(1.0),
                right: LengthPercentage::Points(1.0),
                top: LengthPercentage::Points(1.0),
                bottom: LengthPercentage::Points(1.0),
            };
        });

        h.canopy.core.build(child).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });

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
