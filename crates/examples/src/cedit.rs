use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, derive_commands,
    error::Result,
    event::key,
    layout::{Edges, Layout},
    render::Render,
    widget::Widget,
    widgets::{
        Root,
        editor::{EditMode, Editor, EditorConfig, WrapMode, highlight::SyntectHighlighter},
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
}

impl Widget for Ed {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let config = EditorConfig::new()
            .with_mode(EditMode::Vi)
            .with_wrap(WrapMode::None);
        let mut editor = Editor::with_config(&self.contents, config);
        editor.set_highlighter(Some(Box::new(SyntectHighlighter::plain())));
        let editor_id = c.add_orphan(editor);
        let frame_id = frame::Frame::wrap(c, editor_id)?;
        c.mount_child(frame_id)?;

        c.with_layout_of(editor_id, &mut |layout| {
            *layout = Layout::fill().padding(Edges::all(1));
        })?;
        c.with_layout_of(frame_id, &mut |layout| {
            *layout = Layout::fill().padding(Edges::all(1));
        })?;

        c.with_layout(&mut |layout| {
            *layout = Layout::fill();
        })?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
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
