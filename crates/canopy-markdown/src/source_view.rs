//! Read-only Markdown source presentation over immutable accepted UTF-8 bytes.

use std::{
    cell::Cell, error::Error as StdError, fmt, iter::Peekable, mem::size_of, ops::Range,
    str::CharIndices, sync::Arc,
};

use canopy::{
    Context, EventOutcome, NodeName, Register, Setup, ViewContext, Widget,
    error::Result as UiResult,
    geom::{Point, Rect, Size},
    input::{Event, key, mouse},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement, ScrollAxis, ScrollMark},
    render::Render,
};
#[cfg(feature = "syntax")]
use canopy_widgets::highlight::{Grammar, Highlighter, SyntectHighlighter};
use canopy_widgets::{
    editor::{
        EditMode, Editor, EditorConfig, Interaction, LineNumbers, PreparedEditorView, Selection,
        TextPosition, TextRange, ViMode, WrapMode,
    },
    highlight::{HighlightSpan, palette_style_rules},
};

use crate::search::{CompiledQuery, SearchCase, SearchLimits, SearchMode, SearchQuery};

/// Geometry and automatic syntax-work bounds for a source presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOptions {
    /// Soft wrapping or horizontal scrolling.
    pub wrap: WrapMode,
    /// Optional line-number gutter.
    pub line_numbers: LineNumbers,
    /// Tab stops, in terminal cells.
    pub tab_stop: usize,
    /// Maximum source and terminal-safe display bytes to highlight
    /// automatically.
    pub syntax_limit: usize,
    /// Fence language information, or `None` for plain text. Accepts grammar
    /// names, extensions, aliases, and comma suffixes such as `rust,ignore`.
    /// Markdown uses `md`; unknown languages remain plain text.
    pub language: Option<Arc<str>>,
}

impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            wrap: WrapMode::Soft,
            line_numbers: LineNumbers::None,
            tab_stop: 4,
            syntax_limit: 256 * 1024,
            language: Some(Arc::from("md")),
        }
    }
}

/// A source location within one document epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAnchor {
    /// Document epoch.
    pub epoch: u64,
    /// UTF-8 byte position in the accepted source.
    pub byte: usize,
}

/// Source publication, range, or bounded-output failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// Same-epoch publication would change an already accepted prefix.
    NonAppend,
    /// A publication is older than the displayed revision.
    Stale,
    /// An unfinished drag pins the current geometry.
    DragActive,
    /// An anchor belongs to another document.
    WrongEpoch,
    /// A range is outside the source or cuts a UTF-8 character.
    InvalidRange,
    /// Exact output would exceed its configured byte budget.
    OutputLimit,
    /// The query exceeds its configured byte budget.
    QueryLimit,
    /// A regular expression failed to compile or match.
    Regex(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonAppend => f.write_str("source publication changes the accepted prefix"),
            Self::Stale => f.write_str("source publication is stale"),
            Self::DragActive => f.write_str("source selection drag is active"),
            Self::WrongEpoch => f.write_str("source location belongs to another epoch"),
            Self::InvalidRange => f.write_str("invalid UTF-8 source range"),
            Self::OutputLimit => f.write_str("source copy exceeds the output limit"),
            Self::QueryLimit => f.write_str("source query exceeds the query limit"),
            Self::Regex(error) => f.write_str(error),
        }
    }
}

impl StdError for SourceError {}

/// Matching semantics over raw source lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSearchMode {
    /// Case-sensitive literal matching.
    Literal,
    /// The shared ripgrep regular-expression matcher.
    Regex,
}

/// A bounded source query, suitable for preparation on a worker.
#[derive(Debug, Clone)]
pub struct SourceQuery {
    /// Raw query, retained by the host even when compilation fails.
    pub text: String,
    /// Literal or regular-expression matching.
    pub mode: SourceSearchMode,
    /// Sensitive, insensitive, or smart-case matching.
    pub case: SearchCase,
    /// Maximum query bytes before compilation.
    pub max_query_bytes: usize,
    /// Maximum stored nonempty matches.
    pub max_matches: usize,
    /// Maximum source bytes covered by this search.
    pub max_scan_bytes: usize,
}

impl SourceQuery {
    /// Construct a literal query with bounded work and result storage.
    pub fn literal(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            mode: SourceSearchMode::Literal,
            case: SearchCase::Sensitive,
            max_query_bytes: 4096,
            max_matches: 10_000,
            max_scan_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A bounded match window over a specific immutable revision.
#[derive(Debug, Clone)]
pub struct SourceMatches {
    /// Document epoch.
    pub epoch: u64,
    /// Displayed source revision.
    pub revision: u64,
    /// Exact source byte ranges, in ascending order.
    pub ranges: Vec<Range<usize>>,
    /// Whether the full source was scanned without exhausting result storage.
    pub complete: bool,
}

/// A sparse source/display checkpoint. At most 64 source characters separate
/// marks.
#[derive(Debug, Clone, Copy)]
struct MapMark {
    /// Accepted UTF-8 byte offset at this checkpoint.
    byte: usize,
    /// Terminal-safe display character offset at this checkpoint.
    display: usize,
}

/// Sparse source/display mapping, sharing immutable accepted bytes.
#[derive(Debug)]
struct SourceMap {
    /// Shared exact bytes used for bounded scans between checkpoints.
    source: Arc<str>,
    /// Sparse source and display offsets, in ascending order.
    marks: Vec<MapMark>,
    /// Display character offset of each logical line.
    lines: Vec<usize>,
    /// Total number of terminal-safe display characters.
    display_len: usize,
}

/// Ordinary source characters require no temporary allocation while indexing.
enum DisplayToken {
    /// One ordinary character or visible control-character symbol.
    Character(char),
    /// An ASCII escape for a control without a single visible symbol.
    Escape(String),
}

impl DisplayToken {
    /// Return the number of logical display characters in the token.
    fn char_len(&self) -> usize {
        match self {
            Self::Character(_) => 1,
            Self::Escape(text) => text.len(),
        }
    }

    /// Append the terminal-safe representation to the display buffer.
    fn push_to(&self, display: &mut String) {
        match self {
            Self::Character(ch) => display.push(*ch),
            Self::Escape(text) => display.push_str(text),
        }
    }
}

/// Return a safe display token and the accepted byte interval it represents.
fn display_token(
    start: usize,
    ch: char,
    chars: &mut Peekable<CharIndices<'_>>,
) -> (DisplayToken, usize) {
    let mut end = start + ch.len_utf8();
    let token = if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
        let _ = chars.next();
        end += 1;
        DisplayToken::Character('\n')
    } else if ch == '\n' || ch == '\t' {
        DisplayToken::Character(ch)
    } else if ch <= '\u{1f}' {
        DisplayToken::Character(char::from_u32(0x2400 + ch as u32).unwrap_or('�'))
    } else if ch == '\u{7f}' {
        DisplayToken::Character('␡')
    } else if ch.is_control()
        || matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}')
    {
        DisplayToken::Escape(format!("\\u{{{:x}}}", ch as u32))
    } else {
        DisplayToken::Character(ch)
    };
    (token, end)
}

