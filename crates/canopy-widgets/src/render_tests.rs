//! Whole-widget render checks over a minimal root.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, ContextExt, Loader, NodeName, ViewContext, ViewContextExt, Widget, buf,
        commands::{CommandNode, CommandSpec},
        error::Result,
        event::{key, mouse},
        geom::{Point, PointI32, Size},
        layout::{Edges, Layout},
        testing::harness::Harness,
    };

    use crate::{BoxGlyphs, Button, Dropdown, Frame, List, Selector, Text};

    fn click_at(location: Point) -> mouse::MouseEvent {
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32::try_from(location).expect("test points fit"),
        }
    }

    #[test]
    fn dropdown_scrolls_labels_and_selects_the_visible_row() -> Result<()> {
        let items = [
            "00-Alpha-long",
            "01-Bravo-long",
            "02-Charlie-long",
            "03-Delta-long",
        ]
        .map(String::from)
        .to_vec();
        let root = SnapshotRoot::new(Dropdown::new(items)?);
        let mut harness = Harness::builder(root).size(10, 4).build()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| dropdown.toggle(ctx))
        })?;
        harness.render()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|_, ctx| {
                ctx.set_layout(Layout::fill().padding(Edges::all(1)))
            })
        })?;
        harness.render()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|_, ctx| {
                assert!(ctx.scroll_to(3, 2).changed());
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Charlie-"));
        assert_eq!(harness.buf().get(Point { x: 1, y: 1 }).unwrap().ch, 'C');
        harness.mouse(click_at(Point { x: 1, y: 1 }))?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, _| {
                assert_eq!(dropdown.selected_index(), 2);
                Ok(())
            })
        })?;
        assert!(harness.tbuf().contains_text("Charlie-"));
        Ok(())
    }

    #[test]
    fn selector_scrolls_labels_and_toggles_the_visible_row() -> Result<()> {
        let items = ["Alpha-long", "Bravo-long", "Charlie-long", "Delta-long"]
            .map(String::from)
            .to_vec();
        let root = SnapshotRoot::new(Selector::new(items));
        let mut harness = Harness::builder(root).size(10, 4).build()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|_, ctx| {
                ctx.set_layout(Layout::fill().padding(Edges::all(1)))
            })
        })?;
        harness.render()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|_, ctx| {
                assert!(ctx.scroll_to(4, 2).changed());
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Charlie-"));
        assert_eq!(harness.buf().get(Point { x: 1, y: 1 }).unwrap().ch, 'C');
        harness.mouse(click_at(Point { x: 1, y: 1 }))?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, _| {
                assert_eq!(selector.selected_items(), [&"Charlie-long".to_string()]);
                Ok(())
            })
        })?;
        Ok(())
    }

    #[test]
    fn selector_navigation_reveals_the_active_row() -> Result<()> {
        let items = ["First", "Second", "Third", "Fourth", "Fifth"]
            .map(String::from)
            .to_vec();
        let mut harness = Harness::builder(SnapshotRoot::new(Selector::new(items)))
            .size(12, 2)
            .build()?;
        harness.render()?;
        harness.with_root_context(|_: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, ctx| {
                selector.select_last(ctx)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("[ ] Fifth"));
        harness.with_root_context(|_: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, ctx| {
                selector.toggle(ctx)?;
                selector.select_by(ctx, -3)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("[ ] Second"));
        harness.with_root_context(|_: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, ctx| {
                assert_eq!(selector.selected_items(), [&"Fifth".to_string()]);
                selector.select_first(ctx)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("[ ] First"));
        Ok(())
    }

    #[test]
    fn dropdown_navigation_reveals_the_active_row_after_expanding() -> Result<()> {
        let items = ["First", "Second", "Third", "Fourth", "Fifth"]
            .map(String::from)
            .to_vec();
        let mut harness = Harness::builder(SnapshotRoot::new(Dropdown::new(items)?))
            .size(12, 2)
            .build()?;
        harness.render()?;
        // Expansion and navigation can happen in the same command turn,
        // before the expanded canvas has been measured.
        harness.with_root_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| {
                dropdown.toggle(ctx)?;
                dropdown.select_by(ctx, 4)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Fifth"));
        harness.with_root_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| dropdown.confirm(ctx))
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Fifth ▼"));
        harness.with_root_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| dropdown.toggle(ctx))
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Fifth"));
        harness.with_root_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| {
                dropdown.select_by(ctx, -4)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("First"));
        Ok(())
    }

    const ASCII_BOX: BoxGlyphs = BoxGlyphs {
        topleft: '+',
        topright: '+',
        bottomleft: '+',
        bottomright: '+',
        horizontal: '-',
        vertical: '|',
    };

    struct SnapshotRoot<W> {
        child: Option<W>,
    }

    impl<W> SnapshotRoot<W> {
        fn new(child: W) -> Self {
            Self { child: Some(child) }
        }
    }

    impl<W> CommandNode for SnapshotRoot<W> {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl<W: Widget + 'static> Widget for SnapshotRoot<W> {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let child = self.child.take().expect("snapshot child already mounted");
            let _ = ctx.add_child(child)?;
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("snapshot_root")
        }
    }

    impl<W: Widget + 'static> Loader for SnapshotRoot<W> {
        fn load(_c: &mut Canopy) -> Result<()> {
            Ok(())
        }
    }

    fn mouse_at(action: mouse::Action, x: i32, y: i32) -> mouse::MouseEvent {
        mouse::MouseEvent {
            action,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x, y },
        }
    }

    #[test]
    fn frame_scrollbar_keeps_the_thumb_under_the_pointer() -> Result<()> {
        let text = (0..30)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut harness = Harness::builder(SnapshotRoot::new(Frame::new()))
            .size(12, 8)
            .build()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<Frame>, ctx| {
            ctx.with_unique_descendant::<Frame, _>(|_, ctx| {
                ctx.add_child(Text::new(text))?;
                Ok(())
            })
        })?;
        harness.render()?;
        let scroll_y = |harness: &Harness| {
            harness.canopy.with_root_view(|ctx| {
                let text = ctx
                    .unique_descendant::<Text>()
                    .expect("text lookup")
                    .expect("text node");
                ctx.view_of(text.into()).expect("text view").scroll.y
            })
        };

        // The right edge track spans rows 1 to 6. Thirty lines through a
        // six-row view give a two-row thumb at the top.
        assert_eq!(harness.buf().get(Point { x: 11, y: 1 }).unwrap().ch, '█');
        assert_eq!(harness.buf().get(Point { x: 11, y: 3 }).unwrap().ch, '│');

        // Dragging the thumb to the end of the track reaches the last line.
        harness.mouse(mouse_at(mouse::Action::Down, 11, 1))?;
        harness.mouse(mouse_at(mouse::Action::Drag, 11, 5))?;
        assert_eq!(scroll_y(&harness), 24);
        harness.mouse(mouse_at(mouse::Action::Up, 11, 5))?;

        // A press on the track centers the thumb on the pointer: the thumb
        // then covers rows 2 and 3.
        harness.mouse(mouse_at(mouse::Action::Down, 11, 3))?;
        harness.mouse(mouse_at(mouse::Action::Up, 11, 3))?;
        harness.render()?;
        assert_eq!(scroll_y(&harness), 5);
        for y in [2, 3] {
            assert_eq!(harness.buf().get(Point { x: 11, y }).unwrap().ch, '█');
        }
        Ok(())
    }

    #[test]
    fn inputs_show_focus_across_the_row_and_keep_the_prompt_out_of_the_value() -> Result<()> {
        let mut harness = Harness::builder(SnapshotRoot::new(
            crate::Input::new("hello").with_prompt(" 查: "),
        ))
        .size(20, 3)
        .build()?;
        let first = harness
            .canopy
            .with_root_view(|ctx| ctx.unique_descendant::<crate::Input>().unwrap().unwrap());
        let second = harness.with_root_context(|_: &mut SnapshotRoot<crate::Input>, ctx| {
            ctx.add_child(crate::Input::new(""))
        })?;
        harness.with_root_context(|_: &mut SnapshotRoot<crate::Input>, ctx| {
            ctx.set_focus(first.into()).map(|_| ())
        })?;
        harness.render()?;
        let point = |harness: &Harness, node, x| {
            let origin = harness
                .canopy
                .with_root_view(|ctx| ctx.view_of(node).unwrap().outer.tl);
            Point {
                x: u32::try_from(origin.x).unwrap() + x,
                y: u32::try_from(origin.y).unwrap(),
            }
        };
        let prompt = point(&harness, first.into(), 1);
        let tail = point(&harness, first.into(), 19);
        let active = harness.buf().get(tail).unwrap().style;
        assert!(harness.buf().get(prompt).unwrap().style.attrs.bold);
        assert!(
            harness.tbuf().contains_text("hello"),
            "{:?}",
            harness.tbuf().lines()
        );
        assert_eq!(harness.buf().get(prompt).unwrap().ch, '查');
        harness.with_root_context(|_: &mut SnapshotRoot<crate::Input>, ctx| {
            ctx.with_widget_mut(first, |input: &mut crate::Input, _| {
                assert_eq!(input.value(), "hello");
                assert_eq!(input.cursor().unwrap().location.x, 10);
                Ok(())
            })?;
            ctx.set_focus(second.into()).map(|_| ())
        })?;
        harness.render()?;
        assert_ne!(harness.buf().get(tail).unwrap().style.bg, active.bg);
        assert!(!harness.buf().get(prompt).unwrap().style.attrs.bold);
        assert_eq!(
            harness
                .buf()
                .get(point(&harness, second.into(), 19))
                .unwrap()
                .style
                .bg,
            active.bg,
            "an empty focused field paints the whole row too"
        );
        // A prompt can consume all available columns without breaking
        // rendering.
        harness.canopy.set_root_size(Size::new(2, 3))?;
        harness.render()?;
        Ok(())
    }

    #[test]
    fn an_input_measured_to_its_text_shows_all_of_it() -> Result<()> {
        let mut harness = Harness::builder(SnapshotRoot::new(crate::Input::new("hello")))
            .size(20, 3)
            .build()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<crate::Input>, ctx| {
            ctx.with_unique_descendant::<crate::Input, _>(|_, ctx| ctx.set_layout(Layout::column()))
        })?;
        harness.render()?;
        assert!(
            harness.tbuf().contains_text("hello"),
            "{:?}",
            harness.tbuf().lines()
        );
        Ok(())
    }

    #[test]
    fn tabs_show_the_active_page_and_switch_on_click() -> Result<()> {
        let mut harness = Harness::builder(SnapshotRoot::new(crate::Tabs::new()))
            .size(30, 4)
            .build()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<crate::Tabs>, ctx| {
            ctx.with_unique_descendant::<crate::Tabs, _>(|tabs, ctx| {
                tabs.add_tab(ctx, "One", Text::new("first page"))?;
                tabs.add_tab(ctx, "Two", Text::new("second page"))?;
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text(" One "));
        assert!(harness.tbuf().contains_text(" Two "));
        assert!(harness.tbuf().contains_text("first page"));
        assert!(!harness.tbuf().contains_text("second page"));

        // " One " covers columns 0-4, a gap follows, and " Two " starts at 6.
        harness.mouse(click_at(Point { x: 7, y: 0 }))?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("second page"));
        assert!(!harness.tbuf().contains_text("first page"));

        harness.with_root_context(|_root: &mut SnapshotRoot<crate::Tabs>, ctx| {
            ctx.with_unique_descendant::<crate::Tabs, _>(|tabs, ctx| {
                tabs.select_by(ctx, 1)?;
                assert_eq!(tabs.active(), 0, "moving past the last tab wraps");
                tabs.select(ctx, 9)?;
                assert_eq!(tabs.active(), 1, "an index past the end clamps");
                Ok(())
            })
        })?;
        Ok(())
    }

    #[test]
    fn titled_frames_render_at_tiny_sizes() -> Result<()> {
        for width in 0..=3 {
            for height in 0..=3 {
                let frame = Frame::new().with_title("Title");
                let mut harness = Harness::builder(SnapshotRoot::new(frame))
                    .size(width, height)
                    .build()?;
                harness.render()?;
            }
        }
        Ok(())
    }

    #[test]
    fn text_renders_its_content() -> Result<()> {
        let root = SnapshotRoot::new(Text::new("Hello"));
        let mut harness = Harness::builder(root).size(10, 3).build()?;
        harness.render()?;
        harness.tbuf().assert_matches(buf!["Hello" "" ""]);
        Ok(())
    }

    #[test]
    fn button_renders_a_centred_label_in_a_box() -> Result<()> {
        let root = SnapshotRoot::new(Button::new("OK").with_glyphs(ASCII_BOX));
        let mut harness = Harness::builder(root).size(10, 3).build()?;
        harness.render()?;
        harness
            .tbuf()
            .assert_matches(buf!["+--------+" "|   OK   |" "+--------+"]);
        Ok(())
    }

    #[test]
    fn empty_frame_renders_its_border() -> Result<()> {
        let frame = Frame::new().with_glyphs(ASCII_BOX);
        let root = SnapshotRoot::new(frame);
        let mut harness = Harness::builder(root).size(10, 4).build()?;
        harness.render()?;
        harness
            .tbuf()
            .assert_matches(buf!["+--------+" "|        |" "|        |" "+--------+"]);
        Ok(())
    }

    #[test]
    fn list_marks_the_selected_item() -> Result<()> {
        let list = List::<Text>::new().with_selection_indicator("selected", ">", false);
        let root = SnapshotRoot::new(list);
        let mut harness = Harness::builder(root).size(10, 4).build()?;

        harness.render()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<List<Text>>, ctx| {
            let view = ctx as &dyn ViewContext;
            let list_id = view.typed_id::<List<Text>>(view.find_one("**/list")?)?;
            ctx.with_widget_mut::<List<Text>, _>(list_id, |list, ctx| {
                list.append(ctx, Text::new("One"))?;
                list.append(ctx, Text::new("Two"))?;
                list.append(ctx, Text::new("Three"))?;
                Ok(())
            })?;
            Ok(())
        })?;

        harness.render()?;
        harness
            .tbuf()
            .assert_matches(buf![">One" " Two" " Three" ""]);
        Ok(())
    }
}
