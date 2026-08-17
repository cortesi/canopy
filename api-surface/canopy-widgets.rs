// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy-widgets, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_widgets {
    //! Built-in widgets for canopy applications.
    //!
    //! This crate provides a collection of reusable widgets for building terminal
    //! user interfaces with canopy.

    pub mod editor {
        //! Experimental editor API with syntax highlighting and vi mode.
        //! Editor widget and supporting types.

        pub mod highlight {
            //! Syntax highlighting helpers.

            /// A highlighted span for a single line.
            #[derive(Debug, Clone)]
            pub struct HighlightSpan {
                /// Character range covered by the span.
                pub range: std::ops::Range<usize>,
                /// Style to apply to the span.
                pub style: canopy::style::Style,
            }

            /// Trait for providing syntax highlighting spans.
            pub trait Highlighter: Send {
                /// Return highlight spans for a line of text.
                fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan>;
            }

            /// A basic syntect-backed highlighter.
            #[derive(Debug, Clone, Default)]
            pub struct SyntectHighlighter {}

            impl SyntectHighlighter {
                /// Construct a new syntect highlighter for the provided extension.
                pub fn new(extension: impl Into<String>) -> Self {}

                /// Construct a syntect highlighter with a named theme.
                pub fn with_theme_name(
                    extension: impl Into<String>,
                    theme_name: impl AsRef<str>,
                ) -> Self {
                }

                /// Construct a syntect highlighter using a specific theme.
                pub fn with_theme(extension: impl Into<String>, theme: Theme) -> Self {}

                /// Construct a highlighter using the plain text syntax.
                pub fn plain() -> Self {}
            }

            impl Highlighter for SyntectHighlighter {
                fn highlight_line(&self, _line: usize, text: &str) -> Vec<HighlightSpan> {}
            }
        }

        /// Information about how an edit changed logical line counts.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub struct LineChange {
            /// First affected line index.
            pub start_line: usize,
            /// Number of lines replaced.
            pub old_line_count: usize,
            /// Number of lines inserted.
            pub new_line_count: usize,
        }

        /// Rope-backed text buffer with selection and undo/redo support.
        #[derive(Debug, Clone)]
        pub struct TextBuffer {}

        impl TextBuffer {
            /// Create a new buffer from an initial string.
            pub fn new(text: impl Into<String>) -> Self {}

            /// Return the current buffer revision.
            pub fn revision(&self) -> u64 {}

            /// Return the current selection.
            pub fn selection(&self) -> Selection {}

            /// Replace the selection, clamping to bounds.
            pub fn set_selection(&mut self, selection: Selection) {}

            /// Return the cursor position (selection head).
            pub fn cursor(&self) -> TextPosition {}

            /// Replace the cursor and collapse the selection.
            pub fn set_cursor(&mut self, pos: TextPosition) {}

            /// Return the full buffer contents as a string.
            pub fn text(&self) -> String {}

            /// Return the total number of logical lines.
            pub fn line_count(&self) -> usize {}

            /// Return the line length in chars, excluding any trailing newline.
            pub fn line_char_len(&self, line: usize) -> usize {}

            /// Return the line length in chars, or `None` when the line is out of bounds.
            pub fn try_line_char_len(&self, line: usize) -> Option<usize> {}

            /// Return the text of a logical line without a trailing newline.
            pub fn line_text(&self, line: usize) -> String {}

            /// Return the text of a logical line, or `None` when the line is out of bounds.
            pub fn try_line_text(&self, line: usize) -> Option<String> {}

            /// Take the pending line change, if any.
            pub fn take_change(&mut self) -> Option<LineChange> {}

            /// Begin a grouped transaction.
            pub fn begin_transaction(&mut self) {}

            /// Commit the active transaction, if any.
            pub fn commit_transaction(&mut self) {}

            /// Begin a grouped transaction that commits when the guard is dropped.
            pub fn transaction(&mut self) -> TextTransaction<'_> {}

            /// Undo the most recent transaction.
            pub fn undo(&mut self) -> bool {}

            /// Redo the most recently undone transaction.
            pub fn redo(&mut self) -> bool {}

            /// Insert text at the cursor, replacing any selection.
            pub fn insert_text(&mut self, text: &str) {}

            /// Replace a range with the provided text.
            pub fn replace_range(&mut self, range: TextRange, text: &str) {}

            /// Delete the selection or the grapheme before the cursor.
            pub fn delete_backward(&mut self, allow_line_wrap: bool) -> bool {}

            /// Delete the selection or the grapheme after the cursor.
            pub fn delete_forward(&mut self, allow_line_wrap: bool) -> bool {}

            /// Move the cursor left by one grapheme.
            pub fn move_left(&mut self, allow_line_wrap: bool) -> bool {}

            /// Move the cursor right by one grapheme.
            pub fn move_right(&mut self, allow_line_wrap: bool) -> bool {}

            /// Move the cursor to the start of the current line.
            pub fn move_line_start(&mut self) {}

            /// Move the cursor to the end of the current line.
            pub fn move_line_end(&mut self) {}

            /// Move the cursor to the first non-whitespace character in the line.
            pub fn move_line_first_non_ws(&mut self) {}

            /// Return the display column for a position.
            pub fn column_for_position(&self, pos: TextPosition, tab_stop: usize) -> usize {}

            /// Return the closest position for a display column within a line.
            pub fn position_for_column(
                &self,
                line: usize,
                column: usize,
                tab_stop: usize,
            ) -> TextPosition {
            }

            /// Return the end position for a line, optionally including the newline.
            pub fn line_end_position(&self, line: usize, include_newline: bool) -> TextPosition {}

            /// Return the start position for a line.
            pub fn line_start_position(&self, line: usize) -> TextPosition {}

            /// Return the text in a range.
            pub fn range_text(&self, range: TextRange) -> String {}

            /// Return the text in a range, or `None` if either endpoint is out of bounds.
            pub fn try_range_text(&self, range: TextRange) -> Option<String> {}

            /// Convert a text position to a rope char index, or `None` if it is out of bounds.
            pub fn try_position_to_char(&self, pos: TextPosition) -> Option<usize> {}
        }

        /// A position in the text buffer expressed as a logical line and a char index.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Hash, Ord, PartialOrd)]
        pub struct TextPosition {
            /// Logical line index (0-based).
            pub line: usize,
            /// Char index within the line (0-based).
            pub column: usize,
        }

        impl TextPosition {
            /// Create a new text position.
            pub fn new(line: usize, column: usize) -> Self {}
        }

        /// A half-open text range expressed in buffer coordinates.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Hash)]
        pub struct TextRange {
            /// Range start position (inclusive).
            pub start: TextPosition,
            /// Range end position (exclusive).
            pub end: TextPosition,
        }

        impl TextRange {
            /// Construct a text range.
            pub fn new(start: TextPosition, end: TextPosition) -> Self {}

            /// Return a range with start/end ordered.
            pub fn normalized(self) -> Self {}

            /// Return true if the range is empty.
            pub fn is_empty(self) -> bool {}

            /// Return the range start and end ordered.
            pub fn ordered(self) -> (TextPosition, TextPosition) {}
        }

        /// A text selection expressed as an anchor and head position.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub struct Selection {}

        impl Selection {
            /// Construct a collapsed selection at a position.
            pub fn caret(position: TextPosition) -> Self {}

            /// Construct a selection from anchor and head positions.
            pub fn new(anchor: TextPosition, head: TextPosition) -> Self {}

            /// Return the anchor position.
            pub fn anchor(self) -> TextPosition {}

            /// Return the head position.
            pub fn head(self) -> TextPosition {}

            /// Update the head position.
            pub fn set_head(&mut self, head: TextPosition) {}

            /// Update the anchor position.
            pub fn set_anchor(&mut self, anchor: TextPosition) {}

            /// Collapse the selection to a caret at the head position.
            pub fn collapse_to_head(&mut self) {}

            /// Return the selection range as a normalized text range.
            pub fn range(self) -> TextRange {}

            /// Return true if the selection is empty.
            pub fn is_empty(self) -> bool {}
        }

        /// Compute tab expansion width for a column.
        pub fn tab_width(column: usize, tab_stop: usize) -> usize {}

        /// Editor widget implementation.
        pub struct Editor {}

        impl Editor {
            /// Construct an editor with default configuration.
            pub fn new(text: impl Into<String>) -> Self {}

            /// Construct an editor with a configuration.
            pub fn with_config(text: impl Into<String>, config: EditorConfig) -> Self {}

            /// Return the current editor configuration.
            pub fn config(&self) -> &EditorConfig {}

            /// Replace the editor configuration.
            pub fn set_config(&mut self, config: EditorConfig) {}

            /// Return the buffer contents.
            pub fn text(&self) -> String {}

            /// Replace the buffer contents.
            pub fn set_text(&mut self, text: impl Into<String>) {}

            /// Return the current selection.
            pub fn selection(&self) -> Selection {}

            /// Install a syntax highlighter.
            pub fn set_highlighter(&mut self, highlighter: Option<Box<dyn Highlighter>>) {}

            /// Move the cursor.
            /// @param dir The direction to move the cursor.
            pub fn cursor(&mut self, ctx: &mut dyn Context, dir: Direction) {}

            /// Undo the last edit.
            pub fn undo(&mut self, _ctx: &mut dyn Context) {}

            /// Redo the last undone edit.
            pub fn redo(&mut self, _ctx: &mut dyn Context) {}

            /// Return a typed command reference for this command.
            pub fn cmd_cursor() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_undo() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_redo() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for Editor {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Editor {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn cursor(&self) -> Option<cursor::Cursor> {}

            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

            fn measure(&self, c: MeasureConstraints) -> Measurement {}

            fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

            fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

            fn name(&self) -> NodeName {}
        }

        /// Wrapping behavior for the editor.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub enum WrapMode {
            /// No wrapping; horizontal scrolling is enabled.
            None,
            /// Soft wrapping at the view width.
            Soft,
        }

        /// Editing mode for the editor widget.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub enum EditMode {
            /// Text entry mode with direct insertion.
            Text,
            /// Vi-style modal editing.
            Vi,
        }

        /// Line number rendering mode.
        #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
        pub enum LineNumbers {
            /// Do not render line numbers.
            None,
            /// Render absolute line numbers.
            Absolute,
            /// Render relative line numbers (current line stays absolute).
            Relative,
        }

        /// Configuration for the editor widget.
        #[derive(Debug, Clone, Default)]
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

        impl EditorConfig {
            /// Construct a default editor configuration.
            pub fn new() -> Self {}

            /// Configure multiline behavior.
            pub fn with_multiline(self, multiline: bool) -> Self {}

            /// Configure wrapping mode.
            pub fn with_wrap(self, wrap: WrapMode) -> Self {}

            /// Configure auto-grow behavior.
            pub fn with_auto_grow(self, auto_grow: bool) -> Self {}

            /// Configure the minimum height.
            pub fn with_min_height(self, min_height: u32) -> Self {}

            /// Configure the maximum height.
            pub fn with_max_height(self, max_height: Option<u32>) -> Self {}

            /// Configure the edit mode.
            pub fn with_mode(self, mode: EditMode) -> Self {}

            /// Configure read-only behavior.
            pub fn with_read_only(self, read_only: bool) -> Self {}

            /// Configure line number rendering.
            pub fn with_line_numbers(self, line_numbers: LineNumbers) -> Self {}

            /// Configure the tab stop width.
            pub fn with_tab_stop(self, tab_stop: usize) -> Self {}
        }
    }

    pub mod help {
        //! Experimental contextual help modal internals.
        //! Contextual help modal widget.
        //!
        //! Displays bindings and commands available from the current focus context.

        pub use canopy::help::BindingKind;
        pub use canopy::help::OwnedHelpBinding;
        pub use canopy::help::OwnedHelpSnapshot;
        /// Help modal widget displaying contextual bindings and commands.
        #[derive(Default)]
        pub struct Help {}

        impl Help {
            /// Create a new Help widget.
            pub fn new() -> Self {}

            /// Set the help snapshot to display.
            pub fn set_snapshot(&mut self, snapshot: OwnedHelpSnapshot) {}

            /// Clear the stored snapshot.
            pub fn clear_snapshot(&mut self) {}

            /// Get the current snapshot, if any.
            pub fn snapshot(&self) -> Option<&OwnedHelpSnapshot> {}

            /// Set the snapshot on the HelpContent child widget.
            pub fn set_content_snapshot(
                c: &mut dyn Context,
                snapshot: OwnedHelpSnapshot,
            ) -> Result<()> {
            }

            /// Build the help subtree and return its node id.
            pub fn install(context: &mut dyn Context) -> Result<NodeId> {}
        }

        impl CommandNode for Help {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Help {
            fn render(&mut self, r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }

        impl Loader for Help {
            fn load(c: &mut Canopy) -> Result<()> {}
        }

        /// Content widget for the help modal that displays bindings and commands.
        #[derive(Default)]
        pub struct HelpContent {}

        impl HelpContent {
            /// Create a new help content widget.
            pub fn new() -> Self {}

            /// Set the help snapshot to display.
            pub fn set_snapshot(&mut self, snapshot: OwnedHelpSnapshot) {}

            /// Scroll up by one line.
            pub fn scroll_up(&self, c: &mut dyn Context) {}

            /// Scroll down by one line.
            pub fn scroll_down(&self, c: &mut dyn Context) {}

            /// Scroll to the top.
            pub fn scroll_to_top(&self, c: &mut dyn Context) {}

            /// Scroll to the bottom.
            pub fn scroll_to_bottom(&self, c: &mut dyn Context) {}

            /// Page down by one screen.
            pub fn page_down(&self, c: &mut dyn Context) {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_up() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_down() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_to_top() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_scroll_to_bottom() -> &'static canopy::commands::CommandSpec {}

            /// Return a typed command reference for this command.
            pub fn cmd_page_down() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for HelpContent {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for HelpContent {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

            fn layout(&self) -> Layout {}

            fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

            fn on_event(&mut self, _event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }
    }

    pub mod inspector {
        //! Experimental inspector overlay internals.

        /// Inspector overlay widget.
        #[derive(Default)]
        pub struct Inspector;

        impl Inspector {
            /// Construct a new inspector.
            pub fn new() -> Self {}

            /// Build the inspector subtree and return its node id.
            pub fn install(context: &mut dyn Context) -> Result<NodeId> {}
        }

        impl CommandNode for Inspector {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Inspector {
            fn render(&mut self, r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }

        impl Loader for Inspector {
            fn load(c: &mut Canopy) -> Result<()> {}
        }
    }

    pub mod tabs {
        //! Experimental tab container API.

        /// A tab control managing a set of nodes with titles.
        pub struct Tabs {}

        impl Tabs {
            /// Construct tabs with the provided titles.
            pub fn new<I>(tabs: I) -> Self
            where
                I: IntoIterator,
                I::Item: AsRef<str>, {
            }

            /// Select a tab by signed offset.
            /// @param delta Signed tab delta. Positive moves forward and negative moves backward.
            pub fn select_by(&mut self, _c: &mut dyn Context, delta: i32) {}

            /// Return a typed command reference for this command.
            pub fn cmd_select_by() -> &'static canopy::commands::CommandSpec {}
        }

        impl CommandNode for Tabs {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
        }

        impl Widget for Tabs {
            fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

            fn name(&self) -> NodeName {}
        }
    }

    /// A simple box container around its children.
    #[derive(Default)]
    pub struct Box {}

    impl Box {
        /// Construct a box.
        pub fn new() -> Self {}

        /// Build a box with a specified glyph set.
        pub fn with_glyphs(self, glyphs: BoxGlyphs) -> Self {}

        /// Build a box with a specified border style name.
        pub fn with_border_style(self, style: impl Into<String>) -> Self {}

        /// Update the border style name.
        pub fn set_border_style(&mut self, style: impl Into<String>) {}

        /// Enable interior fill using the default fill style name.
        pub fn with_fill(self) -> Self {}

        /// Enable interior fill using a specified style name.
        pub fn with_fill_style(self, style: impl Into<String>) -> Self {}
    }

    impl CommandNode for Box {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Box {
        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn layout(&self) -> Layout {}

        fn name(&self) -> NodeName {}
    }

    /// Defines the set of glyphs used to draw the box.
    #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq)]
    pub struct BoxGlyphs {
        /// Top-left corner glyph.
        pub topleft: char,
        /// Top-right corner glyph.
        pub topright: char,
        /// Bottom-left corner glyph.
        pub bottomleft: char,
        /// Bottom-right corner glyph.
        pub bottomright: char,
        /// Horizontal border glyph.
        pub horizontal: char,
        /// Vertical border glyph.
        pub vertical: char,
    }

    /// Double line Unicode box drawing set.
    pub const DOUBLE: BoxGlyphs = _;

    /// Round corner thin Unicode box drawing set.
    pub const ROUND: BoxGlyphs = _;

    /// Round corner thick Unicode box drawing set.
    pub const ROUND_THICK: BoxGlyphs = _;

    /// Single line thin Unicode box drawing set.
    pub const SINGLE: BoxGlyphs = _;

    /// Single line thick Unicode box drawing set.
    pub const SINGLE_THICK: BoxGlyphs = _;

    /// Button widget that triggers a command when clicked.
    #[derive(Default)]
    pub struct Button {}

    impl Button {
        /// Construct a new button with a label.
        pub fn new(label: impl Into<String>) -> Self {}

        /// Build a button with a specified glyph set.
        pub fn with_glyphs(self, glyphs: BoxGlyphs) -> Self {}

        /// Build a button that dispatches a command when clicked.
        pub fn with_command(self, command: CommandCall) -> Self {}

        /// Build a button with an active state.
        pub fn with_active(self, active: bool) -> Self {}

        /// Return the button label.
        pub fn label(&self) -> &str {}

        /// Set whether the button is active.
        pub fn set_active(&mut self, active: bool) {}

        /// Replace the button label.
        pub fn set_label(&mut self, ctx: &mut dyn Context, label: impl Into<String>) -> Result<()> {
        }

        /// Trigger the button action.
        pub fn press(&mut self, ctx: &mut dyn Context) -> Result<()> {}

        /// Return a typed command reference for this command.
        pub fn cmd_press() -> &'static canopy::commands::CommandSpec {}
    }

    impl Selectable for Button {
        fn set_selected(&mut self, selected: bool) {}
    }

    impl CommandNode for Button {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Button {
        fn layout(&self) -> Layout {}

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

        fn render(&mut self, rndr: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

        fn name(&self) -> NodeName {}
    }

    /// Container that centers its child within available space.
    #[derive(Default)]
    pub struct Center;

    impl Center {
        /// Create a new Center widget.
        pub fn new() -> Self {}
    }

    impl CommandNode for Center {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Center {
        fn layout(&self) -> Layout {}

        fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

        fn name(&self) -> NodeName {}
    }

    /// A dropdown widget for single-value selection.
    ///
    /// When collapsed, displays the currently selected item with a dropdown indicator.
    /// When expanded, displays all options for selection.
    pub struct Dropdown<T>
    where
        T: DropdownItem, {}

    impl<T> Dropdown<T>
    where
        T: DropdownItem + 'static,
    {
        /// Create a new dropdown with the given items.
        ///
        /// Panics if items is empty.
        pub fn new(items: Vec<T>) -> Self {}

        /// Get the currently selected item.
        pub fn selected(&self) -> &T {}

        /// Get the currently selected index.
        pub fn selected_index(&self) -> usize {}

        /// Set the selected index.
        pub fn set_selected(&mut self, index: usize) {}

        /// Check if the dropdown is expanded.
        pub fn is_expanded(&self) -> bool {}

        /// Toggle the dropdown expanded state.
        pub fn toggle(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Move highlight by a signed offset (when expanded).
        pub fn select_by(&mut self, _c: &mut dyn Context, delta: i32) -> Result<()> {}

        /// Confirm the highlighted selection and collapse.
        pub fn confirm(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Collapse without changing selection.
        pub fn cancel(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Get the number of items.
        pub fn len(&self) -> usize {}

        /// Check if the dropdown is empty.
        pub fn is_empty(&self) -> bool {}

        /// Return a typed command reference for this command.
        pub fn cmd_toggle() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_by() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_confirm() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_cancel() -> &'static canopy::commands::CommandSpec {}
    }

    impl<T> CommandNode for Dropdown<T>
    where
        T: DropdownItem + 'static,
    {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl<T> Widget for Dropdown<T>
    where
        T: DropdownItem + Send + 'static,
    {
        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn canvas(&self, _view: Size<u32>, _ctx: &canopy::layout::CanvasContext<'_>) -> Size<u32> {}

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        fn name(&self) -> NodeName {}
    }

    /// Trait for items that can be displayed in a Dropdown.
    pub trait DropdownItem {
        /// Return the display label for this item.
        fn label(&self) -> &str;
    }

    /// Errors emitted by canopy-widgets helpers.
    #[derive(Debug, Error, Display)]
    pub enum Error {
        /// Font parsing failed.
        FontLoad(&'static str),
        /// Glyph ramp did not include any characters.
        EmptyGlyphRamp,
        /// Font format is not supported.
        UnsupportedFormat(&'static str),
        /// I/O error while reading font bytes.
        Io(std::io::Error),
    }

    impl From<Error> for Error {
        fn from(source: IoError) -> Self {}
    }

    /// Result type for canopy-widgets helpers.
    pub type Result<T> = std::result::Result<T, Error>;

    /// Rasterized font data for terminal rendering.
    #[derive(Clone, FromStr)]
    pub struct Font {}

    impl Font {
        /// Load a font from in-memory bytes.
        pub fn from_bytes(data: impl AsRef<[u8]>) -> Result<Self> {}

        /// Load a font from a reader.
        pub fn from_reader(reader: impl Read) -> Result<Self> {}

        /// Parse an ASCII-art font payload.
        pub fn from_ascii_art(_contents: &str) -> Result<Self> {}

        /// Adjust spacing added after each glyph.
        pub fn with_spacing(self, spacing: f32) -> Self {}

        /// Return the font name, if provided in metadata.
        pub fn name(&self) -> Option<&str> {}
    }

    /// A rendered font cell with coverage weights for foreground and background.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq)]
    pub struct FontCell {
        /// Rendered character for this cell.
        pub ch: char,
        /// Foreground coverage weight (0-255).
        pub fg_coverage: u8,
        /// Background coverage weight (0-255).
        pub bg_coverage: u8,
    }

    /// Rendering effects applied to font output.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct FontEffects {
        /// Thicken strokes by adding extra coverage.
        pub bold: bool,
        /// Slant glyphs to the right.
        pub italic: bool,
        /// Draw an underline through the glyphs.
        pub underline: bool,
        /// Reduce contrast by dimming coverage.
        pub dim: bool,
        /// Draw an overline through the glyphs.
        pub overline: bool,
        /// Draw a strike-through line through the glyphs.
        pub strike: bool,
    }

    /// Cached layout for rasterized font text.
    #[derive(Debug, Clone)]
    pub struct FontLayout {
        /// Target canvas size.
        pub size: canopy::geom::Size,
        /// Size of the rendered content before clipping.
        pub content_size: canopy::geom::Size,
        /// Rendered cell data for each row.
        pub cells: Vec<Vec<FontCell>>,
    }

    /// Renderer that converts fonts into terminal text.
    pub struct FontRenderer {}

    impl FontRenderer {
        /// Create a renderer for the provided font.
        pub fn new(font: Font) -> Self {}

        /// Configure the glyph ramp for this renderer.
        pub fn with_ramp(self, ramp: GlyphRamp) -> Self {}

        /// Configure the fallback glyph used for missing characters.
        pub fn with_fallback(self, fallback: char) -> Self {}

        /// Render text into a layout that fits within the target canvas.
        pub fn layout(
            &mut self,
            text: &str,
            size: Size,
            options: LayoutOptions,
            effects: FontEffects,
        ) -> FontLayout {
        }
    }

    /// Glyph raster data rendered to pixel coverage.
    #[derive(Debug, Clone)]
    pub struct Glyph {
        /// Rasterized coverage mask, row-major, 0-255 per pixel.
        pub bitmap: Vec<u8>,
        /// Glyph width in pixels.
        pub width: u32,
        /// Glyph height in pixels.
        pub height: u32,
        /// Horizontal bearing to the left of the glyph origin, in pixels.
        pub bearing_left: i32,
        /// Horizontal bearing to the right of the glyph advance, in pixels.
        pub bearing_right: i32,
        /// Vertical bearing to the bottom of the glyph relative to the baseline, in pixels.
        pub bearing_bottom: i32,
        /// Horizontal advance width in pixels.
        pub advance: f32,
    }

    /// A glyph ramp used to convert coverage regions into terminal glyphs.
    #[derive(Debug, Clone)]
    pub struct GlyphRamp {}

    impl GlyphRamp {
        /// Default ASCII ramp.
        pub fn ascii() -> Self {}

        /// Nerd Font ramp using private-use glyphs.
        pub fn nerd_font() -> Self {}

        /// Block-element ramp that matches 2x2 quadrant coverage.
        pub fn blocks() -> Self {}

        /// Construct a ramp from a set of characters.
        pub fn from_chars(chars: impl AsRef<str>) -> Result<Self> {}

        /// Construct a ramp from explicit glyph characters.
        pub fn from_glyphs(glyphs: impl IntoIterator<Item = char>) -> Result<Self> {}
    }

    /// Alignment and overflow configuration for font layouts.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct LayoutOptions {
        /// Horizontal alignment within the target canvas.
        pub h_align: canopy::layout::Align,
        /// Vertical alignment within the target canvas.
        pub v_align: canopy::layout::Align,
        /// Overflow handling policy.
        pub overflow: OverflowPolicy,
    }

    /// Policy for handling content overflow.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
    pub enum OverflowPolicy {
        /// Clip glyphs that exceed the target bounds.
        Clip,
    }

    /// Render large ASCII-font text into a bounded region.
    pub struct FontBanner {}

    impl FontBanner {
        /// Construct a banner with text and a renderer.
        pub fn new(text: impl Into<String>, renderer: FontRenderer) -> Self {}

        /// Update the banner text.
        pub fn set_text(&mut self, text: impl Into<String>) {}

        /// Update the banner renderer.
        pub fn set_renderer(&mut self, renderer: FontRenderer) {}

        /// Configure the banner style path.
        pub fn with_style(self, style: impl Into<String>) -> Self {}

        /// Configure the banner style when selected.
        pub fn with_selected_style(self, style: impl Into<String>) -> Self {}

        /// Configure layout options for the banner.
        pub fn with_layout_options(self, options: LayoutOptions) -> Self {}

        /// Configure rendering effects for the banner.
        pub fn with_effects(self, effects: FontEffects) -> Self {}

        /// Update rendering effects for the banner.
        pub fn set_effects(&mut self, effects: FontEffects) {}
    }

    impl Selectable for FontBanner {
        fn set_selected(&mut self, selected: bool) {}
    }

    impl Widget for FontBanner {
        fn layout(&self) -> Layout {}

        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}
    }

    /// A frame around an element with optional title and indicators.
    #[derive(Default)]
    pub struct Frame {}

    impl Frame {
        /// Construct a frame.
        pub fn new() -> Self {}

        /// Build a frame with a specified glyph set.
        pub fn with_glyphs(self, glyphs: BoxGlyphs) -> Self {}

        /// Build a frame with a specified scroll glyph set.
        pub fn with_scroll_glyphs(self, glyphs: ScrollGlyphs) -> Self {}

        /// Build a frame with a specified title.
        pub fn with_title(self, title: impl Into<String>) -> Self {}

        /// Return the glyph set used by the frame.
        pub fn glyphs(&self) -> &BoxGlyphs {}

        /// Return the optional title string.
        pub fn title(&self) -> Option<&str> {}

        /// Wrap an existing child node in a new frame and return the frame node ID.
        pub fn wrap(c: &mut dyn Context, child: impl Into<NodeId>) -> Result<NodeId> {}

        /// Wrap an existing child node in a configured frame and return the frame node ID.
        pub fn wrap_with(
            c: &mut dyn Context,
            child: impl Into<NodeId>,
            frame: Self,
        ) -> Result<NodeId> {
        }
    }

    impl CommandNode for Frame {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Frame {
        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

        fn layout(&self) -> Layout {}

        fn name(&self) -> NodeName {}
    }

    /// Active scroll indicator glyph set.
    pub const SCROLL: ScrollGlyphs = _;

    /// Defines the set of glyphs used to draw active scroll indicators.
    pub struct ScrollGlyphs {
        /// Active vertical indicator glyph.
        pub vertical_active: char,
        /// Active horizontal indicator glyph.
        pub horizontal_active: char,
    }

    /// Widget that renders an image into terminal cells.
    pub struct ImageView {}

    impl ImageView {
        /// Create a new image view widget.
        pub fn new(image: &RgbaImage) -> Self {}

        /// Create a new image view widget from a file path.
        pub fn from_path(path: impl AsRef<Path>) -> canopy_error::Result<Self> {}

        /// Configure whether the image auto-fits to the view.
        pub fn with_auto_fit(self, auto_fit: bool) -> Self {}

        /// Zoom around the view center.
        /// @param dir The zoom direction.
        pub fn zoom(
            &mut self,
            ctx: &mut dyn Context,
            dir: ZoomDirection,
        ) -> canopy_error::Result<()> {
        }

        /// Pan by one step in the specified direction.
        /// @param dir The pan direction.
        pub fn pan(&mut self, ctx: &mut dyn Context, dir: Direction) -> canopy_error::Result<()> {}

        /// Return a typed command reference for this command.
        pub fn cmd_zoom() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_pan() -> &'static canopy::commands::CommandSpec {}
    }

    impl CommandNode for ImageView {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for ImageView {
        /// Fill the available space in the terminal view.
        fn layout(&self) -> Layout {}

        fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

        /// Render the current image view into the terminal buffer.
        fn render(
            &mut self,
            render: &mut Render<'_>,
            ctx: &dyn ViewContext,
        ) -> canopy_error::Result<()> {
        }

        /// Accept focus so key bindings apply to this widget.
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}
    }

    impl Loader for ImageView {
        /// Register commands for the image viewer widget.
        fn load(cnpy: &mut Canopy) -> canopy_error::Result<()> {}
    }

    /// Single-line text input widget.
    pub struct Input {}

    impl Input {
        /// Construct a new input with initial text.
        pub fn new(txt: impl Into<String>) -> Self {}

        /// Return the currently visible input slice.
        pub fn text(&self) -> &str {}

        /// Return the raw input value without padding.
        pub fn value(&self) -> &str {}

        /// Replace the input value and reset the cursor.
        pub fn set_value(&mut self, value: impl Into<String>) {}

        /// Return a typed command reference for this command.
        pub fn cmd_left() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_right() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_backspace() -> &'static canopy::commands::CommandSpec {}
    }

    impl CommandNode for Input {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Input {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        fn cursor(&self) -> Option<cursor::Cursor> {}

        fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {}

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn name(&self) -> NodeName {}
    }

    /// A typed list container for widget items.
    ///
    /// List items are actual widgets in the tree, enabling composition and focus management.
    /// The list arranges items vertically and supports scrolling.
    ///
    /// Items must implement the [`Selectable`] trait so the list can manage their
    /// selection state independently of focus.
    #[derive(Default)]
    pub struct List<W: Selectable> {}

    impl<W: Selectable> List<W> {
        /// Construct an empty list.
        pub fn new() -> Self {}

        /// Build a list with a list-level selection indicator.
        /// Repeat controls whether the indicator renders on every visible line.
        pub fn with_selection_indicator(
            self,
            style: impl Into<String>,
            text: impl Into<String>,
            repeat: bool,
        ) -> Self {
        }

        /// Set a list-level selection indicator.
        /// Repeat controls whether the indicator renders on every visible line.
        pub fn set_selection_indicator(
            &mut self,
            style: impl Into<String>,
            text: impl Into<String>,
            repeat: bool,
        ) {
        }

        /// Clear the list-level selection indicator.
        pub fn clear_selection_indicator(&mut self) {}

        /// Build a list that dispatches a command when a row is activated.
        pub fn with_on_activate(self, command: CommandCall) -> Self {}

        /// Configure an activation command for row clicks.
        pub fn set_on_activate(&mut self, config: Option<ListActivateConfig>) {}

        /// Returns true if the list is empty.
        pub fn is_empty(&self) -> bool {}

        /// Returns the number of items in the list.
        pub fn len(&self) -> usize {}

        /// Returns the typed ID of the item at the given index.
        pub fn item(&self, index: usize) -> Option<TypedId<W>> {}

        /// Returns the currently selected index.
        pub fn selected_index(&self) -> Option<usize> {}

        /// Returns the typed ID of the currently selected item.
        pub fn selected_item(&self) -> Option<TypedId<W>> {}

        /// Append an item widget to the end of the list.
        pub fn append(&mut self, ctx: &mut dyn Context, widget: W) -> Result<TypedId<W>>
        where
            W: 'static, {
        }

        /// Insert an item widget at the specified index.
        pub fn insert(
            &mut self,
            ctx: &mut dyn Context,
            index: usize,
            widget: W,
        ) -> Result<TypedId<W>>
        where
            W: 'static, {
        }

        /// Remove the item at the specified index.
        pub fn remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<bool> {}

        /// Detach the item at the specified index.
        pub fn take(&mut self, ctx: &mut dyn Context, index: usize) -> Result<Option<TypedId<W>>> {}

        /// Clear all items from the list.
        pub fn clear(&mut self, ctx: &mut dyn Context) -> Result<()> {}

        /// Delete the currently selected item.
        pub fn delete_selected(&mut self, ctx: &mut dyn Context) -> Result<bool> {}

        /// Select an item at the given index.
        pub fn select(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {}

        /// Move selection to the first item.
        pub fn select_first(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Move selection to the last item.
        pub fn select_last(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Move selection by a signed offset.
        pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {}

        /// Scroll the view by one line in the specified direction.
        /// @param dir The direction to scroll.
        pub fn scroll(&mut self, c: &mut dyn Context, dir: Direction) {}

        /// Move selection by pages.
        /// Positive values move down; negative values move up.
        /// @param delta Signed page delta. Positive moves down and negative moves up.
        pub fn page(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {}

        /// Return a typed command reference for this command.
        pub fn cmd_clear() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_delete_selected() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_first() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_last() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_by() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_scroll() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_page() -> &'static canopy::commands::CommandSpec {}
    }

    impl<W: Selectable> CommandNode for List<W> {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl<W: Selectable + Send + 'static> Widget for List<W> {
        fn layout(&self) -> Layout {}

        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn canvas(&self, view: Size<u32>, ctx: &CanvasContext<'_>) -> Size<u32> {}

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        fn name(&self) -> NodeName {}
    }

    /// Activation configuration for list row clicks.
    #[derive(Debug, Clone)]
    pub struct ListActivateConfig {}

    impl ListActivateConfig {
        /// Build a new activation config using the default drag threshold.
        pub fn new(command: CommandCall) -> Self {}

        /// Set the drag threshold in cells.
        pub fn with_drag_threshold(self, drag_threshold: u32) -> Self {}
    }

    /// Trait for widgets that can be selected in a list.
    ///
    /// Items in a `List` must implement this trait so the list can manage
    /// their selection state. Selection is independent of focus - an item
    /// remains selected even when the list loses focus.
    pub trait Selectable: Widget {
        /// Set the selection state of this item.
        fn set_selected(&mut self, selected: bool);
    }

    /// A modal container that centers its content.
    ///
    /// For the dimming effect, the parent should push an effect on the background content
    /// using `c.push_effect(background_id, effects::brightness(0.5))`. The Modal itself renders
    /// at full brightness since it's a sibling to the dimmed content, not a descendant.
    ///
    /// This widget is typically inserted as a sibling to the background content inside
    /// a parent configured with `Stack` layout so it can overlay the existing view.
    #[derive(Default)]
    pub struct Modal;

    impl Modal {
        /// Create a new Modal widget.
        pub fn new() -> Self {}
    }

    impl CommandNode for Modal {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Modal {
        fn layout(&self) -> Layout {}

        fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

        fn name(&self) -> NodeName {}
    }

    /// Container that adds padding around its child.
    #[derive(Default)]
    pub struct Pad {}

    impl Pad {
        /// Create a pad with the provided edge padding.
        pub fn new(padding: Edges<u32>) -> Self {}

        /// Create a pad with uniform padding on all sides.
        pub fn uniform(padding: u32) -> Self {}

        /// Wrap an existing child node in a new pad and return the pad node ID.
        pub fn wrap(
            c: &mut dyn Context,
            child: impl Into<NodeId>,
            padding: Edges<u32>,
        ) -> Result<NodeId> {
        }

        /// Wrap an existing child node in a configured pad and return the pad node ID.
        pub fn wrap_with(
            c: &mut dyn Context,
            child: impl Into<NodeId>,
            pad: Self,
        ) -> Result<NodeId> {
        }
    }

    impl CommandNode for Pad {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Pad {
        fn layout(&self) -> Layout {}

        fn render(&mut self, _r: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {}

        fn name(&self) -> NodeName {}
    }

    /// Panes manages a set of child nodes arranged in a 2d grid.
    #[derive(Default)]
    pub struct Panes {}

    impl Panes {
        /// Construct panes with no children.
        pub fn new() -> Self {}

        /// Construct panes with a single child.
        pub fn with_child(child: impl Into<NodeId>) -> Self {}

        /// Return the active column container node IDs in order.
        pub fn column_nodes(&self) -> Vec<NodeId> {}

        /// Return the focused column index, if any.
        pub fn focused_column_index(&self, c: &dyn Context) -> Option<usize> {}

        /// Move focus by a signed column offset (wraps around).
        pub fn focus_column(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {}

        /// Get the offset of the current focus in the children vector.
        pub fn focus_coords(&self, c: &dyn Context) -> Option<(usize, usize)> {}

        /// Delete the focus node. If a column ends up empty, it is removed.
        pub fn delete_focus(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Insert a node, splitting vertically.
        pub fn insert_row(&mut self, c: &mut dyn Context, n: impl Into<NodeId>) -> Result<()> {}

        /// Insert a node in a new column.
        pub fn insert_col(&mut self, c: &mut dyn Context, n: impl Into<NodeId>) -> Result<()> {}

        /// Return a typed command reference for this command.
        pub fn cmd_focus_column() -> &'static canopy::commands::CommandSpec {}
    }

    impl CommandNode for Panes {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Panes {
        fn render(
            &mut self,
            _rndr: &mut canopy::render::Render<'_>,
            _ctx: &dyn ViewContext,
        ) -> Result<()> {
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {}

        fn name(&self) -> NodeName {}
    }

    /// A Root widget that lives at the base of a Canopy app.
    #[derive(Default)]
    pub struct Root {}

    impl Root {
        /// Construct a root widget wrapping the application and inspector nodes.
        pub fn new() -> Self {}

        /// Start with the inspector open.
        pub fn with_inspector(self, state: bool) -> Self {}

        /// Exit from the program, restoring terminal state. If help or inspector is
        /// open, close them first.
        pub fn quit(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Dump diagnostic information about the tree, focus, and bindings.
        pub fn dump_diagnostics(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Move focus in the specified direction.
        /// @param direction The direction to move focus.
        pub fn focus(&mut self, c: &mut dyn Context, direction: FocusDirection) -> Result<()> {}

        /// Hide the inspector.
        pub fn hide_inspector(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Show the inspector.
        pub fn activate_inspector(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Toggle inspector visibility.
        pub fn toggle_inspector(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// If we're currently focused in the inspector, shift focus into the app pane instead.
        pub fn focus_app(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Show the help modal with contextual bindings and commands.
        pub fn show_help(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Hide the help modal.
        pub fn hide_help(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Toggle help modal visibility.
        pub fn toggle_help(&mut self, c: &mut dyn Context) -> Result<()> {}

        /// Helper to install a root widget into a canopy app.
        pub fn install_app<W>(canopy: &mut Canopy, app: W) -> Result<TypedId<W>>
        where
            W: Widget + 'static, {
        }

        /// Helper to install a root widget into the canopy with an optional inspector pane.
        pub fn install_app_with_inspector<W>(
            canopy: &mut Canopy,
            app: W,
            inspector_active: bool,
        ) -> Result<TypedId<W>>
        where
            W: Widget + 'static, {
        }

        /// Helper to install a root widget and configure children.
        pub fn install(canopy: &mut Canopy, app: impl Into<NodeId>) -> Result<NodeId> {}

        /// Helper to install a root widget with an optional inspector pane.
        pub fn install_with_inspector(
            canopy: &mut Canopy,
            app: impl Into<NodeId>,
            inspector_active: bool,
        ) -> Result<NodeId> {
        }

        /// Return a typed command reference for this command.
        pub fn cmd_quit() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_dump_diagnostics() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_focus() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_hide_inspector() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_activate_inspector() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_toggle_inspector() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_focus_app() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_show_help() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_hide_help() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_toggle_help() -> &'static canopy::commands::CommandSpec {}
    }

    impl CommandNode for Root {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Root {
        fn render(
            &mut self,
            _rndr: &mut canopy::render::Render<'_>,
            _ctx: &dyn ViewContext,
        ) -> Result<()> {
        }

        fn layout(&self) -> Layout {}

        fn name(&self) -> NodeName {}
    }

    impl Loader for Root {
        fn load(c: &mut Canopy) -> Result<()> {}
    }

    /// A multi-select widget with checkbox-style items.
    ///
    /// Items can be toggled on/off independently. The selected indices are tracked
    /// in the order they were selected, allowing for ordered selection if needed.
    pub struct Selector<T>
    where
        T: SelectorItem, {}

    impl<T> Selector<T>
    where
        T: SelectorItem + 'static,
    {
        /// Create a new selector with the given items.
        pub fn new(items: Vec<T>) -> Self {}

        /// Get the selected indices in selection order.
        pub fn selected_indices(&self) -> &[usize] {}

        /// Get references to the selected items in selection order.
        pub fn selected_items(&self) -> Vec<&T> {}

        /// Check if an index is selected.
        pub fn is_selected(&self, index: usize) -> bool {}

        /// Get the currently focused index.
        pub fn focused_index(&self) -> usize {}

        /// Toggle selection of the focused item.
        pub fn toggle(&mut self, _c: &mut dyn Context) -> Result<()> {}

        /// Move focus by a signed offset.
        pub fn select_by(&mut self, _c: &mut dyn Context, delta: i32) -> Result<()> {}

        /// Move focus to the first item.
        pub fn select_first(&mut self, _c: &mut dyn Context) -> Result<()> {}

        /// Move focus to the last item.
        pub fn select_last(&mut self, _c: &mut dyn Context) -> Result<()> {}

        /// Clear all selections.
        pub fn clear(&mut self, _c: &mut dyn Context) -> Result<()> {}

        /// Select all items.
        pub fn select_all(&mut self, _c: &mut dyn Context) -> Result<()> {}

        /// Get the number of items.
        pub fn len(&self) -> usize {}

        /// Check if the selector is empty.
        pub fn is_empty(&self) -> bool {}

        /// Get the number of selected items.
        pub fn selected_count(&self) -> usize {}

        /// Return a typed command reference for this command.
        pub fn cmd_toggle() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_by() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_first() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_last() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_clear() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_select_all() -> &'static canopy::commands::CommandSpec {}
    }

    impl<T> CommandNode for Selector<T>
    where
        T: SelectorItem + 'static,
    {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl<T> Widget for Selector<T>
    where
        T: SelectorItem + Send + 'static,
    {
        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {}

        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn canvas(&self, _view: Size<u32>, _ctx: &canopy::layout::CanvasContext<'_>) -> Size<u32> {}

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        fn name(&self) -> NodeName {}
    }

    /// Trait for items that can be displayed in a Selector.
    pub trait SelectorItem {
        /// Return the display label for this item.
        fn label(&self) -> &str;
    }

    /// Terminal widget backed by `itty`.
    pub struct Terminal {}

    impl Terminal {
        /// Construct a new terminal widget with the provided configuration.
        pub fn new(config: TerminalConfig) -> Self {}

        /// Return the exit status of the child process, if it has exited.
        pub fn exit_status(&self) -> Option<ExitStatus> {}

        /// Return true if the child process is still running.
        pub fn is_running(&self) -> bool {}

        /// Return the most recent terminal title, if any.
        pub fn title(&self) -> Option<String> {}

        /// Return the attached `itty` driver handle for scripting integrations.
        pub fn driver_handle(&self) -> Option<Arc<DriverHandle>> {}
    }

    impl CommandNode for Terminal {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Terminal {
        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn on_event(
            &mut self,
            event: &event::Event,
            ctx: &mut dyn Context,
        ) -> Result<EventOutcome> {
        }

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext<'_>) -> Size<u32> {}

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {}

        fn cursor(&self) -> Option<cursor::Cursor> {}

        fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {}

        fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {}

        fn name(&self) -> NodeName {}
    }

    /// Terminal color palette.
    #[derive(Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct TerminalColors {
        /// ANSI black (0).
        pub black: canopy::style::Color,
        /// ANSI red (1).
        pub red: canopy::style::Color,
        /// ANSI green (2).
        pub green: canopy::style::Color,
        /// ANSI yellow (3).
        pub yellow: canopy::style::Color,
        /// ANSI blue (4).
        pub blue: canopy::style::Color,
        /// ANSI magenta (5).
        pub magenta: canopy::style::Color,
        /// ANSI cyan (6).
        pub cyan: canopy::style::Color,
        /// ANSI white (7).
        pub white: canopy::style::Color,
        /// ANSI bright black (8).
        pub bright_black: canopy::style::Color,
        /// ANSI bright red (9).
        pub bright_red: canopy::style::Color,
        /// ANSI bright green (10).
        pub bright_green: canopy::style::Color,
        /// ANSI bright yellow (11).
        pub bright_yellow: canopy::style::Color,
        /// ANSI bright blue (12).
        pub bright_blue: canopy::style::Color,
        /// ANSI bright magenta (13).
        pub bright_magenta: canopy::style::Color,
        /// ANSI bright cyan (14).
        pub bright_cyan: canopy::style::Color,
        /// ANSI bright white (15).
        pub bright_white: canopy::style::Color,
        /// Default foreground color.
        pub foreground: canopy::style::Color,
        /// Default background color.
        pub background: canopy::style::Color,
        /// Cursor color.
        pub cursor: canopy::style::Color,
    }

    /// Terminal widget configuration.
    #[derive(Default)]
    pub struct TerminalConfig {}

    impl TerminalConfig {
        /// Construct a default terminal configuration.
        pub fn new() -> Self {}

        /// Configure the command argv to run instead of the default shell.
        pub fn with_command<I, S>(self, command: I) -> Self
        where
            I: IntoIterator<Item = S>,
            S: Into<String>, {
        }

        /// Configure the working directory for the terminal process.
        pub fn with_cwd(self, cwd: impl Into<PathBuf>) -> Self {}

        /// Add an environment variable for the terminal process.
        pub fn with_env(self, key: impl Into<String>, value: impl Into<String>) -> Self {}

        /// Configure the number of scrollback lines to keep.
        pub fn with_scrollback_lines(self, scrollback_lines: usize) -> Self {}

        /// Configure terminal mouse reporting.
        pub fn with_mouse_reporting(self, mouse_reporting: bool) -> Self {}

        /// Configure bracketed paste support.
        pub fn with_bracketed_paste(self, bracketed_paste: bool) -> Self {}

        /// Configure kitty keyboard protocol support.
        pub fn with_kitty_keyboard(self, kitty_keyboard: bool) -> Self {}

        /// Configure the terminal color palette.
        pub fn with_colors(self, colors: TerminalColors) -> Self {}

        /// Configure the clipboard store callback.
        pub fn with_clipboard_store<F>(self, store: F) -> Self
        where
            F: Fn(String) + Send + Sync + 'static, {
        }

        /// Configure the clipboard load callback.
        pub fn with_clipboard_load<F>(self, load: F) -> Self
        where
            F: Fn() -> String + Send + Sync + 'static, {
        }

        /// Configure the child exit callback.
        pub fn with_on_exit<F>(self, on_exit: F) -> Self
        where
            F: Fn(ExitStatus) + Send + Sync + 'static, {
        }
    }

    /// Canvas width behavior for text widgets.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq)]
    pub enum CanvasWidth {
        /// Match the view width.
        View,
        /// Use the maximum wrapped line width.
        Intrinsic,
        /// Use a fixed canvas width.
        Fixed(u32),
    }

    /// Multiline text widget with wrapping and scrolling.
    pub struct Text {}

    impl Text {
        /// Construct a text widget with raw content.
        pub fn new(raw: impl Into<String>) -> Self {}

        /// Add a fixed width for wrapping.
        pub fn with_wrap_width(self, width: u32) -> Self {}

        /// Configure the canvas width behavior.
        pub fn with_canvas_width(self, width: CanvasWidth) -> Self {}

        /// Set the text rendering style.
        pub fn with_style(self, style: impl Into<String>) -> Self {}

        /// Set the text rendering style when selected.
        pub fn with_selected_style(self, style: impl Into<String>) -> Self {}

        /// Set the tab stop width for tab expansion.
        pub fn with_tab_stop(self, tab_stop: usize) -> Self {}

        /// Return the raw text content.
        pub fn raw(&self) -> &str {}

        /// Replace the raw text content.
        pub fn set_raw(&mut self, raw: impl Into<String>) {}

        /// Scroll to an absolute content position.
        pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {}

        /// Scroll by one line in the specified direction.
        /// @param dir The direction to scroll.
        pub fn scroll(&mut self, c: &mut dyn Context, dir: Direction) {}

        /// Page vertically through the text.
        /// Positive values move down; negative values move up.
        /// @param delta Signed page delta. Positive moves down and negative moves up.
        pub fn page(&mut self, c: &mut dyn Context, delta: i32) {}

        /// Return a typed command reference for this command.
        pub fn cmd_scroll_to() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_scroll() -> &'static canopy::commands::CommandSpec {}

        /// Return a typed command reference for this command.
        pub fn cmd_page() -> &'static canopy::commands::CommandSpec {}
    }

    impl Selectable for Text {
        fn set_selected(&mut self, selected: bool) {}
    }

    impl CommandNode for Text {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for Text {
        fn render(&mut self, rndr: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {}

        fn measure(&self, c: MeasureConstraints) -> Measurement {}

        fn canvas(&self, view: Size<u32>, _ctx: &canopy::layout::CanvasContext<'_>) -> Size<u32> {}

        fn name(&self) -> NodeName {}
    }

    /// A vertical stack that arranges children with fixed or flex heights.
    #[derive(Default)]
    pub struct VStack {}

    impl VStack {
        /// Construct an empty vertical stack.
        pub fn new() -> Self {}

        /// Add a flex row with a weight.
        pub fn push_flex(self, node: impl Into<NodeId>, weight: u32) -> Self {}

        /// Add a fixed-height row.
        pub fn push_fixed(self, node: impl Into<NodeId>, height: u32) -> Self {}
    }

    impl CommandNode for VStack {
        fn commands() -> &'static [&'static canopy::commands::CommandSpec] {}
    }

    impl Widget for VStack {
        fn layout(&self) -> Layout {}

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {}

        fn name(&self) -> NodeName {}
    }
}