impl SourceMap {
    /// Build terminal-safe text, sparse checkpoints, and logical line starts.
    fn prepare(source: Arc<str>) -> (String, Self) {
        let mut display = String::with_capacity(source.len());
        let mut marks = vec![MapMark {
            byte: 0,
            display: 0,
        }];
        let mut lines = vec![0];
        let mut display_len = 0;
        let mut count = 0;
        let mut chars = source.char_indices().peekable();
        while let Some((start, ch)) = chars.next() {
            let (token, end) = display_token(start, ch, &mut chars);
            token.push_to(&mut display);
            display_len += token.char_len();
            if matches!(token, DisplayToken::Character('\n')) {
                lines.push(display_len);
            }
            count += 1;
            if count == 64 {
                marks.push(MapMark {
                    byte: end,
                    display: display_len,
                });
                count = 0;
            }
        }
        (
            display,
            Self {
                source,
                marks,
                lines,
                display_len,
            },
        )
    }

    /// Map a display character position to its accepted source byte boundary.
    fn byte(&self, position: TextPosition) -> usize {
        let line = position.line.min(self.lines.len() - 1);
        let start = self.lines[line];
        let end = self
            .lines
            .get(line + 1)
            .map_or(self.display_len, |next| next - 1);
        let target = start.saturating_add(position.column).min(end);
        let mark = self.marks[self
            .marks
            .partition_point(|mark| mark.display <= target)
            .saturating_sub(1)];
        let mut display = mark.display;
        let mut chars = self.source[mark.byte..].char_indices().peekable();
        while let Some((relative, ch)) = chars.next() {
            let (token, end) = display_token(relative, ch, &mut chars);
            let count = token.char_len();
            if target < display + count {
                return mark.byte + relative;
            }
            display += count;
            if target == display {
                return mark.byte + end;
            }
        }
        self.source.len()
    }

    /// Map an accepted byte boundary to its nearest preceding display position.
    fn position(&self, byte: usize) -> TextPosition {
        let mark = self.marks[self
            .marks
            .partition_point(|mark| mark.byte <= byte)
            .saturating_sub(1)];
        let mut display = mark.display;
        let mut chars = self.source[mark.byte..].char_indices().peekable();
        while let Some((relative, ch)) = chars.next() {
            if mark.byte + relative >= byte {
                break;
            }
            let (token, end) = display_token(relative, ch, &mut chars);
            if mark.byte + end > byte {
                break;
            }
            display += token.char_len();
        }
        let line = self
            .lines
            .partition_point(|start| *start <= display)
            .saturating_sub(1);
        TextPosition::new(line, display - self.lines[line])
    }
}

/// Owned source geometry and highlighting prepared without UI access.
///
/// Preparation scans the source, builds the byte map, wraps every line, and
/// optionally highlights within the syntax cap. Publishing and painting do
/// not repeat this work. Reprepare at a changed width before publishing it.
#[derive(Debug)]
pub struct PreparedSource {
    /// Epoch identifying the accepted document.
    epoch: u64,
    /// Source revision covered by this preparation.
    revision: u64,
    /// Shared immutable accepted source bytes.
    source: Arc<str>,
    /// Allocated body width used for wrapping.
    width: u32,
    /// Presentation policy captured by this preparation.
    options: SourceOptions,
    /// Sparse exact-byte mapping over terminal-safe display text.
    map: SourceMap,
    /// Worker-prepared text buffer and wrapping geometry.
    editor: PreparedEditorView,
    /// Optional syntax spans, prepared within the source/display byte cap.
    highlights: Option<Vec<Vec<HighlightSpan>>>,
    /// Whether the syntax byte cap disabled automatic highlighting.
    syntax_limited: bool,
    /// Prefix proof checked on the preparing worker against this exact Arc.
    verified_prefix: Option<Arc<str>>,
    /// Retained derived-storage estimate computed before publication.
    derived_bytes: usize,
}

impl PreparedSource {
    /// Prepare one immutable source revision at its allocated body width.
    pub fn new(
        epoch: u64,
        revision: u64,
        source: Arc<str>,
        width: u32,
        options: SourceOptions,
    ) -> Self {
        let (display, map) = SourceMap::prepare(Arc::clone(&source));
        let syntax_limited = options.language.is_some()
            && (source.len() > options.syntax_limit || display.len() > options.syntax_limit);
        let highlights = prepare_highlights(&source, &display, &options);
        let config = EditorConfig::new()
            .with_interaction(Interaction::View)
            .with_mode(EditMode::Vi)
            .with_wrap(options.wrap)
            .with_line_numbers(options.line_numbers)
            .with_tab_stop(options.tab_stop);
        let editor = Editor::prepare_view(display, config, width);
        let derived_bytes = size_of::<MarkdownSource>()
            .saturating_add(editor.derived_bytes())
            .saturating_add(map.marks.capacity().saturating_mul(size_of::<MapMark>()))
            .saturating_add(map.lines.capacity().saturating_mul(size_of::<usize>()))
            .saturating_add(
                options
                    .language
                    .as_ref()
                    .map_or(0, |language| language.len()),
            )
            .saturating_add(
                highlights
                    .as_ref()
                    .map_or(0, |lines| highlight_storage_bytes(lines, lines.capacity())),
            );
        Self {
            epoch,
            revision,
            source,
            width,
            options,
            map,
            editor,
            highlights,
            syntax_limited,
            verified_prefix: None,
            derived_bytes,
        }
    }

