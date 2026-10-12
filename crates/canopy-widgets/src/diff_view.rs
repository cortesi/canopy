//! Diff view widget over the [`Diff`] row model.
//!
//! [`DiffView`] renders two full texts in unified or side-by-side layout, with
//! whole-file or context scope. The host supplies the texts and, optionally,
//! one [`Highlighter`] per side, so parser state stays with its own version.
//! Unified code scrolls horizontally with pinned line numbers. Side-by-side
//! panes have equal widths and wrap their code to fit.
//!
//! The view resolves these style names:
//!
//! - `diff/context`: unchanged lines.
//! - `diff/removed`: removed lines.
//! - `diff/added`: added lines.
//! - `diff/gap`: hidden context.
//! - `diff/header`: block headings.
//! - `diff/missing`: the empty half of a one-sided change.
//! - `diff/separator`: the side-by-side divider.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    mem,
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use canopy::{
    Context, NodeName, Register, Setup, ViewContext, Widget, derive_commands,
    error::Result,
    geom::{Line, Point, Rect, Size},
    layout::{CanvasContext, Constraint, MeasureConstraints, Measurement, RevealAlign, ScrollOp},
    render::Render,
    style::Style,
    text,
};

use crate::{
    Spinner,
    diff::{Diff, DiffRow, Scope},
    editor::{
        TextRange, WrapMode,
        layout::{LineLayout, Wrap, WrapSegment, layout_line},
    },
    highlight::{HighlightSpan, Highlighter},
    run_paint,
};

/// Tab stop a model built from texts assumes.
const DEFAULT_TAB_STOP: usize = 4;

/// How long a model may compute before the spinner shows.
const LOADING_DELAY: Duration = Duration::from_millis(150);

/// How a diff view arranges the two versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One column: removed lines above added lines.
    Unified,
    /// Two columns: the old version left, the new version right.
    SideBySide,
}

/// One side of a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    /// The old version.
    Old,
    /// The new version.
    New,
}

/// A display row in side-by-side layout.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SideRow {
    /// One old line and one new line, either of which may be absent.
    Pair {
        /// Zero-based old line.
        old: Option<usize>,
        /// Zero-based new line.
        new: Option<usize>,
        /// Whether the pair belongs to a change rather than to shared text.
        changed: bool,
    },
    /// A run of unchanged lines the context scope hides.
    Gap,
    /// The heading that opens one change block in context scope.
    Header {
        /// Old lines the block shows.
        old: Range<usize>,
        /// New lines the block shows.
        new: Range<usize>,
    },
}

/// Column layout of one diff view body.
struct Geometry {
    /// Digits in the widest line number.
    digits: u32,
    /// Line-number gutter width, marker included.
    gutter: u32,
    /// Width of one side-by-side half, divider excluded.
    half: u32,
    /// Total canvas width.
    width: u32,
}

/// Wrapped source lines and display offsets for the paired rows.
struct SideLayout {
    /// Code width used to wrap both versions.
    width: u32,
    /// Old and new line layouts, in paired row order.
    lines: Vec<(Option<LineLayout>, Option<LineLayout>)>,
    /// Each paired row's first display row, followed by the total height.
    offsets: Vec<usize>,
}

/// The expensive parts of a diff view, ready to adopt without recomputation.
///
/// A host that computes diffs off the UI thread builds one of these there and
/// hands it to [`DiffView::new`] or [`DiffView::set_model`], so the UI thread
/// only moves data. Clones share the immutable diff and prepared rows.
#[derive(Clone)]
pub struct DiffModel {
    /// The line diff.
    diff: Arc<Diff>,
    /// Rows for `scope`, in unified order.
    rows: Arc<[DiffRow]>,
    /// Display width of the widest old line, tabs expanded.
    old_width: u32,
    /// Display width of the widest new line, tabs expanded.
    new_width: u32,
    /// The scope the rows were built for.
    scope: Scope,
    /// The tab stop the widths were built for.
    tab_stop: usize,
}

impl DiffModel {
    /// Compute every part of a view for `diff` at `scope` and `tab_stop`.
    ///
    /// Side-by-side pairing is deferred until a view needs it, because a
    /// unified view never does.
    #[must_use]
    pub fn new(diff: Diff, scope: Scope, tab_stop: usize) -> Self {
        let tab_stop = tab_stop.max(1);
        let old_width = widest_line(&diff, Side::Old, tab_stop);
        let new_width = widest_line(&diff, Side::New, tab_stop);
        let rows = diff.rows(scope).into();
        Self {
            diff: Arc::new(diff),
            rows,
            old_width,
            new_width,
            scope,
            tab_stop,
        }
    }

    /// Compute a whole-file model of `old` and `new` at the default tab stop.
    #[must_use]
    pub fn from_texts(old: impl Into<String>, new: impl Into<String>) -> Self {
        Self::new(Diff::new(old, new), Scope::WholeFile, DEFAULT_TAB_STOP)
    }

    /// Return the line diff.
    #[must_use]
    pub fn diff(&self) -> &Diff {
        &self.diff
    }
}

/// A match in one displayed source line.
struct SearchMatch {
    /// Version containing the match.
    side: Side,
    /// Zero-based source line within that version.
    line: usize,
    /// Character columns containing the matched text.
    columns: Range<usize>,
    /// Logical row in the active layout, before wrapping.
    row: usize,
}

/// A line diff with a display strategy, a scope, and optional highlighting.
pub struct DiffView {
    /// The line diff being shown.
    diff: Arc<Diff>,
    /// How the two sides are laid out.
    strategy: Mode,
    /// How much unchanged text the rows show.
    scope: Scope,
    /// Rows for the current scope, in unified order.
    rows: Arc<[DiffRow]>,
    /// Rows paired for side-by-side layout.
    side_rows: Vec<SideRow>,
    /// Paired wrapping, rebuilt when the pane width or model changes.
    side_layout: RefCell<Option<SideLayout>>,
    /// Display width of the widest old line, tabs expanded.
    old_width: u32,
    /// Display width of the widest new line, tabs expanded.
    new_width: u32,
    /// Tab stop width in columns.
    tab_stop: usize,
    /// Highlighter for the old side.
    old_highlighter: Option<Box<dyn Highlighter>>,
    /// Highlighter for the new side.
    new_highlighter: Option<Box<dyn Highlighter>>,
    /// Whether both highlighters hold the current texts.
    prepared: bool,
    /// Text shown in place of the diff, if any.
    message: Option<String>,
    /// When the view started waiting for a model, while it waits.
    loading: Option<Instant>,
    /// Index into the change heads that the last change move reached.
    change: usize,
    /// Changes whenever the displayed text changes.
    revision: u64,
    /// Matches in display order, indexed by source line for painting.
    matches: Vec<SearchMatch>,
    /// Match indices for each source line, including both halves of context
    /// rows.
    match_lines: BTreeMap<(Side, usize), Vec<usize>>,
    /// Match index selected for navigation and stronger highlighting.
    current_match: Option<usize>,
}

