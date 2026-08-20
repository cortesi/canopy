use canopy::{
    command, derive_commands, geom,
    layout::{CanvasContext, Edges},
    prelude::*,
};
use canopy_widgets::{
    Frame,
    editor::{
        EditMode, Editor, EditorConfig, LineNumbers, WrapMode, highlight::SyntectHighlighter,
    },
};

/// Default bindings for the editor gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind("Tab", { path = "editor_gym", description = "Next focus" }, function()
    root.focus("Next")
end)
canopy.bind("BackTab", { path = "editor_gym", description = "Previous focus" }, function()
    root.focus("Prev")
end)
canopy.bind("PageDown", { path = "editor_gym", description = "Page down" }, function()
    editor_gym.page(1)
end)
canopy.bind("PageUp", { path = "editor_gym", description = "Page up" }, function()
    editor_gym.page(-1)
end)
canopy.bind("Home", { path = "editor_gym", description = "Top" }, function()
    editor_gym.scroll_to(0, 0)
end)
canopy.bind_mouse("ScrollDown", { path = "editor_gym", description = "Scroll down" }, function()
    editor_gym.scroll("Down")
end)
canopy.bind_mouse("ScrollUp", { path = "editor_gym", description = "Scroll up" }, function()
    editor_gym.scroll("Up")
end)
canopy.bind("q", { path = "root", description = "Quit" }, function()
    root.quit()
end)
"#;

/// Seed text for the single-line editor sample.
const SINGLE_LINE_SEED: &str = "Single line input shows horizontal scrolling with wrap off. ";
/// Seed text for long-line samples that require horizontal scrolling.
const LONG_LINE_SEED: &str =
    "This is a long line meant to exceed the frame width so horizontal scrolling is visible. ";
/// Paragraph text used in soft-wrap samples.
const PARAGRAPH: &str = "The editor gym collects several editor configurations in one place so it is easy to compare behaviors. The text here is simple filler meant to wrap across multiple lines when the view is narrow.";
/// Text used in the read-only editor sample.
const READ_ONLY_TEXT: &str = "Read only editors ignore edits but still allow selection and scrolling.\nThey are useful for preview panes and log views.";
/// Tab-delimited sample text for tab stop demonstrations.
const TAB_SAMPLE: &str = "col1\tcol2\tcol3\nshort\tlonger\t123\nalpha\tbravo\tcharlie\n";
/// Rust snippet used for syntax highlighting demonstration.
const RUST_SAMPLE: &str = "use std::collections::HashMap;\n\nfn main() {\n    let mut map = HashMap::new();\n    map.insert(\"alpha\", 1);\n    map.insert(\"beta\", 2);\n    if let Some(value) = map.get(\"alpha\") {\n        println!(\"alpha: {}\", value);\n    }\n}\n";

/// Repeat the provided seed text a given number of times.
fn repeated_line(seed: &str, repeats: usize) -> String {
    seed.repeat(repeats.max(1))
}

/// Build a wrapped paragraph sample.
fn wrap_text() -> String {
    format!("{PARAGRAPH}\n\n{PARAGRAPH}")
}

/// Build a multi-line sample without wrapping.
fn no_wrap_text() -> String {
    let line = repeated_line(LONG_LINE_SEED, 2);
    format!("{line}\n{line}\nShort line for contrast.")
}

/// Generate numbered sample lines.
fn numbered_lines(prefix: &str, count: usize) -> String {
    let mut lines = Vec::with_capacity(count);
    for idx in 1..=count {
        lines.push(format!("{prefix} {idx:02}: sample text for scrolling"));
    }
    lines.join("\n")
}

/// Create a framed editor node and attach it to the parent.
fn add_editor_frame(
    c: &mut dyn Context,
    parent: impl Into<NodeId>,
    title: &str,
    text: impl Into<String>,
    config: EditorConfig,
    height: Option<u32>,
    highlighter: Option<SyntectHighlighter>,
) -> Result<()> {
    let mut editor = Editor::with_config(text, config);
    if let Some(highlighter) = highlighter {
        editor.set_highlighter(Some(Box::new(highlighter)));
    }
    let frame_id = c.add_child_to(parent, Frame::new().with_title(title))?;
    let editor_id = c.add_child_to(frame_id, editor)?;

    c.set_layout_of(editor_id, Layout::fill())?;

    let mut frame_layout = Layout::column().padding(Edges::all(1)).flex_horizontal(1);
    if let Some(height) = height {
        frame_layout = frame_layout.fixed_height(height);
    }
    c.set_layout_of(frame_id, frame_layout)?;

    Ok(())
}