    /// Retained derived storage estimate, computed on the preparing worker.
    /// Accepted source bytes and shared executor allocations are excluded.
    pub fn derived_bytes(&self) -> usize {
        self.derived_bytes
    }

    /// Verify an append against the currently displayed immutable source.
    ///
    /// Run this on the preparation worker. Publication recognizes the shared
    /// prefix identity and does not rescan accepted bytes on the UI thread.
    pub fn verify_prefix(mut self, previous: Arc<str>) -> Result<Self, SourceError> {
        if !self.source.starts_with(previous.as_ref()) {
            return Err(SourceError::NonAppend);
        }
        self.verified_prefix = Some(previous);
        Ok(self)
    }
}

/// Source highlighting uses borrowed palette-role paths without nested
/// allocations. Budget a second span copy and conservative map buckets for
/// Editor's render-time line cache, even before those lines have been visited.
fn highlight_storage_bytes(lines: &[Vec<HighlightSpan>], capacity: usize) -> usize {
    let mut bytes = capacity
        .saturating_mul(size_of::<Vec<HighlightSpan>>())
        .saturating_add(
            lines
                .len()
                .saturating_mul(2 * size_of::<(usize, Vec<HighlightSpan>)>()),
        );
    for line in lines {
        bytes = bytes.saturating_add(
            line.capacity()
                .saturating_mul(2 * size_of::<HighlightSpan>()),
        );
    }
    bytes
}

#[cfg(feature = "syntax")]
/// Prepare syntax spans only when both source and display fit the work cap.
fn prepare_highlights(
    source: &str,
    display: &str,
    options: &SourceOptions,
) -> Option<Vec<Vec<HighlightSpan>>> {
    options
        .language
        .as_ref()
        .filter(|_| source.len() <= options.syntax_limit && display.len() <= options.syntax_limit)
        .and_then(|language| Grammar::for_fence(language))
        .map(|grammar| {
            let highlighter = SyntectHighlighter::new("").with_palette_roles();
            highlighter.set_grammar(grammar);
            highlighter.prepare(display);
            display
                .split('\n')
                .enumerate()
                .map(|(line, text)| highlighter.highlight_line(line, text))
                .collect()
        })
}

#[cfg(not(feature = "syntax"))]
/// Retain plain text when automatic syntax support is not compiled.
fn prepare_highlights(
    _source: &str,
    _display: &str,
    _options: &SourceOptions,
) -> Option<Vec<Vec<HighlightSpan>>> {
    None
}

/// Composable read-only Markdown source widget with exact-byte copying.
/// Register this widget to install syntax colors that follow the active
/// palette.
pub struct MarkdownSource {
    /// Epoch of the displayed immutable document.
    epoch: u64,
    /// Revision currently displayed and available for exact copy.
    revision: u64,
    /// Shared exact bytes behind this presentation.
    source: Arc<str>,
    /// Width of the currently published wrapping geometry.
    width: u32,
    /// Preferences used to prepare the displayed text.
    options: SourceOptions,
    /// Sparse map between accepted bytes and display positions.
    map: SourceMap,
    /// Read-only interaction, selection, and prepared line rendering.
    editor: Editor,
    /// New allocated width awaiting host preparation.
    requested_width: Cell<Option<u32>>,
    /// Whether the syntax-work cap suppressed automatic highlighting.
    syntax_limited: bool,
    /// Exact endpoints retained when CRLF or display decoration hides a byte
    /// boundary.
    retained_selection: Option<(usize, usize, Selection)>,
    /// Worker-computed display, layout, map, and highlighting estimate.
    derived_bytes: usize,
    /// Retained precomputed match storage, updated when matches publish.
    search_bytes: usize,
}