#[derive_commands]
impl DiffView {
    /// Show `model` in unified layout.
    ///
    /// Use [`Self::with_strategy`] to change the layout, and install
    /// highlighters afterwards.
    #[must_use]
    pub fn new(model: DiffModel) -> Self {
        Self {
            diff: model.diff,
            strategy: Mode::Unified,
            scope: model.scope,
            rows: model.rows,
            side_rows: Vec::new(),
            side_layout: RefCell::new(None),
            old_width: model.old_width,
            new_width: model.new_width,
            tab_stop: model.tab_stop,
            old_highlighter: None,
            new_highlighter: None,
            prepared: false,
            message: None,
            loading: None,
            change: 0,
            revision: 0,
            matches: Vec::new(),
            match_lines: BTreeMap::new(),
            current_match: None,
        }
    }

    /// Show a message in place of any diff, such as before one is chosen.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Replace the diff with an already computed model, keeping the layout.
    ///
    /// This clears any message and loading state, drops the highlighters,
    /// and returns change movement to the first change.
    pub fn set_model(&mut self, model: DiffModel) {
        self.diff = model.diff;
        self.scope = model.scope;
        self.rows = model.rows;
        self.side_rows = Vec::new();
        self.side_layout.get_mut().take();
        self.old_width = model.old_width;
        self.new_width = model.new_width;
        self.tab_stop = model.tab_stop;
        self.old_highlighter = None;
        self.new_highlighter = None;
        self.ensure_side_rows();
        self.prepared = false;
        self.message = None;
        self.loading = None;
        self.change = 0;
        self.revision = self.revision.wrapping_add(1);
        self.clear_matches();
    }

    /// Replace the highlighters for the old and new sides.
    pub fn set_highlighters(
        &mut self,
        old: Option<Box<dyn Highlighter>>,
        new: Option<Box<dyn Highlighter>>,
    ) {
        self.old_highlighter = old;
        self.new_highlighter = new;
        self.prepared = false;
    }

