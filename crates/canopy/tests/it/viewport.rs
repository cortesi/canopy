//! Viewport scrolling integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, NodeName, Render, ViewContext, Widget, derive_commands,
        error::Result,
        event::key,
        geom::{Line, Size},
        layout::CanvasContext,
        testing::harness::Harness,
    };

    /// Simple test widget to demonstrate view scrolling behavior.
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

        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            let view = ctx.view();
            let origin = view.content_origin();
            let view_height = view.content.h;
            let view_width = view.content.w;

            let line1 = format!("Scroll position: ({}, {})", view.scroll.x, view.scroll.y);
            r.text("text", Line::new(origin.x, origin.y, view_width), &line1)?;

            for y in 1..view_height.min(5) {
                let content = format!("Line {}", view.scroll.y + y);
                r.text(
                    "text",
                    Line::new(origin.x, origin.y + y, view_width),
                    &content,
                )?;
            }

            Ok(())
        }

        /// Canvas is larger than view to enable scrolling.
        fn canvas(&self, _view: Size, _ctx: &CanvasContext) -> Size {
            Size::new(100, 100)
        }

        fn name(&self) -> NodeName {
            NodeName::convert("scroll_test")
        }
    }

    impl Loader for ScrollTest {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            Ok(())
        }
    }

    #[test]
    fn test_scroll_behavior() -> Result<()> {
        let mut harness = Harness::builder(ScrollTest::new()).size(30, 10).build()?;
        harness.canopy.eval_script(
            r#"
canopy.bind("Down", { description = "Scroll down" }, function()
    scroll_test.scroll_down()
end)
"#,
        )?;

        harness.render()?;
        assert!(harness.tbuf().contains_text("Scroll position: (0, 0)"));
        assert!(harness.tbuf().contains_text("Line 1"));

        harness.key(key::KeyCode::Down)?;

        assert!(harness.tbuf().contains_text("Scroll position: (0, 1)"));
        assert!(harness.tbuf().contains_text("Line 2"));

        Ok(())
    }
}