impl MarkdownSource {
    /// Construct a standalone widget. Mounted hosts should prepare at their
    /// actual width with [`PreparedSource::new`].
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self::with_options(source, SourceOptions::default())
    }

    /// Construct a standalone widget with explicit presentation options.
    pub fn with_options(source: impl Into<Arc<str>>, options: SourceOptions) -> Self {
        Self::from_prepared(PreparedSource::new(1, 0, source.into(), 80, options))
    }

    /// Mount already prepared data without scanning its text.
    pub fn from_prepared(prepared: PreparedSource) -> Self {
        let syntax_limited = prepared.syntax_limited;
        let mut editor =
            Editor::with_config("", EditorConfig::new().with_interaction(Interaction::View));
        editor.publish_view(prepared.editor, false);
        if let Some(lines) = prepared.highlights {
            editor.set_highlight_spans(lines);
        }
        Self {
            epoch: prepared.epoch,
            revision: prepared.revision,
            source: prepared.source,
            width: prepared.width,
            options: prepared.options,
            map: prepared.map,
            editor,
            requested_width: Cell::new(None),
            syntax_limited,
            retained_selection: None,
            derived_bytes: prepared.derived_bytes,
            search_bytes: 0,
        }
    }

    /// Publish a revision or geometry. Same-epoch bytes must preserve the
    /// prefix. This does not focus, reveal, or scroll the widget.
    pub fn publish(&mut self, prepared: PreparedSource) -> Result<(), SourceError> {
        if self.editor.selection_drag_active() {
            return Err(SourceError::DragActive);
        }
        let preserve = prepared.epoch == self.epoch;
        if preserve && prepared.revision < self.revision {
            return Err(SourceError::Stale);
        }
        let verified = Arc::ptr_eq(&prepared.source, &self.source)
            || prepared
                .verified_prefix
                .as_ref()
                .is_some_and(|prefix| Arc::ptr_eq(prefix, &self.source));
        if preserve && !verified && !prepared.source.starts_with(self.source.as_ref()) {
            return Err(SourceError::NonAppend);
        }
        if preserve
            && prepared.revision == self.revision
            && prepared.source.len() != self.source.len()
        {
            return Err(SourceError::NonAppend);
        }
        let (anchor, head) = self.selection_endpoints();
        self.editor.publish_view(prepared.editor, preserve);
        if preserve {
            self.editor.set_selection(Selection::new(
                prepared.map.position(anchor),
                prepared.map.position(head),
            ));
            self.retained_selection = Some((anchor, head, self.editor.selection()));
        } else {
            self.retained_selection = None;
        }
        self.syntax_limited = prepared.syntax_limited;
        if let Some(lines) = prepared.highlights {
            self.editor.set_highlight_spans(lines);
        }
        self.epoch = prepared.epoch;
        self.revision = prepared.revision;
        self.source = prepared.source;
        self.map = prepared.map;
        self.width = prepared.width;
        self.options = prepared.options;
        self.derived_bytes = prepared.derived_bytes;
        self.search_bytes = 0;
        self.requested_width.set(None);
        Ok(())
    }

    /// Return the immutable displayed bytes.
    pub fn source_text(&self) -> Arc<str> {
        Arc::clone(&self.source)
    }
    /// Constant-time estimate of retained display, layout, map, highlighting,
    /// and search storage. Accepted source bytes and shared executor storage
    /// are excluded; active readability can exceed the inactive cache budget.
    pub fn derived_bytes(&self) -> usize {
        self.derived_bytes.saturating_add(self.search_bytes)
    }
    /// Return the displayed epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    /// Return the displayed revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Return current presentation options.
    pub fn options(&self) -> &SourceOptions {
        &self.options
    }
    /// Return whether automatic syntax work was disabled by its byte cap.
    pub fn syntax_limited(&self) -> bool {
        self.syntax_limited
    }
    /// Return the allocated width awaiting preparation, if geometry changed.
    pub fn requested_width(&self) -> Option<u32> {
        self.requested_width.get()
    }
    /// Return the width of the displayed prepared geometry.
    pub fn prepared_width(&self) -> u32 {
        self.width
    }
    /// Return whether publication must wait for a pointer drag to finish.
    pub fn selection_drag_active(&self) -> bool {
        self.editor.selection_drag_active()
    }
    /// Cancel a pointer gesture and release its capture before replacement or
    /// dismissal.
    pub fn cancel_drag(&mut self, ctx: &mut dyn Context) -> UiResult<()> {
        self.editor.end_selection_drag();
        if ctx.has_mouse_capture() {
            ctx.release_mouse()?;
        }
        Ok(())
    }
    /// Return the editor's logical display cursor.
    pub fn cursor(&self) -> TextPosition {
        self.editor.cursor()
    }
    /// Move the display cursor and reveal it without taking focus.
    pub fn set_cursor(&mut self, ctx: &mut dyn Context, position: TextPosition) {
        self.retained_selection = None;
        self.editor.set_cursor(ctx, position);
    }
    /// Return the source byte at the display cursor.
    pub fn source_anchor(&self) -> SourceAnchor {
        SourceAnchor {
            epoch: self.epoch,
            byte: self.selection_endpoints().1,
        }
    }
    /// Return the source location at a prepared content point, such as the
    /// viewport top.
    pub fn source_anchor_at(&self, point: Point) -> SourceAnchor {
        SourceAnchor {
            epoch: self.epoch,
            byte: self.map.byte(self.editor.position_at(point)),
        }
    }
    /// Return the prepared content point for a byte location in this epoch.
    pub fn point_for_anchor(&self, anchor: SourceAnchor) -> Result<Point, SourceError> {
        if anchor.epoch != self.epoch {
            return Err(SourceError::WrongEpoch);
        }
        if anchor.byte > self.source.len() || !self.source.is_char_boundary(anchor.byte) {
            return Err(SourceError::InvalidRange);
        }
        Ok(self.editor.point_at(self.map.position(anchor.byte)))
    }
    /// Restore a byte location and reveal it without taking focus.
    pub fn restore_anchor(
        &mut self,
        ctx: &mut dyn Context,
        anchor: SourceAnchor,
    ) -> Result<(), SourceError> {
        if anchor.epoch != self.epoch {
            return Err(SourceError::WrongEpoch);
        }
        if anchor.byte > self.source.len() || !self.source.is_char_boundary(anchor.byte) {
            return Err(SourceError::InvalidRange);
        }
        self.editor.set_cursor(ctx, self.map.position(anchor.byte));
        self.retained_selection = Some((anchor.byte, anchor.byte, self.editor.selection()));
        Ok(())
    }
    /// Jump to a one-based logical source line.
    pub fn jump_line(&mut self, ctx: &mut dyn Context, line: usize) {
        self.retained_selection = None;
        self.editor
            .set_cursor(ctx, TextPosition::new(line.saturating_sub(1), 0));
    }
    /// Return the selected exact source interval, excluding display decoration.
    pub fn selection_range(&self) -> Range<usize> {
        let (anchor, head) = self.selection_endpoints();
        anchor.min(head)..anchor.max(head)
    }
    /// Recover exact selection endpoints even inside hidden CRLF boundaries.
    fn selection_endpoints(&self) -> (usize, usize) {
        let selection = self.editor.selection();
        if let Some((anchor, head, retained)) = self.retained_selection
            && retained == selection
        {
            return (anchor, head);
        }
        (
            self.map.byte(selection.anchor()),
            self.map.byte(selection.head()),
        )
    }
    /// Return exact source-byte selection endpoints, preserving drag direction.
    /// Hosts can retain these when evicting an inactive prepared surface.
    pub fn selection_bytes(&self) -> (usize, usize) {
        self.selection_endpoints()
    }
    /// Set byte-addressed selection endpoints without scrolling or taking
    /// focus.
    pub fn set_selection(&mut self, anchor: usize, head: usize) -> Result<(), SourceError> {
        if [anchor, head]
            .iter()
            .any(|byte| *byte > self.source.len() || !self.source.is_char_boundary(*byte))
        {
            return Err(SourceError::InvalidRange);
        }
        self.editor.set_selection(Selection::new(
            self.map.position(anchor),
            self.map.position(head),
        ));
        self.retained_selection = Some((anchor, head, self.editor.selection()));
        Ok(())
    }
    /// Copy an exact byte range only when its full output fits the budget.
    pub fn copy_range(&self, range: Range<usize>, limit: usize) -> Result<String, SourceError> {
        let text = self.source.get(range).ok_or(SourceError::InvalidRange)?;
        if text.len() > limit {
            return Err(SourceError::OutputLimit);
        }
        Ok(text.to_owned())
    }
    /// Copy the exact selected bytes within an explicit byte budget.
    pub fn copy_selection(&self, limit: usize) -> Result<String, SourceError> {
        self.copy_range(self.selection_range(), limit)
    }
    /// Copy the whole displayed revision, including its final newline.
    pub fn copy_source(&self, limit: usize) -> Result<String, SourceError> {
        self.copy_range(0..self.source.len(), limit)
    }

    /// Prepare bounded matching over raw source lines, independent of
    /// rendering.
    pub fn find_matches(
        epoch: u64,
        revision: u64,
        source: &str,
        query: &SourceQuery,
    ) -> Result<SourceMatches, SourceError> {
        if query.text.len() > query.max_query_bytes {
            return Err(SourceError::QueryLimit);
        }
        let mut result = SourceMatches {
            epoch,
            revision,
            ranges: Vec::new(),
            complete: true,
        };
        if query.text.is_empty() {
            return Ok(result);
        }
        let matcher = CompiledQuery::new(
            &SearchQuery {
                text: query.text.clone(),
                mode: if query.mode == SourceSearchMode::Literal {
                    SearchMode::Literal
                } else {
                    SearchMode::Regex
                },
                case: query.case,
            },
            &SearchLimits {
                max_query_bytes: query.max_query_bytes,
                max_matches: query.max_matches,
                max_scan_bytes: query.max_scan_bytes,
            },
        )
        .map_err(|error| SourceError::Regex(error.to_string()))?;
        let mut scanned = query.max_scan_bytes.min(source.len());
        while !source.is_char_boundary(scanned) {
            scanned -= 1;
        }
        result.complete = scanned == source.len();
        let mut offset = 0;
        for line in source[..scanned].split_inclusive('\n') {
            // A truncated logical line has no known ending within the work
            // budget, so it cannot safely supply regex end/word context.
            if !result.complete && !line.ends_with('\n') && offset + line.len() == scanned {
                break;
            }
            let raw = line
                .strip_suffix('\n')
                .map(|raw| raw.strip_suffix('\r').unwrap_or(raw))
                .unwrap_or(line);
            let mut start = 0;
            while start < raw.len() {
                let found = matcher
                    .find_at(raw, start)
                    .map_err(|error| SourceError::Regex(error.to_string()))?;
                let Some(found) = found else { break };
                if found.is_empty() {
                    start = found.end + 1;
                    while start < raw.len() && !raw.is_char_boundary(start) {
                        start += 1;
                    }
                    continue;
                }
                if result.ranges.len() == query.max_matches {
                    result.complete = false;
                    return Ok(result);
                }
                result.ranges.push(offset + found.start..offset + found.end);
                start = found.end;
            }
            offset += line.len();
        }
        Ok(result)
    }

    /// Publish precomputed source matches without moving the reader.
    pub fn publish_matches(&mut self, matches: SourceMatches) -> Result<(), SourceError> {
        if matches.epoch != self.epoch {
            return Err(SourceError::WrongEpoch);
        }
        if matches.revision != self.revision {
            return Err(SourceError::Stale);
        }
        let mut previous = 0;
        let mut ranges = Vec::with_capacity(matches.ranges.len());
        for range in matches.ranges {
            if range.start < previous || self.source.get(range.clone()).is_none() {
                return Err(SourceError::InvalidRange);
            }
            previous = range.end;
            let start = self.map.position(range.start);
            let end = self.map.position(range.end);
            if start.line != end.line {
                return Err(SourceError::InvalidRange);
            }
            ranges.push(TextRange::new(start, end));
        }
        self.editor.set_matches_preserving_position(ranges);
        self.search_bytes = self
            .editor
            .search_matches()
            .saturating_mul(size_of::<TextRange>());
        Ok(())
    }
    /// Install precomputed display ranges, for hosts with an existing matcher.
    pub fn set_matches(&mut self, _ctx: &mut dyn Context, matches: Vec<TextRange>) {
        self.search_bytes = matches.capacity().saturating_mul(size_of::<TextRange>());
        self.editor.set_matches_preserving_position(matches);
    }
    /// Navigate the current match window and reveal the activated result.
    pub fn search_next(&mut self, ctx: &mut dyn Context, delta: i32) {
        self.retained_selection = None;
        self.editor.search_next(ctx, delta);
    }
    /// Activate an explicit zero-based result without depending on Editor's
    /// retained match cursor. Out-of-range indices are ignored.
    pub fn activate_match(&mut self, ctx: &mut dyn Context, index: usize) {
        if index >= self.editor.search_matches() {
            return;
        }
        let current = self.editor.search_position().saturating_sub(1);
        let Ok(delta) = i32::try_from(index as i64 - current as i64) else {
            return;
        };
        self.retained_selection = None;
        self.editor.search_next(ctx, delta);
    }
    /// Return the stored match count.
    pub fn search_matches(&self) -> usize {
        self.editor.search_matches()
    }
    /// Return the current one-based match index, or zero when there is none.
    pub fn search_position(&self) -> usize {
        self.editor.search_position()
    }
    /// Clear stored search results without moving the reader.
    pub fn clear_search(&mut self, ctx: &mut dyn Context) {
        self.search_bytes = 0;
        self.editor.clear_search(ctx);
    }

    /// Whether Escape must end a selection before reaching the host.
    fn selection_active(&self) -> bool {
        self.editor.selection_drag_active()
            || !self.editor.selection().is_empty()
            || matches!(self.editor.vi_mode(), Some(ViMode::Visual(_)))
    }
}

