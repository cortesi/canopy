use canopy::{derive_commands, layout::Edges, prelude::*};
use canopy_widgets::{
    Frame, Pad,
    editor::{EditMode, Editor, EditorConfig, WrapMode, highlight::SyntectHighlighter},
};

/// Default bindings for the cedit demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind_with("Tab", { path = "ed/", desc = "Next focus" }, function()
    root.focus_next()
end)
canopy.bind_with("p", { path = "ed/", desc = "Log demo message" }, function()
    canopy.log("cedit")
end)
"#;

/// Simple editor wrapper for the cedit demo.
pub struct Ed {
    /// Initial contents for the editor.
    contents: String,
    /// File extension hint for syntax highlighting.
    extension: String,
}

#[derive_commands]
impl Ed {
    /// Construct an editor with initial contents.
    pub fn new(contents: &str, extension: &str) -> Self {
        Self {
            contents: contents.to_string(),
            extension: extension.to_string(),
        }
    }
}

impl Widget for Ed {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let config = EditorConfig::new()
            .with_mode(EditMode::Vi)
            .with_wrap(WrapMode::None);
        let mut editor = Editor::with_config(&self.contents, config);
        editor.set_highlighter(Some(Box::new(SyntectHighlighter::new(
            self.extension.as_str(),
        ))));
        let pad_id = c.add_child(Pad::uniform(1))?;
        let frame_id = c.add_child_to(pad_id, Frame::new())?;
        let editor_id = c.add_child_to(frame_id, editor)?;

        c.set_layout_of(editor_id, Layout::fill().padding(Edges::all(1)))?;

        c.set_layout(Layout::fill())?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

impl Loader for Ed {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Install key bindings for the cedit demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.run_default_script(DEFAULT_BINDINGS).unwrap();
}
