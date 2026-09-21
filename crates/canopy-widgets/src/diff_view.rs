//! Diff view widget over the [`Diff`] row model.
//!
//! [`DiffView`] renders two full texts in unified or side-by-side layout, with
//! whole-file or context scope. The host supplies the texts and, when the
//! `editor` feature is on, one [`Highlighter`] per side, so parser state stays
//! with its own version. Line numbers and each half's gutter stay pinned while
//! the code scrolls horizontally.
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

use std::ops::Range;

#[cfg(feature = "editor")]
use canopy::style::Style;
use canopy::{
    Context, EventOutcome, FocusDirection, NodeName, Render, ViewContext, Widget, derive_commands,
    error::Result,
    event::key,
    geom::{Line, Point, Rect, Size},
    layout::{CanvasContext, Constraint, MeasureConstraints, Measurement},
    text,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::diff::{Diff, DiffRow, Scope};
#[cfg(feature = "editor")]
use crate::editor::highlight::{HighlightSpan, Highlighter};

/// How a diff view arranges the two versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// One column: removed lines above added lines.
    Unified,
    /// Two columns: the old version left, the new version right.
    SideBySide,
}

/// One side of a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The expensive parts of a diff view, ready to adopt without recomputation.
///
/// A host that computes diffs off the UI thread builds one of these there and
/// hands it to [`DiffView::from_prepared`], so the UI thread only moves data.
pub struct PreparedDiff {
    /// The line diff.
    diff: Diff,
    /// Rows for `scope`, in unified order.
    rows: Vec<DiffRow>,
    /// Display width of the widest old line, tabs expanded.
    old_width: u32,
    /// Display width of the widest new line, tabs expanded.
    new_width: u32,
    /// The scope the rows were built for.
    scope: Scope,
    /// The tab stop the widths were built for.
    tab_stop: usize,
}

impl PreparedDiff {
    /// Compute every part of a view for `diff` at `scope` and `tab_stop`.
    ///
    /// Side-by-side pairing is deferred until a view needs it, because a
    /// unified view never does.
    #[must_use]
    pub fn new(diff: Diff, scope: Scope, tab_stop: usize) -> Self {
        let tab_stop = tab_stop.max(1);
        let old_width = widest_line(&diff, Side::Old, tab_stop);
        let new_width = widest_line(&diff, Side::New, tab_stop);
        let rows = diff.rows(scope);
        Self {
            diff,
            rows,
            old_width,
            new_width,
            scope,
            tab_stop,
        }
    }

    /// Return the line diff.
    #[must_use]
    pub fn diff(&self) -> &Diff {
        &self.diff
    }
}

/// A line diff with a display strategy, a scope, and optional highlighting.
pub struct DiffView {
    /// The line diff being shown.
    diff: Diff,
    /// How the two sides are laid out.
    strategy: Strategy,
    /// How much unchanged text the rows show.
    scope: Scope,
    /// Rows for the current scope, in unified order.
    rows: Vec<DiffRow>,
    /// Rows paired for side-by-side layout.
    side_rows: Vec<SideRow>,
    /// Display width of the widest old line, tabs expanded.
    old_width: u32,
    /// Display width of the widest new line, tabs expanded.
    new_width: u32,
    /// Tab stop width in columns.
    tab_stop: usize,
    /// Highlighter for the old side.
    #[cfg(feature = "editor")]
    old_highlighter: Option<Box<dyn Highlighter>>,
    /// Highlighter for the new side.
    #[cfg(feature = "editor")]
    new_highlighter: Option<Box<dyn Highlighter>>,
    /// Whether both highlighters hold the current texts.
    #[cfg(feature = "editor")]
    prepared: bool,
}

#[derive_commands]
impl DiffView {
    /// Construct a view of `old` and `new` in unified, whole-file layout.
    pub fn new(old: impl Into<String>, new: impl Into<String>) -> Self {
        Self::from_prepared(PreparedDiff::new(Diff::new(old, new), Scope::WholeFile, 4))
    }

    /// Adopt a diff whose expensive parts are already computed.
    ///
    /// The view starts in unified layout; use [`Self::with_strategy`] to
    /// change that. Highlighters are installed afterwards, as for
    /// [`Self::new`].
    #[must_use]
    pub fn from_prepared(prepared: PreparedDiff) -> Self {
        Self {
            diff: prepared.diff,
            strategy: Strategy::Unified,
            scope: prepared.scope,
            rows: prepared.rows,
            side_rows: Vec::new(),
            old_width: prepared.old_width,
            new_width: prepared.new_width,
            tab_stop: prepared.tab_stop,
            #[cfg(feature = "editor")]
            old_highlighter: None,
            #[cfg(feature = "editor")]
            new_highlighter: None,
            #[cfg(feature = "editor")]
            prepared: false,
        }
    }

