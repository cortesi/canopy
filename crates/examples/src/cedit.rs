use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, derive_commands,
    error::Result,
    event::key,
    geom::Rect,
    layout::Dimension,
    render::Render,
    widget::Widget,
    widgets::{Root, editor::Editor, frame},
};

/// Simple editor wrapper for the cedit demo.
pub struct Ed {
    /// Initial contents for the editor.
    contents: String,
}

#[derive_commands]
impl Ed {
    /// Construct an editor with initial contents.
    pub fn new(contents: &str) -> Self {
        Self {
            contents: contents.to_string(),
        }
    }

    /// Ensure the editor subtree is constructed and styled.
    fn ensure_tree(&self, c: &mut dyn Context) {
        if !c.children().is_empty() {
            return;
        }

        let frame_id = c
            .add_child(frame::Frame::new())
            .expect("Failed to mount frame");
        let editor = c
            .add_child_to(frame_id, Editor::new(&self.contents))
            .expect("Failed to mount editor");

        c.with_layout(&mut |layout| {
            layout.flex_col();
        })
        .expect("Failed to configure layout");
        c.with_layout_of(frame_id, &mut |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })
        .expect("Failed to configure frame layout");
        c.with_layout_of(editor, &mut |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })
        .expect("Failed to configure editor layout");
    }
}

impl Widget for Ed {
    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
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
