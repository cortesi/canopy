use std::{collections::HashMap, time::Duration};

use canopy::{
    Context, EventOutcome, FocusDirection, NodeName, Render, RevealAlign, ScrollAxis, ScrollMark,
    ViewContext, Widget, cursor, derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::{Line, Point, Rect, Size},
    layout::{CanvasContext, Constraint, MeasureConstraints, Measurement},
    style::Style,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    EditMode, EditorConfig, LineNumbers, WrapMode, display_width,
    highlight::{HighlightSpan, Highlighter},
    layout::{LayoutCache, LineLayout, WrapSegment, layout_line, metrics, point_for_position},
    search::{PromptState, SearchDirection, SearchState},
    vi::{ViMode, ViState},
};
use crate::{
    click::ClickTracker,
    scrollbar::THIN,
    text_buffer::{Selection, TextBuffer, TextPosition, TextRange, single_line},
};

/// Maximum delay between clicks to count as multi-click selection.
const DOUBLE_CLICK_MS: u64 = 500;

/// Rows of context kept above a revealed search match, when space allows.
const SEARCH_MATCH_TOP_CONTEXT: u32 = 3;

/// Glyph of a search-match mark on a scrollbar track.
///
/// This is the shared thin track vertical, so a mark blends into the
/// surrounding divider or border and only its color stands out.
const SEARCH_MARK: char = THIN.track_vertical;

/// Editor widget implementation.
pub struct Editor {
    /// Editor configuration.
    pub(super) config: EditorConfig,
    /// Text buffer backing the editor.
    pub(super) buffer: TextBuffer,
    /// Layout cache for wrapping and mapping.
    pub(super) layout: LayoutCache,
    /// Cached cursor position in view coordinates.
    pub(super) cursor_view_point: Option<Point>,
    /// Preferred display column for vertical movement.
    pub(super) preferred_column: usize,
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
    click_state: ClickTracker,
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
    /// Cell styles for plain text, selection, the current search match, and
    /// other matches. The effect stack is constant for one widget render,
    /// so these resolve once per frame.
    styles: CellStyles,
}

/// The four effect-applied cell styles a display line draws with.
struct CellStyles {
    /// Style for unhighlighted text.
    text: Style,
    /// Style for selected text.
    selection: Style,
    /// Style for the current search match.
    search_current: Style,
    /// Style for other search matches.
    search_match: Style,
}

impl<'a, 'b> RenderLineContext<'a, 'b> {
    /// Construct a new render context, resolving the fixed cell styles once.
    fn new(r: &'a mut Render<'b>, view_rect: Rect, origin: Point, gutter_width: u32) -> Self {
        let styles = CellStyles {
            text: r.resolve_style("editor/text"),
            selection: r.resolve_style("editor/selection"),
            search_current: r.resolve_style("editor/search/current"),
            search_match: r.resolve_style("editor/search/match"),
        };
        Self {
            r,
            view_rect,
            origin,
            gutter_width,
            styles,
        }
    }
}

/// Cache of syntax highlight spans keyed by buffer revision and line index.
#[derive(Debug, Clone)]
pub(super) struct HighlightCache {
    /// Prepared buffer revision, or `None` when preparation is pending.
    revision: Option<u64>,
    /// Cached spans per line.
    lines: HashMap<usize, Vec<HighlightSpan>>,
}

impl HighlightCache {
    /// Construct an empty highlight cache.
    fn new() -> Self {
        Self {
            revision: None,
            lines: HashMap::new(),
        }
    }

    /// Invalidate preparation and cached spans.
    fn clear(&mut self) {
        self.revision = None;
        self.lines.clear();
    }

    /// Reset the cache when the buffer revision changes, reporting whether it
    /// did.
    fn sync_revision(&mut self, revision: u64) -> bool {
        if self.revision == Some(revision) {
            return false;
        }
        self.revision = Some(revision);
        self.lines.clear();
        true
    }

    /// Return cached spans for a line or compute and store them.
    fn spans_for_line(
        &mut self,
        line: usize,
        compute: impl FnOnce() -> Vec<HighlightSpan>,
    ) -> &[HighlightSpan] {
        self.lines.entry(line).or_insert_with(compute)
    }
}