    /// Replace the view with an already computed diff.
    pub fn set_prepared(&mut self, prepared: PreparedDiff) {
        self.diff = prepared.diff;
        self.scope = prepared.scope;
        self.rows = prepared.rows;
        self.side_rows = Vec::new();
        self.old_width = prepared.old_width;
        self.new_width = prepared.new_width;
        self.tab_stop = prepared.tab_stop;
        self.ensure_side_rows();
        #[cfg(feature = "editor")]
        {
            self.prepared = false;
        }
    }

    /// Set the display strategy.
    #[must_use]
    pub fn with_strategy(mut self, strategy: Strategy) -> Self {
        self.set_strategy(strategy);
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
    #[cfg(feature = "editor")]
    #[must_use]
    pub fn with_old_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        self.old_highlighter = Some(highlighter);
        self.prepared = false;
        self
    }

    /// Install a highlighter for the new side.
    #[cfg(feature = "editor")]
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

    /// Return the display strategy.
    #[must_use]
    pub fn strategy(&self) -> Strategy {
        self.strategy
    }

    /// Return the display scope.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Replace the diff, discarding highlight state.
    pub fn set_diff(&mut self, diff: Diff) {
        self.diff = diff;
        self.rebuild();
    }

    /// Set the display strategy.
    pub fn set_strategy(&mut self, strategy: Strategy) {
        self.strategy = strategy;
        self.ensure_side_rows();
    }

    /// Pair the rows for side-by-side layout, once, when that layout needs it.
    fn ensure_side_rows(&mut self) {
        if self.strategy == Strategy::SideBySide && self.side_rows.is_empty() {
            self.side_rows = side_rows(&self.rows);
        }
    }

