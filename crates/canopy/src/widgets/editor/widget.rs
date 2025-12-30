use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use unicode_segmentation::UnicodeSegmentation;

use super::{
    EditMode, EditorConfig, LineNumbers, WrapMode,
    highlight::{HighlightSpan, Highlighter},
    layout::{LayoutCache, WrapSegment, layout_line},
    search::{SearchDirection, SearchState, find_matches},
    vi::{PendingKey, RepeatableEdit, ViMode, ViState, VisualMode},
};
use crate::{
    Context, ViewContext, command,
    core::text,
    cursor, derive_commands,
    editor::{Selection, TextBuffer, TextPosition, TextRange, tab_width},
    error::Result,
    event::{Event, key, mouse},
    geom::{Line, Point, Rect},
    layout::{CanvasContext, Constraint, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
};

/// Maximum delay between clicks to count as multi-click selection.
const DOUBLE_CLICK_MS: u64 = 500;

/// Editor widget implementation.
pub struct Editor {
    /// Editor configuration.
    config: EditorConfig,
    /// Text buffer backing the editor.
    buffer: TextBuffer,
    /// Layout cache for wrapping and mapping.
    layout: LayoutCache,
    /// Preferred display column for vertical movement.
    preferred_column: usize,
    /// Vi mode state when enabled.
    vi: ViState,
    /// Yank register for vi operations.
    yank: String,
    /// Whether the yank register represents a full line range.
    yank_linewise: bool,
    /// Search state.
    search: SearchState,
    /// Prompt state for search and replace.
    prompt: Option<PromptState>,
    /// Mouse interaction state.
    mouse: MouseState,
    /// Optional syntax highlighter.
    highlighter: Option<Box<dyn Highlighter>>,
    /// Cached syntax highlight spans.
    highlight_cache: HighlightCache,
    /// Cached cursor position in display coordinates.
    cursor_point: Option<Point>,
    /// Whether a text-entry transaction is active.
    text_entry_transaction: bool,
}

/// Prompt modes for search and replace interactions.
#[derive(Debug, Clone)]
enum PromptState {
    /// Search query input.
    Search {
        /// Search direction.
        direction: SearchDirection,
        /// Current query text.
        query: String,
    },
    /// Replace query input.
    ReplaceQuery {
        /// Current query text.
        query: String,
    },
    /// Replace replacement input.
    ReplaceWith {
        /// Query text.
        query: String,
        /// Replacement text.
        replacement: String,
    },
    /// Confirm replacements one by one.
    ReplaceConfirm {
        /// Query text.
        query: String,
        /// Replacement text.
        replacement: String,
        /// Match list.
        matches: Vec<TextRange>,
        /// Current match index.
        index: usize,
        /// Whether to replace all remaining matches.
        replace_all: bool,
    },
}

/// Mouse selection tracking state.
#[derive(Debug, Clone)]
struct MouseState {
    /// Whether a drag selection is active.
    selecting: bool,
    /// Anchor position for the selection.
    anchor: Option<TextPosition>,
    /// Multi-click tracking state.
    click_state: Option<ClickState>,
}

/// Multi-click tracking details.
#[derive(Debug, Clone)]
struct ClickState {
    /// Last click location.
    location: Point,
    /// Last click timestamp.
    last_click: Instant,
    /// Click count in the sequence.
    count: u8,
}

/// Render context for a single editor line.
struct RenderLineContext<'a, 'b> {
    /// Renderer used for drawing.
    r: &'a mut Render<'b>,
    /// View rectangle for the editor.
    view_rect: Rect,
    /// Content origin for the editor.
    origin: Point,
    /// Width of the line-number gutter.
    gutter_width: u32,
}

impl<'a, 'b> RenderLineContext<'a, 'b> {
    /// Construct a new render context.
    fn new(r: &'a mut Render<'b>, view_rect: Rect, origin: Point, gutter_width: u32) -> Self {
        Self {
            r,
            view_rect,
            origin,
            gutter_width,
        }
    }
}

/// Cache of syntax highlight spans keyed by buffer revision and line index.
#[derive(Debug, Clone)]
struct HighlightCache {
    /// Buffer revision the cache corresponds to.
    revision: u64,
    /// Cached spans per line.
    lines: HashMap<usize, Vec<HighlightSpan>>,
}

impl HighlightCache {
    /// Construct an empty highlight cache.
    fn new() -> Self {
        Self {
            revision: 0,
            lines: HashMap::new(),
        }
    }

    /// Clear cached spans.
    fn clear(&mut self) {
        self.lines.clear();
    }

    /// Reset the cache when the buffer revision changes.
    fn sync_revision(&mut self, revision: u64) {
        if self.revision != revision {
            self.revision = revision;
            self.lines.clear();
        }
    }

    /// Return cached spans for a line or compute and store them.
    fn spans_for_line(
        &mut self,
        line: usize,
        compute: impl FnOnce() -> Vec<HighlightSpan>,
    ) -> Vec<HighlightSpan> {
        if let Some(spans) = self.lines.get(&line) {
            return spans.clone();
        }
        let spans = compute();
        self.lines.insert(line, spans.clone());
        spans
    }
}

#[derive_commands]
impl Editor {
    /// Construct an editor with default configuration.
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_config(text, EditorConfig::default())
    }

    /// Construct an editor with a configuration.
    pub fn with_config(text: impl Into<String>, config: EditorConfig) -> Self {
        let mut buffer = TextBuffer::new(text);
        buffer.set_cursor(TextPosition::new(0, 0));
        let preferred_column = buffer.column_for_position(buffer.cursor(), config.tab_stop);
        Self {
            config,
            buffer,
            layout: LayoutCache::new(),
            preferred_column,
            vi: ViState::new(),
            yank: String::new(),
            yank_linewise: false,
            search: SearchState::new(),
            prompt: None,
            mouse: MouseState::new(),
            highlighter: None,
            highlight_cache: HighlightCache::new(),
            cursor_point: None,
            text_entry_transaction: false,
        }
    }

    /// Return the current editor configuration.
    pub fn config(&self) -> &EditorConfig {
        &self.config
    }

    /// Replace the editor configuration.
    pub fn set_config(&mut self, config: EditorConfig) {
        self.config = config;
        self.preferred_column = self
            .buffer
            .column_for_position(self.buffer.cursor(), self.config.tab_stop);
    }

    /// Return the buffer contents.
    pub fn text(&self) -> String {
        self.buffer.text()
    }

    /// Replace the buffer contents.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buffer = TextBuffer::new(text);
        self.buffer.set_cursor(TextPosition::new(0, 0));
        self.preferred_column = self
            .buffer
            .column_for_position(self.buffer.cursor(), self.config.tab_stop);
        self.highlight_cache.clear();
    }

    /// Return the current selection.
    pub fn selection(&self) -> Selection {
        self.buffer.selection()
    }

    /// Install a syntax highlighter.
    pub fn set_highlighter(&mut self, highlighter: Option<Box<dyn Highlighter>>) {
        self.highlighter = highlighter;
        self.highlight_cache.clear();
    }

    /// Return a reference to the internal buffer.
    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Compute the wrap width available for text content.
    fn view_wrap_width(&self, view_rect: Rect, gutter_width: u32) -> usize {
        let available = view_rect.w.saturating_sub(gutter_width).max(1);
        available as usize
    }

    /// Compute the line-number gutter width.
    fn gutter_width(&self) -> u32 {
        match self.config.line_numbers {
            LineNumbers::None => 0,
            LineNumbers::Absolute | LineNumbers::Relative => {
                let digits = self.buffer.line_count().max(1).to_string().len() as u32;
                digits.saturating_add(1)
            }
        }
    }

    /// Synchronize layout and cached cursor position.
    fn update_layout(&mut self, view_rect: Rect, gutter_width: u32) {
        let wrap_width = self.view_wrap_width(view_rect, gutter_width);
        self.layout.sync(
            &mut self.buffer,
            wrap_width,
            self.config.wrap,
            self.config.tab_stop,
        );
        let cursor = self.buffer.cursor();
        let point = self
            .layout
            .point_for_position(&self.buffer, cursor, self.config.tab_stop);
        self.cursor_point = Some(Point {
            x: point.x.saturating_add(gutter_width),
            y: point.y,
        });
    }

    /// Ensure the cursor is visible within the current scroll view.
    fn ensure_cursor_visible(&mut self, ctx: &mut dyn Context) {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let gutter_width = self.gutter_width();
        self.update_layout(view_rect, gutter_width);

        let Some(cursor) = self.cursor_point else {
            return;
        };
        let cursor_x = cursor.x;
        let cursor_y = cursor.y;

        let mut target_x = view_rect.tl.x;
        let mut target_y = view_rect.tl.y;

        if cursor_x < view_rect.tl.x {
            target_x = cursor_x;
        } else if cursor_x >= view_rect.tl.x.saturating_add(view_rect.w.saturating_sub(1)) {
            target_x = cursor_x.saturating_sub(view_rect.w.saturating_sub(1));
        }

        if cursor_y < view_rect.tl.y {
            target_y = cursor_y;
        } else if cursor_y >= view_rect.tl.y.saturating_add(view_rect.h.saturating_sub(1)) {
            target_y = cursor_y.saturating_sub(view_rect.h.saturating_sub(1));
        }

        if self.config.wrap == WrapMode::Soft {
            target_x = 0;
        }

        let _ = ctx.scroll_to(target_x, target_y);
    }

    /// Refresh the preferred display column from the cursor position.
    fn update_preferred_column(&mut self) {
        self.preferred_column = self
            .buffer
            .column_for_position(self.buffer.cursor(), self.config.tab_stop);
    }

    /// Move vertically by logical lines, preserving preferred column.
    fn move_vertical(&mut self, delta: isize) {
        let cursor = self.buffer.cursor();
        let line_count = self.buffer.line_count().max(1);
        let mut line = cursor.line as isize + delta;
        line = line.clamp(0, line_count.saturating_sub(1) as isize);
        let target = self.buffer.position_for_column(
            line as usize,
            self.preferred_column,
            self.config.tab_stop,
        );
        self.buffer.set_cursor(target);
    }

    /// Move vertically by display lines using the layout cache.
    fn move_display_line(&mut self, delta: isize, ctx: &dyn Context) {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let gutter_width = self.gutter_width();
        self.update_layout(view_rect, gutter_width);
        let point = self.layout.point_for_position(
            &self.buffer,
            self.buffer.cursor(),
            self.config.tab_stop,
        );
        let mut y = point.y as isize + delta;
        if y < 0 {
            y = 0;
        }
        let max_y = self.layout.total_lines().saturating_sub(1) as isize;
        if y > max_y {
            y = max_y;
        }
        let new_point = Point {
            x: point.x,
            y: y as u32,
        };
        let pos = self
            .layout
            .position_for_point(&self.buffer, new_point, self.config.tab_stop);
        self.buffer.set_cursor(pos);
        self.update_preferred_column();
    }

    /// Enter visual mode and initialize selection.
    fn enter_visual(&mut self, mode: VisualMode) {
        let cursor = self.buffer.cursor();
        self.vi.set_mode(ViMode::Visual(mode));
        let selection = match mode {
            VisualMode::Line => {
                let start = TextPosition::new(cursor.line, 0);
                let end = self.buffer.line_end_position(cursor.line, false);
                Selection::new(start, end)
            }
            VisualMode::Character => Selection::new(cursor, cursor),
        };
        self.buffer.set_selection(selection);
    }

    /// Exit visual mode and collapse selection.
    fn exit_visual(&mut self) {
        let cursor = self.buffer.cursor();
        self.vi.set_mode(ViMode::Normal);
        self.buffer.set_selection(Selection::caret(cursor));
    }

    /// Insert text at the cursor, respecting read-only state.
    fn handle_insert_text(&mut self, text: &str) {
        if self.config.read_only {
            return;
        }
        let content = self.normalize_insert_text(text);
        self.buffer.insert_text(&content);
        self.update_preferred_column();
    }

    /// Normalize inserted text for single-line editors.
    fn normalize_insert_text(&self, text: &str) -> String {
        if self.config.multiline {
            text.to_string()
        } else {
            text.replace(['\n', '\r'], " ")
        }
    }

    /// Delete backward respecting selection and multiline rules.
    fn handle_delete_backward(&mut self) -> bool {
        if self.config.read_only {
            return false;
        }
        let deleted = self.buffer.delete_backward(self.config.multiline);
        if deleted {
            self.update_preferred_column();
        }
        deleted
    }

    /// Delete forward respecting selection and multiline rules.
    fn handle_delete_forward(&mut self) -> bool {
        if self.config.read_only {
            return false;
        }
        let deleted = self.buffer.delete_forward(self.config.multiline);
        if deleted {
            self.update_preferred_column();
        }
        deleted
    }

    /// Delete the grapheme under the cursor and update yank register.
    fn delete_char_forward(&mut self) -> bool {
        if self.config.read_only {
            return false;
        }
        let cursor = self.buffer.cursor();
        let line_len = self.buffer.line_char_len(cursor.line);
        if cursor.column >= line_len {
            if !self.config.multiline || cursor.line + 1 >= self.buffer.line_count() {
                return false;
            }
            let end = TextPosition::new(cursor.line + 1, 0);
            let range = TextRange::new(cursor, end);
            self.set_yank(range, false);
            self.buffer.replace_range(range, "");
            self.update_preferred_column();
            return true;
        }

        let line_text = self.buffer.line_text(cursor.line);
        let next = next_grapheme_boundary(&line_text, cursor.column);
        let range = TextRange::new(cursor, TextPosition::new(cursor.line, next));
        self.set_yank(range, false);
        self.buffer.replace_range(range, "");
        self.update_preferred_column();
        true
    }

    /// Normalize and insert pasted text, returning the inserted string.
    fn handle_paste(&mut self, text: &str) -> String {
        let content = self.normalize_insert_text(text);
        if self.config.read_only {
            return String::new();
        }
        self.buffer.insert_text(&content);
        self.update_preferred_column();
        content
    }

    /// Begin a grouped text-entry transaction if needed.
    fn begin_text_entry_transaction(&mut self) {
        if !self.text_entry_transaction {
            self.buffer.begin_transaction();
            self.text_entry_transaction = true;
        }
    }

    /// Commit the active text-entry transaction if present.
    fn commit_text_entry_transaction(&mut self) {
        if self.text_entry_transaction {
            self.buffer.commit_transaction();
            self.text_entry_transaction = false;
        }
    }

    /// Start a search prompt in the specified direction.
    fn start_search_prompt(&mut self, direction: SearchDirection) {
        self.prompt = Some(PromptState::Search {
            direction,
            query: String::new(),
        });
    }

    /// Start a replace prompt.
    fn start_replace_prompt(&mut self) {
        self.prompt = Some(PromptState::ReplaceQuery {
            query: String::new(),
        });
    }

    /// Handle prompt input events.
    fn handle_prompt_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        let Some(prompt) = self.prompt.clone() else {
            return EventOutcome::Ignore;
        };

        match (prompt, event) {
            (
                PromptState::Search { direction, query },
                Event::Key(key::Key {
                    key: key::KeyCode::Enter,
                    ..
                }),
            ) => {
                self.search.set_query(&self.buffer, query, direction);
                if let Some(pos) = self.search.current_match().map(|range| range.start) {
                    self.buffer.set_cursor(pos);
                    self.ensure_cursor_visible(ctx);
                }
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::Search {
                    direction,
                    mut query,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Backspace,
                    ..
                }),
            ) => {
                let _ = query.pop();
                self.prompt = Some(PromptState::Search { direction, query });
                EventOutcome::Handle
            }
            (
                PromptState::Search {
                    direction,
                    mut query,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    mods,
                }),
            ) if !mods.ctrl && !mods.alt => {
                query.push(*c);
                self.prompt = Some(PromptState::Search { direction, query });
                EventOutcome::Handle
            }
            (
                PromptState::Search { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { query },
                Event::Key(key::Key {
                    key: key::KeyCode::Enter,
                    ..
                }),
            ) => {
                self.prompt = Some(PromptState::ReplaceWith {
                    query,
                    replacement: String::new(),
                });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { mut query },
                Event::Key(key::Key {
                    key: key::KeyCode::Backspace,
                    ..
                }),
            ) => {
                let _ = query.pop();
                self.prompt = Some(PromptState::ReplaceQuery { query });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { mut query },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    mods,
                }),
            ) if !mods.ctrl && !mods.alt => {
                query.push(*c);
                self.prompt = Some(PromptState::ReplaceQuery { query });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith { query, replacement },
                Event::Key(key::Key {
                    key: key::KeyCode::Enter,
                    ..
                }),
            ) => {
                let matches = find_matches(&self.buffer, &query);
                self.prompt = Some(PromptState::ReplaceConfirm {
                    query,
                    replacement,
                    matches,
                    index: 0,
                    replace_all: false,
                });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith {
                    query,
                    mut replacement,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Backspace,
                    ..
                }),
            ) => {
                let _ = replacement.pop();
                self.prompt = Some(PromptState::ReplaceWith { query, replacement });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith {
                    query,
                    mut replacement,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    mods,
                }),
            ) if !mods.ctrl && !mods.alt => {
                replacement.push(*c);
                self.prompt = Some(PromptState::ReplaceWith { query, replacement });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceConfirm {
                    query,
                    replacement,
                    mut matches,
                    mut index,
                    mut replace_all,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    ..
                }),
            ) => {
                match *c {
                    'y' => {
                        let (new_matches, next_index) =
                            self.replace_match(&query, &replacement, matches, index, ctx);
                        matches = new_matches;
                        index = next_index;
                    }
                    'n' => {
                        index = index.saturating_add(1);
                    }
                    'a' => {
                        replace_all = true;
                    }
                    'q' => {
                        self.prompt = None;
                        return EventOutcome::Handle;
                    }
                    _ => {}
                }

                if replace_all {
                    while index < matches.len() {
                        let (new_matches, next_index) =
                            self.replace_match(&query, &replacement, matches, index, ctx);
                        matches = new_matches;
                        index = next_index;
                    }
                }

                if index >= matches.len() {
                    self.prompt = None;
                } else {
                    self.prompt = Some(PromptState::ReplaceConfirm {
                        query,
                        replacement,
                        matches,
                        index,
                        replace_all,
                    });
                }
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceConfirm { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Replace a match at an index and return updated matches and next index.
    fn replace_match(
        &mut self,
        query: &str,
        replacement: &str,
        matches: Vec<TextRange>,
        index: usize,
        ctx: &mut dyn Context,
    ) -> (Vec<TextRange>, usize) {
        let Some(range) = matches.get(index).copied() else {
            return (matches, index);
        };
        self.buffer
            .set_selection(Selection::new(range.start, range.end));
        self.handle_insert_text(replacement);
        self.ensure_cursor_visible(ctx);
        let updated = find_matches(&self.buffer, query);
        let next_index = updated
            .iter()
            .position(|candidate| candidate.start > range.start)
            .unwrap_or(updated.len());
        (updated, next_index)
    }

    /// Handle events in text-entry mode.
    fn handle_text_entry_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                mods,
            }) if !mods.ctrl && !mods.alt => {
                self.begin_text_entry_transaction();
                self.handle_insert_text(&c.to_string());
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Backspace,
                ..
            }) => {
                self.commit_text_entry_transaction();
                if self.handle_delete_backward() {
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Delete,
                ..
            }) => {
                self.commit_text_entry_transaction();
                if self.handle_delete_forward() {
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                self.commit_text_entry_transaction();
                let moved = self.buffer.move_left(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                self.commit_text_entry_transaction();
                let moved = self.buffer.move_right(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                self.commit_text_entry_transaction();
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                self.commit_text_entry_transaction();
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Home,
                ..
            }) => {
                self.commit_text_entry_transaction();
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::End,
                ..
            }) => {
                self.commit_text_entry_transaction();
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Enter,
                ..
            }) => {
                self.commit_text_entry_transaction();
                if self.config.multiline {
                    self.handle_insert_text("\n");
                    self.ensure_cursor_visible(ctx);
                    EventOutcome::Handle
                } else {
                    EventOutcome::Ignore
                }
            }
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.commit_text_entry_transaction();
                EventOutcome::Ignore
            }
            Event::Paste(content) => {
                self.begin_text_entry_transaction();
                let _ = self.handle_paste(content);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => {
                self.commit_text_entry_transaction();
                EventOutcome::Ignore
            }
        }
    }

    /// Handle events in vi mode.
    fn handle_vi_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if self.prompt.is_some() {
            return self.handle_prompt_event(event, ctx);
        }

        match self.vi.mode() {
            ViMode::Insert => return self.handle_vi_insert(event, ctx),
            ViMode::Visual(mode) => return self.handle_vi_visual(event, ctx, mode),
            ViMode::Normal => {}
        }

        if let Some(pending) = self.vi.pending() {
            let outcome = self.handle_pending_vi(pending, event, ctx);
            if outcome != EventOutcome::Ignore {
                return outcome;
            }
        }

        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char('i'),
                ..
            }) => {
                self.buffer.begin_transaction();
                self.vi.begin_insert(self.buffer.cursor());
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('a'),
                ..
            }) => {
                let _ = self.buffer.move_right(self.config.multiline);
                self.update_preferred_column();
                self.buffer.begin_transaction();
                self.vi.begin_insert(self.buffer.cursor());
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('I'),
                ..
            }) => {
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.buffer.begin_transaction();
                self.vi.begin_insert(self.buffer.cursor());
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('A'),
                ..
            }) => {
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.buffer.begin_transaction();
                self.vi.begin_insert(self.buffer.cursor());
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('o'),
                ..
            }) => {
                self.buffer.begin_transaction();
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let end = self.buffer.line_end_position(cursor.line, true);
                    self.buffer.set_cursor(end);
                    self.handle_insert_text("\n");
                }
                self.vi.begin_insert(self.buffer.cursor());
                self.vi.set_last_edit(RepeatableEdit::OpenBelow);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('O'),
                ..
            }) => {
                self.buffer.begin_transaction();
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let start = self.buffer.line_start_position(cursor.line);
                    self.buffer.set_cursor(start);
                    self.handle_insert_text("\n");
                    let _ = self.buffer.move_left(true);
                }
                self.vi.begin_insert(self.buffer.cursor());
                self.vi.set_last_edit(RepeatableEdit::OpenAbove);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('v'),
                ..
            }) => {
                if let ViMode::Visual(_) = self.vi.mode() {
                    self.exit_visual();
                } else {
                    self.enter_visual(VisualMode::Character);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('V'),
                ..
            }) => {
                self.enter_visual(VisualMode::Line);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('/'),
                ..
            }) => {
                self.start_search_prompt(SearchDirection::Forward);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('?'),
                ..
            }) => {
                self.start_search_prompt(SearchDirection::Backward);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('n'),
                ..
            }) => {
                if let Some(pos) = self.search.move_next(&self.buffer, false) {
                    self.buffer.set_cursor(pos);
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('N'),
                ..
            }) => {
                if let Some(pos) = self.search.move_next(&self.buffer, true) {
                    self.buffer.set_cursor(pos);
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('R'),
                ..
            }) => {
                self.start_replace_prompt();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('u'),
                ..
            }) => {
                self.buffer.undo();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('r'),
                mods,
            }) if mods.ctrl => {
                self.buffer.redo();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('.'),
                ..
            }) => {
                self.repeat_last_edit();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('h'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                let moved = self.buffer.move_left(true);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('l'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                let moved = self.buffer.move_right(true);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('j'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('k'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('0'),
                ..
            }) => {
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('$'),
                ..
            }) => {
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('^'),
                ..
            }) => {
                self.buffer.move_line_first_non_ws();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('w'),
                ..
            }) => {
                self.move_word_forward();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('b'),
                ..
            }) => {
                self.move_word_backward();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('e'),
                ..
            }) => {
                self.move_word_end();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('g'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::G));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('d'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::Delete));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('c'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::Change));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('y'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::Yank));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('Y'),
                ..
            }) => {
                self.yank_line();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('p'),
                ..
            }) => {
                let text = self.yank.clone();
                let linewise = self.yank_linewise;
                self.put_yank(false);
                if !text.is_empty() {
                    self.vi.set_last_edit(RepeatableEdit::Put {
                        text,
                        linewise,
                        before: false,
                    });
                }
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('P'),
                ..
            }) => {
                let text = self.yank.clone();
                let linewise = self.yank_linewise;
                self.put_yank(true);
                if !text.is_empty() {
                    self.vi.set_last_edit(RepeatableEdit::Put {
                        text,
                        linewise,
                        before: true,
                    });
                }
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('x'),
                ..
            }) => {
                if self.delete_char_forward() {
                    self.vi.set_last_edit(RepeatableEdit::DeleteChar);
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('D'),
                ..
            }) => {
                self.delete_to_line_end();
                self.vi.set_last_edit(RepeatableEdit::DeleteToEnd);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('C'),
                ..
            }) => {
                self.buffer.begin_transaction();
                self.delete_to_line_end();
                self.vi.set_last_edit(RepeatableEdit::ChangeToEnd);
                self.vi.begin_insert(self.buffer.cursor());
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('G'),
                ..
            }) => {
                let last_line = self.buffer.line_count().saturating_sub(1);
                let pos = TextPosition::new(last_line, 0);
                self.buffer.set_cursor(pos);
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Handle a pending multi-key vi command.
    fn handle_pending_vi(
        &mut self,
        pending: PendingKey,
        event: &Event,
        ctx: &mut dyn Context,
    ) -> EventOutcome {
        match (pending, event) {
            (
                PendingKey::G,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('g'),
                    ..
                }),
            ) => {
                self.buffer.set_cursor(TextPosition::new(0, 0));
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            (
                PendingKey::G,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('j'),
                    ..
                }),
            ) => {
                self.move_display_line(1, ctx);
                self.ensure_cursor_visible(ctx);
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            (
                PendingKey::G,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('k'),
                    ..
                }),
            ) => {
                self.move_display_line(-1, ctx);
                self.ensure_cursor_visible(ctx);
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            (
                PendingKey::Delete,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('d'),
                    ..
                }),
            ) => {
                self.delete_line();
                self.vi.set_last_edit(RepeatableEdit::DeleteLine);
                self.vi.set_pending(None);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            (
                PendingKey::Change,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('c'),
                    ..
                }),
            ) => {
                self.buffer.begin_transaction();
                self.delete_line();
                self.vi.set_last_edit(RepeatableEdit::ChangeLine);
                self.vi.begin_insert(self.buffer.cursor());
                self.vi.set_pending(None);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            (
                PendingKey::Yank,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('y'),
                    ..
                }),
            ) => {
                self.yank_line();
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            _ => {
                self.vi.set_pending(None);
                EventOutcome::Ignore
            }
        }
    }

    /// Handle insert-mode vi events.
    fn handle_vi_insert(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.buffer.commit_transaction();
                let _ = self.vi.end_insert();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                mods,
            }) if !mods.ctrl && !mods.alt => {
                self.handle_insert_text(&c.to_string());
                self.vi.push_inserted(&c.to_string());
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Backspace,
                ..
            }) => {
                if self.handle_delete_backward() {
                    self.vi.pop_inserted_grapheme();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Delete,
                ..
            }) => {
                if self.handle_delete_forward() {
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Enter,
                ..
            }) => {
                if self.config.multiline {
                    self.handle_insert_text("\n");
                    self.vi.push_inserted("\n");
                    self.ensure_cursor_visible(ctx);
                    EventOutcome::Handle
                } else {
                    EventOutcome::Ignore
                }
            }
            Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                let moved = self.buffer.move_left(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                let moved = self.buffer.move_right(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Paste(content) => {
                let inserted = self.handle_paste(content);
                self.vi.push_inserted(&inserted);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Handle visual-mode vi events.
    fn handle_vi_visual(
        &mut self,
        event: &Event,
        ctx: &mut dyn Context,
        mode: VisualMode,
    ) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.exit_visual();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('d'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Char('x'),
                ..
            }) => {
                if self.config.read_only {
                    self.exit_visual();
                    return EventOutcome::Handle;
                }
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.buffer.replace_range(range, "");
                self.update_preferred_column();
                self.exit_visual();
                self.vi.set_last_edit(RepeatableEdit::DeleteChar);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('y'),
                ..
            }) => {
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.exit_visual();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('c'),
                ..
            }) => {
                if self.config.read_only {
                    self.exit_visual();
                    return EventOutcome::Handle;
                }
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.buffer.begin_transaction();
                self.buffer.replace_range(range, "");
                self.vi.begin_insert(self.buffer.cursor());
                self.exit_visual();
                self.vi.set_last_edit(RepeatableEdit::ChangeLine);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('>'),
                ..
            }) => {
                self.indent_selection(true, mode);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('<'),
                ..
            }) => {
                self.indent_selection(false, mode);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('h'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                let moved = self.buffer.move_left(true);
                if moved {
                    self.update_visual_selection(anchor, mode);
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('l'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                let moved = self.buffer.move_right(true);
                if moved {
                    self.update_visual_selection(anchor, mode);
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('j'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                self.move_vertical(1);
                self.update_visual_selection(anchor, mode);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('k'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                self.move_vertical(-1);
                self.update_visual_selection(anchor, mode);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Extend the current selection in visual mode.
    fn extend_selection(&mut self, mode: VisualMode) {
        let mut selection = self.buffer.selection();
        selection.set_head(self.buffer.cursor());
        if let VisualMode::Line = mode {
            let range = selection.range();
            let start = TextPosition::new(range.start.line, 0);
            let end = self.buffer.line_end_position(range.end.line, false);
            self.buffer.set_selection(Selection::new(start, end));
        } else {
            self.buffer.set_selection(selection);
        }
    }

    /// Update a visual selection while preserving the anchor.
    fn update_visual_selection(&mut self, anchor: TextPosition, mode: VisualMode) {
        let head = self.buffer.cursor();
        self.buffer.set_selection(Selection::new(anchor, head));
        self.extend_selection(mode);
    }

    /// Expand a range to full line boundaries, including trailing newline.
    fn linewise_range(&self, range: TextRange) -> TextRange {
        let range = range.normalized();
        let start = TextPosition::new(range.start.line, 0);
        let end = self.buffer.line_end_position(range.end.line, true);
        TextRange::new(start, end)
    }

    /// Delete the current line and update yank register.
    fn delete_line(&mut self) {
        if self.config.read_only {
            return;
        }
        let cursor = self.buffer.cursor();
        let start = TextPosition::new(cursor.line, 0);
        let end = self.buffer.line_end_position(cursor.line, true);
        let range = TextRange::new(start, end);
        self.set_yank(range, true);
        self.buffer.replace_range(range, "");
        self.update_preferred_column();
    }

    /// Delete from the cursor to the line end and update yank register.
    fn delete_to_line_end(&mut self) {
        if self.config.read_only {
            return;
        }
        let cursor = self.buffer.cursor();
        let end = self.buffer.line_end_position(cursor.line, false);
        let range = TextRange::new(cursor, end);
        if range.is_empty() {
            return;
        }
        self.set_yank(range, false);
        self.buffer.replace_range(range, "");
        self.update_preferred_column();
    }

    /// Update the yank register with a range.
    fn set_yank(&mut self, range: TextRange, linewise: bool) {
        self.yank = self.buffer.range_text(range);
        self.yank_linewise = linewise;
    }

    /// Yank the current line into the register.
    fn yank_line(&mut self) {
        let cursor = self.buffer.cursor();
        let start = TextPosition::new(cursor.line, 0);
        let end = self.buffer.line_end_position(cursor.line, true);
        let range = TextRange::new(start, end);
        self.set_yank(range, true);
    }

    /// Put the yank register contents before or after the cursor.
    fn put_yank(&mut self, before: bool) {
        if self.config.read_only || self.yank.is_empty() {
            return;
        }
        let yank = self.yank.clone();
        self.buffer.begin_transaction();
        if self.yank_linewise {
            let cursor = self.buffer.cursor();
            let target = if before {
                self.buffer.line_start_position(cursor.line)
            } else {
                self.buffer.line_end_position(cursor.line, true)
            };
            self.buffer.set_cursor(target);
        } else if !before {
            let _ = self.buffer.move_right(self.config.multiline);
        }
        self.handle_insert_text(&yank);
        self.buffer.commit_transaction();
        self.update_preferred_column();
    }

    /// Indent or outdent the selected lines.
    fn indent_selection(&mut self, indent: bool, mode: VisualMode) {
        if self.config.read_only {
            return;
        }
        if !self.config.multiline {
            return;
        }
        let range = self.buffer.selection().range();
        let start_line = range.start.line;
        let end_line = range.end.line;
        let tab = " ".repeat(self.config.tab_stop.max(1));
        self.buffer.begin_transaction();
        for line in start_line..=end_line {
            let line_start = TextPosition::new(line, 0);
            if indent {
                self.buffer
                    .replace_range(TextRange::new(line_start, line_start), &tab);
            } else {
                let line_text = self.buffer.line_text(line);
                let remove = line_text
                    .chars()
                    .take(self.config.tab_stop)
                    .take_while(|c| *c == ' ')
                    .count();
                if remove > 0 {
                    let end = TextPosition::new(line, remove);
                    self.buffer
                        .replace_range(TextRange::new(line_start, end), "");
                }
            }
        }
        self.buffer.commit_transaction();
        if let VisualMode::Line = mode {
            self.extend_selection(mode);
        }
    }

    /// Move to the start of the next word on the current line.
    fn move_word_forward(&mut self) {
        let mut line = self.buffer.cursor().line;
        let mut column = self.buffer.cursor().column;
        let line_count = self.buffer.line_count().max(1);
        let mut crossed_line = false;

        loop {
            let line_text = self.buffer.line_text(line);
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            if column >= len {
                if line + 1 >= line_count {
                    self.buffer.set_cursor(TextPosition::new(line, len));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_add(1);
                column = 0;
                crossed_line = true;
                continue;
            }

            if !crossed_line && is_word_char(chars[column]) {
                while column < len && is_word_char(chars[column]) {
                    column = column.saturating_add(1);
                }
            }
            while column < len && !is_word_char(chars[column]) {
                column = column.saturating_add(1);
            }

            if column < len {
                self.buffer.set_cursor(TextPosition::new(line, column));
                self.update_preferred_column();
                return;
            }

            if line + 1 >= line_count {
                self.buffer.set_cursor(TextPosition::new(line, len));
                self.update_preferred_column();
                return;
            }
            line = line.saturating_add(1);
            column = 0;
            crossed_line = true;
        }
    }

    /// Move to the start of the previous word on the current line.
    fn move_word_backward(&mut self) {
        let mut line = self.buffer.cursor().line;
        let mut column = self.buffer.cursor().column;

        loop {
            let line_text = self.buffer.line_text(line);
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            let mut idx = column.min(len);
            if idx == 0 {
                if line == 0 {
                    self.buffer.set_cursor(TextPosition::new(0, 0));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_sub(1);
                column = self.buffer.line_char_len(line);
                continue;
            }

            idx = idx.saturating_sub(1);
            while idx > 0 && !is_word_char(chars[idx]) {
                idx = idx.saturating_sub(1);
            }

            if !is_word_char(chars[idx]) {
                if line == 0 {
                    self.buffer.set_cursor(TextPosition::new(0, 0));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_sub(1);
                column = self.buffer.line_char_len(line);
                continue;
            }

            while idx > 0 && is_word_char(chars[idx.saturating_sub(1)]) {
                idx = idx.saturating_sub(1);
            }

            self.buffer.set_cursor(TextPosition::new(line, idx));
            self.update_preferred_column();
            return;
        }
    }

    /// Move to the end of the current word on the current line.
    fn move_word_end(&mut self) {
        let mut line = self.buffer.cursor().line;
        let mut column = self.buffer.cursor().column;
        let line_count = self.buffer.line_count().max(1);

        loop {
            let line_text = self.buffer.line_text(line);
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            if column >= len {
                if line + 1 >= line_count {
                    self.buffer.set_cursor(TextPosition::new(line, len));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_add(1);
                column = 0;
                continue;
            }

            let mut idx = column;
            while idx < len && !is_word_char(chars[idx]) {
                idx = idx.saturating_add(1);
            }
            if idx >= len {
                if line + 1 >= line_count {
                    self.buffer.set_cursor(TextPosition::new(line, len));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_add(1);
                column = 0;
                continue;
            }
            while idx + 1 < len && is_word_char(chars[idx + 1]) {
                idx = idx.saturating_add(1);
            }
            self.buffer.set_cursor(TextPosition::new(line, idx));
            self.update_preferred_column();
            return;
        }
    }

    /// Repeat the last recorded vi edit.
    fn repeat_last_edit(&mut self) {
        if self.config.read_only {
            return;
        }
        let Some(edit) = self.vi.last_edit() else {
            return;
        };
        match edit {
            RepeatableEdit::Insert { text } => {
                self.handle_insert_text(&text);
            }
            RepeatableEdit::Put {
                text,
                linewise,
                before,
            } => {
                self.yank = text;
                self.yank_linewise = linewise;
                self.put_yank(before);
            }
            RepeatableEdit::DeleteLine => {
                self.delete_line();
            }
            RepeatableEdit::ChangeLine => {
                self.delete_line();
                self.vi.begin_insert(self.buffer.cursor());
                self.buffer.begin_transaction();
            }
            RepeatableEdit::DeleteChar => {
                let _ = self.handle_delete_forward();
            }
            RepeatableEdit::DeleteToEnd => {
                self.delete_to_line_end();
            }
            RepeatableEdit::ChangeToEnd => {
                self.delete_to_line_end();
                self.vi.begin_insert(self.buffer.cursor());
                self.buffer.begin_transaction();
            }
            RepeatableEdit::OpenBelow => {
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let end = self.buffer.line_end_position(cursor.line, true);
                    self.buffer.set_cursor(end);
                    self.handle_insert_text("\n");
                }
                self.vi.begin_insert(self.buffer.cursor());
                self.buffer.begin_transaction();
            }
            RepeatableEdit::OpenAbove => {
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let start = self.buffer.line_start_position(cursor.line);
                    self.buffer.set_cursor(start);
                    self.handle_insert_text("\n");
                    let _ = self.buffer.move_left(true);
                }
                self.vi.begin_insert(self.buffer.cursor());
                self.buffer.begin_transaction();
            }
        }
    }

    /// Handle mouse interactions for selection and cursor movement.
    fn handle_mouse_event(&mut self, event: &mouse::MouseEvent, ctx: &mut dyn Context) -> bool {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let gutter_width = self.gutter_width();
        self.update_layout(view_rect, gutter_width);
        if event.location.x < origin.x || event.location.y < origin.y {
            return false;
        }
        let local = Point {
            x: event.location.x.saturating_sub(origin.x),
            y: event.location.y.saturating_sub(origin.y),
        };
        let content_point = Point {
            x: view.tl.x.saturating_add(local.x),
            y: view.tl.y.saturating_add(local.y),
        };
        let mut text_point = content_point;
        if text_point.x > gutter_width {
            text_point.x = text_point.x.saturating_sub(gutter_width);
        } else {
            text_point.x = 0;
        }
        let pos = self
            .layout
            .position_for_point(&self.buffer, text_point, self.config.tab_stop);

        match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                ctx.set_focus(ctx.node_id());
                let click_type = self.mouse.click_type(event.location);
                match click_type {
                    ClickType::Single => {
                        self.mouse.selecting = true;
                        self.mouse.anchor = Some(pos);
                        self.buffer.set_selection(Selection::new(pos, pos));
                    }
                    ClickType::Double => {
                        let range = word_range(&self.buffer, pos);
                        self.mouse.selecting = true;
                        self.mouse.anchor = Some(range.start);
                        self.buffer
                            .set_selection(Selection::new(range.start, range.end));
                    }
                    ClickType::Triple => {
                        let start = TextPosition::new(pos.line, 0);
                        let end = self.buffer.line_end_position(pos.line, true);
                        self.mouse.selecting = true;
                        self.mouse.anchor = Some(start);
                        self.buffer.set_selection(Selection::new(start, end));
                    }
                }
                self.update_preferred_column();
                true
            }
            mouse::Action::Drag if event.button == mouse::Button::Left => {
                if self.mouse.selecting
                    && let Some(anchor) = self.mouse.anchor
                {
                    self.buffer.set_selection(Selection::new(anchor, pos));
                    self.update_preferred_column();
                }
                true
            }
            mouse::Action::Up if event.button == mouse::Button::Left => {
                self.mouse.selecting = false;
                true
            }
            _ => false,
        }
    }

    /// Render the search/replace prompt overlay.
    fn render_prompt(&self, r: &mut Render, view_rect: Rect, origin: Point) -> Result<()> {
        let Some(prompt) = &self.prompt else {
            return Ok(());
        };
        let y = origin.y.saturating_add(view_rect.h.saturating_sub(1));
        let line = Line::new(origin.x, y, view_rect.w);
        r.text("editor/prompt", line, &prompt_text(prompt))
    }

    /// Render a single display line of text and gutter content.
    fn render_line(
        &mut self,
        ctx: &mut RenderLineContext<'_, '_>,
        y: u32,
        line_idx: usize,
        segment: &WrapSegment,
    ) -> Result<()> {
        let line_y = ctx.origin.y.saturating_add(y);
        let line_rect = Rect::new(ctx.origin.x, line_y, ctx.view_rect.w, 1);
        ctx.r.fill("editor/text", line_rect, ' ')?;

        if ctx.gutter_width > 0 {
            let gutter_line = Line::new(ctx.origin.x, line_y, ctx.gutter_width);
            let number_text = line_number_text(
                self.config.line_numbers,
                line_idx,
                self.buffer.cursor().line,
                ctx.gutter_width,
            );
            let style = if line_idx == self.buffer.cursor().line {
                "editor/line-number/current"
            } else {
                "editor/line-number"
            };
            ctx.r.text(style, gutter_line, &number_text)?;
        }

        let line_text = self.buffer.line_text(line_idx);
        let selection = self.buffer.selection();
        let selection_range = selection.range();
        let selection_active = !selection.is_empty();
        let selection_on_line = selection_active
            && line_idx >= selection_range.start.line
            && line_idx <= selection_range.end.line;
        let line_start_sel = if selection_on_line && selection_range.start.line == line_idx {
            selection_range.start.column
        } else {
            0
        };
        let line_end_sel = if selection_on_line && selection_range.end.line == line_idx {
            selection_range.end.column
        } else {
            self.buffer.line_char_len(line_idx)
        };

        let mut highlight_spans = Vec::new();
        if let Some(highlighter) = &self.highlighter {
            highlight_spans = self.highlight_cache.spans_for_line(line_idx, || {
                highlighter.highlight_line(line_idx, &line_text)
            });
        }

        let mut span_idx = 0usize;
        let search_ranges = self.search.matches_for_line(line_idx);
        let current_search = self
            .search
            .current_match()
            .filter(|r| r.start.line == line_idx);
        let current_search_range = current_search.map(|r| (r.start.column, r.end.column));

        let mut col = 0usize;
        let mut char_index = 0usize;
        for grapheme in line_text.graphemes(true) {
            let grapheme_chars = grapheme.chars().count();
            let width = if grapheme == "\t" {
                tab_width(col, self.config.tab_stop)
            } else {
                text::grapheme_width(grapheme)
            };

            let g_start = char_index;
            let g_end = char_index.saturating_add(grapheme_chars);
            if g_end <= segment.start_char {
                col = col.saturating_add(width);
                char_index = g_end;
                continue;
            }
            if g_start >= segment.end_char {
                break;
            }

            let draw_col = col
                .saturating_sub(segment.start_col)
                .saturating_add(ctx.gutter_width as usize);
            let view_start = ctx.view_rect.tl.x as usize;
            let view_end = view_start.saturating_add(ctx.view_rect.w as usize);
            if draw_col.saturating_add(width) <= view_start {
                col = col.saturating_add(width);
                char_index = g_end;
                continue;
            }
            if draw_col >= view_end {
                break;
            }

            let mut style_name = "editor/text";
            let mut style = None;

            if selection_on_line && g_start < line_end_sel && g_end > line_start_sel {
                style_name = "editor/selection";
            } else if let Some((start, end)) = current_search_range {
                if g_start < end && g_end > start {
                    style_name = "editor/search/current";
                }
            } else if search_ranges
                .iter()
                .any(|(start, end)| g_start < *end && g_end > *start)
            {
                style_name = "editor/search/match";
            } else {
                while let Some(span) = highlight_spans.get(span_idx) {
                    if span.range.end <= g_start {
                        span_idx = span_idx.saturating_add(1);
                        continue;
                    }
                    if span.range.start < g_end && span.range.end > g_start {
                        style = Some(span.style.clone());
                    }
                    break;
                }
            }

            if grapheme == "\t" {
                let start = draw_col;
                let end = draw_col.saturating_add(width);
                for offset in start..end {
                    let x = offset.saturating_sub(view_start) as u32;
                    let p = Point {
                        x: ctx.origin.x.saturating_add(x),
                        y: line_y,
                    };
                    let resolved = style
                        .clone()
                        .unwrap_or_else(|| ctx.r.resolve_style_name(style_name));
                    ctx.r.put_cell(resolved, p, ' ')?;
                }
            } else {
                let x = draw_col.saturating_sub(view_start) as u32;
                let p = Point {
                    x: ctx.origin.x.saturating_add(x),
                    y: line_y,
                };
                let resolved = style
                    .clone()
                    .unwrap_or_else(|| ctx.r.resolve_style_name(style_name));
                ctx.r.put_grapheme(resolved, p, grapheme)?;
            }

            col = col.saturating_add(width);
            char_index = g_end;
        }

        Ok(())
    }

    /// Move the cursor left.
    #[command]
    pub fn cursor_left(&mut self, ctx: &mut dyn Context) {
        let _ = self.buffer.move_left(self.config.multiline);
        self.update_preferred_column();
        self.ensure_cursor_visible(ctx);
    }

    /// Move the cursor right.
    #[command]
    pub fn cursor_right(&mut self, ctx: &mut dyn Context) {
        let _ = self.buffer.move_right(self.config.multiline);
        self.update_preferred_column();
        self.ensure_cursor_visible(ctx);
    }

    /// Move the cursor up.
    #[command]
    pub fn cursor_up(&mut self, ctx: &mut dyn Context) {
        self.move_vertical(-1);
        self.ensure_cursor_visible(ctx);
    }

    /// Move the cursor down.
    #[command]
    pub fn cursor_down(&mut self, ctx: &mut dyn Context) {
        self.move_vertical(1);
        self.ensure_cursor_visible(ctx);
    }

    /// Undo the last edit.
    #[command]
    pub fn undo(&mut self, _ctx: &mut dyn Context) {
        self.buffer.undo();
        self.update_preferred_column();
    }

    /// Redo the last undone edit.
    #[command]
    pub fn redo(&mut self, _ctx: &mut dyn Context) {
        self.buffer.redo();
        self.update_preferred_column();
    }
}

impl Widget for Editor {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        let location = self.cursor_point.unwrap_or(Point { x: 0, y: 0 });
        let shape = match self.config.mode {
            EditMode::Text => cursor::CursorShape::Line,
            EditMode::Vi => match self.vi.mode() {
                ViMode::Insert => cursor::CursorShape::Line,
                _ => cursor::CursorShape::Block,
            },
        };
        Some(cursor::Cursor {
            location,
            shape,
            blink: true,
        })
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let gutter_width = self.gutter_width();
        self.update_layout(view_rect, gutter_width);
        self.highlight_cache.sync_revision(self.buffer.revision());

        self.search.update(&self.buffer);

        {
            let mut line_ctx = RenderLineContext::new(r, view_rect, origin, gutter_width);
            for row in 0..view_rect.h {
                let display_line = view_rect.tl.y.saturating_add(row) as usize;
                if display_line >= self.layout.total_lines() {
                    continue;
                }
                let line_idx = self.layout.line_for_display(display_line);
                let line_start = self.layout.line_offset(line_idx);
                let seg_idx = display_line.saturating_sub(line_start);
                let segment = self
                    .layout
                    .line(line_idx)
                    .and_then(|line| line.segment(seg_idx).cloned());
                if let Some(segment) = segment {
                    self.render_line(&mut line_ctx, row, line_idx, &segment)?;
                }
            }
        }

        self.render_prompt(r, view_rect, origin)?;
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let mut width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => self.layout.max_line_width() as u32,
        };
        width = width.max(1);

        let gutter = self.gutter_width();
        let wrap_width = width.saturating_sub(gutter).max(1) as usize;

        let mut height = if self.config.auto_grow {
            display_line_count(
                &self.buffer,
                self.config.wrap,
                wrap_width,
                self.config.tab_stop,
            ) as u32
        } else {
            self.config.min_height.max(1)
        };
        if let Some(max) = self.config.max_height {
            height = height.min(max.max(1));
        }
        height = height.max(self.config.min_height.max(1));
        c.clamp(Size::new(width, height))
    }

    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        let gutter = self.gutter_width();
        let wrap_width = view.width.saturating_sub(gutter).max(1) as usize;
        let height = display_line_count(
            &self.buffer,
            self.config.wrap,
            wrap_width,
            self.config.tab_stop,
        ) as u32;
        let width = match self.config.wrap {
            WrapMode::None => {
                let max_width = display_line_width(&self.buffer, self.config.tab_stop) as u32;
                max_width.saturating_add(gutter).max(view.width.max(1))
            }
            WrapMode::Soft => view.width.max(1),
        };
        Size::new(width.max(1), height.max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if let Event::Mouse(mouse_event) = event {
            let handled = self.handle_mouse_event(mouse_event, ctx);
            if handled {
                self.ensure_cursor_visible(ctx);
                return EventOutcome::Handle;
            }
        }

        match self.config.mode {
            EditMode::Text => self.handle_text_entry_event(event, ctx),
            EditMode::Vi => self.handle_vi_event(event, ctx),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("editor")
    }
}

impl MouseState {
    /// Construct a new mouse state.
    fn new() -> Self {
        Self {
            selecting: false,
            anchor: None,
            click_state: None,
        }
    }

    /// Determine click type based on click timing.
    fn click_type(&mut self, location: Point) -> ClickType {
        let now = Instant::now();
        let threshold = Duration::from_millis(DOUBLE_CLICK_MS);
        let mut count = 1u8;

        if let Some(state) = self.click_state.as_mut() {
            if state.location == location && now.duration_since(state.last_click) <= threshold {
                state.count = state.count.saturating_add(1).min(3);
                state.last_click = now;
                count = state.count;
            } else {
                state.location = location;
                state.count = 1;
                state.last_click = now;
            }
        } else {
            self.click_state = Some(ClickState {
                location,
                last_click: now,
                count: 1,
            });
        }

        match count {
            2 => ClickType::Double,
            3 => ClickType::Triple,
            _ => ClickType::Single,
        }
    }
}

/// Mouse click selection types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickType {
    /// Single click.
    Single,
    /// Double click.
    Double,
    /// Triple click.
    Triple,
}

/// Build prompt text for search and replace overlays.
fn prompt_text(prompt: &PromptState) -> String {
    match prompt {
        PromptState::Search { direction, query } => match direction {
            SearchDirection::Forward => format!("/{query}"),
            SearchDirection::Backward => format!("?{query}"),
        },
        PromptState::ReplaceQuery { query } => format!("Replace: {query}"),
        PromptState::ReplaceWith { query, replacement } => {
            format!("Replace {query} with: {replacement}")
        }
        PromptState::ReplaceConfirm { .. } => "Replace? (y/n/a/q)".to_string(),
    }
}

/// Format a line number gutter entry.
fn line_number_text(mode: LineNumbers, line: usize, cursor_line: usize, width: u32) -> String {
    let number = match mode {
        LineNumbers::None => 0,
        LineNumbers::Absolute => line + 1,
        LineNumbers::Relative => {
            if line == cursor_line {
                line + 1
            } else {
                line.max(cursor_line) - line.min(cursor_line)
            }
        }
    };
    let content = if mode == LineNumbers::None {
        "".to_string()
    } else {
        number.to_string()
    };
    format!(
        "{:>width$} ",
        content,
        width = width.saturating_sub(1) as usize
    )
}

/// Compute the total display line count for a buffer.
fn display_line_count(
    buffer: &TextBuffer,
    wrap_mode: WrapMode,
    wrap_width: usize,
    tab_stop: usize,
) -> usize {
    let mut total = 0usize;
    for line in 0..buffer.line_count().max(1) {
        let text = buffer.line_text(line);
        let layout = layout_line(&text, wrap_mode, wrap_width, tab_stop);
        total = total.saturating_add(layout.display_lines());
    }
    total.max(1)
}

/// Compute the maximum display width for a buffer.
fn display_line_width(buffer: &TextBuffer, tab_stop: usize) -> usize {
    let mut max_width = 1usize;
    for line in 0..buffer.line_count().max(1) {
        let text = buffer.line_text(line);
        let layout = layout_line(&text, WrapMode::None, 1, tab_stop);
        max_width = max_width.max(layout.display_width);
    }
    max_width
}

/// Determine if a character counts as a word constituent.
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Find the next grapheme boundary after a column.
fn next_grapheme_boundary(line: &str, column: usize) -> usize {
    let mut count = 0usize;
    for grapheme in line.graphemes(true) {
        let next = count.saturating_add(grapheme.chars().count());
        if column < next {
            return next;
        }
        count = next;
    }
    column
}

/// Compute the word range at a position.
fn word_range(buffer: &TextBuffer, pos: TextPosition) -> TextRange {
    let line_text = buffer.line_text(pos.line);
    let chars: Vec<char> = line_text.chars().collect();
    let mut start = pos.column.min(chars.len());
    let mut end = start;
    while start > 0 && is_word_char(chars[start.saturating_sub(1)]) {
        start = start.saturating_sub(1);
    }
    while end < chars.len() && is_word_char(chars[end]) {
        end = end.saturating_add(1);
    }
    TextRange::new(
        TextPosition::new(pos.line, start),
        TextPosition::new(pos.line, end),
    )
}
