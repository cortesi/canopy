use canopy::{
    Binder, Canopy, Context, Loader, NodeId, Widget, command,
    commands::{ScrollDirection, VerticalDirection},
    derive_commands,
    error::Result,
    event::{key, mouse},
    layout::{CanvasContext, Direction, Edges, Layout, Size},
};
use canopy_widgets::{
    Frame, Root,
    editor::{
        EditMode, Editor, EditorConfig, LineNumbers, WrapMode, highlight::SyntectHighlighter,
    },
};

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

/// Create a framed editor node and return the frame node id.
fn add_editor_frame(
    c: &mut dyn Context,
    title: &str,
    text: impl Into<String>,
    config: EditorConfig,
    height: Option<u32>,
    highlighter: Option<SyntectHighlighter>,
) -> Result<NodeId> {
    let mut editor = Editor::with_config(text, config);
    if let Some(highlighter) = highlighter {
        editor.set_highlighter(Some(Box::new(highlighter)));
    }
    let frame_id = c.create_detached(Frame::new().with_title(title));
    let editor_id = c.add_child_to(frame_id, editor)?;

    c.set_layout_of(editor_id, Layout::fill())?;

    let mut frame_layout = Layout::column().padding(Edges::all(1)).flex_horizontal(1);
    if let Some(height) = height {
        frame_layout = frame_layout.fixed_height(height);
    }
    c.set_layout_of(frame_id, frame_layout)?;

    Ok(frame_id)
}

/// Column container for editor frames.
struct EditorColumn;

#[derive_commands]
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

    /// Scroll the outer pane by one line in the specified direction.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: ScrollDirection) {
        match dir {
            ScrollDirection::Up => c.scroll_up(),
            ScrollDirection::Down => c.scroll_down(),
            ScrollDirection::Left => c.scroll_left(),
            ScrollDirection::Right => c.scroll_right(),
        };
    }

    /// Page in the outer pane.
    pub fn page(&mut self, c: &mut dyn Context, dir: VerticalDirection) {
        match dir {
            VerticalDirection::Up => c.page_up(),
            VerticalDirection::Down => c.page_down(),
        };
    }

    #[command]
    /// Scroll the outer pane up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Up);
    }

    #[command]
    /// Scroll the outer pane down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Down);
    }

    #[command]
    /// Scroll the outer pane left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Left);
    }

    #[command]
    /// Scroll the outer pane right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Right);
    }

    #[command]
    /// Page up in the outer pane.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Up);
    }

    #[command]
    /// Page down in the outer pane.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Down);
    }

    #[command]
    /// Scroll the outer pane to an absolute content position.
    pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {
        c.scroll_to(x, y);
    }

    /// Build the left column of editor samples.
    fn build_left_column(&self, c: &mut dyn Context) -> Result<NodeId> {
        let single_line = repeated_line(SINGLE_LINE_SEED, 2);
        let soft_wrap = wrap_text();
        let no_wrap = no_wrap_text();
        let line_numbers = numbered_lines("Line", 30);
        let auto_grow = numbered_lines("Auto", 4);

        let frames = vec![
            add_editor_frame(
                c,
                "Single line (text)",
                single_line,
                EditorConfig::new()
                    .with_multiline(false)
                    .with_wrap(WrapMode::None),
                Some(3),
                None,
            )?,
            add_editor_frame(
                c,
                "Soft wrap (multiline)",
                soft_wrap,
                EditorConfig::new().with_wrap(WrapMode::Soft),
                Some(8),
                None,
            )?,
            add_editor_frame(
                c,
                "No wrap (multiline)",
                no_wrap,
                EditorConfig::new().with_wrap(WrapMode::None),
                Some(6),
                None,
            )?,
            add_editor_frame(
                c,
                "Line numbers (absolute)",
                line_numbers,
                EditorConfig::new().with_line_numbers(LineNumbers::Absolute),
                Some(8),
                None,
            )?,
            add_editor_frame(
                c,
                "Auto grow (min 2 max 6)",
                auto_grow,
                EditorConfig::new()
                    .with_auto_grow(true)
                    .with_min_height(2)
                    .with_max_height(Some(6)),
                None,
                None,
            )?,
            add_editor_frame(
                c,
                "Read only",
                READ_ONLY_TEXT,
                EditorConfig::new().with_read_only(true),
                Some(6),
                None,
            )?,
        ];

        let column_id = c.create_detached(EditorColumn::new());
        c.set_children_of(column_id, frames)?;
        Ok(column_id)
    }

    /// Build the right column of editor samples.
    fn build_right_column(&self, c: &mut dyn Context) -> Result<NodeId> {
        let vi_text = numbered_lines("Vi", 24);

        let frames = vec![
            add_editor_frame(
                c,
                "Vi mode (relative numbers)",
                vi_text,
                EditorConfig::new()
                    .with_mode(EditMode::Vi)
                    .with_line_numbers(LineNumbers::Relative),
                Some(8),
                None,
            )?,
            add_editor_frame(
                c,
                "Syntax highlight (rust)",
                RUST_SAMPLE,
                EditorConfig::new().with_line_numbers(LineNumbers::Absolute),
                Some(10),
                Some(SyntectHighlighter::new("rs")),
            )?,
            add_editor_frame(
                c,
                "Tab stop 2",
                TAB_SAMPLE,
                EditorConfig::new()
                    .with_tab_stop(2)
                    .with_wrap(WrapMode::None),
                Some(6),
                None,
            )?,
            add_editor_frame(
                c,
                "Text mode (no numbers)",
                wrap_text(),
                EditorConfig::new().with_wrap(WrapMode::Soft),
                Some(7),
                None,
            )?,
            add_editor_frame(
                c,
                "Vi mode (no wrap)",
                no_wrap_text(),
                EditorConfig::new()
                    .with_mode(EditMode::Vi)
                    .with_wrap(WrapMode::None),
                Some(6),
                None,
            )?,
        ];

        let column_id = c.create_detached(EditorColumn::new());
        c.set_children_of(column_id, frames)?;
        Ok(column_id)
    }
}

impl Widget for EditorGym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Row).gap(1).overflow_y()
    }

    fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
        let extent = ctx.children_extent();
        Size::new(view.width.max(extent.width), view.height.max(extent.height))
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let left = self.build_left_column(c)?;
        let right = self.build_right_column(c)?;
        c.set_children(vec![left, right])?;
        Ok(())
    }
}

impl Loader for EditorGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<EditorColumn>()?;
        c.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Install key bindings for the editor gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("editor_gym")
        .key(key::KeyCode::Tab, "root::focus_next()")
        .key(key::KeyCode::BackTab, "root::focus_prev()")
        .key(key::KeyCode::PageDown, "editor_gym::page_down()")
        .key(key::KeyCode::PageUp, "editor_gym::page_up()")
        .key(key::KeyCode::Home, "editor_gym::scroll_to(0, 0)")
        .mouse(mouse::Action::ScrollDown, "editor_gym::scroll_down()")
        .mouse(mouse::Action::ScrollUp, "editor_gym::scroll_up()")
        .with_path("root")
        .key('q', "root::quit()");
}