/// Column container for editor frames.
struct EditorColumn;

impl EditorColumn {
    /// Construct an editor column container.
    fn new() -> Self {
        Self
    }
}

impl Widget for EditorColumn {
    fn layout(&self) -> Layout {
        Layout::column().flex_horizontal(1).gap(1).overflow_y()
    }
}

/// Root widget for the editor gym demo.
pub struct EditorGym;

impl Default for EditorGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl EditorGym {
    /// Construct a new editor gym demo.
    pub fn new() -> Self {
        Self
    }

    #[command]
    /// Scroll the outer pane by one line in the specified direction.
    /// @param dir The direction to scroll.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: geom::Direction) {
        match dir {
            geom::Direction::Up => c.scroll_up(),
            geom::Direction::Down => c.scroll_down(),
            geom::Direction::Left => c.scroll_left(),
            geom::Direction::Right => c.scroll_right(),
        };
    }

    #[command]
    /// Page the outer pane. Negative values move up; positive values move down.
    /// @param delta Signed page delta.
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) {
        if delta < 0 {
            c.page_up();
        } else if delta > 0 {
            c.page_down();
        }
    }

    #[command]
    /// Scroll the outer pane to an absolute content position.
    pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {
        c.scroll_to(x, y);
    }

    /// Build the left column of editor samples.
    fn build_left_column(&self, c: &mut dyn Context) -> Result<()> {
        let single_line = repeated_line(SINGLE_LINE_SEED, 2);
        let soft_wrap = wrap_text();
        let no_wrap = no_wrap_text();
        let line_numbers = numbered_lines("Line", 30);
        let auto_grow = numbered_lines("Auto", 4);

        let column_id = c.add_child(EditorColumn::new())?;
        add_editor_frame(
            c,
            column_id,
            "Single line (text)",
            single_line,
            EditorConfig::new()
                .with_multiline(false)
                .with_wrap(WrapMode::None),
            Some(3),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Soft wrap (multiline)",
            soft_wrap,
            EditorConfig::new().with_wrap(WrapMode::Soft),
            Some(8),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "No wrap (multiline)",
            no_wrap,
            EditorConfig::new().with_wrap(WrapMode::None),
            Some(6),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Line numbers (absolute)",
            line_numbers,
            EditorConfig::new().with_line_numbers(LineNumbers::Absolute),
            Some(8),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Auto grow (min 2 max 6)",
            auto_grow,
            EditorConfig::new()
                .with_auto_grow(true)
                .with_min_height(2)
                .with_max_height(Some(6)),
            None,
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Read only",
            READ_ONLY_TEXT,
            EditorConfig::new().with_read_only(true),
            Some(6),
            None,
        )?;
        Ok(())
    }

    /// Build the right column of editor samples.
    fn build_right_column(&self, c: &mut dyn Context) -> Result<()> {
        let vi_text = numbered_lines("Vi", 24);

        let column_id = c.add_child(EditorColumn::new())?;
        add_editor_frame(
            c,
            column_id,
            "Vi mode (relative numbers)",
            vi_text,
            EditorConfig::new()
                .with_mode(EditMode::Vi)
                .with_line_numbers(LineNumbers::Relative),
            Some(8),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Syntax highlight (rust)",
            RUST_SAMPLE,
            EditorConfig::new().with_line_numbers(LineNumbers::Absolute),
            Some(10),
            Some(SyntectHighlighter::new("rs")),
        )?;
        add_editor_frame(
            c,
            column_id,
            "Tab stop 2",
            TAB_SAMPLE,
            EditorConfig::new()
                .with_tab_stop(2)
                .with_wrap(WrapMode::None),
            Some(6),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Text mode (no numbers)",
            wrap_text(),
            EditorConfig::new().with_wrap(WrapMode::Soft),
            Some(7),
            None,
        )?;
        add_editor_frame(
            c,
            column_id,
            "Vi mode (no wrap)",
            no_wrap_text(),
            EditorConfig::new()
                .with_mode(EditMode::Vi)
                .with_wrap(WrapMode::None),
            Some(6),
            None,
        )?;
        Ok(())
    }
}

impl Widget for EditorGym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Row).gap(1).overflow_y()
    }

    fn canvas(&self, view: Size, ctx: &CanvasContext) -> Size {
        let extent = ctx.children_extent();
        Size::new(view.w.max(extent.w), view.h.max(extent.h))
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        self.build_left_column(c)?;
        self.build_right_column(c)?;
        Ok(())
    }
}

impl Loader for EditorGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Install key bindings for the editor gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(DEFAULT_BINDINGS)?;
    Ok(())
}
