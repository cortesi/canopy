use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, derive_commands,
    error::Result,
    event::{Event, key},
    geom::Rect,
    render::Render,
    widget::{EventOutcome, Widget},
    widgets::{Root, editor::Editor, frame},
};
use taffy::style::{Dimension, Display, FlexDirection, Style};

/// Simple editor wrapper for the cedit demo.
pub struct Ed {
    /// Initial contents for the editor.
    contents: String,
    /// Editor node id.
    editor: Option<NodeId>,
}

#[derive_commands]
impl Ed {
    /// Construct an editor with initial contents.
    pub fn new(contents: &str) -> Self {
        Self {
            contents: contents.to_string(),
            editor: None,
        }
    }

    /// Ensure the editor subtree is constructed and styled.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.editor.is_some() {
            return;
        }

        let editor = c.add(Box::new(Editor::new(&self.contents)));
        let frame_id = c.add(Box::new(frame::Frame::new()));

        c.mount_child(frame_id, editor)
            .expect("Failed to mount editor");
        c.set_children(c.node_id(), vec![frame_id])
            .expect("Failed to attach frame");

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)
            .expect("Failed to style root");

        let mut grow = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(frame_id, &mut grow)
            .expect("Failed to style frame");
        c.with_style(editor, &mut grow)
            .expect("Failed to style editor");

        self.editor = Some(editor);
    }
}

impl Widget for Ed {
    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
    }
}

impl Loader for Ed {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<Editor>();
    }
}

/// Install key bindings for the cedit demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("ed/")
        .key(key::KeyCode::Left, "editor::cursor_shift(1)")
        .key(key::KeyCode::Right, "editor::cursor_shift(-1)")
        .key(key::KeyCode::Down, "editor::cursor_shift_lines(1)")
        .key(key::KeyCode::Up, "editor::cursor_shift_lines(-1)")
        .key('h', "editor::cursor_shift(-1)")
        .key('l', "editor::cursor_shift(1)")
        .key('j', "editor::cursor_shift_chunk(1)")
        .key('k', "editor::cursor_shift_chunk(-1)")
        .key(key::KeyCode::Tab, "root::focus_next()")
        .key('p', "print(\"xxxx\")");
}