#[derive_commands]
impl Editor {
    /// Construct an editor with a configuration.
    pub fn with_config(text: impl Into<String>, config: EditorConfig) -> Self {
        let mut buffer = TextBuffer::new(text);
        buffer.set_cursor(TextPosition::new(0, 0));
        let preferred_column = buffer.column_for_position(buffer.cursor(), config.tab_stop);
        Self {
            config,
            buffer,
            layout: LayoutCache::new(),
            cursor_view_point: None,
            preferred_column,
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
        self.layout = LayoutCache::new();
        self.search = SearchState::new();
    }

    /// Install a syntax highlighter.
    ///
    /// Preparation is deferred until the next render, using the latest buffer
    /// contents. Repeated changes before rendering prepare only the final
    /// state.
    pub fn set_highlighter(&mut self, highlighter: Option<Box<dyn Highlighter>>) {
        self.highlighter = highlighter;
        self.highlight_cache.clear();
    }

    /// Return a reference to the internal buffer.
    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Return the buffer for editing, or `None` when the editor is read-only.
    ///
    /// Every path that changes buffer contents goes through this accessor, so
    /// read-only is decided in one place instead of at each call site.
    pub(super) fn editable(&mut self) -> Option<&mut TextBuffer> {
        if self.config.read_only {
            return None;
        }
        Some(&mut self.buffer)
    }

    /// Replace `range` with `text`, reporting whether the edit was applied.
    ///
    /// Does nothing when the editor is read-only.
    pub(super) fn replace_range(&mut self, range: TextRange, text: &str) -> bool {
        let Some(buffer) = self.editable() else {
            return false;
        };
        buffer.replace_range(range, text);
        true
    }

    /// Undo the last edit unless the editor is read-only.
    pub(super) fn undo_edit(&mut self) -> bool {
        let Some(buffer) = self.editable() else {
            return false;
        };
        buffer.undo();
        true
    }

    /// Redo the last undone edit unless the editor is read-only.
    pub(super) fn redo_edit(&mut self) -> bool {
        let Some(buffer) = self.editable() else {
            return false;
        };
        buffer.redo();
        true
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

    /// Return `(display_line_count, max_line_width)` for a wrap width. The
    /// layout cache serves the answer when it is current; otherwise the
    /// buffer is scanned directly.
    fn display_metrics(&self, wrap_width: usize) -> (usize, usize) {
        self.layout
            .metrics_for(
                &self.buffer,
                wrap_width,
                self.config.wrap,
                self.config.tab_stop,
            )
            .unwrap_or_else(|| {
                metrics(
                    &self.buffer,
                    self.config.wrap,
                    wrap_width,
                    self.config.tab_stop,
                )
            })
    }

    /// Synchronize layout and return the cursor position in content
    /// coordinates.
    pub(super) fn update_layout(&mut self, view_rect: Rect, gutter_width: u32) -> Point {
        let wrap_width = view_rect.w.saturating_sub(gutter_width).max(1) as usize;
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
        self.cursor_view_point = view_rect.contains_point(cursor_point).then(|| Point {
            x: cursor_point.x - view_rect.tl.x,
            y: cursor_point.y - view_rect.tl.y,
        });
        cursor_point
    }

    /// Synchronize the wrap layout and reveal the cursor once layout settles.
    ///
    /// The reveal reads the cursor from [`Widget::reveal_anchor`] with the
    /// final content width, so edits and resizes in the same turn count.
    pub(super) fn ensure_cursor_visible(&mut self, ctx: &mut dyn Context) {
        let view_rect = ctx.view().view_rect();
        self.update_layout(view_rect, self.gutter_width());
        ctx.reveal_anchor(RevealAlign::Nearest);
    }

    /// Return the cursor cell in content coordinates for a content width.
    ///
    /// A current layout cache answers directly. Otherwise the position is
    /// computed without changing the cache.
    fn cursor_point(&self, width: u32) -> Point {
        let gutter = self.gutter_width();
        let wrap_width = width.saturating_sub(gutter).max(1) as usize;
        let (wrap, tab_stop) = (self.config.wrap, self.config.tab_stop);
        let cursor = self.buffer.cursor();
        let point = if self
            .layout
            .metrics_for(&self.buffer, wrap_width, wrap, tab_stop)
            .is_some()
        {
            self.layout
                .point_for_position(&self.buffer, cursor, tab_stop)
        } else {
            point_for_position(&self.buffer, cursor, wrap, wrap_width, tab_stop)
        };
        Point {
            x: point.x.saturating_add(gutter),
            y: point.y,
        }
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

    /// Insert text at the cursor, respecting read-only state. Returns the
    /// normalized text that was inserted, or an empty string when read-only.
    pub(super) fn handle_insert_text(&mut self, text: &str) -> String {
        let content = self.normalize_insert_text(text);
        let Some(buffer) = self.editable() else {
            return String::new();
        };
        buffer.insert_text(&content);
        self.update_preferred_column();
        content
    }

    /// Normalize inserted text for single-line editors.
    pub(super) fn normalize_insert_text(&self, text: &str) -> String {
        if self.config.multiline {
            text.to_string()
        } else {
            single_line(text)
        }
    }

    /// Delete backward respecting selection and multiline rules.
    pub(super) fn handle_delete_backward(&mut self) -> bool {
        let multiline = self.config.multiline;
        let Some(buffer) = self.editable() else {
            return false;
        };
        let deleted = buffer.delete_backward(multiline);
        if deleted {
            self.update_preferred_column();
        }
        deleted
    }

    /// Delete forward respecting selection and multiline rules.
    pub(super) fn handle_delete_forward(&mut self) -> bool {
        let multiline = self.config.multiline;
        let Some(buffer) = self.editable() else {
            return false;
        };
        let deleted = buffer.delete_forward(multiline);
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
        let Some(range) = self
            .buffer
            .forward_delete_range(cursor, self.config.multiline)
        else {
            return false;
        };
        self.set_yank(range, false);
        self.replace_range(range, "");
        self.update_preferred_column();
        true
    }

    /// Begin a grouped text-entry transaction if needed.
    pub(super) fn begin_text_entry_transaction(&mut self) {
        self.buffer.begin_transaction();
    }

    /// Commit the active text-entry transaction if present.
    pub(super) fn commit_text_entry_transaction(&mut self) {
        self.buffer.commit_transaction();
    }

    /// Return whether `event` edits text in text-entry mode.
    fn is_text_edit(event: &Event) -> bool {
        matches!(
            event,
            Event::Paste(_)
                | Event::Key(key::Key {
                    key: key::KeyCode::Char(_)
                        | key::KeyCode::Backspace
                        | key::KeyCode::Delete
                        | key::KeyCode::Enter,
                    ..
                })
        )
    }

    /// Handle events in text-entry mode.
    ///
    /// A read-only editor ignores edit input, so application bindings see it.
    fn handle_text_entry_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if self.config.read_only && Self::is_text_edit(event) {
            return EventOutcome::Ignore;
        }
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                mods,
            }) if !mods.ctrl && !mods.alt => {
                self.begin_text_entry_transaction();
                self.handle_insert_text(c.encode_utf8(&mut [0; 4]));
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
                self.handle_insert_text(content);
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
        let gutter_width = self.gutter_width();
        self.update_layout(view_rect, gutter_width);
        // A drag beyond the top or left edge selects toward the start.
        let content_point = view.viewport_to_content(event.location)?.clamped_point();
        let mut text_point = content_point;
        text_point.x = text_point.x.saturating_sub(gutter_width);
        let pos = self
            .layout
            .position_for_point(&self.buffer, text_point, self.config.tab_stop);

        Ok(match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                ctx.set_focus(ctx.node_id())?;
                match self.mouse.click_state.count(content_point) {
                    2 => {
                        let range = word_range(&self.buffer, pos);
                        self.mouse.selecting = true;
                        self.mouse.anchor = Some(range.start);
                        self.buffer
                            .set_selection(Selection::new(range.start, range.end));
                    }
                    3 => {
                        let start = TextPosition::new(pos.line, 0);
                        let end = self.buffer.line_end_position(pos.line, true);
                        self.mouse.selecting = true;
                        self.mouse.anchor = Some(start);
                        self.buffer.set_selection(Selection::new(start, end));
                    }
                    _ => {
                        self.mouse.selecting = true;
                        self.mouse.anchor = Some(pos);
                        self.buffer.set_selection(Selection::new(pos, pos));
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

        if ctx.gutter_width > 0 {
            let gutter_line = Line::new(ctx.origin.x, line_y, ctx.gutter_width);
            let number_text = line_number_text(
                self.config.line_numbers == LineNumbers::Relative,
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

        let highlight_spans: &[HighlightSpan] = match &self.highlighter {
            Some(highlighter) => {
                let buffer = &self.buffer;
                self.highlight_cache.spans_for_line(line_idx, || {
                    highlighter.highlight_line(line_idx, &buffer.line_text(line_idx))
                })
            }
            None => &[],
        };

        let mut span_idx = 0usize;
        let mut span_style: Option<Style> = None;
        let search_ranges = self.search.matches_for_line(line_idx);
        let current_search = self
            .search
            .current_match()
            .filter(|r| r.start.line == line_idx);
        let current_search_range = current_search.map(|r| (r.start.column, r.end.column));

        let segment_text = self.buffer.range_text(TextRange::new(
            TextPosition::new(line_idx, segment.start_char),
            TextPosition::new(line_idx, segment.end_char),
        ));

        let mut col = segment.start_col;
        let mut char_index = segment.start_char;
        for grapheme in segment_text.graphemes(true) {
            let grapheme_chars = grapheme.chars().count();
            let width = display_width(grapheme, col, self.config.tab_stop);

            let g_start = char_index;
            let g_end = char_index.saturating_add(grapheme_chars);

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

            let mut style = &ctx.styles.text;

            if selection_on_line && g_start < line_end_sel && g_end > line_start_sel {
                style = &ctx.styles.selection;
            } else if current_search_range
                .is_some_and(|(start, end)| g_start < end && g_end > start)
            {
                style = &ctx.styles.search_current;
            } else if search_ranges
                .iter()
                .any(|range| g_start < range.end.column && g_end > range.start.column)
            {
                style = &ctx.styles.search_match;
            } else {
                while let Some(span) = highlight_spans.get(span_idx) {
                    if span.range.end <= g_start {
                        span_idx = span_idx.saturating_add(1);
                        span_style = None;
                        continue;
                    }
                    if span.range.start < g_end && span.range.end > g_start {
                        let resolved = span_style.get_or_insert_with(|| {
                            let mut merged = span.style.clone();
                            merged.bg = ctx.styles.text.bg.clone();
                            ctx.r.apply_effects(merged)
                        });
                        style = resolved;
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
                    ctx.r.put_cell(style.resolve_at(line_rect, p), p, ' ')?;
                }
            } else {
                let x = draw_col.saturating_sub(view_start) as u32;
                let p = Point {
                    x: ctx.origin.x.saturating_add(x),
                    y: line_y,
                };
                ctx.r
                    .put_grapheme(style.resolve_at(line_rect, p), p, grapheme)?;
            }

            col = col.saturating_add(width);
            char_index = g_end;
        }

        Ok(())
    }

    /// Move the cursor.
    /// @param dir The direction to move the cursor.
    #[command]
    pub fn move_cursor(&mut self, ctx: &mut dyn Context, dir: FocusDirection) {
        match dir {
            FocusDirection::Left | FocusDirection::Prev => {
                let _ = self.buffer.move_left(self.config.multiline);
                self.update_preferred_column();
            }
            FocusDirection::Right | FocusDirection::Next => {
                let _ = self.buffer.move_right(self.config.multiline);
                self.update_preferred_column();
            }
            FocusDirection::Up => {
                self.move_vertical(-1);
            }
            FocusDirection::Down => {
                self.move_vertical(1);
            }
        }
        self.ensure_cursor_visible(ctx);
    }

    /// Undo the last edit.
    #[command]
    pub fn undo(&mut self, _ctx: &mut dyn Context) {
        self.undo_edit();
        self.update_preferred_column();
    }

    /// Redo the last undone edit.
    #[command]
    pub fn redo(&mut self, _ctx: &mut dyn Context) {
        self.redo_edit();
        self.update_preferred_column();
    }

    /// Search the text and highlight every match.
    ///
    /// The first match at or below the top of the view becomes current, and
    /// the view scrolls to it. A query without uppercase letters ignores ASCII
    /// case. An empty query clears the search.
    /// @param query Text to find.
    #[command]
    pub fn search(&mut self, ctx: &mut dyn Context, query: String) {
        if query.is_empty() {
            self.search = SearchState::new();
            return;
        }
        let view_rect = ctx.view().view_rect();
        self.update_layout(view_rect, self.gutter_width());
        let top = match self.layout.total_lines() {
            0 => 0,
            lines => self
                .layout
                .line_for_display((view_rect.tl.y as usize).min(lines - 1)),
        };
        self.search
            .set_smart_query(&self.buffer, query, TextPosition::new(top, 0));
        self.reveal_search_match(ctx);
    }

    /// Highlight precomputed match ranges.
    ///
    /// The first match at or below the top of the view becomes current, and
    /// the view scrolls to it. Ranges must arrive in ascending order without
    /// spanning lines; see [`SearchState::set_matches`]. An empty set clears
    /// the search. Unlike [`Self::search`], the ranges need not come from a
    /// literal query, so callers can highlight regular-expression matches.
    pub fn highlight_matches(&mut self, ctx: &mut dyn Context, matches: Vec<TextRange>) {
        if matches.is_empty() {
            self.search = SearchState::new();
            return;
        }
        let view_rect = ctx.view().view_rect();
        self.update_layout(view_rect, self.gutter_width());
        let top = match self.layout.total_lines() {
            0 => 0,
            lines => self
                .layout
                .line_for_display((view_rect.tl.y as usize).min(lines - 1)),
        };
        self.search
            .set_matches(&self.buffer, matches, TextPosition::new(top, 0));
        self.reveal_search_match(ctx);
    }

    /// Make another search match current and scroll to it.
    /// @param delta Matches to move; negative values move backward.
    #[command]
    pub fn search_next(&mut self, ctx: &mut dyn Context, delta: i32) {
        let count = self.search.match_count();
        if count == 0 {
            return;
        }
        for _ in 0..(delta.unsigned_abs() as usize % count) {
            self.search.move_next(&self.buffer, delta < 0);
        }
        self.reveal_search_match(ctx);
    }

    /// Remove the search and its highlights.
    #[command]
    pub fn clear_search(&mut self, _ctx: &mut dyn Context) {
        self.search = SearchState::new();
    }

    /// Return the number of search matches.
    #[command]
    pub fn search_matches(&self) -> usize {
        self.search.match_count()
    }

    /// Return the one-based position of the current search match.
    /// @return The position, or 0 when there is no current match.
    #[command]
    pub fn search_position(&self) -> usize {
        self.search.current_index().map_or(0, |index| index + 1)
    }

    /// Return one display row per match for the scrollbar track.
    ///
    /// Matches arrive in ascending order without spanning lines, so one
    /// forward pass maps them all: lines are visited monotonically, and each
    /// lays out at most twice, once while advancing and once for its own
    /// matches. The layout cache answers when it is current for `content`;
    /// otherwise lines lay out directly, which still costs a single scan no
    /// matter how many matches share a line. Same-row matches collapse into
    /// one mark, so marks never outnumber the rows they sit on. A match past
    /// the end of the buffer belongs to a changed buffer and is skipped.
    fn match_rows(&self, content: Size) -> Vec<u32> {
        if self.config.wrap == WrapMode::None {
            let mut rows = Vec::new();
            for range in self.search.matches() {
                if range.start.line >= self.buffer.line_count() {
                    continue;
                }
                let row = u32::try_from(range.start.line).unwrap_or(u32::MAX);
                if rows.last() != Some(&row) {
                    rows.push(row);
                }
            }
            return rows;
        }
        let gutter = self.gutter_width();
        let wrap_width = content.w.saturating_sub(gutter).max(1) as usize;
        let (wrap, tab_stop) = (self.config.wrap, self.config.tab_stop);
        let layout = &self.layout;
        let buffer = &self.buffer;
        let cached = layout
            .metrics_for(buffer, wrap_width, wrap, tab_stop)
            .is_some();
        let mut memo: Option<(usize, LineLayout)> = None;
        let mut layout_of = |index: usize| -> LineLayout {
            if let Some((line, laid_out)) = &memo
                && *line == index
            {
                return laid_out.clone();
            }
            let laid_out = if cached {
                layout.line(index).cloned()
            } else {
                None
            }
            .unwrap_or_else(|| layout_line(&buffer.line_text(index), wrap, wrap_width, tab_stop));
            memo = Some((index, laid_out.clone()));
            laid_out
        };
        let mut rows = Vec::new();
        let mut line_idx = 0usize;
        let mut row = 0u32;
        for range in self.search.matches() {
            let line = range.start.line;
            if line >= buffer.line_count() {
                continue;
            }
            while line_idx < line {
                row = row.saturating_add(layout_of(line_idx).display_lines() as u32);
                line_idx += 1;
            }
            let column = buffer.column_for_position(range.start, tab_stop);
            let segment = layout_of(line).segment_for_column(column) as u32;
            let mark = row.saturating_add(segment);
            if rows.last() != Some(&mark) {
                rows.push(mark);
            }
        }
        rows
    }

    /// Put the cursor on the current search match and scroll it into view.
    ///
    /// The match lands [`SEARCH_MATCH_TOP_CONTEXT`] rows below the top of the
    /// view when space allows, so its surroundings read above it; near the
    /// start or end of the text the view clamps to its edge instead. The
    /// reveal queues behind layout, so it resolves against settled geometry.
    fn reveal_search_match(&mut self, ctx: &mut dyn Context) {
        let Some(range) = self.search.current_match() else {
            return;
        };
        self.buffer.set_cursor(range.start);
        self.update_preferred_column();
        ctx.reveal_anchor(RevealAlign::Top(SEARCH_MATCH_TOP_CONTEXT));
    }
}

impl Widget for Editor {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.config.focusable
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
        if self.highlight_cache.sync_revision(self.buffer.revision())
            && let Some(highlighter) = &self.highlighter
        {
            highlighter.prepare(&self.buffer.text());
        }

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
            self.display_metrics(wrap_width).0 as u32
        } else {
            self.config.min_height.max(1)
        };
        if let Some(max) = self.config.max_height {
            height = height.min(max.max(1));
        }
        height = height.max(self.config.min_height.max(1));
        c.clamp(Size::new(width, height))
    }

    fn reveal_anchor(&self, view: Size) -> Option<Rect> {
        let mut cursor = self.cursor_point(view.w);
        // Soft-wrapped text never scrolls sideways.
        if self.config.wrap == WrapMode::Soft {
            cursor.x = 0;
        }
        Some(Rect::new(cursor.x, cursor.y, 1, 1))
    }

    fn scroll_marks(&self, axis: ScrollAxis, content: Size) -> Vec<ScrollMark> {
        if axis != ScrollAxis::Vertical {
            return Vec::new();
        }
        // Matches never span lines, so each one marks a single display row.
        // The mark style carries the match color as a foreground: text
        // match styles put it in the background, which a block thumb glyph
        // would hide on scroll-over.
        self.match_rows(content)
            .into_iter()
            .map(|row| ScrollMark {
                start: row,
                end: row.saturating_add(1),
                style: "editor/search/mark",
                glyph: SEARCH_MARK,
            })
            .collect()
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let gutter = self.gutter_width();
        let wrap_width = view.w.saturating_sub(gutter).max(1) as usize;
        let (line_count, max_line_width) = self.display_metrics(wrap_width);
        let width = match self.config.wrap {
            // One more column holds the caret after the longest line.
            WrapMode::None => (max_line_width as u32)
                .saturating_add(1)
                .saturating_add(gutter)
                .max(view.w.max(1)),
            WrapMode::Soft => view.w.max(1),
        };
        Size::new(width.max(1), (line_count as u32).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
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
            click_state: ClickTracker::new(Duration::from_millis(DOUBLE_CLICK_MS)),
        }
    }
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
///
/// A relative gutter shows the distance to the cursor line, and the cursor
/// line's own number.
fn line_number_text(relative: bool, line: usize, cursor_line: usize, width: u32) -> String {
    let number = if relative && line != cursor_line {
        line.abs_diff(cursor_line)
    } else {
        line + 1
    };
    format!(
        "{number:>width$} ",
        width = width.saturating_sub(1) as usize
    )
}

/// Determine if a character counts as a word constituent.
pub(super) fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
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
