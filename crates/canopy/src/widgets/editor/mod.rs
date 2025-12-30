//! Editor widget and supporting types.

/// Syntax highlighting helpers.
pub mod highlight;
/// Layout and wrapping cache.
mod layout;
/// Search state and match helpers.
mod search;
/// Vi mode state helpers.
mod vi;
/// Editor widget implementation.
mod widget;

pub use widget::Editor;

#[cfg(test)]
mod tests;

/// Wrapping behavior for the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    /// No wrapping; horizontal scrolling is enabled.
    None,
    /// Soft wrapping at the view width.
    Soft,
}

/// Editing mode for the editor widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Text entry mode with direct insertion.
    Text,
    /// Vi-style modal editing.
    Vi,
}

/// Line number rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineNumbers {
    /// Do not render line numbers.
    None,
    /// Render absolute line numbers.
    Absolute,
    /// Render relative line numbers (current line stays absolute).
    Relative,
}

/// Configuration for the editor widget.
#[derive(Debug, Clone)]
pub struct EditorConfig {
    /// Allow multi-line content.
    pub multiline: bool,
    /// Wrapping mode.
    pub wrap: WrapMode,
    /// Auto-grow height to fit contents.
    pub auto_grow: bool,
    /// Minimum height when auto-growing.
    pub min_height: u32,
    /// Maximum height when auto-growing.
    pub max_height: Option<u32>,
    /// Edit mode behavior.
    pub mode: EditMode,
    /// Whether the editor is read-only.
    pub read_only: bool,
    /// Line number rendering mode.
    pub line_numbers: LineNumbers,
    /// Tab stop width in columns.
    pub tab_stop: usize,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            multiline: true,
            wrap: WrapMode::Soft,
            auto_grow: false,
            min_height: 1,
            max_height: None,
            mode: EditMode::Text,
            read_only: false,
            line_numbers: LineNumbers::None,
            tab_stop: 4,
        }
    }
}

impl EditorConfig {
    /// Construct a default editor configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure multiline behavior.
    pub fn with_multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    /// Configure wrapping mode.
    pub fn with_wrap(mut self, wrap: WrapMode) -> Self {
        self.wrap = wrap;
        self
    }

    /// Configure auto-grow behavior.
    pub fn with_auto_grow(mut self, auto_grow: bool) -> Self {
        self.auto_grow = auto_grow;
        self
    }

    /// Configure the minimum height.
    pub fn with_min_height(mut self, min_height: u32) -> Self {
        self.min_height = min_height;
        self
    }

    /// Configure the maximum height.
    pub fn with_max_height(mut self, max_height: Option<u32>) -> Self {
        self.max_height = max_height;
        self
    }

    /// Configure the edit mode.
    pub fn with_mode(mut self, mode: EditMode) -> Self {
        self.mode = mode;
        self
    }

    /// Configure read-only behavior.
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Configure line number rendering.
    pub fn with_line_numbers(mut self, line_numbers: LineNumbers) -> Self {
        self.line_numbers = line_numbers;
        self
    }

    /// Configure the tab stop width.
    pub fn with_tab_stop(mut self, tab_stop: usize) -> Self {
        self.tab_stop = tab_stop.max(1);
        self
    }
}
