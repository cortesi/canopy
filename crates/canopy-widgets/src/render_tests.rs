//! Whole-widget render checks over a minimal root.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext, Widget, buf,
        commands::{CommandNode, CommandSpec},
        error::Result,
        event::{key, mouse},
        geom::Point,
        layout::{Edges, Layout},
        state::NodeName,
        testing::harness::Harness,
    };

    use crate::{BoxGlyphs, Button, Dropdown, Frame, List, Selector, Text};

    fn click_at(location: Point) -> mouse::MouseEvent {
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
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
