use std::path::Path;

use canopy::{
    CanopyBuilder, Context, ContextExt, Register, Setup, Widget,
    error::Result,
    layout::{Edges, Layout},
};
use canopy_widgets::{
    Frame, Pad,
    editor::{EditMode, Editor, EditorConfig, WrapMode, highlight::SyntectHighlighter},
};

/// Default bindings for the widget editor demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind("Tab", {
    path = "widget_editor/",
    phase = "before_widget",
    description = "Next focus",
}, command.root.focus("next"))
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

        let pad_id = c.add_child(c.node_id(), Pad::uniform(1))?;
        let frame_id = c.add_child(pad_id, Frame::new().with_title(self.title.clone()))?;
        let editor_id = c.add_child(frame_id, editor)?;

        c.set_layout_override(
            editor_id.into(),
            Layout::fill().padding(Edges::all(1)).into(),
        )?;
        c.set_layout_override(c.node_id(), Layout::fill().into())?;
        Ok(())
    }
}

impl Register for WidgetEditor {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Return a lowercase file extension hint for syntax selection.
pub fn file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "txt".to_string())
}

/// Return a short title for the editor frame.
pub fn file_title(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("widget_editor", DEFAULT_BINDINGS)
}
