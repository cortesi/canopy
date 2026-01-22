#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use canopy::{
        Canopy, Context, Loader, Widget,
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

    fn snapshot_dir() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidate = manifest_dir.join("../../tests/snapshots");
        if candidate.is_dir() {
            return candidate;
        }

        let cwd_candidate = env::current_dir()
            .ok()
            .map(|cwd| cwd.join("tests/snapshots"));
        if let Some(cwd_candidate) = cwd_candidate.filter(|path| path.is_dir()) {
            return cwd_candidate;
        }

        candidate
    }

    fn snapshot_path(name: &str) -> PathBuf {
        snapshot_dir().join(format!("{name}.txt"))
    }

    fn visible_snapshot(lines: Vec<String>) -> String {
        let mapped: Vec<String> = lines
            .into_iter()
            .map(|line| {
                line.chars()
                    .map(|ch| if ch == ' ' { '.' } else { ch })
                    .collect()
            })
            .collect();
        mapped.join("\n")
    }

    fn render_snapshot(harness: &mut Harness) -> Result<String> {
        harness.render()?;
        Ok(visible_snapshot(harness.tbuf().lines()))
    }

    fn assert_snapshot(name: &str, actual: &str) {
        let path = snapshot_path(name);
        let expected = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("missing snapshot {}: {err}", path.display()));
        let expected = expected.trim_end_matches('\n');
        assert_eq!(expected, actual, "snapshot mismatch for {name}");
    }

    #[test]
    fn snapshot_text() -> Result<()> {
        let root = SnapshotRoot::new(Text::new("Hello"));
        let mut harness = Harness::builder(root).size(10, 3).build()?;
        let snapshot = render_snapshot(&mut harness)?;
        assert_snapshot("text", &snapshot);
        Ok(())
    }

    #[test]
    fn snapshot_button() -> Result<()> {
        let root = SnapshotRoot::new(Button::new("OK").with_glyphs(ASCII_BOX));
        let mut harness = Harness::builder(root).size(10, 3).build()?;
        let snapshot = render_snapshot(&mut harness)?;
        assert_snapshot("button", &snapshot);
        Ok(())
    }

    #[test]
    fn snapshot_frame() -> Result<()> {
        let frame = Frame::new()
            .with_glyphs(ASCII_BOX)
            .with_scroll_glyphs(ASCII_SCROLL);
        let root = SnapshotRoot::new(frame);
        let mut harness = Harness::builder(root).size(10, 4).build()?;
        let snapshot = render_snapshot(&mut harness)?;
        assert_snapshot("frame", &snapshot);
        Ok(())
    }

    #[test]
    fn snapshot_list() -> Result<()> {
        let list = List::<Text>::new().with_selection_indicator("selected", ">", false);
        let root = SnapshotRoot::new(list);
        let mut harness = Harness::builder(root).size(10, 4).build()?;

        harness.render()?;
        harness.with_root_context(|_root: &mut SnapshotRoot<List<Text>>, ctx| {
            let list_id = ctx.find_one("**/list")?;
            ctx.with_widget::<List<Text>, _>(list_id, |list, ctx| {
                list.append(ctx, Text::new("One"))?;
                list.append(ctx, Text::new("Two"))?;
                list.append(ctx, Text::new("Three"))?;
                Ok(())
            })?;
            Ok(())
        })?;

        let snapshot = render_snapshot(&mut harness)?;
        assert_snapshot("list", &snapshot);
        Ok(())
    }
}
