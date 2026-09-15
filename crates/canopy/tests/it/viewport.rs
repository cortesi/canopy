//! Viewport scrolling integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, ContextExt, EventOutcome, Loader, ModalBindings, ModalOptions, NodeId,
        NodeName, Render, RoutePhase, ViewContext, Widget, derive_commands,
        error::Result,
        event::{Event, key, mouse},
        geom::{Line, Point, PointI32, Size},
        layout::{CanvasContext, Layout},
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

    /// A focusable viewport with a fixed canvas that can claim wheel input.
    struct Pane {
        /// Canvas size in content cells.
        canvas: Size,
        /// Whether the widget handles wheel events itself.
        consume_wheel: bool,
    }

    impl Widget for Pane {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn canvas(&self, _view: Size, _ctx: &CanvasContext) -> Size {
            self.canvas
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            let wheel =
                matches!(event, Event::Mouse(mouse) if mouse.action.scroll_delta().is_some());
            Ok(if wheel && self.consume_wheel {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            })
        }

        fn name(&self) -> NodeName {
            NodeName::convert("inner")
        }
    }

    /// A 40 by 40 cell viewport holding a 10 by 4 cell inner pane at its
    /// origin.
    struct Outer;

    impl Widget for Outer {
        fn canvas(&self, _view: Size, _ctx: &CanvasContext) -> Size {
            Size::new(40, 40)
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let inner = c.add_child(Pane {
                canvas: Size::new(30, 30),
                consume_wheel: false,
            })?;
            c.set_layout_of(inner, Layout::column().fixed_width(10).fixed_height(4))
        }

        fn name(&self) -> NodeName {
            NodeName::convert("outer")
        }
    }

    impl Loader for Outer {}

    /// Build a 20 by 10 cell harness around [`Outer`] and return the inner
    /// pane.
    fn nested() -> Result<(Harness, NodeId)> {
        let harness = Harness::builder(Outer).size(20, 10).build()?;
        let inner = harness
            .canopy
            .with_root_view(|context| context.children_of(context.root_id())[0]);
        Ok((harness, inner))
    }

    /// Send one wheel event at a screen location.
    fn wheel(harness: &mut Harness, action: mouse::Action, x: i32, y: i32) -> Result<()> {
        harness.mouse(mouse::MouseEvent {
            action,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: PointI32 { x, y },
        })
    }

    /// Return a node's scroll offset.
    fn scroll(harness: &Harness, node: NodeId) -> Point {
        harness
            .canopy
            .with_root_view(|context| context.view_of(node).expect("live node").scroll)
    }

    /// Return the phases of the last input route.
    fn phases(harness: &Harness) -> Vec<RoutePhase> {
        harness
            .canopy
            .route_trace()
            .iter()
            .map(|entry| entry.phase)
            .collect()
    }

    #[test]
    fn the_wheel_scrolls_the_viewport_under_the_pointer_on_both_axes() -> Result<()> {
        let (mut harness, inner) = nested()?;
        let outer = harness.root;
        for (action, expected) in [
            (mouse::Action::ScrollDown, Point { x: 0, y: 3 }),
            (mouse::Action::ScrollRight, Point { x: 3, y: 3 }),
            (mouse::Action::ScrollUp, Point { x: 3, y: 0 }),
            (mouse::Action::ScrollLeft, Point { x: 0, y: 0 }),
        ] {
            wheel(&mut harness, action, 1, 1)?;
            assert_eq!(scroll(&harness, inner), expected, "{action:?}");
            assert!(phases(&harness).contains(&RoutePhase::DefaultAction));
        }
        assert_eq!(scroll(&harness, outer), Point::ZERO);
        Ok(())
    }

    #[test]
    fn a_step_that_cannot_move_bubbles_to_an_ancestor_that_can() -> Result<()> {
        let (mut harness, inner) = nested()?;
        let outer = harness.root;

        wheel(&mut harness, mouse::Action::ScrollUp, 1, 1)?;
        assert!(!phases(&harness).contains(&RoutePhase::DefaultAction));
        assert!(phases(&harness).contains(&RoutePhase::Unhandled));

        harness
            .canopy
            .with_root_context(|context| context.scroll_to_of(inner, 0, u32::MAX))?;
        harness.render()?;
        assert_eq!(scroll(&harness, inner), Point { x: 0, y: 26 });
        wheel(&mut harness, mouse::Action::ScrollDown, 1, 1)?;
        assert_eq!(scroll(&harness, inner), Point { x: 0, y: 26 });
        assert_eq!(scroll(&harness, outer), Point { x: 0, y: 3 });
        Ok(())
    }

    #[test]
    fn widgets_and_bindings_take_the_wheel_before_the_default_action() -> Result<()> {
        let (mut harness, inner) = nested()?;
        let outer = harness.root;

        harness.with_widget(inner, |pane: &mut Pane| pane.consume_wheel = true);
        wheel(&mut harness, mouse::Action::ScrollDown, 1, 1)?;
        assert_eq!(scroll(&harness, inner), Point::ZERO);
        assert!(!phases(&harness).contains(&RoutePhase::DefaultAction));
        harness.with_widget(inner, |pane: &mut Pane| pane.consume_wheel = false);

        harness.script(
            r#"
canopy.bind_mouse("ScrollDown", { path = "inner/", description = "Inner" }, function()
    canopy.set_mode("inner")
end)
canopy.bind_mouse("ScrollUp", { path = "outer/", description = "Outer" }, function()
    canopy.set_mode("outer")
end)
"#,
        )?;
        wheel(&mut harness, mouse::Action::ScrollDown, 1, 1)?;
        assert_eq!(harness.canopy.input_mode(), "inner");
        assert_eq!(scroll(&harness, inner), Point::ZERO);

        // The outer binding waits until the inner pane declines.
        harness.canopy.set_input_mode("");
        harness
            .canopy
            .with_root_context(|context| context.scroll_to_of(inner, 0, 9))?;
        wheel(&mut harness, mouse::Action::ScrollUp, 1, 1)?;
        assert_eq!(harness.canopy.input_mode(), "");
        assert_eq!(scroll(&harness, inner), Point { x: 0, y: 6 });
        harness
            .canopy
            .with_root_context(|context| context.scroll_to_of(inner, 0, 0))?;
        wheel(&mut harness, mouse::Action::ScrollUp, 1, 1)?;
        assert_eq!(harness.canopy.input_mode(), "outer");
        assert_eq!(scroll(&harness, outer), Point::ZERO);
        Ok(())
    }

    #[test]
    fn a_modal_scope_confines_default_scrolling_to_its_region() -> Result<()> {
        let (mut harness, inner) = nested()?;
        let outer = harness.root;
        harness.canopy.with_root_context(|context| {
            context.scroll_to_of(inner, 0, u32::MAX)?;
            context.open_modal(ModalOptions {
                owner: outer,
                modal: inner,
                initial_focus: inner,
                dim_target: None,
                bindings: ModalBindings::Application,
            })
        })?;
        harness.render()?;

        wheel(&mut harness, mouse::Action::ScrollDown, 1, 1)?;
        wheel(&mut harness, mouse::Action::ScrollDown, 15, 8)?;
        assert_eq!(
            scroll(&harness, outer),
            Point::ZERO,
            "input outside the region, or declined inside it, stays confined"
        );
        wheel(&mut harness, mouse::Action::ScrollUp, 1, 1)?;
        assert_eq!(scroll(&harness, inner), Point { x: 0, y: 23 });
        Ok(())
    }
}
