//! Integration tests for layout behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::Result,
        event::Event,
        geom::{Expanse, Point, Rect},
        render::Render,
        state::NodeName,
        testing::harness::Harness,
        widget::{EventOutcome, Widget},
    };
    use taffy::{
        geometry::{Rect as TaffyRect, Size},
        style::{AvailableSpace, Dimension, LengthPercentageAuto, Position},
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

        fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
            EventOutcome::Ignore
        }

        fn name(&self) -> NodeName {
            NodeName::convert("big")
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

        fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
            EventOutcome::Ignore
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
            style.inset = TaffyRect {
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
}