    /// Show `message` in place of the diff, such as for a binary file.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.set_model(DiffModel::from_texts("", ""));
        self.message = Some(message.into());
    }

    /// Note that a model is computing. After a short delay the view shows a
    /// spinner until [`Self::set_model`] or [`Self::set_message`] arrives.
    pub fn set_loading(&mut self) {
        self.loading = Some(Instant::now());
    }

    /// Return whether the view shows a message rather than a diff.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Return the revision of the diff text and its display layout.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the displayed source text in unified row order.
    ///
    /// Hidden context is omitted. Headers and gaps contribute empty lines,
    /// so each text line has the same index as its unified display row.
    #[must_use]
    pub fn search_text(&self) -> String {
        if self.message.is_some() {
            return String::new();
        }
        self.rows
            .iter()
            .map(|row| match row {
                DiffRow::Unchanged { old, .. } | DiffRow::Removed { old } => {
                    self.diff.old_line(*old)
                }
                DiffRow::Added { new } => self.diff.new_line(*new),
                _ => "",
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Highlight matches over [`Self::search_text`] and reveal the first at
    /// or below the top of the view. Ranges use character columns and must
    /// stay within one source line. Side-by-side layout maps them to its rows.
    pub fn set_matches(&mut self, ctx: &mut dyn Context, ranges: Vec<TextRange>) {
        self.clear_matches();
        let mut side_rows = BTreeMap::new();
        if self.strategy == Mode::SideBySide {
            for (row, pair) in self.side_rows.iter().enumerate() {
                if let SideRow::Pair { old, new, .. } = pair {
                    if let Some(line) = old {
                        side_rows.insert((Side::Old, *line), row);
                    }
                    if let Some(line) = new {
                        side_rows.insert((Side::New, *line), row);
                    }
                }
            }
        }
        for range in ranges {
            if range.start.line != range.end.line || range.start.column >= range.end.column {
                continue;
            }
            let source = match self.rows.get(range.start.line) {
                Some(DiffRow::Unchanged { old, .. } | DiffRow::Removed { old }) => {
                    (Side::Old, *old)
                }
                Some(DiffRow::Added { new }) => (Side::New, *new),
                _ => continue,
            };
            let row = side_rows.get(&source).copied().unwrap_or(range.start.line);
            self.matches.push(SearchMatch {
                side: source.0,
                line: source.1,
                columns: range.start.column..range.end.column,
                row,
            });
        }
        let mut matches = mem::take(&mut self.matches);
        matches.sort_by_key(|found| {
            (
                self.match_area(found, ctx.view().view_rect().w).tl.y,
                found.side,
                found.columns.start,
            )
        });
        self.matches = matches;
        for (index, found) in self.matches.iter().enumerate() {
            self.match_lines
                .entry((found.side, found.line))
                .or_default()
                .push(index);
            // Context is drawn on both sides but counted once.
            if self.strategy == Mode::SideBySide
                && found.side == Side::Old
                && let Some(SideRow::Pair {
                    new: Some(new),
                    changed: false,
                    ..
                }) = self.side_rows.get(found.row)
            {
                self.match_lines
                    .entry((Side::New, *new))
                    .or_default()
                    .push(index);
            }
        }
        let top = ctx.view().view_rect().tl.y as usize;
        self.current_match = (!self.matches.is_empty()).then(|| {
            self.matches
                .iter()
                .position(|found| {
                    self.match_area(found, ctx.view().view_rect().w).tl.y as usize >= top
                })
                .unwrap_or(0)
        });
        self.reveal_match(ctx);
    }

    /// Move between matches, wrapping at either end, and reveal the match.
    pub fn search_next(&mut self, ctx: &mut dyn Context, delta: i32) {
        let count = self.matches.len();
        if count == 0 {
            return;
        }
        let step = delta.unsigned_abs() as usize % count;
        let current = self.current_match.unwrap_or(0);
        self.current_match = Some(if delta < 0 {
            (current + count - step) % count
        } else {
            (current + step) % count
        });
        self.reveal_match(ctx);
    }

    /// Remove the search and its highlights without moving the view.
    pub fn clear_search(&mut self, _ctx: &mut dyn Context) {
        self.clear_matches();
    }

    /// Return the number of displayed search matches.
    #[must_use]
    pub fn search_matches(&self) -> usize {
        self.matches.len()
    }

    /// Return the one-based current match position, or zero with no match.
    #[must_use]
    pub fn search_position(&self) -> usize {
        self.current_match.map_or(0, |index| index + 1)
    }

    /// Drop cached matches and their current position.
    fn clear_matches(&mut self) {
        self.matches.clear();
        self.match_lines.clear();
        self.current_match = None;
    }

    /// Reveal the current match with up to three preceding rows of context.
    fn reveal_match(&self, ctx: &mut dyn Context) {
        let Some(found) = self.current_match.and_then(|index| self.matches.get(index)) else {
            return;
        };
        ctx.reveal_area(
            self.match_area(found, ctx.view().view_rect().w),
            RevealAlign::Top(3),
        );
    }

    /// Map a source match through the current wrapping, including after resize.
    fn match_area(&self, found: &SearchMatch, width: u32) -> Rect {
        let geometry = self.geometry(width);
        let source = match found.side {
            Side::Old => self.diff.old_line(found.line),
            Side::New => self.diff.new_line(found.line),
        };
        let prefix = source.chars().take(found.columns.start).collect::<String>();
        let start = text::width(&text::expand_tabs(&prefix, self.tab_stop));
        let through = source.chars().take(found.columns.end).collect::<String>();
        let end = text::width(&text::expand_tabs(&through, self.tab_stop));
        if self.strategy == Mode::Unified {
            return Rect::new(
                geometry.gutter + start,
                column(found.row),
                end.saturating_sub(start).max(1),
                1,
            );
        }
        self.with_side_layout(geometry.half.saturating_sub(geometry.gutter), |layout| {
            let lines = &layout.lines[found.row];
            let line = match found.side {
                Side::Old => &lines.0,
                Side::New => &lines.1,
            }
            .as_ref()
            .expect("a match has a source line");
            let index = line.segment_for_column(start as usize);
            let last = line.segment_for_column(end.saturating_sub(1).max(start) as usize);
            let segment = &line.segments[index];
            let local = start.saturating_sub(column(segment.start_col));
            let half = if found.side == Side::New {
                geometry.width - geometry.half
            } else {
                0
            };
            Rect::new(
                half + geometry.gutter + local.min(layout.width.saturating_sub(1)),
                column(layout.offsets[found.row] + index),
                end.saturating_sub(start)
                    .max(1)
                    .min(layout.width.saturating_sub(local).max(1)),
                column(last - index + 1),
            )
        })
    }

    /// Scroll to the next change.
    #[command]
    pub fn next_change(&mut self, c: &mut dyn Context) {
        let heads = self.change_heads(c.view().view_rect().w);
        if heads.is_empty() {
            return;
        }
        self.change = (self.change + 1).min(heads.len() - 1);
        c.scroll(ScrollOp::To(Point {
            x: 0,
            y: column(heads[self.change]),
        }));
    }

    /// Scroll to the previous change.
    #[command]
    pub fn prev_change(&mut self, c: &mut dyn Context) {
        let heads = self.change_heads(c.view().view_rect().w);
        if heads.is_empty() {
            return;
        }
        self.change = self.change.saturating_sub(1).min(heads.len() - 1);
        c.scroll(ScrollOp::To(Point {
            x: 0,
            y: column(heads[self.change]),
        }));
    }

    /// Change the display layout without replacing the model or highlighters.
    ///
    /// Search ranges need to be applied again because their display rows
    /// changed. The revision advances so a host can refresh its held query.
    /// Messages and loading state remain in place.
    pub fn set_strategy(&mut self, strategy: Mode) {
        if self.strategy == strategy {
            return;
        }
        self.strategy = strategy;
        self.ensure_side_rows();
        self.change = 0;
        self.revision = self.revision.wrapping_add(1);
        self.clear_matches();
    }

    /// Return the current display layout.
    #[must_use]
    pub fn strategy(&self) -> Mode {
        self.strategy
    }

    /// Set the display strategy while constructing a view.
    #[must_use]
    pub fn with_strategy(mut self, strategy: Mode) -> Self {
        self.strategy = strategy;
        self.ensure_side_rows();
        self
    }

    /// Set the amount of unchanged text the rows show.
    #[must_use]
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self.rebuild_rows();
        self
    }

    /// Set the tab stop width in columns.
    #[must_use]
    pub fn with_tab_stop(mut self, tab_stop: usize) -> Self {
        self.tab_stop = tab_stop.max(1);
        self.rebuild();
        self
    }

    /// Install a highlighter for the old side.
    #[must_use]
    pub fn with_old_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        self.old_highlighter = Some(highlighter);
        self.prepared = false;
        self
    }

    /// Install a highlighter for the new side.
    #[must_use]
    pub fn with_new_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        self.new_highlighter = Some(highlighter);
        self.prepared = false;
        self
    }

    /// Return the diff being shown.
    #[must_use]
    pub fn diff(&self) -> &Diff {
        &self.diff
    }

    /// Return the rows for the current scope.
    #[must_use]
    pub fn rows(&self) -> &[DiffRow] {
        &self.rows
    }

    /// Return the display scope.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Pair the rows for side-by-side layout, once, when that layout needs it.
    fn ensure_side_rows(&mut self) {
        if self.strategy == Mode::SideBySide && self.side_rows.is_empty() {
            self.side_rows = side_rows(&self.rows);
        }
    }

    /// Return change heads in the active layout's display coordinates.
    fn change_heads(&self, width: u32) -> Vec<usize> {
        match self.strategy {
            Mode::Unified => change_heads(
                self.rows.iter().map(|row| {
                    (
                        matches!(row, DiffRow::Header { .. }),
                        matches!(row, DiffRow::Removed { .. } | DiffRow::Added { .. }),
                    )
                }),
                self.scope,
            ),
            Mode::SideBySide => {
                let heads = change_heads(
                    self.side_rows.iter().map(|row| {
                        (
                            matches!(row, SideRow::Header { .. }),
                            matches!(row, SideRow::Pair { changed: true, .. }),
                        )
                    }),
                    self.scope,
                );
                let geom = self.geometry(width);
                self.with_side_layout(geom.half.saturating_sub(geom.gutter), |layout| {
                    heads.into_iter().map(|row| layout.offsets[row]).collect()
                })
            }
        }
    }

    /// Rebuild the rows and the cached widths.
    fn rebuild(&mut self) {
        self.old_width = widest_line(&self.diff, Side::Old, self.tab_stop);
        self.new_width = widest_line(&self.diff, Side::New, self.tab_stop);
        self.rebuild_rows();
    }

    /// Rebuild the rows for the current scope.
    fn rebuild_rows(&mut self) {
        self.rows = self.diff.rows(self.scope).into();
        self.side_rows = side_rows(&self.rows);
        self.side_layout.get_mut().take();
        self.prepared = false;
        self.revision = self.revision.wrapping_add(1);
        self.clear_matches();
    }

    /// Prepare both highlighters for the current texts, once.
    fn prepare_highlighters(&mut self) {
        if !self.prepared {
            if let Some(highlighter) = &self.old_highlighter {
                highlighter.prepare(self.diff.old_text());
            }
            if let Some(highlighter) = &self.new_highlighter {
                highlighter.prepare(self.diff.new_text());
            }
            self.prepared = true;
        }
    }

    /// Return highlight spans for one line of a side.
    fn highlight(&self, side: Side, line: usize, text: &str) -> Vec<HighlightSpan> {
        let highlighter = match side {
            Side::Old => &self.old_highlighter,
            Side::New => &self.new_highlighter,
        };
        if let Some(highlighter) = highlighter {
            return highlighter.highlight_line(line, text);
        }
        let _ = (line, text);
        Vec::new()
    }

    /// Return the column layout for a view of `view_w` columns.
    fn geometry(&self, view_w: u32) -> Geometry {
        let digits = digits(self.diff.old_len().max(self.diff.new_len()));
        let gutter = digits + 2;
        let line_width = self.old_width.max(self.new_width) + 1;
        match self.strategy {
            Mode::Unified => {
                let width = (gutter + line_width).max(view_w);
                Geometry {
                    digits,
                    gutter,
                    half: 0,
                    width,
                }
            }
            Mode::SideBySide => {
                let half = view_w.saturating_sub(1) / 2;
                Geometry {
                    digits,
                    gutter: gutter.min(half),
                    half,
                    width: view_w,
                }
            }
        }
    }

    /// Draw one line-number gutter cell run.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_number(
        rndr: &mut Render,
        x: u32,
        y: u32,
        width: u32,
        marker: char,
        number: Option<usize>,
        digits: u32,
        style: &str,
    ) -> Result<()> {
        let cell = match number {
            Some(number) => format!("{marker}{:>width$} ", number + 1, width = digits as usize),
            None => format!("{marker}{:>width$} ", "", width = digits as usize),
        };
        rndr.text(style, Line::new(x, y, width), &cell)
    }

    /// Return the number of body rows for the current strategy.
    fn body_rows(&self, width: u32) -> usize {
        match self.strategy {
            Mode::Unified => self.rows.len(),
            Mode::SideBySide => {
                let geom = self.geometry(width);
                self.with_side_layout(geom.half.saturating_sub(geom.gutter), |layout| {
                    *layout.offsets.last().expect("layout has a total height")
                })
            }
        }
    }

    /// Access wrapping shared by rendering, scrolling, and search navigation.
    fn with_side_layout<R>(&self, width: u32, f: impl FnOnce(&SideLayout) -> R) -> R {
        let mut cache = self.side_layout.borrow_mut();
        if cache.as_ref().is_none_or(|layout| layout.width != width) {
            let wrap = Wrap::new(WrapMode::Soft, width as usize, self.tab_stop, false);
            let mut lines = Vec::with_capacity(self.side_rows.len());
            let mut offsets = vec![0usize];
            for row in &self.side_rows {
                let pair = match row {
                    SideRow::Pair { old, new, .. } => (
                        old.map(|line| layout_line(self.diff.old_line(line), wrap)),
                        new.map(|line| layout_line(self.diff.new_line(line), wrap)),
                    ),
                    _ => (None, None),
                };
                let height = pair
                    .0
                    .as_ref()
                    .map_or(1, LineLayout::display_lines)
                    .max(pair.1.as_ref().map_or(1, LineLayout::display_lines));
                offsets.push(offsets.last().copied().unwrap_or(0).saturating_add(height));
                lines.push(pair);
            }
            *cache = Some(SideLayout {
                width,
                lines,
                offsets,
            });
        }
        f(cache.as_ref().expect("side layout initialized"))
    }

    /// Draw code scrolled horizontally or from a wrapped segment.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_code(
        &self,
        rndr: &mut Render,
        view_rect: Rect,
        origin: Point,
        x: u32,
        width: u32,
        style: &str,
        text: &str,
        side: Side,
        line: usize,
        y: u32,
        segment: Option<&WrapSegment>,
    ) -> Result<()> {
        let scroll = view_rect.tl.x;
        let base = rndr.resolve_style(style);
        let spans = self.highlight(side, line, text);
        let matches = self.match_lines.get(&(side, line));
        let search_current = rndr.resolve_style("search/current");
        let search_match = rndr.resolve_style("search/match");
        let mut span_index = 0usize;
        let mut span_style: Option<Style> = None;
        let line_rect = Rect::new(origin.x, y, width, 1);
        let wrapped;
        let (code, tab_col, start_char, scroll) = if let Some(segment) = segment {
            wrapped = text
                .chars()
                .skip(segment.start_char)
                .take(segment.end_char - segment.start_char)
                .collect::<String>();
            (
                wrapped.as_str(),
                column(segment.start_col),
                segment.start_char,
                0,
            )
        } else {
            (text, 0, 0, scroll)
        };
        run_paint::paint_run(
            rndr,
            code,
            tab_col,
            0,
            start_char,
            self.tab_stop,
            scroll,
            width,
            x,
            y,
            line_rect,
            |render, g_start, g_end| {
                if let Some(index) = matches.and_then(|indices| {
                    indices.iter().find(|&&index| {
                        let found = &self.matches[index];
                        g_start < found.columns.end && g_end > found.columns.start
                    })
                }) {
                    return if Some(*index) == self.current_match {
                        search_current.clone()
                    } else {
                        search_match.clone()
                    };
                }
                while let Some(span) = spans.get(span_index) {
                    if span.range.end <= g_start {
                        span_index += 1;
                        span_style = None;
                        continue;
                    }
                    if span.range.start < g_end && span.range.end > g_start {
                        let merged =
                            span_style.get_or_insert_with(|| span.paint_style(render, &base));
                        return merged.clone();
                    }
                    break;
                }
                base.clone()
            },
        )
    }

    /// Draw the unified body.
    fn draw_unified(&self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let geom = self.geometry(view_rect.w);
        let first = view_rect.tl.y as usize;
        let last = first
            .saturating_add(view_rect.h as usize)
            .min(self.rows.len());
        for index in first..last {
            let y = origin.y + column(index - first);
            match &self.rows[index] {
                DiffRow::Unchanged { old, .. } => {
                    Self::draw_number(
                        rndr,
                        origin.x,
                        y,
                        geom.gutter,
                        ' ',
                        Some(*old),
                        geom.digits,
                        "context",
                    )?;
                    self.draw_code(
                        rndr,
                        view_rect,
                        origin,
                        origin.x + geom.gutter,
                        geom.width - geom.gutter,
                        "context",
                        self.diff.old_line(*old),
                        Side::Old,
                        *old,
                        y,
                        None,
                    )?;
                }
                DiffRow::Removed { old } => {
                    Self::draw_number(
                        rndr,
                        origin.x,
                        y,
                        geom.gutter,
                        '-',
                        Some(*old),
                        geom.digits,
                        "removed",
                    )?;
                    self.draw_code(
                        rndr,
                        view_rect,
                        origin,
                        origin.x + geom.gutter,
                        geom.width - geom.gutter,
                        "removed",
                        self.diff.old_line(*old),
                        Side::Old,
                        *old,
                        y,
                        None,
                    )?;
                }
                DiffRow::Added { new } => {
                    Self::draw_number(
                        rndr,
                        origin.x,
                        y,
                        geom.gutter,
                        '+',
                        Some(*new),
                        geom.digits,
                        "added",
                    )?;
                    self.draw_code(
                        rndr,
                        view_rect,
                        origin,
                        origin.x + geom.gutter,
                        geom.width - geom.gutter,
                        "added",
                        self.diff.new_line(*new),
                        Side::New,
                        *new,
                        y,
                        None,
                    )?;
                }
                DiffRow::Gap { .. } => {
                    rndr.fill("gap", Rect::new(origin.x, y, geom.width, 1), ' ')?;
                    Self::draw_number(
                        rndr,
                        origin.x,
                        y,
                        geom.gutter,
                        '…',
                        None,
                        geom.digits,
                        "gap",
                    )?;
                }
                DiffRow::Header { old, new } => {
                    rndr.fill("header", Rect::new(origin.x, y, geom.width, 1), ' ')?;
                    rndr.text(
                        "header",
                        Line::new(origin.x + geom.gutter, y, geom.width - geom.gutter),
                        &header_text(old, new),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Draw the side-by-side body.
    fn draw_side_by_side(&self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let geom = self.geometry(view_rect.w);
        let left = origin.x;
        let right = origin.x + geom.width - geom.half;
        let first = view_rect.tl.y as usize;
        self.with_side_layout(
            geom.half.saturating_sub(geom.gutter),
            |layout| -> Result<()> {
                let last = first
                    .saturating_add(view_rect.h as usize)
                    .min(*layout.offsets.last().expect("layout has a total height"));
                let mut index = layout
                    .offsets
                    .partition_point(|&row| row <= first)
                    .saturating_sub(1);
                for display in first..last {
                    while layout.offsets[index + 1] <= display {
                        index += 1;
                    }
                    let continuation = display - layout.offsets[index];
                    let pair = &layout.lines[index];
                    let y = origin.y + column(display - first);
                    match &self.side_rows[index] {
                        SideRow::Pair { old, new, changed } => {
                            let (left_style, right_style) = if *changed {
                                ("removed", "added")
                            } else {
                                ("context", "context")
                            };
                            let left_marker = if *changed { '-' } else { ' ' };
                            let right_marker = if *changed { '+' } else { ' ' };
                            self.draw_pair_side(
                                rndr,
                                view_rect,
                                origin,
                                &geom,
                                *old,
                                Side::Old,
                                left,
                                left_marker,
                                left_style,
                                y,
                                pair.0.as_ref().and_then(|line| line.segment(continuation)),
                            )?;
                            self.draw_pair_side(
                                rndr,
                                view_rect,
                                origin,
                                &geom,
                                *new,
                                Side::New,
                                right,
                                right_marker,
                                right_style,
                                y,
                                pair.1.as_ref().and_then(|line| line.segment(continuation)),
                            )?;
                        }
                        SideRow::Gap => {
                            rndr.fill("gap", Rect::new(left, y, geom.half, 1), ' ')?;
                            rndr.fill("gap", Rect::new(right, y, geom.half, 1), ' ')?;
                            Self::draw_number(
                                rndr,
                                left,
                                y,
                                geom.gutter,
                                '…',
                                None,
                                geom.digits,
                                "gap",
                            )?;
                            Self::draw_number(
                                rndr,
                                right,
                                y,
                                geom.gutter,
                                '…',
                                None,
                                geom.digits,
                                "gap",
                            )?;
                        }
                        SideRow::Header { old, new } => {
                            rndr.fill("header", Rect::new(left, y, geom.width, 1), ' ')?;
                            rndr.text(
                                "header",
                                Line::new(left + geom.gutter, y, geom.width - geom.gutter),
                                &header_text(old, new),
                            )?;
                        }
                    }
                }
                Ok(())
            },
        )
    }

    /// Draw one half of a side-by-side pair row.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_pair_side(
        &self,
        rndr: &mut Render,
        view_rect: Rect,
        origin: Point,
        geom: &Geometry,
        line: Option<usize>,
        side: Side,
        x: u32,
        marker: char,
        style: &str,
        y: u32,
        segment: Option<&WrapSegment>,
    ) -> Result<()> {
        let Some((line, segment)) = line.zip(segment) else {
            return rndr.fill("missing", Rect::new(x, y, geom.half, 1), ' ');
        };
        rndr.fill(style, Rect::new(x, y, geom.half, 1), ' ')?;
        Self::draw_number(
            rndr,
            x,
            y,
            geom.gutter,
            if segment.start_char == 0 { marker } else { ' ' },
            (segment.start_char == 0).then_some(line),
            geom.digits,
            style,
        )?;
        let text = match side {
            Side::Old => self.diff.old_line(line),
            Side::New => self.diff.new_line(line),
        };
        self.draw_code(
            rndr,
            view_rect,
            origin,
            x + geom.gutter,
            geom.half.saturating_sub(geom.gutter),
            style,
            text,
            side,
            line,
            y,
            Some(segment),
        )
    }
}

impl Register for DiffView {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()
    }
}

impl Widget for DiffView {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("diff_view");
        let view = ctx.view();
        let origin = view.content_origin();
        let width = view.view_rect().w;
        if let Some(message) = &self.message {
            rndr.text("message", Line::new(origin.x, origin.y, width), message)?;
        } else {
            self.prepare_highlighters();
            match self.strategy {
                Mode::Unified => self.draw_unified(rndr, ctx)?,
                Mode::SideBySide => self.draw_side_by_side(rndr, ctx)?,
            }
        }
        if self.strategy == Mode::SideBySide && width > 0 {
            let geom = self.geometry(width);
            // Even viewport widths leave two central cells so the panes
            // remain exactly equal. Only one carries the divider glyph.
            rndr.fill(
                "separator",
                Rect::new(
                    origin.x + geom.half,
                    origin.y,
                    width - geom.half * 2,
                    view.view_rect().h,
                ),
                ' ',
            )?;
            rndr.fill(
                "separator",
                Rect::new(origin.x + geom.half, origin.y, 1, view.view_rect().h),
                '│',
            )?;
        }
        // A model that takes a while to compute shows a spinner in the top
        // right corner, over whatever the view showed before.
        if let Some(started) = self.loading
            && started.elapsed() >= LOADING_DELAY
        {
            let label = format!("{} diffing", Spinner::LINE.frame(started.elapsed()));
            let label_width = text::width(&label).min(width);
            let x = origin.x + width.saturating_sub(label_width);
            rndr.text("loading", Line::new(x, origin.y, label_width), &label)?;
        }
        Ok(())
    }

    fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
        // A waiting view repaints to turn its spinner.
        Ok(self.loading.map(|_| Spinner::LINE.period()))
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => {
                let gutter = digits(self.diff.old_len().max(self.diff.new_len())) + 2;
                let half = gutter + self.old_width.max(self.new_width) + 1;
                if self.strategy == Mode::Unified {
                    half
                } else {
                    half * 2 + 1
                }
            }
        };
        let height = match c.height {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => column(self.body_rows(width)),
        };
        c.clamp(Size::new(width.max(1), height.max(1)))
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let geom = self.geometry(view.w);
        let rows = self.body_rows(view.w);
        Size::new(geom.width, column(rows))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("diff_view")
    }
}