impl Widget for MarkdownSource {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_source")
    }
    fn accept_focus(&self, ctx: &dyn ViewContext) -> bool {
        self.editor.accept_focus(ctx)
    }
    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        self.editor.measure(constraints)
    }
    fn canvas(&self, content: Size, ctx: &CanvasContext) -> Size {
        if content.w != self.width {
            self.requested_width.set(Some(content.w));
        }
        self.editor.canvas(content, ctx)
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> UiResult<()> {
        if self.editor.selection_drag_active() && !ctx.has_mouse_capture() {
            self.editor.end_selection_drag();
        }
        let width = ctx.view().view_rect().w;
        if width != self.width {
            self.requested_width.set(Some(width));
        }
        self.editor.render(render, ctx)
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> UiResult<EventOutcome> {
        if let Event::Key(key) = event {
            if key.key == key::KeyCode::Esc {
                let selected = self.selection_active();
                if selected {
                    self.cancel_drag(ctx)?;
                    self.editor.set_vi_mode(ViMode::Normal);
                    self.editor
                        .set_selection(Selection::caret(self.editor.cursor()));
                    self.retained_selection = None;
                }
                return Ok(if selected {
                    EventOutcome::Handle
                } else {
                    EventOutcome::Ignore
                });
            }
            if host_search_or_copy(*key) {
                return Ok(EventOutcome::Ignore);
            }
        }
        let outcome = self.editor.on_event(event, ctx)?;
        if matches!(event, Event::Key(_)) && outcome == EventOutcome::Handle {
            self.retained_selection = None;
        }
        if let Event::Mouse(mouse) = event
            && mouse.button == mouse::Button::Left
        {
            match mouse.action {
                mouse::Action::Down if self.editor.selection_drag_active() => {
                    self.retained_selection = None;
                    ctx.capture_mouse()?;
                }
                mouse::Action::Drag => self.retained_selection = None,
                mouse::Action::Up => self.cancel_drag(ctx)?,
                _ => {}
            }
        }
        Ok(outcome)
    }
    fn key_outcome(&self, key: key::Key, ctx: &dyn ViewContext) -> EventOutcome {
        if key.key == key::KeyCode::Esc {
            return if self.selection_active() {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            };
        }
        if host_search_or_copy(key) {
            return EventOutcome::Ignore;
        }
        self.editor.key_outcome(key, ctx)
    }
    fn reveal_anchor(&self, content: Size) -> Option<Rect> {
        self.editor.reveal_anchor(content)
    }
    fn scroll_marks(&self, axis: ScrollAxis, content: Size) -> Vec<ScrollMark> {
        self.editor.scroll_marks(axis, content)
    }
    fn on_unmount(&mut self, ctx: &mut dyn Context) {
        self.cancel_drag(ctx).unwrap_or_default();
    }
}

impl Register for MarkdownSource {
    fn register(setup: &mut Setup) -> UiResult<()> {
        setup.widget_styles(palette_style_rules);
        Ok(())
    }
}

/// Shared bounded search and host copy own these keys, rather than Editor's
/// private yank register or automatic prompt matching.
fn host_search_or_copy(key: key::Key) -> bool {
    matches!(key.key, key::KeyCode::Char('/' | '?' | 'n' | 'N' | 'y'))
        && !key.mods.ctrl
        && !key.mods.alt
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "syntax")]
    use canopy::style::themes;
    use canopy::{layout::ScrollOp, testing::harness::Harness};
    #[cfg(feature = "syntax")]
    use canopy_widgets::highlight::SpanStyle;

    use super::*;

    fn prepared(epoch: u64, revision: u64, text: &str) -> PreparedSource {
        PreparedSource::new(
            epoch,
            revision,
            Arc::from(text),
            12,
            SourceOptions::default(),
        )
    }

    #[test]
    fn exact_source_copy_keeps_crlf_tabs_final_newline_and_controls() {
        let text = "# α\r\n\ttext\u{1b}[31m\0\r\n";
        let mut source = MarkdownSource::from_prepared(prepared(1, 0, text));
        assert_eq!(source.copy_source(text.len()).unwrap(), text);
        source.set_selection(0, text.len()).unwrap();
        assert_eq!(source.copy_selection(text.len()).unwrap(), text);
        assert_eq!(
            source.copy_source(text.len() - 1),
            Err(SourceError::OutputLimit)
        );
        assert!(!source.editor.text().contains('\r'));
        assert!(!source.editor.text().contains('\u{1b}'));
        assert!(!source.editor.text().contains('\0'));
        assert_eq!(source.editor.text(), "# α\n\ttext␛[31m␀\n");
    }

    #[test]
    fn append_preserves_selection_cursor_and_epoch_replacement_resets() {
        let mut source = MarkdownSource::from_prepared(prepared(3, 0, "a\r\nb\n"));
        source.set_selection(1, 4).unwrap();
        let before = source.source_anchor();
        source.publish(prepared(3, 1, "a\r\nb\nc\n")).unwrap();
        assert_eq!(source.source_anchor(), before);
        assert_eq!(source.selection_range(), 1..4);
        assert_eq!(source.copy_selection(20).unwrap(), "\r\nb");
        source.publish(prepared(4, 0, "new")).unwrap();
        assert_eq!(source.selection_range(), 0..0);
        assert_eq!(source.source_anchor(), SourceAnchor { epoch: 4, byte: 0 });
    }

    #[test]
    fn mounted_append_keeps_focus_selection_and_paused_viewport() {
        let text = "zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive\r\n";
        let source = MarkdownSource::from_prepared(prepared(1, 0, text));
        let mut harness = Harness::builder(source).size(12, 2).build().unwrap();
        harness
            .with_root_widget_context(|source: &mut MarkdownSource, ctx| {
                ctx.set_focus(ctx.node_id())?;
                source.set_selection(6, 12).unwrap();
                ctx.scroll(ScrollOp::To(Point::new(0, 2)));
                Ok(())
            })
            .unwrap();
        harness.render().unwrap();
        let focus = harness.canopy.with_root_view(|view| view.focused_node());
        harness.with_root_widget(|source: &mut MarkdownSource| {
            source
                .publish(prepared(1, 1, &format!("{text}six\r\n")))
                .unwrap();
            assert_eq!(source.selection_range(), 6..12);
        });
        harness.render().unwrap();
        assert_eq!(
            harness.canopy.with_root_view(|view| view.focused_node()),
            focus
        );
        harness
            .with_root_widget_context(|source: &mut MarkdownSource, ctx| {
                assert_eq!(ctx.view().scroll, Point::new(0, 2));
                assert_eq!(source.copy_selection(100).unwrap(), &text[6..12]);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn split_crlf_and_empty_source_have_valid_maps() {
        let mut source = MarkdownSource::from_prepared(prepared(1, 0, "a\r"));
        source.set_selection(0, 2).unwrap();
        source.publish(prepared(1, 1, "a\r\n")).unwrap();
        assert_eq!(source.copy_selection(20).unwrap(), "a\r");
        // A CR-to-CRLF merge maps a cursor inside the line separator to
        // its preceding display boundary, while exact byte copy remains
        // available.
        assert_eq!(source.copy_range(0..2, 20).unwrap(), "a\r");
        let empty = MarkdownSource::new("");
        assert_eq!(empty.selection_range(), 0..0);
        assert_eq!(empty.copy_source(0).unwrap(), "");
    }

    #[test]
    fn byte_map_never_copies_control_decoration() {
        let source = "a\u{85}b\u{2028}c";
        let (display, map) = SourceMap::prepare(Arc::from(source));
        assert_eq!(display, "a\\u{85}b\\u{2028}c");
        for byte in (0..=source.len()).filter(|byte| source.is_char_boundary(*byte)) {
            assert_eq!(map.byte(map.position(byte)), byte);
        }
        assert_eq!(map.byte(TextPosition::new(0, 4)), 1);
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn syntax_cap_is_checked_before_highlighter_preparation() {
        let options = SourceOptions {
            syntax_limit: 3,
            ..SourceOptions::default()
        };
        let below = PreparedSource::new(1, 0, Arc::from("# "), 10, options.clone());
        let at = PreparedSource::new(1, 0, Arc::from("# a"), 10, options.clone());
        let above = PreparedSource::new(1, 0, Arc::from("# ab"), 10, options);
        assert!(below.highlights.is_some());
        assert!(at.highlights.is_some());
        assert!(above.highlights.is_none());
        let escaped = PreparedSource::new(
            1,
            0,
            Arc::from("\u{85}"),
            10,
            SourceOptions {
                syntax_limit: 3,
                ..SourceOptions::default()
            },
        );
        assert!(
            escaped.highlights.is_none(),
            "decorations count against syntax work too"
        );
        assert!(escaped.syntax_limited);
    }

    #[test]
    fn anchors_map_wrapped_rows_and_number_gutter_without_copying_decoration() {
        let options = SourceOptions {
            line_numbers: LineNumbers::Absolute,
            ..SourceOptions::default()
        };
        let source = MarkdownSource::from_prepared(PreparedSource::new(
            1,
            0,
            Arc::from("abc def\r\n"),
            7,
            options,
        ));
        let point = source
            .point_for_anchor(SourceAnchor { epoch: 1, byte: 4 })
            .unwrap();
        assert_eq!(point, Point::new(2, 1));
        assert_eq!(
            source.source_anchor_at(point),
            SourceAnchor { epoch: 1, byte: 4 }
        );
        assert_eq!(source.copy_source(9).unwrap(), "abc def\r\n");
    }

    #[test]
    fn bounded_search_uses_raw_source_smart_case_and_skips_empty_matches() {
        let source = "[Link](DEST)\r\nDest destination\n";
        let mut query = SourceQuery::literal("dest");
        query.mode = SourceSearchMode::Regex;
        query.case = SearchCase::Smart;
        let found = MarkdownSource::find_matches(1, 2, source, &query).unwrap();
        assert_eq!(found.ranges.len(), 3);
        assert!(found.complete);
        query.max_matches = 1;
        let found = MarkdownSource::find_matches(1, 2, source, &query).unwrap();
        assert_eq!(found.ranges, vec![7..11]);
        assert!(!found.complete);
        query.text = "x*".into();
        assert!(
            MarkdownSource::find_matches(1, 2, source, &query)
                .unwrap()
                .ranges
                .is_empty()
        );
        query.text = "(".into();
        assert!(matches!(
            MarkdownSource::find_matches(1, 2, source, &query),
            Err(SourceError::Regex(_))
        ));
    }

    #[test]
    fn scan_cutoff_does_not_invent_regex_endings() {
        for pattern in ["foo$", r"foo\b"] {
            let query = SourceQuery {
                mode: SourceSearchMode::Regex,
                max_scan_bytes: 3,
                ..SourceQuery::literal(pattern)
            };
            let found = MarkdownSource::find_matches(1, 0, "foobar", &query).unwrap();
            assert!(found.ranges.is_empty());
            assert!(!found.complete);
            let query = SourceQuery {
                max_scan_bytes: 7,
                ..query
            };
            let found = MarkdownSource::find_matches(1, 0, "foo\nfoobar", &query).unwrap();
            assert_eq!(found.ranges, vec![0..3]);
            assert!(!found.complete);
        }
    }

    #[test]
    fn stale_or_nonappend_publication_does_not_change_displayed_bytes() {
        let mut source = MarkdownSource::from_prepared(prepared(1, 2, "abc"));
        assert_eq!(
            source.publish(prepared(1, 1, "abcd")),
            Err(SourceError::Stale)
        );
        assert_eq!(
            source.publish(prepared(1, 3, "abd")),
            Err(SourceError::NonAppend)
        );
        assert_eq!(source.copy_source(3).unwrap(), "abc");
    }

    #[test]
    fn derived_accounting_tracks_wrapping_and_search_without_charging_shared_source() {
        let text: Arc<str> = Arc::from("word ".repeat(200));
        let options = SourceOptions {
            language: None,
            wrap: WrapMode::None,
            ..SourceOptions::default()
        };
        let unwrapped = PreparedSource::new(1, 0, text.clone(), 8, options.clone());
        let wrapped = PreparedSource::new(
            1,
            0,
            text.clone(),
            8,
            SourceOptions {
                wrap: WrapMode::Soft,
                ..options
            },
        );
        assert!(wrapped.derived_bytes() > unwrapped.derived_bytes());
        let prepared_bytes = wrapped.derived_bytes();
        let mut source = MarkdownSource::from_prepared(wrapped);
        assert_eq!(source.derived_bytes(), prepared_bytes);
        let found =
            MarkdownSource::find_matches(1, 0, &text, &SourceQuery::literal("word")).unwrap();
        source.publish_matches(found).unwrap();
        assert!(source.derived_bytes() > prepared_bytes);
        assert!(Arc::ptr_eq(&source.source_text(), &text));
        source.set_selection(text.len(), 0).unwrap();
        assert_eq!(source.selection_bytes(), (text.len(), 0));
        assert_eq!(
            source.copy_selection(text.len()).unwrap().as_str(),
            text.as_ref()
        );
    }

    #[test]
    fn explicit_match_activation_reveals_its_prepared_content_point() {
        let text = format!("{}needle\n", "filler\n".repeat(30));
        let found =
            MarkdownSource::find_matches(1, 0, &text, &SourceQuery::literal("needle")).unwrap();
        let source = MarkdownSource::from_prepared(prepared(1, 0, &text));
        let mut harness = Harness::builder(source).size(12, 4).build().unwrap();
        harness
            .with_root_widget_context(|source: &mut MarkdownSource, ctx| {
                source.publish_matches(found).unwrap();
                assert_eq!(ctx.view().scroll.y, 0);
                source.activate_match(ctx, 0);
                Ok(())
            })
            .unwrap();
        harness.render().unwrap();
        harness
            .with_root_widget_context(|source: &mut MarkdownSource, ctx| {
                let point = source
                    .point_for_anchor(SourceAnchor {
                        epoch: 1,
                        byte: text.find("needle").unwrap(),
                    })
                    .unwrap();
                assert!(ctx.view().scroll.y > 0);
                assert!(ctx.view().view_rect().contains_point(point));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn mounted_syntax_tracks_palette_without_repreparing_source_or_geometry() {
        let options = SourceOptions {
            language: Some(Arc::from("rs")),
            ..SourceOptions::default()
        };
        let prepared = PreparedSource::new(
            1,
            0,
            Arc::from("fn main() { let answer = 42; }\n"),
            40,
            options,
        );
        assert!(
            prepared
                .highlights
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .any(|span| matches!(span.style, SpanStyle::Path(_)))
        );
        let mut harness = Harness::builder(MarkdownSource::from_prepared(prepared))
            .register::<MarkdownSource>()
            .size(40, 2)
            .build()
            .unwrap();
        let identity = |harness: &mut Harness| {
            harness.with_root_widget(|source: &mut MarkdownSource| {
                (
                    source.map.marks.as_ptr(),
                    source.map.lines.as_ptr(),
                    source.prepared_width(),
                    source.derived_bytes(),
                    source.revision(),
                )
            })
        };
        let colors = |harness: &mut Harness| {
            harness.render().unwrap();
            harness
                .canopy
                .snapshot()
                .unwrap()
                .buffer
                .cells()
                .iter()
                .map(|cell| cell.style)
                .collect::<Vec<_>>()
        };
        let prepared_identity = identity(&mut harness);
        let dark = colors(&mut harness);
        harness.canopy.set_theme(themes::solarized_light());
        let light = colors(&mut harness);
        assert_ne!(
            dark[1].fg, light[1].fg,
            "a keyword takes the active palette"
        );
        assert_ne!(
            dark[1].bg, light[1].bg,
            "the source ground follows the active palette"
        );
        assert_eq!(identity(&mut harness), prepared_identity);
        harness.canopy.set_theme(themes::default_dark());
        assert_eq!(colors(&mut harness), dark);
        assert_eq!(identity(&mut harness), prepared_identity);
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn full_fence_info_uses_shared_grammar_and_unknown_does_not_guess_shebangs() {
        let signature = |info: &str, text: &str| {
            let prepared = PreparedSource::new(
                1,
                0,
                Arc::from(text),
                40,
                SourceOptions {
                    language: Some(Arc::from(info)),
                    ..SourceOptions::default()
                },
            );
            assert_eq!(prepared.source.as_ref(), text);
            prepared.highlights.map(|lines| {
                lines
                    .into_iter()
                    .flatten()
                    .map(|span| {
                        let SpanStyle::Path(path) = span.style else {
                            panic!("prepared syntax uses live palette roles");
                        };
                        (span.range, path.into_owned())
                    })
                    .collect::<Vec<_>>()
            })
        };
        let rust = "fn main() { let answer = 42; }\n";
        let canonical = signature("rs", rust);
        assert!(canonical.is_some());
        assert_eq!(signature("rust,ignore", rust), canonical);
        assert_eq!(signature("Rust extra metadata", rust), canonical);
        assert!(signature("md", "# Heading\n").is_some());
        assert!(signature("unknown-language", "#!/bin/bash\necho unchanged\n").is_none());
    }
}
