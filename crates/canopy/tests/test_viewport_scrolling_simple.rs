//! Integration tests for view scrolling.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ReadContext, Widget, command, derive_commands,
        error::Result,
        event::key,
        geom::Line,
        layout::{CanvasContext, Size},
        render::Render,
        state::NodeName,
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
        fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
            true
        }

        fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
            let view = ctx.view();
            let origin = view.content_origin();
            let view_height = view.content.h;
            let view_width = view.content.w;

            let line1 = format!("Scroll position: ({}, {})", view.tl.x, view.tl.y);
            r.text("text", Line::new(origin.x, origin.y, view_width), &line1)?;

            for y in 1..view_height.min(5) {
                let content = format!("Line {}", view.tl.y + y);
                r.text(
                    "text",
                    Line::new(origin.x, origin.y + y, view_width),
                    &content,
                )?;
            }

            Ok(())
        }

        /// Canvas is larger than view to enable scrolling.
        fn canvas(&self, _view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
            Size::new(100, 100)
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