/// Convert a `usize` cell count to the coordinate type.
fn column(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Return each change head's index from its header and changed-row flags.
///
/// Context scope opens each block with a header. Whole-file scope has no
/// headers, so the head of a change is its first removed or added row.
fn change_heads(rows: impl IntoIterator<Item = (bool, bool)>, scope: Scope) -> Vec<usize> {
    let mut heads = Vec::new();
    let mut in_change = false;
    for (index, (header, changed)) in rows.into_iter().enumerate() {
        if header || (scope == Scope::WholeFile && changed && !in_change) {
            heads.push(index);
        }
        in_change = header || changed;
    }
    heads
}

/// Return the number of decimal digits in `value`, at least one.
fn digits(value: usize) -> u32 {
    let mut digits = 1u32;
    let mut value = value / 10;
    while value > 0 {
        digits += 1;
        value /= 10;
    }
    digits
}

/// Return the display width of the widest line on one side.
fn widest_line(diff: &Diff, side: Side, tab_stop: usize) -> u32 {
    let len = match side {
        Side::Old => diff.old_len(),
        Side::New => diff.new_len(),
    };
    (0..len)
        .map(|line| {
            let text = match side {
                Side::Old => diff.old_line(line),
                Side::New => diff.new_line(line),
            };
            text::width(&text::expand_tabs(text, tab_stop))
        })
        .max()
        .unwrap_or(0)
}

/// Pair unified rows into side-by-side rows.
fn side_rows(rows: &[DiffRow]) -> Vec<SideRow> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < rows.len() {
        match &rows[index] {
            DiffRow::Unchanged { old, new } => {
                out.push(SideRow::Pair {
                    old: Some(*old),
                    new: Some(*new),
                    changed: false,
                });
                index += 1;
            }
            DiffRow::Gap { .. } => {
                out.push(SideRow::Gap);
                index += 1;
            }
            DiffRow::Header { old, new } => {
                out.push(SideRow::Header {
                    old: old.clone(),
                    new: new.clone(),
                });
                index += 1;
            }
            DiffRow::Removed { .. } | DiffRow::Added { .. } => {
                let mut removed = Vec::new();
                let mut added = Vec::new();
                while let Some(DiffRow::Removed { old }) = rows.get(index) {
                    removed.push(*old);
                    index += 1;
                }
                while let Some(DiffRow::Added { new }) = rows.get(index) {
                    added.push(*new);
                    index += 1;
                }
                for pair in 0..removed.len().max(added.len()) {
                    out.push(SideRow::Pair {
                        old: removed.get(pair).copied(),
                        new: added.get(pair).copied(),
                        changed: true,
                    });
                }
            }
        }
    }
    out
}

