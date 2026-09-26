//! Whole-widget render checks over a minimal root.

#[cfg(test)]
mod tests {
    use canopy::{
        CanopyBuilder, Context, ContextExt, NodeName, Register, ViewContextExt, Widget, buf,
        commands::{CommandNode, CommandSpec},
        error::Result,
        geom::{Point, PointI32, Size},
        input::{Event, key, mouse},
        layout::{Edges, Layout, ScrollOp},
        testing::harness::Harness,
    };

    use crate::{
        BoxGlyphs, Button, Dialog, DiffView, Dropdown, Frame, KeyHint, List, Root, Selector,
        StatusBar, Text,
        diff::{DiffModel, Mode, Scope},
    };

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
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| dropdown.toggle(ctx))
        })?;
        harness.render()?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|_, ctx| {
                ctx.set_layout_override(ctx.node_id(), Layout::fill().padding(Edges::all(1)).into())
            })
        })?;
        harness.render()?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|_, ctx| {
                assert!(ctx.scroll(ScrollOp::To(Point { x: 3, y: 2 })).changed());
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Charlie-"));
        assert_eq!(harness.buf().get(Point { x: 1, y: 1 }).unwrap().ch, 'C');
        harness.mouse(click_at(Point { x: 1, y: 1 }))?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, _| {
                assert_eq!(dropdown.selected_index(), 2);
                Ok(())
            })
        })?;
        assert!(harness.tbuf().contains_text("Charlie-"));
        Ok(())
    }

    #[test]
    fn selector_scrolls_labels_and_chooses_the_visible_row() -> Result<()> {
        let items = ["Alpha-long", "Bravo-long", "Charlie-long", "Delta-long"]
            .map(String::from)
            .to_vec();
        let root = SnapshotRoot::new(Selector::new(items));
        let mut harness = Harness::builder(root).size(10, 4).build()?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|_, ctx| {
                ctx.set_layout_override(ctx.node_id(), Layout::fill().padding(Edges::all(1)).into())
            })
        })?;
        harness.render()?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|_, ctx| {
                assert!(ctx.scroll(ScrollOp::To(Point { x: 4, y: 2 })).changed());
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Charlie-"));
        assert_eq!(harness.buf().get(Point { x: 1, y: 1 }).unwrap().ch, 'C');
        harness.mouse(click_at(Point { x: 1, y: 1 }))?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, _| {
                assert_eq!(selector.chosen(), Some(&"Charlie-long".to_string()));
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
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, ctx| {
                selector.select_last(ctx)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("( ) Fifth"));
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, ctx| {
                selector.choose(ctx)?;
                selector.select_by(ctx, -3)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("( ) Second"));
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Selector<String>>, ctx| {
            ctx.with_unique_descendant::<Selector<String>, _>(|selector, ctx| {
                assert_eq!(selector.chosen(), Some(&"Fifth".to_string()));
                selector.select_first(ctx)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("( ) First"));
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
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| {
                dropdown.toggle(ctx)?;
                dropdown.select_by(ctx, 4)
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Fifth"));
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, _ctx| dropdown.confirm())
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Fifth ▼"));
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
            ctx.with_unique_descendant::<Dropdown<String>, _>(|dropdown, ctx| dropdown.toggle(ctx))
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Fifth"));
        harness.with_root_widget_context(|_: &mut SnapshotRoot<Dropdown<String>>, ctx| {
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
            let _ = ctx.add_child(ctx.node_id(), child)?;
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("snapshot_root")
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
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<Frame>, ctx| {
            ctx.with_unique_descendant::<Frame, _>(|_, ctx| {
                ctx.add_child(ctx.node_id(), Text::new(text))?;
                Ok(())
            })
        })?;
        harness.render()?;
        let scroll_y = |harness: &Harness| {
            harness.canopy.with_root_view(|ctx| {
                let text = ctx
                    .unique_descendant::<Text>(ctx.node_id())
                    .expect("text lookup")
                    .expect("text node");
                ctx.view_of(text.into()).expect("text view").scroll.y
            })
        };

        // The right edge track spans rows 1 to 6. Thirty lines through a
        // six-row view give a one-row thumb at the top.
        assert_eq!(harness.buf().get(Point { x: 11, y: 1 }).unwrap().ch, '█');
        assert_eq!(harness.buf().get(Point { x: 11, y: 3 }).unwrap().ch, '│');

        // Dragging the thumb to the end of the track reaches the last line.
        harness.mouse(mouse_at(mouse::Action::Down, 11, 1))?;
        harness.mouse(mouse_at(mouse::Action::Drag, 11, 6))?;
        assert_eq!(scroll_y(&harness), 24);
        harness.mouse(mouse_at(mouse::Action::Up, 11, 6))?;

        // A press on the track centers the one-row thumb on the pointer: the
        // thumb then covers row 3.
        harness.mouse(mouse_at(mouse::Action::Down, 11, 3))?;
        harness.mouse(mouse_at(mouse::Action::Up, 11, 3))?;
        harness.render()?;
        assert_eq!(scroll_y(&harness), 8);
        assert_eq!(harness.buf().get(Point { x: 11, y: 3 }).unwrap().ch, '█');
        assert_eq!(harness.buf().get(Point { x: 11, y: 2 }).unwrap().ch, '│');
        Ok(())
    }

    #[test]
    fn inputs_show_focus_across_the_row_and_keep_the_prompt_out_of_the_value() -> Result<()> {
        let mut harness = Harness::builder(SnapshotRoot::new(
            crate::Input::new("hello").with_prompt(" 查: "),
        ))
        .size(20, 3)
        .build()?;
        let first = harness.canopy.with_root_view(|ctx| {
            ctx.unique_descendant::<crate::Input>(ctx.node_id())
                .unwrap()
                .unwrap()
        });
        let second =
            harness.with_root_widget_context(|_: &mut SnapshotRoot<crate::Input>, ctx| {
                ctx.add_child(ctx.node_id(), crate::Input::new(""))
            })?;
        harness.with_root_widget_context(|_: &mut SnapshotRoot<crate::Input>, ctx| {
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
        let caret = harness
            .canopy
            .snapshot()
            .expect("published frame")
            .cursors
            .first()
            .map(|cursor| cursor.location);
        assert_eq!(caret, Some(point(&harness, first.into(), 10)));
        harness.with_root_widget_context(|_: &mut SnapshotRoot<crate::Input>, ctx| {
            ctx.with_widget_mut(first, |input: &mut crate::Input, _| {
                assert_eq!(input.value(), "hello");
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
        harness.canopy.set_screen_size(Size::new(2, 3))?;
        harness.render()?;
        Ok(())
    }

    #[test]
    fn an_input_measured_to_its_text_shows_all_of_it() -> Result<()> {
        let mut harness = Harness::builder(SnapshotRoot::new(crate::Input::new("hello")))
            .size(20, 3)
            .build()?;
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<crate::Input>, ctx| {
            ctx.with_unique_descendant::<crate::Input, _>(|_, ctx| {
                ctx.set_layout_override(ctx.node_id(), Layout::column().into())
            })
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
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<crate::Tabs>, ctx| {
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

        harness.with_root_widget_context(|_root: &mut SnapshotRoot<crate::Tabs>, ctx| {
            ctx.with_unique_descendant::<crate::Tabs, _>(|tabs, ctx| {
                tabs.cycle(ctx, 1)?;
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

    /// A root that mounts a titled dialog around one line of text.
    struct DialogRoot;

    impl CommandNode for DialogRoot {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for DialogRoot {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let root = ctx.node_id();
            Dialog::new()
                .with_title("Ask")
                .add(ctx, root, Text::new("hi"))?;
            Ok(())
        }
    }

    #[test]
    fn a_dialog_frames_its_body_in_the_centre_and_swallows_margin_clicks() -> Result<()> {
        let mut harness = Harness::builder(DialogRoot).size(11, 5).build()?;
        harness.render()?;
        harness.tbuf().assert_matches(buf![
            "           "
            " ╭ Ask ──╮ "
            " │hi     │ "
            " ╰───────╯ "
            "           "
        ]);
        let dialog = harness.find_nodes("**/dialog")?[0];
        let outcome = harness.canopy.with_root_context(|ctx| {
            ctx.with_widget_mut(dialog, |dialog: &mut Dialog, ctx| {
                dialog.on_event(&Event::Mouse(click_at(Point { x: 0, y: 0 })), ctx)
            })
        })?;
        assert_eq!(outcome, canopy::EventOutcome::Handle);
        Ok(())
    }

    #[test]
    fn navigation_intents_scroll_a_view_that_can_move_and_decline_otherwise() -> Result<()> {
        let lines = (0..10)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut harness =
            Harness::builder(SnapshotRoot::new(Text::new(lines).with_focusable(true)))
                .script(
                    "nav",
                    r#"canopy.keymap({
                    { key = "j", description = "Down", action = "canopy.nav.down" },
                    { key = "G", description = "Last", action = "canopy.nav.last" },
                })"#,
                )
                .size(10, 3)
                .build()?;
        harness.render()?;
        let scroll_y = |harness: &Harness| {
            let text = harness.find_nodes("**/text").unwrap()[0];
            harness
                .canopy
                .with_root_view(|ctx| ctx.view_of(text).expect("text view").scroll.y)
        };
        harness.key('j')?;
        assert_eq!(scroll_y(&harness), 1, "the focused text scrolls a row");
        harness.key('G')?;
        assert_eq!(scroll_y(&harness), 7, "last scrolls to the end");
        harness.key('j')?;
        assert_eq!(scroll_y(&harness), 7, "a view at its end declines");
        Ok(())
    }

    #[test]
    fn a_diff_view_moves_between_changes_and_shows_a_message() -> Result<()> {
        let old = (0..20).map(|n| format!("line {n}\n")).collect::<String>();
        let new = old
            .replace("line 2\n", "two\n")
            .replace("line 15\n", "fifteen\n");
        let view = DiffView::new(DiffModel::from_texts(old, new));
        let mut harness = Harness::builder(SnapshotRoot::new(view))
            .register::<DiffView>()
            .size(30, 5)
            .build()?;
        harness.render()?;
        let scroll_y = |harness: &Harness| {
            let node = harness.find_nodes("**/diff_view").unwrap()[0];
            harness
                .canopy
                .with_root_view(|ctx| ctx.view_of(node).expect("diff view").scroll.y)
        };
        let step = |harness: &mut Harness, next: bool| -> Result<()> {
            harness.with_root_widget_context(|_: &mut SnapshotRoot<DiffView>, ctx| {
                ctx.with_unique_descendant::<DiffView, _>(|view, ctx| {
                    if next {
                        view.next_change(ctx);
                    } else {
                        view.prev_change(ctx);
                    }
                    Ok(())
                })
            })?;
            harness.render()
        };
        step(&mut harness, true)?;
        let second = scroll_y(&harness);
        assert!(
            second > 2,
            "the second change is further down, got {second}"
        );
        step(&mut harness, false)?;
        assert_eq!(scroll_y(&harness), 2, "the first change is its removed row");
        harness.with_root_widget_context(|_: &mut SnapshotRoot<DiffView>, ctx| {
            ctx.with_unique_descendant::<DiffView, _>(|view, _| {
                view.set_message("Binary file");
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Binary file"));
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

    /// A status bar with a hint for the help command, bound to `Ctrl+g`.
    ///
    /// A real [`Root`] hosts the bar, because a hint names a key only while its
    /// command has a target that shows.
    fn help_bar_harness(width: u32, height: u32) -> Result<Harness> {
        let bar = StatusBar::new()
            .with_left(Text::new("demo"))
            .with_right(KeyHint::for_command(Root::call_toggle_help(), "help"));
        let mut canopy = CanopyBuilder::new()
            .configure(Root::register)
            .script(
                "help",
                r#"canopy.bind("Ctrl+g", { description = "Help" }, command.root.toggle_help())"#,
            )
            .build()?;
        Root::new().install(&mut canopy, bar)?;
        Harness::from_canopy(canopy, Size::new(width, height))
    }

    #[test]
    fn a_hint_follows_its_binding() -> Result<()> {
        let mut harness = help_bar_harness(20, 1)?;
        harness.script(
            r#"canopy.unbind_key("Ctrl+g")
            canopy.bind("F1", { description = "Help" }, command.root.toggle_help())"#,
        )?;
        harness.render()?;
        harness.tbuf().assert_matches(buf!["demo        f1: help"]);
        Ok(())
    }

    #[test]
    fn status_bar_pins_its_status_left_and_its_hint_right() -> Result<()> {
        let mut harness = help_bar_harness(20, 2)?;
        harness.render()?;
        harness
            .tbuf()
            .assert_matches(buf!["demo    ctrl+g: help" ""]);
        Ok(())
    }

    #[test]
    fn a_narrow_status_bar_keeps_the_hint_at_the_left_edge() -> Result<()> {
        let mut harness = help_bar_harness(10, 1)?;
        harness.render()?;
        harness.tbuf().assert_matches(buf!["ctrl+g: he"]);
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
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<List<Text>>, ctx| {
            let list_id = ctx
                .unique_descendant::<List<Text>>(ctx.node_id())?
                .expect("snapshot root holds a list");
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

    #[test]
    fn diff_view_renders_unified_whole_file_rows() -> Result<()> {
        let view = DiffView::new(DiffModel::from_texts(
            "one\ntwo\nthree\n",
            "one\nchanged\nthree\n",
        ));
        let root = SnapshotRoot::new(view);
        let mut harness = Harness::builder(root).size(20, 4).build()?;
        harness.render()?;
        harness
            .tbuf()
            .assert_matches(buf![" 1 one" "-2 two" "+2 changed" " 3 three"]);
        Ok(())
    }

    #[test]
    fn diff_view_renders_context_blocks_with_gaps() -> Result<()> {
        let lines: Vec<String> = (0..20).map(|line| format!("line {line}\n")).collect();
        let old = lines.concat();
        let mut changed = lines;
        changed[10] = "changed\n".to_string();
        let view = DiffView::new(DiffModel::from_texts(old, changed.concat()))
            .with_scope(Scope::Context(1));
        let root = SnapshotRoot::new(view);
        let mut harness = Harness::builder(root).size(25, 7).build()?;
        harness.render()?;
        harness.tbuf().assert_matches(buf![
            "…"
            "    @@ -10,3 +10,3 @@"
            " 10 line 9"
            "-11 line 10"
            "+11 changed"
            " 12 line 11"
            "…"
        ]);
        Ok(())
    }

    #[test]
    fn diff_view_pairs_sides_in_side_by_side_layout() -> Result<()> {
        let view = DiffView::new(DiffModel::from_texts(
            "one\ntwo\nthree\n",
            "one\nchanged\nthree\n",
        ))
        .with_strategy(Mode::SideBySide);
        let root = SnapshotRoot::new(view);
        let mut harness = Harness::builder(root).size(24, 4).build()?;
        harness.render()?;
        harness.tbuf().assert_matches(buf![
            " 1 one     │ 1 one"
            "-2 two     │+2 changed"
            " 3 three   │ 3 three"
            ""
        ]);
        Ok(())
    }

    #[test]
    fn diff_view_scrolls_long_lines_horizontally() -> Result<()> {
        let view = DiffView::new(DiffModel::from_texts("abcdefghij\n", "abcdefghij\n"));
        let root = SnapshotRoot::new(view);
        let mut harness = Harness::builder(root).size(8, 1).build()?;
        harness.render()?;
        harness.tbuf().assert_matches(buf![" 1 abcde"]);
        harness.with_root_widget_context(|_root: &mut SnapshotRoot<DiffView>, ctx| {
            ctx.with_unique_descendant::<DiffView, _>(|_, ctx| {
                assert!(ctx.scroll(ScrollOp::To(Point { x: 5, y: 0 })).changed());
                Ok(())
            })
        })?;
        harness.render()?;
        harness.tbuf().assert_matches(buf![" 1 fghij"]);
        Ok(())
    }

    #[test]
    fn diff_view_prepares_one_highlighter_per_side() -> Result<()> {
        use std::{cell::Cell, rc::Rc};

        use crate::highlight::{HighlightSpan, Highlighter};

        struct Counting {
            prepared: Rc<Cell<usize>>,
            lines: Rc<Cell<usize>>,
        }

        impl Highlighter for Counting {
            fn prepare(&self, _text: &str) {
                self.prepared.set(self.prepared.get() + 1);
            }

            fn highlight_line(&self, _line: usize, _text: &str) -> Vec<HighlightSpan> {
                self.lines.set(self.lines.get() + 1);
                Vec::new()
            }
        }

        let old_prepared = Rc::new(Cell::new(0));
        let new_prepared = Rc::new(Cell::new(0));
        let old_lines = Rc::new(Cell::new(0));
        let new_lines = Rc::new(Cell::new(0));
        let view = DiffView::new(DiffModel::from_texts("one\ntwo\n", "one\nthree\n"))
            .with_old_highlighter(Box::new(Counting {
                prepared: Rc::clone(&old_prepared),
                lines: Rc::clone(&old_lines),
            }))
            .with_new_highlighter(Box::new(Counting {
                prepared: Rc::clone(&new_prepared),
                lines: Rc::clone(&new_lines),
            }));
        let root = SnapshotRoot::new(view);
        let mut harness = Harness::builder(root).size(20, 3).build()?;
        harness.render()?;
        assert_eq!(old_prepared.get(), 1, "the old source prepares once");
        assert_eq!(new_prepared.get(), 1, "the new source prepares once");
        assert!(old_lines.get() > 0, "the old side highlights its lines");
        assert!(new_lines.get() > 0, "the new side highlights its lines");

        harness.render()?;
        assert_eq!(old_prepared.get(), 1, "a second frame reuses the source");
        assert_eq!(new_prepared.get(), 1, "a second frame reuses the source");
        Ok(())
    }

    #[test]
    fn diff_view_highlight_spans_paint_over_the_row_background() -> Result<()> {
        use canopy::style::{AttrSet, Color, Paint, Style};

        use crate::highlight::{HighlightSpan, Highlighter};

        struct FirstChar(Style);

        impl Highlighter for FirstChar {
            fn highlight_line(&self, _line: usize, text: &str) -> Vec<HighlightSpan> {
                if text.is_empty() {
                    return Vec::new();
                }
                vec![HighlightSpan {
                    range: 0..1,
                    style: self.0.clone(),
                }]
            }
        }

        let highlight_style = Style {
            fg: Paint::solid(Color::Green),
            bg: Paint::solid(Color::Red),
            attrs: AttrSet::default(),
        };
        let view = DiffView::new(DiffModel::from_texts("hi\n", "hi\n"))
            .with_old_highlighter(Box::new(FirstChar(highlight_style)));
        let root = SnapshotRoot::new(view);
        let mut harness = Harness::builder(root).size(20, 1).build()?;
        harness.render()?;

        // Row layout: gutter (marker, one digit, one space), then the code.
        let buf = harness.buf();
        let highlighted = buf.get(Point { x: 3, y: 0 }).expect("highlighted cell");
        let plain = buf.get(Point { x: 4, y: 0 }).expect("plain cell");
        assert_eq!(
            highlighted.style.fg,
            Color::Green,
            "the span's foreground paints through"
        );
        assert_eq!(
            highlighted.style.bg, plain.style.bg,
            "the row's background wins over the span's own background"
        );
        assert_ne!(
            highlighted.style.bg,
            Color::Red,
            "the span's own background is discarded"
        );
        Ok(())
    }
}
