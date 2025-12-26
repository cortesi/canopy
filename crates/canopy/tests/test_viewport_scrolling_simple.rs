//! Integration tests for viewport scrolling.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext, command, derive_commands, error::Result, event::key,
        geom::Rect, layout::Size, render::Render, state::NodeName, testing::harness::Harness,
        widget::Widget,
    };

    /// Simple test widget to demonstrate viewport scrolling behavior.
    struct ScrollTest;

    #[derive_commands]
    impl ScrollTest {
        fn new() -> Self {
            Self
        }

        #[command]
        fn scroll_down(&self, c: &mut dyn Context) {
            let _ = c.scroll_down();
        }
    }

    impl Widget for ScrollTest {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            let view = ctx.view();

            let line1 = format!("Scroll position: ({}, {})", view.tl.x, view.tl.y);
            r.text("text", view.line(0), &line1)?;

            for y in 1..view.h.min(5) {
                let content = format!("Line {}", view.tl.y + y);
                r.text("text", view.line(y), &content)?;
            }

            Ok(())
        }

        /// Canvas is larger than view to enable scrolling.
        fn canvas_size(&self, _view: Size<f32>) -> Size<f32> {
            Size {
                width: 100.0,
                height: 100.0,
            }
        }

        fn name(&self) -> NodeName {
            NodeName::convert("scroll_test")
        }
    }

    impl Loader for ScrollTest {
        fn load(c: &mut Canopy) {
            c.add_commands::<Self>();
        }
    }

    #[test]
    fn test_scroll_behavior() -> Result<()> {
        let mut harness = Harness::builder(ScrollTest::new()).size(30, 10).build()?;
        harness
            .canopy
            .bind_key(key::KeyCode::Down, "", "scroll_test::scroll_down()")?;

        harness.render()?;
        assert!(harness.tbuf().contains_text("Scroll position: (0, 0)"));
        assert!(harness.tbuf().contains_text("Line 1"));

        harness.key(key::KeyCode::Down)?;

        assert!(harness.tbuf().contains_text("Scroll position: (0, 1)"));
        assert!(harness.tbuf().contains_text("Line 2"));

        Ok(())
    }
}
