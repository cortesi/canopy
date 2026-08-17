//! Whole-widget render checks over a minimal root.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Loader, ViewContext, Widget, buf,
        commands::{CommandNode, CommandSpec},
        error::Result,
        layout::Layout,
        state::NodeName,
        testing::harness::Harness,
    };

    use crate::{BoxGlyphs, Button, Frame, List, ScrollGlyphs, Text};

    const ASCII_BOX: BoxGlyphs = BoxGlyphs {
        topleft: '+',
        topright: '+',
        bottomleft: '+',
        bottomright: '+',
        horizontal: '-',
        vertical: '|',
    };

    const ASCII_SCROLL: ScrollGlyphs = ScrollGlyphs {
        horizontal_active: '-',
        vertical_active: '|',
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
        let frame = Frame::new()
            .with_glyphs(ASCII_BOX)
            .with_scroll_glyphs(ASCII_SCROLL);
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
            ctx.with_widget::<List<Text>, _>(list_id, |list, ctx| {
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
