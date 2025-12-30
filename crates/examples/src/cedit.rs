use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, derive_commands,
    error::Result,
    event::key,
    layout::Layout,
    render::Render,
    widget::Widget,
    widgets::{
        Root,
        editor::{EditMode, Editor, EditorConfig, LineNumbers, highlight::SyntectHighlighter},
        frame,
    },
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

        let config = EditorConfig::new()
            .with_mode(EditMode::Vi)
            .with_line_numbers(LineNumbers::Relative);
        let mut editor = Editor::with_config(&self.contents, config);
        editor.set_highlighter(Some(Box::new(SyntectHighlighter::plain())));
        let editor_id = c.add_orphan(editor);
        let frame_id = frame::Frame::wrap(c, editor_id).expect("Failed to wrap frame");
        c.mount_child(frame_id).expect("Failed to mount frame");

        c.with_layout(&mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to configure layout");
    }
}

impl Widget for Ed {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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
        .key(key::KeyCode::Tab, "root::focus_next()")
        .key('p', "print(\"cedit\")");
}
