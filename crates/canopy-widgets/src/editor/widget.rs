use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use canopy::{
    Context, EventOutcome, ViewContext, Widget, command, cursor, derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::{Direction, Line, Point, Rect},
    layout::{CanvasContext, Constraint, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    EditMode, EditorConfig, LineNumbers, Selection, TextBuffer, TextPosition, TextRange, WrapMode,
    display_width,
    highlight::{HighlightSpan, Highlighter},
    layout::{LayoutCache, WrapSegment, layout_line},
    search::{PromptState, SearchDirection, SearchState},
    vi::{ViMode, ViState},
};

/// Maximum delay between clicks to count as multi-click selection.
const DOUBLE_CLICK_MS: u64 = 500;
/// Lines to scroll per mouse wheel tick within the editor.
const WHEEL_SCROLL_LINES: i32 = 3;

/// Editor widget implementation.
pub struct Editor {
    /// Editor configuration.
    pub(super) config: EditorConfig,
    /// Text buffer backing the editor.
    pub(super) buffer: TextBuffer,
    /// Layout cache for wrapping and mapping.
    pub(super) layout: LayoutCache,
    /// Cached cursor position in content coordinates.
    pub(super) cursor_point: Option<Point>,
    /// Cached cursor position in view coordinates.
    pub(super) cursor_view_point: Option<Point>,
    /// Preferred display column for vertical movement.
    pub(super) preferred_column: usize,
    /// Whether a text-entry transaction is active.
    pub(super) text_entry_transaction: bool,
    /// Vi mode state when enabled.
    pub(super) vi: ViState,
    /// Yank register for vi operations.
    pub(super) yank: String,
    /// Whether the yank register represents a full line range.
    pub(super) yank_linewise: bool,
    /// Search state.
    pub(super) search: SearchState,
    /// Prompt state for search and replace.
    pub(super) prompt: Option<PromptState>,
    /// Mouse interaction state.
    pub(super) mouse: MouseState,
    /// Optional syntax highlighter.
    pub(super) highlighter: Option<Box<dyn Highlighter>>,
    /// Cached syntax highlight spans.
    pub(super) highlight_cache: HighlightCache,
}

/// Mouse selection tracking state.
#[derive(Debug, Clone)]
pub(super) struct MouseState {
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
pub(super) struct HighlightCache {
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
            cursor_point: None,
            cursor_view_point: None,
            preferred_column,
            text_entry_transaction: false,
            vi: ViState::new(),
            yank: String::new(),
            yank_linewise: false,
            search: SearchState::new(),
            prompt: None,
            mouse: MouseState::new(),
            highlighter: None,
            highlight_cache: HighlightCache::new(),
        }
    }

    /// Return the current editor configuration.
    pub fn config(&self) -> &EditorConfig {
        &self.config
    }

    /// Replace the editor configuration.
    pub fn set_config(&mut self, config: EditorConfig) {
        self.config = config;
        self.update_preferred_column();
    }

    /// Return the buffer contents.
    pub fn text(&self) -> String {
        self.buffer.text()
    }

    /// Replace the buffer contents.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buffer = TextBuffer::new(text);
        self.buffer.set_cursor(TextPosition::new(0, 0));
        self.update_preferred_column();
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
    pub(super) fn view_wrap_width(&self, view_rect: Rect, gutter_width: u32) -> usize {
        let available = view_rect.w.saturating_sub(gutter_width).max(1);
        available as usize
    }

    /// Compute the line-number gutter width.
    pub(super) fn gutter_width(&self) -> u32 {
        match self.config.line_numbers {
            LineNumbers::None => 0,
            LineNumbers::Absolute | LineNumbers::Relative => {
                let digits = self.buffer.line_count().max(1).to_string().len() as u32;
                digits.saturating_add(1)
            }
        }
    }

    /// Synchronize layout and cached cursor position.
    pub(super) fn update_layout(&mut self, view_rect: Rect, gutter_width: u32) {
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
        let cursor_point = Point {
            x: point.x.saturating_add(gutter_width),
            y: point.y,
        };
        self.cursor_point = Some(cursor_point);
        self.cursor_view_point = view_rect.contains_point(cursor_point).then(|| Point {
            x: cursor_point.x - view_rect.tl.x,
            y: cursor_point.y - view_rect.tl.y,
        });
    }

    /// Ensure the cursor is visible within the current scroll view.
    pub(super) fn ensure_cursor_visible(&mut self, ctx: &mut dyn Context) {
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
    pub(super) fn update_preferred_column(&mut self) {
        self.preferred_column = self
            .buffer
            .column_for_position(self.buffer.cursor(), self.config.tab_stop);
    }

    /// Move vertically by logical lines, preserving preferred column.
    pub(super) fn move_vertical(&mut self, delta: isize) {
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
    pub(super) fn move_display_line(&mut self, delta: isize, ctx: &dyn Context) {
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

    /// Insert text at the cursor, respecting read-only state.
    pub(super) fn handle_insert_text(&mut self, text: &str) {
        if self.config.read_only {
            return;
        }
        let content = self.normalize_insert_text(text);
        self.buffer.insert_text(&content);
        self.update_preferred_column();
    }

    /// Normalize inserted text for single-line editors.
    pub(super) fn normalize_insert_text(&self, text: &str) -> String {
        if self.config.multiline {
            text.to_string()
        } else {
            text.replace(['\n', '\r'], " ")
        }
    }

    /// Delete backward respecting selection and multiline rules.
    pub(super) fn handle_delete_backward(&mut self) -> bool {
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
    pub(super) fn handle_delete_forward(&mut self) -> bool {
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
    pub(super) fn delete_char_forward(&mut self) -> bool {
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
    pub(super) fn handle_paste(&mut self, text: &str) -> String {
        let content = self.normalize_insert_text(text);
        if self.config.read_only {
            return String::new();
        }
        self.buffer.insert_text(&content);
        self.update_preferred_column();
        content
    }

    /// Begin a grouped text-entry transaction if needed.
    pub(super) fn begin_text_entry_transaction(&mut self) {
        if !self.text_entry_transaction {
            self.buffer.begin_transaction();
            self.text_entry_transaction = true;
        }
    }

    /// Commit the active text-entry transaction if present.
    pub(super) fn commit_text_entry_transaction(&mut self) {
        if self.text_entry_transaction {
            self.buffer.commit_transaction();
            self.text_entry_transaction = false;
        }
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

    /// Handle mouse interactions for selection and cursor movement.
    fn handle_mouse_event(
        &mut self,
        event: &mouse::MouseEvent,
        ctx: &mut dyn Context,
    ) -> Result<bool> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let gutter_width = self.gutter_width();
        self.update_layout(view_rect, gutter_width);
        if event.location.x < origin.x || event.location.y < origin.y {
            return Ok(false);
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

        Ok(match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                ctx.set_focus(ctx.node_id())?;
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
        })
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

        let base_text_style = ctx.r.resolve_style_name_raw("editor/text");

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
            let width = display_width(grapheme, col, self.config.tab_stop);

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
                        let mut span_style = span.style.clone();
                        span_style.bg = base_text_style.bg.clone();
                        style = Some(span_style);
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
                    let resolved = match style.as_ref() {
                        Some(custom) => ctx.r.resolve_style_at(custom.clone(), line_rect, p),
                        None => ctx.r.resolve_style_name_at(style_name, line_rect, p),
                    };
                    ctx.r.put_cell(resolved, p, ' ')?;
                }
            } else {
                let x = draw_col.saturating_sub(view_start) as u32;
                let p = Point {
                    x: ctx.origin.x.saturating_add(x),
                    y: line_y,
                };
                let resolved = match style.as_ref() {
                    Some(custom) => ctx.r.resolve_style_at(custom.clone(), line_rect, p),
                    None => ctx.r.resolve_style_name_at(style_name, line_rect, p),
                };
                ctx.r.put_grapheme(resolved, p, grapheme)?;
            }

            col = col.saturating_add(width);
            char_index = g_end;
        }

        Ok(())
    }

    /// Move the cursor.
    /// @param dir The direction to move the cursor.
    #[command]
    pub fn cursor(&mut self, ctx: &mut dyn Context, dir: Direction) {
        match dir {
            Direction::Left => {
                let _ = self.buffer.move_left(self.config.multiline);
                self.update_preferred_column();
            }
            Direction::Right => {
                let _ = self.buffer.move_right(self.config.multiline);
                self.update_preferred_column();
            }
            Direction::Up => {
                self.move_vertical(-1);
            }
            Direction::Down => {
                self.move_vertical(1);
            }
        }
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
        let location = self.cursor_view_point?;
        let shape = match self.config.mode {
            EditMode::Text => cursor::CursorShape::Line,
            EditMode::Vi => match self.vi.mode() {
                ViMode::Insert => cursor::CursorShape::Line,
                _ => cursor::CursorShape::Block,
            },
        };
        Some(cursor::Cursor { location, shape })
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
        let wrap_width = view.w.saturating_sub(gutter).max(1) as usize;
        let height = display_line_count(
            &self.buffer,
            self.config.wrap,
            wrap_width,
            self.config.tab_stop,
        ) as u32;
        let width = match self.config.wrap {
            WrapMode::None => {
                let max_width = display_line_width(&self.buffer, self.config.tab_stop) as u32;
                max_width.saturating_add(gutter).max(view.w.max(1))
            }
            WrapMode::Soft => view.w.max(1),
        };
        Size::new(width.max(1), height.max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
            match mouse_event.action {
                mouse::Action::ScrollUp if ctx.scroll_by(0, -WHEEL_SCROLL_LINES) => {
                    return Ok(EventOutcome::Handle);
                }
                mouse::Action::ScrollDown if ctx.scroll_by(0, WHEEL_SCROLL_LINES) => {
                    return Ok(EventOutcome::Handle);
                }
                mouse::Action::ScrollLeft if ctx.scroll_by(-WHEEL_SCROLL_LINES, 0) => {
                    return Ok(EventOutcome::Handle);
                }
                mouse::Action::ScrollRight if ctx.scroll_by(WHEEL_SCROLL_LINES, 0) => {
                    return Ok(EventOutcome::Handle);
                }
                _ => {}
            }

            let handled = self.handle_mouse_event(mouse_event, ctx)?;
            if handled {
                self.ensure_cursor_visible(ctx);
                return Ok(EventOutcome::Handle);
            }
        }

        Ok(match self.config.mode {
            EditMode::Text => self.handle_text_entry_event(event, ctx),
            EditMode::Vi => self.handle_vi_event(event, ctx),
        })
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
pub(super) fn prompt_text(prompt: &PromptState) -> String {
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
pub(super) fn is_word_char(ch: char) -> bool {
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