/// Return the heading text for one change block.
fn header_text(old: &Range<usize>, new: &Range<usize>) -> String {
    format!(
        "@@ -{},{} +{},{} @@",
        old.start + 1,
        old.len(),
        new.start + 1,
        new.len()
    )
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use canopy::{
        error::Result,
        geom::{Point, Size},
        testing::harness::Harness,
    };

    use super::{DiffModel, DiffRow, DiffView, Mode, Scope, SideRow, header_text, side_rows};
    use crate::editor::{TextPosition, TextRange};

    #[test]
    fn paired_panes_wrap_equally_and_keep_the_divider_through_empty_rows() -> Result<()> {
        for width in [31, 32] {
            let view = DiffView::new(DiffModel::from_texts(
                "abcdefghijklmnopqrstuvwx\nshared\n",
                "ABCDEFGHIJKLMNOPQRSTUVWXtail\nshared\n",
            ))
            .with_strategy(Mode::SideBySide);
            let mut harness = Harness::builder(view).size(width, 8).build()?;
            harness.render()?;
            let rows = harness.tbuf().lines();
            let separator = if width == 31 { "│" } else { "│ " };
            assert_eq!(
                rows[0],
                format!("-1 abcdefghijkl{separator}+1 ABCDEFGHIJKL")
            );
            assert_eq!(
                rows[1],
                format!("   mnopqrstuvwx{separator}   MNOPQRSTUVWX")
            );
            assert_eq!(
                rows[2],
                format!("               {separator}   tail        ")
            );
            assert!(
                rows[3].contains("shared"),
                "paired lines stay aligned after wrapping"
            );
            for y in 0..8 {
                assert_eq!(
                    harness.buf().get(Point::new(15, y)).expect("divider").ch,
                    '│'
                );
            }
            harness.with_root_widget_context(|_: &mut DiffView, ctx| {
                assert_eq!(
                    ctx.view().canvas.w,
                    width,
                    "paired code never scrolls horizontally"
                );
                Ok(())
            })?;
        }
        Ok(())
    }

    #[test]
    fn wrapped_search_and_hunks_follow_width_changes() -> Result<()> {
        let old = format!(
            "{}old needle\nshared\nold last\n{}",
            "a".repeat(48),
            "tail\n".repeat(10)
        );
        let new = format!(
            "{}new needle\nshared\nnew last\n{}",
            "b".repeat(48),
            "tail\n".repeat(10)
        );
        let view = DiffView::new(DiffModel::from_texts(old, new)).with_strategy(Mode::SideBySide);
        let mut harness = Harness::builder(view).size(33, 4).build()?;
        harness.render()?;
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            view.set_matches(
                ctx,
                vec![
                    TextRange::new(TextPosition::new(0, 52), TextPosition::new(0, 58)),
                    TextRange::new(TextPosition::new(1, 52), TextPosition::new(1, 58)),
                ],
            );
            view.search_next(ctx, 1);
            Ok(())
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("needle"));
        let current = harness.canopy.style().resolve("diff_view/search/current");
        let point = harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            let area = view.match_area(&view.matches[1], ctx.view().view_rect().w);
            Ok(Point::new(area.tl.x, area.tl.y - ctx.view().scroll.y))
        })?;
        assert_eq!(
            harness.buf().get(point).expect("wrapped match").style.bg,
            current.bg.solid_color().expect("solid search style"),
            "wrapped match at {point:?}: {:?}",
            harness.tbuf().lines()
        );
        harness.canopy.set_screen_size(Size::new(25, 4))?;
        harness.render()?;
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            view.search_next(ctx, 0);
            assert_eq!(view.change_heads(25), [0, 9]);
            Ok(())
        })?;
        harness.render()?;
        assert!(
            harness.tbuf().contains_text("needle"),
            "a held match reveals its new wrapped row"
        );
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            assert_eq!(ctx.view().scroll.x, 0);
            view.next_change(ctx);
            Ok(())
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("new last"));
        harness.with_root_widget_context(|_: &mut DiffView, ctx| {
            assert_eq!(ctx.view().scroll.y, 9);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn wrapped_panes_preserve_tabs_wide_graphemes_and_combining_marks() -> Result<()> {
        let source = "abc\t界e\u{301}fghijklmnopqrst\n";
        let view =
            DiffView::new(DiffModel::from_texts(source, source)).with_strategy(Mode::SideBySide);
        let mut harness = Harness::builder(view).size(33, 10).build()?;
        harness.render()?;
        for start in [3, 20] {
            let mut text = String::new();
            for y in 0..10 {
                for x in start..start + 13 {
                    let cell = harness.buf().get(Point::new(x, y)).expect("code cell");
                    if !cell.continuation && cell.ch != '\0' && !cell.ch.is_whitespace() {
                        text.push(cell.ch);
                        text.push_str(&cell.suffix);
                    }
                }
            }
            assert_eq!(text, source.replace(['\t', '\n'], ""));
        }
        Ok(())
    }

    #[test]
    fn switching_layout_keeps_the_model_and_refreshes_search_rows() -> Result<()> {
        let model = DiffModel::from_texts("old needle\n", "new needle\n");
        let mut harness = Harness::builder(DiffView::new(model.clone()))
            .size(50, 5)
            .build()?;
        harness.render()?;
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            let ranges = vec![
                TextRange::new(TextPosition::new(0, 4), TextPosition::new(0, 10)),
                TextRange::new(TextPosition::new(1, 4), TextPosition::new(1, 10)),
            ];
            view.set_matches(ctx, ranges.clone());
            view.search_next(ctx, 1);
            view.set_loading();
            view.set_strategy(Mode::Unified);
            assert_eq!(view.revision(), 0, "the current layout is a no-op");
            assert_eq!((view.search_matches(), view.search_position()), (2, 2));

            view.set_strategy(Mode::SideBySide);
            assert_eq!(view.strategy(), Mode::SideBySide);
            assert!(ptr::eq(view.diff(), model.diff()));
            assert!(ptr::eq(view.rows(), model.rows.as_ref()));
            assert!(view.loading.is_some());
            assert_eq!(view.revision(), 1);
            assert_eq!(view.search_matches(), 0, "the host refreshes its query");
            view.set_matches(ctx, ranges);
            assert_eq!((view.search_matches(), view.search_position()), (2, 1));
            assert!(view.matches.iter().all(|found| found.row == 0));

            view.set_strategy(Mode::Unified);
            assert_eq!(view.revision(), 2);
            assert!(ptr::eq(view.diff(), model.diff()));
            view.set_model(model.clone());
            assert_eq!(view.strategy(), Mode::Unified);
            view.set_message("binary file");
            view.set_strategy(Mode::SideBySide);
            assert_eq!(view.message(), Some("binary file"));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn hunk_navigation_uses_display_rows_in_both_layouts() -> Result<()> {
        let shared = (0..10)
            .map(|index| format!("shared {index}\n"))
            .collect::<String>();
        let tail = (0..6)
            .map(|index| format!("tail {index}\n"))
            .collect::<String>();
        let old = format!("{}{}old last\n{}", "old\n".repeat(8), shared, tail);
        let new = format!("new\n{shared}new last\n{tail}");
        for (scope, unified_head, paired_head) in
            [(Scope::WholeFile, 19, 18), (Scope::Context(0), 11, 10)]
        {
            for (mode, head) in [
                (Mode::Unified, unified_head),
                (Mode::SideBySide, paired_head),
            ] {
                let view = DiffView::new(DiffModel::from_texts(&old, &new))
                    .with_scope(scope)
                    .with_strategy(mode);
                let mut harness = Harness::builder(view).size(80, 2).build()?;
                harness.render()?;
                harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                    assert_eq!(view.change_heads(80), [0, head]);
                    view.next_change(ctx);
                    Ok(())
                })?;
                harness.render()?;
                harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                    assert_eq!(
                        ctx.view().scroll.y,
                        u32::try_from(head).expect("a short fixture")
                    );
                    view.prev_change(ctx);
                    Ok(())
                })?;
                harness.render()?;
                harness.with_root_widget_context(|_: &mut DiffView, ctx| {
                    assert_eq!(ctx.view().scroll.y, 0);
                    Ok(())
                })?;
            }
        }
        Ok(())
    }

    #[test]
    fn model_clones_share_storage_while_view_builders_replace_only_their_rows() {
        let model = DiffModel::from_texts(
            "alpha\tone\nold needle\nshared\n",
            "alpha\tone\nnew needle\nshared\n",
        );
        let cloned = model.clone();
        assert!(ptr::eq(model.diff(), cloned.diff()));

        let mut view = DiffView::new(cloned);
        let other = DiffView::new(model.clone()).with_strategy(Mode::SideBySide);
        assert!(ptr::eq(view.diff(), other.diff()));
        assert!(ptr::eq(view.rows(), other.rows()));
        let original_rows = other.rows().to_vec();
        let original_widths = (other.old_width, other.new_width);

        view = view.with_scope(Scope::Context(0));
        assert!(ptr::eq(view.diff(), model.diff()));
        assert!(!ptr::eq(view.rows(), other.rows()));
        assert!(!view.search_text().contains("shared"));
        assert_eq!(other.rows(), original_rows);
        assert_eq!(other.scope(), Scope::WholeFile);
        assert_eq!(other.revision(), 0);

        let scoped_rows = view.rows.clone();
        view = view.with_tab_stop(16);
        assert!(!ptr::eq(view.rows(), scoped_rows.as_ref()));
        assert_eq!(view.rows(), scoped_rows.as_ref());
        assert_ne!((view.old_width, view.new_width), original_widths);
        assert!(ptr::eq(view.diff(), model.diff()));
        assert_eq!(other.rows(), original_rows);
        assert_eq!((other.old_width, other.new_width), original_widths);
        assert_eq!(other.tab_stop, super::DEFAULT_TAB_STOP);

        let original = DiffView::new(model);
        assert!(ptr::eq(original.rows(), other.rows()));
        assert_eq!(original.rows(), original_rows);
        assert_eq!(original.scope(), Scope::WholeFile);
        assert_eq!((original.old_width, original.new_width), original_widths);
    }

    #[test]
    fn views_from_model_clones_keep_search_and_loading_state_independent() -> Result<()> {
        let model = DiffModel::from_texts("old needle\n", "new needle\n");
        let mut first = Harness::builder(DiffView::new(model.clone()))
            .size(50, 5)
            .build()?;
        let mut second =
            Harness::builder(DiffView::new(model.clone()).with_strategy(Mode::SideBySide))
                .size(50, 5)
                .build()?;
        first.render()?;
        second.render()?;
        for harness in [&mut first, &mut second] {
            harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                view.set_matches(
                    ctx,
                    vec![
                        TextRange::new(TextPosition::new(0, 4), TextPosition::new(0, 10)),
                        TextRange::new(TextPosition::new(1, 4), TextPosition::new(1, 10)),
                    ],
                );
                Ok(())
            })?;
        }
        first.with_root_widget_context(|view: &mut DiffView, ctx| {
            view.search_next(ctx, 1);
            view.set_loading();
            assert_eq!((view.search_matches(), view.search_position()), (2, 2));
            view.set_model(model.clone());
            assert!(ptr::eq(view.diff(), model.diff()));
            assert_eq!(view.revision(), 1);
            assert_eq!((view.search_matches(), view.search_position()), (0, 0));
            assert!(view.loading.is_none());
            assert!(!view.prepared);
            Ok(())
        })?;
        second.with_root_widget_context(|view: &mut DiffView, _| {
            assert!(ptr::eq(view.diff(), model.diff()));
            assert_eq!(view.strategy, Mode::SideBySide);
            assert!(!view.side_rows.is_empty());
            assert_eq!(view.revision(), 0);
            assert_eq!((view.search_matches(), view.search_position()), (2, 1));
            assert!(view.loading.is_none());
            assert!(view.prepared);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn search_highlights_both_versions_and_wraps_in_each_layout() -> Result<()> {
        for mode in [Mode::Unified, Mode::SideBySide] {
            let view = DiffView::new(DiffModel::from_texts(
                "alpha\nold needle\nsame needle\n",
                "alpha\nnew needle\nsame needle\n",
            ))
            .with_strategy(mode);
            let mut harness = Harness::builder(view).size(50, 5).build()?;
            harness.render()?;
            harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                assert_eq!(
                    view.search_text(),
                    "alpha\nold needle\nnew needle\nsame needle"
                );
                view.set_matches(
                    ctx,
                    vec![
                        TextRange::new(TextPosition::new(1, 4), TextPosition::new(1, 10)),
                        TextRange::new(TextPosition::new(2, 4), TextPosition::new(2, 10)),
                        TextRange::new(TextPosition::new(3, 5), TextPosition::new(3, 11)),
                    ],
                );
                assert_eq!((view.search_matches(), view.search_position()), (3, 1));
                Ok(())
            })?;
            harness.render()?;
            let current = harness.canopy.style().resolve("diff_view/search/current");
            let cell = harness.buf().get(Point::new(7, 1)).expect("first match");
            assert_eq!(
                cell.style.bg,
                current.bg.solid_color().unwrap(),
                "the removed text is highlighted"
            );
            harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                view.search_next(ctx, 1);
                assert_eq!(view.search_position(), 2);
                Ok(())
            })?;
            harness.render()?;
            let point = match mode {
                Mode::Unified => Point::new(7, 2),
                Mode::SideBySide => Point::new(33, 1),
            };
            assert_eq!(
                harness.buf().get(point).expect("added match").style.bg,
                current.bg.solid_color().unwrap()
            );
            harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                view.search_next(ctx, -2);
                assert_eq!(view.search_position(), 3, "backwards navigation wraps");
                let revision = view.revision();
                view.set_model(DiffModel::from_texts("", "replacement\n"));
                assert_ne!(view.revision(), revision);
                assert_eq!(
                    view.search_matches(),
                    0,
                    "old matches leave with their model"
                );
                Ok(())
            })?;
        }
        Ok(())
    }

    #[test]
    fn search_omits_hidden_context() {
        let view = DiffView::new(DiffModel::from_texts(
            "alpha\nold\nshared\n",
            "alpha\nnew\nshared\n",
        ))
        .with_scope(Scope::Context(0));
        let text = view.search_text();
        assert!(text.contains("old\nnew"));
        assert!(!text.contains("alpha"));
        assert!(!text.contains("shared"));
    }

    #[test]
    fn side_rows_pair_removed_and_added_runs() {
        let rows = vec![
            DiffRow::Unchanged { old: 0, new: 0 },
            DiffRow::Removed { old: 1 },
            DiffRow::Removed { old: 2 },
            DiffRow::Added { new: 1 },
            DiffRow::Unchanged { old: 3, new: 2 },
        ];
        assert_eq!(
            side_rows(&rows),
            vec![
                SideRow::Pair {
                    old: Some(0),
                    new: Some(0),
                    changed: false
                },
                SideRow::Pair {
                    old: Some(1),
                    new: Some(1),
                    changed: true
                },
                SideRow::Pair {
                    old: Some(2),
                    new: None,
                    changed: true
                },
                SideRow::Pair {
                    old: Some(3),
                    new: Some(2),
                    changed: false
                },
            ]
        );
    }

    #[test]
    fn side_rows_pair_a_lone_addition_with_an_empty_old_side() {
        let rows = vec![
            DiffRow::Unchanged { old: 0, new: 0 },
            DiffRow::Added { new: 1 },
            DiffRow::Added { new: 2 },
        ];
        assert_eq!(
            side_rows(&rows),
            vec![
                SideRow::Pair {
                    old: Some(0),
                    new: Some(0),
                    changed: false
                },
                SideRow::Pair {
                    old: None,
                    new: Some(1),
                    changed: true
                },
                SideRow::Pair {
                    old: None,
                    new: Some(2),
                    changed: true
                },
            ]
        );
    }

    #[test]
    fn headers_name_both_ranges_one_based() {
        assert_eq!(header_text(&(0..3), &(0..2)), "@@ -1,3 +1,2 @@");
        assert_eq!(header_text(&(9..12), &(9..12)), "@@ -10,3 +10,3 @@");
    }
}