    /// Set the display scope and rebuild the rows.
    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
        self.rebuild_rows();
    }

    #[command]
    /// Scroll by one line or column in the specified direction.
    /// @param dir The direction to scroll.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: FocusDirection) {
        match dir {
            FocusDirection::Up | FocusDirection::Prev => c.scroll_up(),
            FocusDirection::Down | FocusDirection::Next => c.scroll_down(),
            FocusDirection::Left => c.scroll_left(),
            FocusDirection::Right => c.scroll_right(),
        };
    }

    #[command]
    /// Page through the diff.
    /// @param delta Signed page delta. Positive moves down and negative moves
    /// up.
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) {
        if delta < 0 {
            c.page_up();
        } else if delta > 0 {
            c.page_down();
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
        self.rows = self.diff.rows(self.scope);
        self.side_rows = side_rows(&self.rows);
        #[cfg(feature = "editor")]
        {
            self.prepared = false;
        }
    }

    /// Prepare both highlighters for the current texts, once.
    fn prepare_highlighters(&mut self) {
        #[cfg(feature = "editor")]
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
    #[cfg(feature = "editor")]
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
            Strategy::Unified => {
                let width = (gutter + line_width).max(view_w);
                Geometry {
                    digits,
                    gutter,
                    half: 0,
                    width,
                }
            }
            Strategy::SideBySide => {
                let natural_half = gutter + line_width;
                let natural = natural_half * 2 + 1;
                let half = if view_w >= natural {
                    (view_w - 1) / 2
                } else {
                    natural_half
                };
                Geometry {
                    digits,
                    gutter,
                    half,
                    width: half * 2 + 1,
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
    fn body_rows(&self) -> usize {
        match self.strategy {
            Strategy::Unified => self.rows.len(),
            Strategy::SideBySide => self.side_rows.len(),
        }
    }

    /// Draw one code line, scrolled horizontally and clipped to `width`.
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
    ) -> Result<()> {
        let scroll = view_rect.tl.x;
        let base = rndr.resolve_style(style);
        #[cfg(feature = "editor")]
        let spans = self.highlight(side, line, text);
        #[cfg(feature = "editor")]
        let mut span_index = 0usize;
        #[cfg(feature = "editor")]
        let mut span_style: Option<Style> = None;
        let line_rect = Rect::new(origin.x, y, width, 1);
        let mut col = 0u32;
        let mut char_index = 0usize;
        for grapheme in text.graphemes(true) {
            let chars = grapheme.chars().count();
            let g_start = char_index;
            let g_end = char_index.saturating_add(chars);
            let cells = if grapheme == "\t" {
                column(text::tab_width(col as usize, self.tab_stop))
            } else {
                column(text::grapheme_width(grapheme))
            };
            let start_col = col;
            let end_col = col.saturating_add(cells);
            col = end_col;
            char_index = g_end;
            if end_col <= scroll {
                continue;
            }
            let draw = start_col.saturating_sub(scroll);
            if draw >= width {
                break;
            }
            let mut chosen = base.clone();
            #[cfg(feature = "editor")]
            while let Some(span) = spans.get(span_index) {
                if span.range.end <= g_start {
                    span_index += 1;
                    span_style = None;
                    continue;
                }
                if span.range.start < g_end && span.range.end > g_start {
                    let merged = span_style.get_or_insert_with(|| {
                        let mut styled = span.style.clone();
                        styled.bg = base.bg.clone();
                        rndr.apply_effects(styled)
                    });
                    chosen = merged.clone();
                }
                break;
            }
            let point = Point {
                x: x.saturating_add(draw),
                y,
            };
            if grapheme == "\t" {
                for offset in 0..cells {
                    let position = draw.saturating_add(offset);
                    if position >= width {
                        break;
                    }
                    let cell = Point {
                        x: x.saturating_add(position),
                        y,
                    };
                    rndr.put_cell(chosen.resolve_at(line_rect, cell), cell, ' ')?;
                }
            } else {
                rndr.put_grapheme(chosen.resolve_at(line_rect, point), point, grapheme)?;
            }
        }
        Ok(())
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
                        "diff/context",
                    )?;
                    self.draw_code(
                        rndr,
                        view_rect,
                        origin,
                        origin.x + geom.gutter,
                        geom.width - geom.gutter,
                        "diff/context",
                        self.diff.old_line(*old),
                        Side::Old,
                        *old,
                        y,
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
                        "diff/removed",
                    )?;
                    self.draw_code(
                        rndr,
                        view_rect,
                        origin,
                        origin.x + geom.gutter,
                        geom.width - geom.gutter,
                        "diff/removed",
                        self.diff.old_line(*old),
                        Side::Old,
                        *old,
                        y,
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
                        "diff/added",
                    )?;
                    self.draw_code(
                        rndr,
                        view_rect,
                        origin,
                        origin.x + geom.gutter,
                        geom.width - geom.gutter,
                        "diff/added",
                        self.diff.new_line(*new),
                        Side::New,
                        *new,
                        y,
                    )?;
                }
                DiffRow::Gap { .. } => {
                    rndr.fill("diff/gap", Rect::new(origin.x, y, geom.width, 1), ' ')?;
                    Self::draw_number(
                        rndr,
                        origin.x,
                        y,
                        geom.gutter,
                        '…',
                        None,
                        geom.digits,
                        "diff/gap",
                    )?;
                }
                DiffRow::Header { old, new } => {
                    rndr.fill("diff/header", Rect::new(origin.x, y, geom.width, 1), ' ')?;
                    rndr.text(
                        "diff/header",
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
        let right = origin.x + geom.half + 1;
        let first = view_rect.tl.y as usize;
        let last = first
            .saturating_add(view_rect.h as usize)
            .min(self.side_rows.len());
        for index in first..last {
            let y = origin.y + column(index - first);
            match &self.side_rows[index] {
                SideRow::Pair { old, new, changed } => {
                    let (left_style, right_style) = if *changed {
                        ("diff/removed", "diff/added")
                    } else {
                        ("diff/context", "diff/context")
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
                    )?;
                }
                SideRow::Gap => {
                    rndr.fill("diff/gap", Rect::new(left, y, geom.half, 1), ' ')?;
                    rndr.fill("diff/gap", Rect::new(right, y, geom.half, 1), ' ')?;
                    Self::draw_number(
                        rndr,
                        left,
                        y,
                        geom.gutter,
                        '…',
                        None,
                        geom.digits,
                        "diff/gap",
                    )?;
                    Self::draw_number(
                        rndr,
                        right,
                        y,
                        geom.gutter,
                        '…',
                        None,
                        geom.digits,
                        "diff/gap",
                    )?;
                }
                SideRow::Header { old, new } => {
                    rndr.fill("diff/header", Rect::new(left, y, geom.width, 1), ' ')?;
                    rndr.text(
                        "diff/header",
                        Line::new(left + geom.gutter, y, geom.width - geom.gutter),
                        &header_text(old, new),
                    )?;
                }
            }
            rndr.fill(
                "diff/separator",
                Rect::new(origin.x + geom.half, y, 1, 1),
                '│',
            )?;
        }
        Ok(())
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
    ) -> Result<()> {
        let Some(line) = line else {
            return rndr.fill("diff/missing", Rect::new(x, y, geom.half, 1), ' ');
        };
        Self::draw_number(
            rndr,
            x,
            y,
            geom.gutter,
            marker,
            Some(line),
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
        )
    }
}

impl Widget for DiffView {
    fn key_outcome(&self, _key: key::Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
        Some(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        self.prepare_highlighters();
        match self.strategy {
            Strategy::Unified => self.draw_unified(rndr, ctx),
            Strategy::SideBySide => self.draw_side_by_side(rndr, ctx),
        }
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => self.geometry(0).width,
        };
        let height = match c.height {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => column(self.body_rows()),
        };
        c.clamp(Size::new(width.max(1), height.max(1)))
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let geom = self.geometry(view.w);
        let rows = match self.strategy {
            Strategy::Unified => self.rows.len(),
            Strategy::SideBySide => self.side_rows.len(),
        };
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
            column(text::display_width(&text::expand_tabs(text, tab_stop)))
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
    use super::{DiffRow, SideRow, header_text, side_rows};

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
