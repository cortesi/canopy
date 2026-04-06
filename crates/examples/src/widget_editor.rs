use canopy::{derive_commands, layout::Edges, prelude::*};
use canopy_widgets::{
    Frame, Pad,
    editor::{EditMode, Editor, EditorConfig, WrapMode, highlight::SyntectHighlighter},
};

/// Default bindings for the widget editor demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind_with("Tab", { path = "widget_editor/", desc = "Next focus" }, function()
    root.focus("Next")
end)
"#;

/// Widget editor example that opens a Rust file with syntax highlighting.
pub struct WidgetEditor {
    /// Source contents to display.
    contents: String,
    /// File extension hint for syntax selection.
    extension: String,
    /// Frame title to display.
    title: String,
}

#[derive_commands]
impl WidgetEditor {
    /// Construct a widget editor from file contents and metadata.
    pub fn new(
        contents: impl Into<String>,
        extension: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            contents: contents.into(),
            extension: extension.into(),
            title: title.into(),
        }
    }
}

impl Widget for WidgetEditor {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let config = EditorConfig::new()
            .with_mode(EditMode::Vi)
            .with_wrap(WrapMode::None);
        let mut editor = Editor::with_config(&self.contents, config);
        editor.set_highlighter(Some(Box::new(SyntectHighlighter::new(
            self.extension.as_str(),
        ))));

        let pad_id = c.add_child(Pad::uniform(1))?;
        let frame_id = c.add_child_to(pad_id, Frame::new().with_title(self.title.clone()))?;
        let editor_id = c.add_child_to(frame_id, editor)?;

        c.set_layout_of(editor_id, Layout::fill().padding(Edges::all(1)))?;
        c.set_layout(Layout::fill())?;
        Ok(())
    }
}

impl Loader for WidgetEditor {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Install key bindings for the widget editor example.
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.run_default_script(DEFAULT_BINDINGS).unwrap();
}
