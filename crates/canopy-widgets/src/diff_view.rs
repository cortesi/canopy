//! Diff view widget over the [`Diff`] row model.
//!
//! [`DiffView`] renders two full texts in unified or side-by-side layout, with
//! whole-file or context scope. The host supplies the texts and, optionally,
//! one [`Highlighter`] per side, so parser state stays with its own version.
//! Unified code scrolls horizontally with pinned line numbers. Side-by-side
//! panes have equal widths and wrap their code to fit.
//!
//! A view can show several diffs one after the other, such as the files of a
//! change set. Each is a [`DiffSection`] under a title line of its own, and
//! the view scrolls, searches, and steps through them as one text.
//!
//! The view resolves these style names:
//!
//! - `diff_view/context`: unchanged lines.
//! - `diff_view/removed`: removed lines.
//! - `diff_view/added`: added lines.
//! - `diff_view/gap`: hidden context.
//! - `diff_view/header`: block headings.
//! - `diff_view/missing`: the empty half of a one-sided change.
//! - `diff_view/separator`: the side-by-side divider.
//! - `diff_view/title`: the line that names a section.
//! - `diff_view/message`: a message in place of a diff.
//! - `diff_view/text`: text in place of a diff.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    iter, mem,
    ops::Range,
    sync::{Arc, LazyLock},
    time::Duration,
};

use canopy::{
    Context, NodeName, Register, Setup, ViewContext, Widget, derive_commands,
    error::Result,
    geom::{Line, Point, Rect, Size},
    layout::{CanvasContext, MeasureConstraints, Measurement, ScrollAxis, ScrollMark, ScrollOp},
    render::Render,
    runtime::{NodeWakeHandle, PollLifetime},
    text,
};

use crate::{
    Spinner,
    diff::{Diff, DiffRow, Scope},
    editor::{
        TextRange, WrapMode,
        layout::{LineLayout, Wrap, WrapSegment, layout_line},
    },
    highlight::{HighlightSpan, Highlighter, SpanWalk},
    run_paint::{self, column},
    spinner::SpinnerRun,
    view_search,
};

/// Tab stop a model built from texts assumes.
const DEFAULT_TAB_STOP: usize = 4;

/// How long a model may compute before the spinner shows.
const LOADING_DELAY: Duration = Duration::from_millis(150);

/// How a diff view arranges the two versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLayout {
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

impl Side {
    /// Return the text of zero-based `line` on this side of `diff`.
    fn line(self, diff: &Diff, line: usize) -> &str {
        match self {
            Self::Old => diff.old_line(line),
            Self::New => diff.new_line(line),
        }
    }
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

/// Column layout of one diff view body, which every section shares, so the
/// gutters of a view line up.
struct Geometry {
    /// The display layout the columns are for.
    layout: DiffLayout,
    /// Width of the view the columns fit.
    view: u32,
    /// Digits in the widest line number.
    digits: u32,
    /// Line-number gutter width, marker included.
    gutter: u32,
    /// Width of one side-by-side half, divider excluded.
    half: u32,
    /// Total canvas width.
    width: u32,
}

impl Geometry {
    /// Return the code width of one side-by-side half, gutter excluded.
    fn pane_code_width(&self) -> u32 {
        self.half.saturating_sub(self.gutter)
    }
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
    /// Section containing the match.
    section: usize,
    /// Version containing the match.
    side: Side,
    /// Zero-based source line within that version.
    line: usize,
    /// Character columns containing the matched text.
    columns: Range<usize>,
    /// Logical row in the section's body in the active layout, before
    /// wrapping.
    row: usize,
}

/// What a section shows.
enum Content {
    /// A model of a diff.
    Diff(DiffModel),
    /// A message in place of any diff.
    Message(String),
    /// Text in place of any diff, shown as it is written.
    Text(String),
}

impl Content {
    /// Return the lines of a body that is no diff, wrapped to `width`, and
    /// the style that draws them.
    fn prose(&self, width: u32) -> Option<(&'static str, Vec<String>)> {
        match self {
            Self::Diff(_) => None,
            Self::Message(message) => Some(("message", wrap_message(message, width))),
            Self::Text(written) => Some((
                "text",
                text::wrap_with(written, width, text::WrapOptions::PROSE),
            )),
        }
    }
}

/// The model that a section showing a message or text presents to its
/// queries: one with no lines.
static EMPTY_MODEL: LazyLock<DiffModel> = LazyLock::new(|| DiffModel::from_texts("", ""));

/// One diff of a [`DiffView`] that shows several, one after the other, such
/// as one file of a change set.
///
/// A section shows a diff model, a message, or text. A title names it on a
/// line of its own above its rows, which a blank row sets apart from the
/// section before.
pub struct DiffSection {
    /// The line that names the section.
    title: Option<String>,
    /// The diff, the message, or the text being shown.
    content: Content,
    /// Rows paired for side-by-side layout.
    side_rows: Vec<SideRow>,
    /// Paired wrapping, rebuilt when the pane width or model changes.
    side_layout: RefCell<Option<SideLayout>>,
    /// Highlighter for the old side.
    old_highlighter: Option<Box<dyn Highlighter>>,
    /// Highlighter for the new side.
    new_highlighter: Option<Box<dyn Highlighter>>,
    /// Whether both highlighters hold the current texts.
    prepared: bool,
}

impl DiffSection {
    /// Show `model`. Install highlighters with [`Self::with_highlighters`].
    #[must_use]
    pub fn new(model: DiffModel) -> Self {
        Self::of(Content::Diff(model))
    }

    /// Show `message` in place of a diff, such as for a binary file. The
    /// message wraps to the view width.
    #[must_use]
    pub fn message(message: impl Into<String>) -> Self {
        Self::of(Content::Message(message.into()))
    }

    /// Show `text` in place of a diff, as it is written, such as the
    /// message of a commit above the diffs of its files. Each of its lines
    /// starts a row, and wraps to the view width.
    ///
    /// A message says why the view shows no diff, and reads as one
    /// paragraph. Text is content of its own, so it keeps its lines. A
    /// search does not read it.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::of(Content::Text(text.into()))
    }

    /// Construct an untitled section of `content`.
    fn of(content: Content) -> Self {
        Self {
            title: None,
            content,
            side_rows: Vec::new(),
            side_layout: RefCell::new(None),
            old_highlighter: None,
            new_highlighter: None,
            prepared: false,
        }
    }

    /// Name the section with `title`, such as a file's path.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Install the highlighters for the old and new sides.
    #[must_use]
    pub fn with_highlighters(
        mut self,
        old: Option<Box<dyn Highlighter>>,
        new: Option<Box<dyn Highlighter>>,
    ) -> Self {
        self.set_highlighters(old, new);
        self
    }

    /// Replace the highlighters for the old and new sides.
    fn set_highlighters(
        &mut self,
        old: Option<Box<dyn Highlighter>>,
        new: Option<Box<dyn Highlighter>>,
    ) {
        self.old_highlighter = old;
        self.new_highlighter = new;
        self.prepared = false;
    }

    /// Return the model shown, or a model with no lines for a message or
    /// text.
    fn model(&self) -> &DiffModel {
        match &self.content {
            Content::Diff(model) => model,
            Content::Message(_) | Content::Text(_) => &EMPTY_MODEL,
        }
    }

    /// Pair the rows for side-by-side layout, once, when `layout` needs it.
    fn pair_rows(&mut self, layout: DiffLayout) {
        if layout == DiffLayout::SideBySide && self.side_rows.is_empty() {
            self.side_rows = side_rows(&self.model().rows);
        }
    }

    /// Drop what the rows derived, after the rows changed.
    fn reset_rows(&mut self, layout: DiffLayout) {
        self.side_rows = Vec::new();
        self.pair_rows(layout);
        self.side_layout.get_mut().take();
        self.prepared = false;
    }

    /// Prepare both highlighters for the current texts, once. A view calls
    /// this only for a section it draws, so a section out of view costs no
    /// parse.
    fn prepare_highlighters(&mut self) {
        if !self.prepared {
            let diff = &self.model().diff;
            if let Some(highlighter) = &self.old_highlighter {
                highlighter.prepare(diff.old_text());
            }
            if let Some(highlighter) = &self.new_highlighter {
                highlighter.prepare(diff.new_text());
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
        highlighter.as_ref().map_or_else(Vec::new, |highlighter| {
            highlighter.highlight_line(line, text)
        })
    }

    /// Return the rows the section's body takes in the display.
    fn body_rows(&self, geometry: &Geometry) -> usize {
        if let Some((_, lines)) = self.content.prose(geometry.view) {
            return lines.len();
        }
        match geometry.layout {
            DiffLayout::Unified => self.model().rows.len(),
            DiffLayout::SideBySide => self.with_side_layout(geometry.pane_code_width(), |layout| {
                *layout.offsets.last().expect("layout has a total height")
            }),
        }
    }

    /// Return the change heads of the body, in its display rows.
    fn change_heads(&self, geometry: &Geometry) -> Vec<usize> {
        let model = self.model();
        match geometry.layout {
            DiffLayout::Unified => change_heads(
                model.rows.iter().map(|row| {
                    (
                        matches!(row, DiffRow::Header { .. }),
                        matches!(row, DiffRow::Removed { .. } | DiffRow::Added { .. }),
                    )
                }),
                model.scope,
            ),
            DiffLayout::SideBySide => {
                let heads = change_heads(
                    self.side_rows.iter().map(|row| {
                        (
                            matches!(row, SideRow::Header { .. }),
                            matches!(row, SideRow::Pair { changed: true, .. }),
                        )
                    }),
                    model.scope,
                );
                self.with_side_layout(geometry.pane_code_width(), |layout| {
                    heads.into_iter().map(|row| layout.offsets[row]).collect()
                })
            }
        }
    }

    /// Access wrapping shared by rendering, scrolling, and search navigation.
    fn with_side_layout<R>(&self, width: u32, f: impl FnOnce(&SideLayout) -> R) -> R {
        let mut cache = self.side_layout.borrow_mut();
        if cache.as_ref().is_none_or(|layout| layout.width != width) {
            let model = self.model();
            let wrap = Wrap::new(WrapMode::Soft, width as usize, model.tab_stop, false);
            let mut lines = Vec::with_capacity(self.side_rows.len());
            let mut offsets = vec![0usize];
            for row in &self.side_rows {
                let pair = match row {
                    SideRow::Pair { old, new, .. } => (
                        old.map(|line| layout_line(model.diff.old_line(line), wrap)),
                        new.map(|line| layout_line(model.diff.new_line(line), wrap)),
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
}

/// Where the sections lie in the display, for one view width.
struct Stack {
    /// The view width the rows were counted for.
    width: u32,
    /// Each section's first display row, followed by the total height.
    starts: Vec<usize>,
}

impl Stack {
    /// Return the sections that show rows in `rows`.
    fn visible(&self, rows: &Range<usize>) -> Range<usize> {
        let count = self.starts.len() - 1;
        let first = self
            .starts
            .partition_point(|&start| start <= rows.start)
            .saturating_sub(1)
            .min(count);
        let last = self.starts[..count].partition_point(|&start| start < rows.end);
        first..last.max(first)
    }
}

/// A line diff with a display layout, a scope, and optional highlighting, or
/// several such diffs in sections.
pub struct DiffView {
    /// What the view shows, in display order. A view of one diff holds one
    /// untitled section.
    sections: Vec<DiffSection>,
    /// How the two sides are laid out.
    layout: DiffLayout,
    /// Where the sections lie, rebuilt when the view width or a section
    /// changes.
    stack: RefCell<Option<Stack>>,
    /// Whether the view waits for a model, and since when.
    loading: SpinnerRun,
    /// Wake handle that starts the polls of a wait, after the view mounts.
    wake: Option<NodeWakeHandle>,
    /// Index into the change heads that the last change move reached, or
    /// `None` before the first move.
    change: Option<usize>,
    /// Changes whenever the displayed text changes.
    revision: u64,
    /// Matches in display order, indexed by source line for painting.
    matches: Vec<SearchMatch>,
    /// Match indices for each source line of each section, including both
    /// halves of context rows.
    match_lines: BTreeMap<(usize, Side, usize), Vec<usize>>,
    /// Match index selected for navigation and stronger highlighting.
    current_match: Option<usize>,
}

#[derive_commands]
impl DiffView {
    /// Show `model` in unified layout.
    ///
    /// Use [`Self::with_layout`] to change the layout, and install
    /// highlighters afterwards.
    #[must_use]
    pub fn new(model: DiffModel) -> Self {
        Self {
            sections: vec![DiffSection::new(model)],
            layout: DiffLayout::Unified,
            stack: RefCell::new(None),
            loading: SpinnerRun::Idle,
            wake: None,
            change: None,
            revision: 0,
            matches: Vec::new(),
            match_lines: BTreeMap::new(),
            current_match: None,
        }
    }

    /// Show a message in place of any diff, such as before one is chosen.
    ///
    /// The view drops its model, as [`Self::set_message`] does, but unlike
    /// it keeps the highlighters installed so far.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        let (old, new) = self
            .sections
            .drain(..)
            .next()
            .map_or((None, None), |first| {
                (first.old_highlighter, first.new_highlighter)
            });
        self.show(vec![
            DiffSection::message(message).with_highlighters(old, new),
        ]);
        self
    }

    /// Replace the diff with an already computed model, keeping the layout.
    ///
    /// This clears any message and loading state, drops the highlighters,
    /// and restarts change movement from the rows in view.
    pub fn set_model(&mut self, model: DiffModel) {
        self.set_sections(vec![DiffSection::new(model)]);
    }

    /// Show `sections` one after the other, in place of what the view shows,
    /// keeping the layout.
    ///
    /// This clears the loading state and restarts change movement from the
    /// rows in view. Search, change movement, and scrolling take the
    /// sections as one text.
    pub fn set_sections(&mut self, sections: Vec<DiffSection>) {
        self.show(sections);
        self.revision = self.revision.wrapping_add(1);
    }

    /// Replace the section at `index`, such as when its diff arrives, and
    /// return whether the view holds such a section. The other sections keep
    /// their state, so the view prepares no highlighter again.
    pub fn set_section(&mut self, index: usize, mut section: DiffSection) -> bool {
        let Some(slot) = self.sections.get_mut(index) else {
            return false;
        };
        section.pair_rows(self.layout);
        *slot = section;
        self.reset();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Return the canvas row each section starts at, in a view `width`
    /// columns wide: the row of its title, or its first row without one.
    #[must_use]
    pub fn section_starts(&self, width: u32) -> Vec<u32> {
        self.with_stack(width, |stack| {
            (0..self.sections.len())
                .map(|index| column(stack.starts[index] + self.lead(index).0))
                .collect()
        })
    }

    /// Replace the sections, and reset what the old ones derived.
    fn show(&mut self, mut sections: Vec<DiffSection>) {
        for section in &mut sections {
            section.pair_rows(self.layout);
        }
        self.sections = sections;
        self.reset();
    }

    /// Reset what the view derives from its sections: where they lie, the
    /// change position, the matches, and the loading state.
    fn reset(&mut self) {
        self.stack.get_mut().take();
        self.loading.set(false);
        self.change = None;
        self.clear_matches();
    }

    /// Return the model of the first section, or a model with no lines for
    /// a message or a view without sections.
    fn model(&self) -> &DiffModel {
        self.sections
            .first()
            .map_or(&EMPTY_MODEL, DiffSection::model)
    }

    /// Replace the highlighters for the old and new sides of the first
    /// section.
    pub fn set_highlighters(
        &mut self,
        old: Option<Box<dyn Highlighter>>,
        new: Option<Box<dyn Highlighter>>,
    ) {
        if let Some(first) = self.sections.first_mut() {
            first.set_highlighters(old, new);
        }
    }

    /// Show `message` in place of the diff, such as for a binary file.
    ///
    /// This drops the model and the highlighters, and clears the loading
    /// state.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.set_sections(vec![DiffSection::message(message)]);
    }

    /// Note that a model is computing. After a short delay the view shows a
    /// spinner until [`Self::set_model`] or [`Self::set_message`] arrives.
    ///
    /// The view repaints itself to turn the spinner while motion is active.
    /// At rest, as in headless runs, the spinner shows its first frame.
    pub fn set_loading(&mut self) {
        if self.loading.set(true)
            && let Some(wake) = &self.wake
        {
            // An expired handle means that the view left the tree, and its
            // next mount takes a new one.
            let _outcome = wake.wake();
        }
    }

    /// Return the message of a view that shows one in place of a diff: the
    /// message of its first section.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match &self.sections.first()?.content {
            Content::Diff(_) | Content::Text(_) => None,
            Content::Message(message) => Some(message),
        }
    }

    /// Return the revision of the diff text and its display layout.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the rows above the body of the section at `index`: the blank
    /// row that sets a titled section apart from the one before, and its
    /// title.
    fn lead(&self, index: usize) -> (usize, usize) {
        match self.sections[index].title {
            Some(_) => (usize::from(index > 0), 1),
            None => (0, 0),
        }
    }

    /// Return the displayed source text in unified row order.
    ///
    /// Hidden context is omitted. Headers, gaps, and the rows above a
    /// section's body contribute empty lines, and a message or text
    /// contributes none. In a view of one diff, each text line then has the
    /// same index as its unified display row.
    #[must_use]
    pub fn search_text(&self) -> String {
        let mut lines = Vec::new();
        for (index, section) in self.sections.iter().enumerate() {
            let (gap, title) = self.lead(index);
            lines.extend(iter::repeat_n("", gap + title));
            let model = section.model();
            lines.extend(model.rows.iter().map(|row| match row {
                DiffRow::Unchanged { old, .. } | DiffRow::Removed { old } => {
                    model.diff.old_line(*old)
                }
                DiffRow::Added { new } => model.diff.new_line(*new),
                _ => "",
            }));
        }
        lines.join("\n")
    }

    /// Return the section and the row of its body that line `line` of
    /// [`Self::search_text`] reads.
    fn search_row(&self, line: usize) -> Option<(usize, usize)> {
        let mut start = 0usize;
        for (index, section) in self.sections.iter().enumerate() {
            let (gap, title) = self.lead(index);
            let body = start + gap + title;
            let end = body + section.model().rows.len();
            if line < end {
                return line.checked_sub(body).map(|row| (index, row));
            }
            start = end;
        }
        None
    }

    /// Highlight matches over [`Self::search_text`] and reveal the first at
    /// or below the top of the view. Ranges use character columns and must
    /// stay within one source line. Side-by-side layout maps them to its rows.
    pub fn set_matches(&mut self, ctx: &mut dyn Context, ranges: Vec<TextRange>) {
        self.clear_matches();
        // The paired row of each source line, for each section with a match.
        let mut paired: HashMap<usize, BTreeMap<(Side, usize), usize>> = HashMap::new();
        for range in ranges {
            if range.start.line != range.end.line || range.start.column >= range.end.column {
                continue;
            }
            let Some((section, body_row)) = self.search_row(range.start.line) else {
                continue;
            };
            let held = &self.sections[section];
            let source = match held.model().rows.get(body_row) {
                Some(DiffRow::Unchanged { old, .. } | DiffRow::Removed { old }) => {
                    (Side::Old, *old)
                }
                Some(DiffRow::Added { new }) => (Side::New, *new),
                _ => continue,
            };
            let row = if self.layout == DiffLayout::SideBySide {
                let rows = paired
                    .entry(section)
                    .or_insert_with(|| source_rows(&held.side_rows));
                rows.get(&source).copied().unwrap_or(body_row)
            } else {
                body_row
            };
            self.matches.push(SearchMatch {
                section,
                side: source.0,
                line: source.1,
                columns: range.start.column..range.end.column,
                row,
            });
        }
        let view = ctx.view().view_rect();
        let mut matches = mem::take(&mut self.matches);
        matches.sort_by_cached_key(|found| {
            (
                self.match_area(found, view.w).tl.y,
                found.side,
                found.columns.start,
            )
        });
        self.matches = matches;
        for (index, found) in self.matches.iter().enumerate() {
            self.match_lines
                .entry((found.section, found.side, found.line))
                .or_default()
                .push(index);
            // Context is drawn on both sides but counted once.
            if self.layout == DiffLayout::SideBySide
                && found.side == Side::Old
                && let Some(SideRow::Pair {
                    new: Some(new),
                    changed: false,
                    ..
                }) = self.sections[found.section].side_rows.get(found.row)
            {
                self.match_lines
                    .entry((found.section, Side::New, *new))
                    .or_default()
                    .push(index);
            }
        }
        self.current_match = view_search::first_at_or_below(
            self.matches
                .iter()
                .map(|found| self.match_area(found, view.w).tl.y),
            view.tl.y,
        );
        self.reveal_match(ctx);
    }

    /// Move between matches, wrapping at either end, and reveal the match.
    pub fn search_next(&mut self, ctx: &mut dyn Context, delta: i32) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match = view_search::step(self.current_match, self.matches.len(), delta);
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

    /// Reveal the current match near the middle of the view.
    fn reveal_match(&self, ctx: &mut dyn Context) {
        let Some(found) = self.current_match.and_then(|index| self.matches.get(index)) else {
            return;
        };
        ctx.reveal_area(
            self.match_area(found, ctx.view().view_rect().w),
            view_search::REVEAL,
        );
    }

    /// Map a source match through the current wrapping, including after resize.
    fn match_area(&self, found: &SearchMatch, width: u32) -> Rect {
        let geometry = self.geometry(width);
        let section = &self.sections[found.section];
        let body = self.body_start(found.section, width);
        let model = section.model();
        let source = found.side.line(&model.diff, found.line);
        let prefix = source.chars().take(found.columns.start).collect::<String>();
        let start = text::width(&text::expand_tabs(&prefix, model.tab_stop));
        let through = source.chars().take(found.columns.end).collect::<String>();
        let end = text::width(&text::expand_tabs(&through, model.tab_stop));
        if self.layout == DiffLayout::Unified {
            return Rect::new(
                geometry.gutter + start,
                column(body + found.row),
                end.saturating_sub(start).max(1),
                1,
            );
        }
        section.with_side_layout(geometry.pane_code_width(), |layout| {
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
                column(body + layout.offsets[found.row] + index),
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
        self.step_change(c, 1);
    }

    /// Scroll to the previous change.
    #[command]
    pub fn prev_change(&mut self, c: &mut dyn Context) {
        self.step_change(c, -1);
    }

    /// Scroll `step` changes from the current one, stopping at either end.
    fn step_change(&mut self, c: &mut dyn Context, step: isize) {
        let view_rect = c.view().view_rect();
        let heads = self.change_heads(view_rect.w);
        if heads.is_empty() {
            return;
        }
        // Before the first move, the current change is the last one that
        // starts above the bottom of the view: a change in view counts as
        // seen, and a change below it is the first to visit.
        let bottom = view_rect.tl.y.saturating_add(view_rect.h) as usize;
        let current = self
            .change
            .or_else(|| heads.iter().rposition(|&head| head < bottom));
        let change = current.map_or(0, |index| {
            index.saturating_add_signed(step).min(heads.len() - 1)
        });
        self.change = Some(change);
        c.scroll(ScrollOp::To(Point {
            x: 0,
            y: column(heads[change]),
        }));
    }

    /// Change the display layout without replacing the model or highlighters.
    ///
    /// Search ranges need to be applied again because their display rows
    /// changed. The revision advances so a host can refresh its held query.
    /// Messages and loading state remain in place.
    pub fn set_layout(&mut self, layout: DiffLayout) {
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        self.pair_rows();
        self.stack.get_mut().take();
        self.change = None;
        self.revision = self.revision.wrapping_add(1);
        self.clear_matches();
    }

    /// Return the current display layout.
    #[must_use]
    pub fn layout(&self) -> DiffLayout {
        self.layout
    }

    /// Set the display layout while constructing a view.
    #[must_use]
    pub fn with_layout(mut self, layout: DiffLayout) -> Self {
        self.layout = layout;
        self.pair_rows();
        self.stack.get_mut().take();
        self
    }

    /// Set the amount of unchanged text the rows show. On a view that shows
    /// a message, which has no model, this does nothing.
    #[must_use]
    pub fn with_scope(mut self, scope: Scope) -> Self {
        for section in &mut self.sections {
            if let Content::Diff(model) = &mut section.content {
                model.scope = scope;
            }
        }
        self.rebuild_rows();
        self
    }

    /// Set the tab stop width in columns. On a view that shows a message,
    /// which has no model, this does nothing.
    #[must_use]
    pub fn with_tab_stop(mut self, tab_stop: usize) -> Self {
        for section in &mut self.sections {
            if let Content::Diff(model) = &mut section.content {
                model.tab_stop = tab_stop.max(1);
                model.old_width = widest_line(&model.diff, Side::Old, model.tab_stop);
                model.new_width = widest_line(&model.diff, Side::New, model.tab_stop);
            }
        }
        self.rebuild_rows();
        self
    }

    /// Install a highlighter for the old side of the first section.
    #[must_use]
    pub fn with_old_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        if let Some(first) = self.sections.first_mut() {
            first.old_highlighter = Some(highlighter);
            first.prepared = false;
        }
        self
    }

    /// Install a highlighter for the new side of the first section.
    #[must_use]
    pub fn with_new_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        if let Some(first) = self.sections.first_mut() {
            first.new_highlighter = Some(highlighter);
            first.prepared = false;
        }
        self
    }

    /// Return the diff being shown: the first section's.
    #[must_use]
    pub fn diff(&self) -> &Diff {
        &self.model().diff
    }

    /// Return the first section's rows for the current scope.
    #[must_use]
    pub fn rows(&self) -> &[DiffRow] {
        &self.model().rows
    }

    /// Return the display scope.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.model().scope
    }

    /// Pair the rows of each section for side-by-side layout, when that
    /// layout needs it.
    fn pair_rows(&mut self) {
        for section in &mut self.sections {
            section.pair_rows(self.layout);
        }
    }

    /// Return every change head in the active layout's display coordinates.
    ///
    /// A titled section stops the movement at its title when its first
    /// change opens its body, or when it has no change to stop at, as a
    /// message has none. A step through the changes then names each
    /// section it enters.
    fn change_heads(&self, width: u32) -> Vec<usize> {
        let geometry = self.geometry(width);
        let mut heads = Vec::new();
        for (index, section) in self.sections.iter().enumerate() {
            let body = self.body_start(index, width);
            let titled = section.title.is_some();
            let local = section.change_heads(&geometry);
            if titled && local.first().is_none_or(|&first| first == 0) {
                heads.push(body - 1);
            }
            heads.extend(
                local
                    .into_iter()
                    .filter(|&head| !(titled && head == 0))
                    .map(|head| body + head),
            );
        }
        heads
    }

    /// Rebuild the rows of each section for its scope.
    fn rebuild_rows(&mut self) {
        for section in &mut self.sections {
            if let Content::Diff(model) = &mut section.content {
                model.rows = model.diff.rows(model.scope).into();
            }
            section.reset_rows(self.layout);
        }
        self.stack.get_mut().take();
        self.revision = self.revision.wrapping_add(1);
        self.clear_matches();
    }

    /// Return the digits of the widest line number, and the width of the
    /// widest line with the cell after it, over every section.
    fn extents(&self) -> (u32, u32) {
        let models = || self.sections.iter().map(DiffSection::model);
        let lines = models()
            .map(|model| model.diff.old_len().max(model.diff.new_len()))
            .max()
            .unwrap_or(0);
        let widest = models()
            .map(|model| model.old_width.max(model.new_width))
            .max()
            .unwrap_or(0);
        (digits(lines), widest + 1)
    }

    /// Return the column layout for a view of `view_w` columns.
    fn geometry(&self, view_w: u32) -> Geometry {
        let (digits, line_width) = self.extents();
        let gutter = digits + 2;
        match self.layout {
            DiffLayout::Unified => Geometry {
                layout: self.layout,
                view: view_w,
                digits,
                gutter,
                half: 0,
                width: (gutter + line_width).max(view_w),
            },
            DiffLayout::SideBySide => {
                let half = view_w.saturating_sub(1) / 2;
                Geometry {
                    layout: self.layout,
                    view: view_w,
                    digits,
                    gutter: gutter.min(half),
                    half,
                    width: view_w,
                }
            }
        }
    }

    /// Access where the sections lie in a view `width` columns wide.
    fn with_stack<R>(&self, width: u32, f: impl FnOnce(&Stack) -> R) -> R {
        let mut cache = self.stack.borrow_mut();
        if cache.as_ref().is_none_or(|stack| stack.width != width) {
            let geometry = self.geometry(width);
            let mut starts = Vec::with_capacity(self.sections.len() + 1);
            let mut row = 0usize;
            for (index, section) in self.sections.iter().enumerate() {
                starts.push(row);
                let (gap, title) = self.lead(index);
                row = row
                    .saturating_add(gap + title)
                    .saturating_add(section.body_rows(&geometry));
            }
            starts.push(row);
            *cache = Some(Stack { width, starts });
        }
        f(cache.as_ref().expect("stack initialized"))
    }

    /// Return the display row of the first body row of the section at
    /// `index`.
    fn body_start(&self, index: usize, width: u32) -> usize {
        let (gap, title) = self.lead(index);
        self.with_stack(width, |stack| stack.starts[index]) + gap + title
    }

    /// Return the number of display rows for the current layout.
    fn display_rows(&self, width: u32) -> usize {
        self.with_stack(width, |stack| {
            *stack.starts.last().expect("a stack has a total height")
        })
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

    /// Draw code scrolled horizontally or from a wrapped segment.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_code(
        &self,
        rndr: &mut Render,
        section: usize,
        view_rect: Rect,
        origin: Point,
        x: u32,
        width: u32,
        style: &str,
        side: Side,
        line: usize,
        y: u32,
        segment: Option<&WrapSegment>,
    ) -> Result<()> {
        let scroll = view_rect.tl.x;
        let base = rndr.resolve_style(style);
        let held = &self.sections[section];
        let model = held.model();
        let text = side.line(&model.diff, line);
        let spans = held.highlight(side, line, text);
        let matches = self.match_lines.get(&(section, side, line));
        let search_current = rndr.resolve_style(view_search::CURRENT);
        let search_match = rndr.resolve_style(view_search::MATCH);
        let mut span_walk = SpanWalk::new(&spans);
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
            model.tab_stop,
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
                span_walk.style(render, &base, g_start, g_end)
            },
        )
    }

    /// Draw the rows of one section that lie in `rows`, a range of display
    /// rows in view. `start` is the section's first display row.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_section(
        &self,
        rndr: &mut Render,
        geom: &Geometry,
        view_rect: Rect,
        origin: Point,
        index: usize,
        start: usize,
        rows: &Range<usize>,
    ) -> Result<()> {
        let section = &self.sections[index];
        let (gap, title) = self.lead(index);
        let y_of = |row: usize| origin.y + column(row - rows.start);
        let title_row = start + gap;
        if let Some(text) = &section.title
            && rows.contains(&title_row)
        {
            // The title keeps its place while the code scrolls sideways.
            let y = y_of(title_row);
            rndr.fill("title", Rect::new(origin.x, y, view_rect.w, 1), ' ')?;
            rndr.text(
                "title",
                Line::new(origin.x + 1, y, view_rect.w.saturating_sub(1)),
                text,
            )?;
        }
        let body = title_row + title;
        let height = section.body_rows(geom);
        let first = rows.start.max(body);
        let last = rows.end.min(body + height);
        if first >= last {
            return Ok(());
        }
        let local = first - body..last - body;
        let y = y_of(first);
        if let Some((style, lines)) = section.content.prose(view_rect.w) {
            for (offset, line) in (0..).zip(&lines[local]) {
                rndr.text(style, Line::new(origin.x, y + offset, view_rect.w), line)?;
            }
            return Ok(());
        }
        match self.layout {
            DiffLayout::Unified => {
                self.draw_unified(rndr, geom, view_rect, origin, index, local, y)
            }
            DiffLayout::SideBySide => {
                self.draw_side_by_side(rndr, geom, view_rect, origin, index, local, y)
            }
        }
    }

    /// Draw the rows `rows` of a section's unified body from screen row `y`.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_unified(
        &self,
        rndr: &mut Render,
        geom: &Geometry,
        view_rect: Rect,
        origin: Point,
        section: usize,
        rows: Range<usize>,
        y: u32,
    ) -> Result<()> {
        let model = self.sections[section].model();
        for (offset, index) in (0..).zip(rows) {
            let y = y + offset;
            let (marker, style, side, line) = match &model.rows[index] {
                DiffRow::Unchanged { old, .. } => (' ', "context", Side::Old, *old),
                DiffRow::Removed { old } => ('-', "removed", Side::Old, *old),
                DiffRow::Added { new } => ('+', "added", Side::New, *new),
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
                    continue;
                }
                DiffRow::Header { old, new } => {
                    rndr.fill("header", Rect::new(origin.x, y, geom.width, 1), ' ')?;
                    rndr.text(
                        "header",
                        Line::new(origin.x + geom.gutter, y, geom.width - geom.gutter),
                        &header_text(old, new),
                    )?;
                    continue;
                }
            };
            Self::draw_number(
                rndr,
                origin.x,
                y,
                geom.gutter,
                marker,
                Some(line),
                geom.digits,
                style,
            )?;
            self.draw_code(
                rndr,
                section,
                view_rect,
                origin,
                origin.x + geom.gutter,
                geom.width - geom.gutter,
                style,
                side,
                line,
                y,
                None,
            )?;
        }
        Ok(())
    }

    /// Draw the display rows `rows` of a section's side-by-side body from
    /// screen row `y`.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_side_by_side(
        &self,
        rndr: &mut Render,
        geom: &Geometry,
        view_rect: Rect,
        origin: Point,
        section: usize,
        rows: Range<usize>,
        y: u32,
    ) -> Result<()> {
        let held = &self.sections[section];
        let left = origin.x;
        let right = origin.x + geom.width - geom.half;
        let first = rows.start;
        held.with_side_layout(geom.pane_code_width(), |layout| -> Result<()> {
            let mut index = layout
                .offsets
                .partition_point(|&row| row <= first)
                .saturating_sub(1);
            for display in rows {
                while layout.offsets[index + 1] <= display {
                    index += 1;
                }
                let continuation = display - layout.offsets[index];
                let pair = &layout.lines[index];
                let y = y + column(display - first);
                match &held.side_rows[index] {
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
                            section,
                            view_rect,
                            origin,
                            geom,
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
                            section,
                            view_rect,
                            origin,
                            geom,
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
        })
    }

    /// Draw one half of a side-by-side pair row.
    #[expect(clippy::too_many_arguments, reason = "one render step")]
    fn draw_pair_side(
        &self,
        rndr: &mut Render,
        section: usize,
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
        self.draw_code(
            rndr,
            section,
            view_rect,
            origin,
            x + geom.gutter,
            geom.pane_code_width(),
            style,
            side,
            line,
            y,
            Some(segment),
        )
    }

    /// Draw the divider between the panes of each side-by-side diff in
    /// `visible`, over the rows of its body in view. Below the last diff of
    /// the view, the divider runs on to the end of the view. A title, a
    /// message, and text span the whole view, so none of them has one.
    fn draw_dividers(
        &self,
        rndr: &mut Render,
        geom: &Geometry,
        view_rect: Rect,
        origin: Point,
        visible: Range<usize>,
        starts: &[usize],
    ) -> Result<()> {
        let top = view_rect.tl.y as usize;
        let bottom = top.saturating_add(view_rect.h as usize);
        for index in visible {
            if !matches!(self.sections[index].content, Content::Diff(_)) {
                continue;
            }
            let (gap, title) = self.lead(index);
            let first = (starts[index] + gap + title).max(top);
            let last = if index + 1 == self.sections.len() {
                bottom
            } else {
                starts[index + 1].min(bottom)
            };
            if first >= last {
                continue;
            }
            let y = origin.y + column(first - top);
            let height = column(last - first);
            // Even viewport widths leave two central cells so the panes
            // remain exactly equal. Only one carries the divider glyph.
            rndr.fill(
                "separator",
                Rect::new(origin.x + geom.half, y, view_rect.w - geom.half * 2, height),
                ' ',
            )?;
            rndr.fill(
                "separator",
                Rect::new(origin.x + geom.half, y, 1, height),
                '│',
            )?;
        }
        Ok(())
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
        let view_rect = view.view_rect();
        let width = view_rect.w;
        let geom = self.geometry(width);
        let top = view_rect.tl.y as usize;
        let rows = top..top.saturating_add(view_rect.h as usize);
        let (visible, starts) =
            self.with_stack(width, |stack| (stack.visible(&rows), stack.starts.clone()));
        // Only a section in view parses its texts.
        for section in &mut self.sections[visible.clone()] {
            section.prepare_highlighters();
        }
        for index in visible.clone() {
            self.draw_section(rndr, &geom, view_rect, origin, index, starts[index], &rows)?;
        }
        if self.layout == DiffLayout::SideBySide && width > 0 {
            self.draw_dividers(rndr, &geom, view_rect, origin, visible, &starts)?;
        }
        // A model that takes a while to compute shows a spinner in the top
        // right corner, over whatever the view showed before. A view at rest
        // dates the wait when it paints, as no poll does.
        self.loading.turn(ctx.now());
        if let SpinnerRun::Running { since: Some(since) } = self.loading
            && let waited = ctx.now().saturating_duration_since(since)
            && waited >= LOADING_DELAY
        {
            let frame = if ctx.motion_active() {
                Spinner::LINE.frame(waited)
            } else {
                Spinner::LINE.step(0)
            };
            let label = format!("{frame} diffing");
            let label_width = text::width(&label).min(width);
            let x = origin.x + width.saturating_sub(label_width);
            rndr.text("loading", Line::new(x, origin.y, label_width), &label)?;
        }
        Ok(())
    }

    fn scroll_marks(&self, axis: ScrollAxis, content: Size) -> Vec<ScrollMark> {
        if axis != ScrollAxis::Vertical {
            return Vec::new();
        }
        // Matches sort by the row they wrap onto, so their rows ascend.
        view_search::marks(
            self.matches
                .iter()
                .map(|found| self.match_area(found, content.w).tl.y),
        )
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        // A waiting view repaints to turn its spinner, while motion shows it.
        if !ctx.motion_active() || !self.loading.turn(ctx.now()) {
            return Ok(None);
        }
        Ok(Some(Spinner::LINE.period()))
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.wake = Some(ctx.wake_handle(PollLifetime::Node)?);
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = c.width.limit().unwrap_or_else(|| {
            let (digits, line_width) = self.extents();
            let half = digits + 2 + line_width;
            if self.layout == DiffLayout::Unified {
                half
            } else {
                half * 2 + 1
            }
        });
        let height = c
            .height
            .limit()
            .unwrap_or_else(|| column(self.display_rows(width)));
        c.clamp(Size::new(width.max(1), height.max(1)))
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let geom = self.geometry(view.w);
        let rows = self.display_rows(view.w);
        Size::new(geom.width, column(rows))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("diff_view")
    }
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
        .map(|line| text::width(&text::expand_tabs(side.line(diff, line), tab_stop)))
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

/// Wrap a message at word boundaries, so a narrow pane shows all of it
/// rather than a clipped first line.
fn wrap_message(message: &str, width: u32) -> Vec<String> {
    text::wrap_with(message, width, text::WrapOptions::PLAIN)
}

/// Return the paired row that shows each source line.
fn source_rows(rows: &[SideRow]) -> BTreeMap<(Side, usize), usize> {
    let mut sources = BTreeMap::new();
    for (row, pair) in rows.iter().enumerate() {
        if let SideRow::Pair { old, new, .. } = pair {
            if let Some(line) = old {
                sources.insert((Side::Old, *line), row);
            }
            if let Some(line) = new {
                sources.insert((Side::New, *line), row);
            }
        }
    }
    sources
}

#[cfg(test)]
mod tests {
    use std::{ptr, sync::Arc, time::Duration};

    use canopy::{
        Widget,
        error::Result,
        geom::{Point, Size},
        testing::{ManualClock, harness::Harness},
    };

    use super::{
        DiffLayout, DiffModel, DiffRow, DiffSection, DiffView, LOADING_DELAY, Scope, SideRow,
        header_text, side_rows,
    };
    use crate::{
        Spinner,
        editor::{TextPosition, TextRange},
        view_search,
    };

    /// Return the spinner frame of the loading label, if it shows.
    fn loading_frame(harness: &Harness) -> Option<char> {
        let top = harness.tbuf().lines()[0].trim_end().to_string();
        top.strip_suffix(" diffing")?.chars().last()
    }

    /// Build a view of a small diff on `clock`, with motion or at rest.
    fn loading_harness(clock: &Arc<ManualClock>, motion: bool) -> Result<Harness> {
        let model = DiffModel::from_texts("old\n", "new\n");
        let mut harness = Harness::builder(DiffView::new(model))
            .size(30, 3)
            .clock(Arc::clone(clock))
            .motion(motion)
            .build()?;
        harness.render()?;
        Ok(harness)
    }

    #[test]
    fn with_motion_the_loading_spinner_turns_on_the_driver_clock() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = loading_harness(&clock, true)?;
        harness.with_root_widget(DiffView::set_loading);
        harness.render()?;
        assert_eq!(loading_frame(&harness), None, "a short wait shows nothing");
        // The view polls itself: no host ticks it.
        harness.wait_until(Duration::from_secs(10), |harness| {
            clock.advance(Spinner::LINE.period())?;
            Ok(loading_frame(harness) == Some(Spinner::LINE.step(3)))
        })?;
        harness.with_root_widget(|view: &mut DiffView| {
            view.set_model(DiffModel::from_texts("a\n", "b\n"));
        });
        harness.render()?;
        assert_eq!(loading_frame(&harness), None, "a model ends the wait");
        Ok(())
    }

    #[test]
    fn at_rest_the_loading_spinner_shows_its_first_frame_and_never_polls() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = loading_harness(&clock, false)?;
        harness.with_root_widget(DiffView::set_loading);
        harness.render()?;
        let poll = harness.with_root_widget_context(|view: &mut DiffView, ctx| view.poll(ctx))?;
        assert_eq!(poll, None, "a view at rest schedules no repaint");
        assert_eq!(loading_frame(&harness), None);
        clock.advance(LOADING_DELAY * 3)?;
        harness.render()?;
        assert_eq!(loading_frame(&harness), Some(Spinner::LINE.step(0)));
        Ok(())
    }

    #[test]
    fn paired_panes_wrap_equally_and_keep_the_divider_through_empty_rows() -> Result<()> {
        for width in [31, 32] {
            let view = DiffView::new(DiffModel::from_texts(
                "abcdefghijklmnopqrstuvwx\nshared\n",
                "ABCDEFGHIJKLMNOPQRSTUVWXtail\nshared\n",
            ))
            .with_layout(DiffLayout::SideBySide);
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
        let view =
            DiffView::new(DiffModel::from_texts(old, new)).with_layout(DiffLayout::SideBySide);
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
        let current = harness.canopy.style().resolve(view_search::CURRENT);
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
        let view = DiffView::new(DiffModel::from_texts(source, source))
            .with_layout(DiffLayout::SideBySide);
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
            view.set_layout(DiffLayout::Unified);
            assert_eq!(view.revision(), 0, "the current layout is a no-op");
            assert_eq!((view.search_matches(), view.search_position()), (2, 2));

            view.set_layout(DiffLayout::SideBySide);
            assert_eq!(view.layout(), DiffLayout::SideBySide);
            assert!(ptr::eq(view.diff(), model.diff()));
            assert!(ptr::eq(view.rows(), model.rows.as_ref()));
            assert!(view.loading.running());
            assert_eq!(view.revision(), 1);
            assert_eq!(view.search_matches(), 0, "the host refreshes its query");
            view.set_matches(ctx, ranges);
            assert_eq!((view.search_matches(), view.search_position()), (2, 1));
            assert!(view.matches.iter().all(|found| found.row == 0));

            view.set_layout(DiffLayout::Unified);
            assert_eq!(view.revision(), 2);
            assert!(ptr::eq(view.diff(), model.diff()));
            view.set_model(model.clone());
            assert_eq!(view.layout(), DiffLayout::Unified);
            view.set_message("binary file");
            view.set_layout(DiffLayout::SideBySide);
            assert_eq!(view.message(), Some("binary file"));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn regression_a_built_message_hides_the_model() -> Result<()> {
        let same = "same\n".repeat(20);
        let old = format!("{same}old\n{same}");
        let new = format!("{same}new\n{same}");
        let model = || DiffModel::from_texts(old.clone(), new.clone());
        let canvas = |view: DiffView| -> Result<(Size, u32)> {
            let mut harness = Harness::builder(view).size(20, 5).build()?;
            harness.render()?;
            harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                view.next_change(ctx);
                view.next_change(ctx);
                Ok(())
            })?;
            harness.render()?;
            let root = harness.root;
            let view = harness
                .canopy
                .with_root_view(|ctx| ctx.view_of(root).expect("view"));
            Ok((view.canvas, view.scroll.y))
        };
        let mut set = DiffView::new(model());
        set.set_message("Choose a file");
        let built = DiffView::new(model()).with_message("Choose a file");
        assert!(built.rows().is_empty(), "the message hides the model");
        assert_eq!(built.search_text(), "");
        let (built, scroll) = canvas(built)?;
        assert_eq!(built, canvas(set)?.0, "the canvas of the message");
        assert_eq!(scroll, 0, "a message has no changes to step through");
        Ok(())
    }

    #[test]
    fn a_message_wraps_to_the_view_width() -> Result<()> {
        let mut view = DiffView::new(DiffModel::from_texts("a\n", "b\n"));
        view.set_message("Unresolved conflict: deleted by them. The worktree matches HEAD.");
        let mut harness = Harness::builder(view).size(25, 6).build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        assert_eq!(
            lines.iter().map(|line| line.trim_end()).collect::<Vec<_>>(),
            [
                "Unresolved conflict:",
                "deleted by them. The",
                "worktree matches HEAD.",
                "",
                "",
                ""
            ]
        );
        Ok(())
    }

    #[test]
    fn a_side_by_side_message_has_no_divider() -> Result<()> {
        let view = DiffView::new(DiffModel::from_texts("a\n", "b\n"))
            .with_layout(DiffLayout::SideBySide)
            .with_message("Unresolved conflict: deleted by them.");
        let mut harness = Harness::builder(view).size(25, 3).build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        assert_eq!(
            lines.iter().map(|line| line.trim_end()).collect::<Vec<_>>(),
            ["Unresolved conflict:", "deleted by them.", ""]
        );
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
                (DiffLayout::Unified, unified_head),
                (DiffLayout::SideBySide, paired_head),
            ] {
                let view = DiffView::new(DiffModel::from_texts(&old, &new))
                    .with_scope(scope)
                    .with_layout(mode);
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
    fn the_first_next_change_goes_to_a_first_change_below_the_view() -> Result<()> {
        let shared = (0..10)
            .map(|index| format!("shared {index}\n"))
            .collect::<String>();
        let old = format!("{shared}old one\n{shared}old two\n{shared}");
        let new = format!("{shared}new one\n{shared}new two\n{shared}");
        let view = DiffView::new(DiffModel::from_texts(&old, &new));
        let mut harness = Harness::builder(view).size(80, 4).build()?;
        harness.render()?;
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            assert_eq!(view.change_heads(80), [10, 22]);
            view.next_change(ctx);
            Ok(())
        })?;
        harness.render()?;
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            assert_eq!(ctx.view().scroll.y, 10, "the first change");
            view.next_change(ctx);
            Ok(())
        })?;
        harness.render()?;
        harness.with_root_widget_context(|_: &mut DiffView, ctx| {
            assert_eq!(ctx.view().scroll.y, 22, "the second change");
            Ok(())
        })?;
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
        let other = DiffView::new(model.clone()).with_layout(DiffLayout::SideBySide);
        assert!(ptr::eq(view.diff(), other.diff()));
        assert!(ptr::eq(view.rows(), other.rows()));
        let original_rows = other.rows().to_vec();
        let original_widths = (other.model().old_width, other.model().new_width);

        view = view.with_scope(Scope::Context(0));
        assert!(ptr::eq(view.diff(), model.diff()));
        assert!(!ptr::eq(view.rows(), other.rows()));
        assert!(!view.search_text().contains("shared"));
        assert_eq!(other.rows(), original_rows);
        assert_eq!(other.scope(), Scope::WholeFile);
        assert_eq!(other.revision(), 0);

        let scoped_rows = view.model().rows.clone();
        view = view.with_tab_stop(16);
        assert!(!ptr::eq(view.rows(), scoped_rows.as_ref()));
        assert_eq!(view.rows(), scoped_rows.as_ref());
        assert_ne!(
            (view.model().old_width, view.model().new_width),
            original_widths
        );
        assert!(ptr::eq(view.diff(), model.diff()));
        assert_eq!(other.rows(), original_rows);
        assert_eq!(
            (other.model().old_width, other.model().new_width),
            original_widths
        );
        assert_eq!(other.model().tab_stop, super::DEFAULT_TAB_STOP);

        let original = DiffView::new(model);
        assert!(ptr::eq(original.rows(), other.rows()));
        assert_eq!(original.rows(), original_rows);
        assert_eq!(original.scope(), Scope::WholeFile);
        assert_eq!(
            (original.model().old_width, original.model().new_width),
            original_widths
        );
    }

    #[test]
    fn views_from_model_clones_keep_search_and_loading_state_independent() -> Result<()> {
        let model = DiffModel::from_texts("old needle\n", "new needle\n");
        let mut first = Harness::builder(DiffView::new(model.clone()))
            .size(50, 5)
            .build()?;
        let mut second =
            Harness::builder(DiffView::new(model.clone()).with_layout(DiffLayout::SideBySide))
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
            assert!(!view.loading.running());
            assert!(!view.sections[0].prepared);
            Ok(())
        })?;
        second.with_root_widget_context(|view: &mut DiffView, _| {
            assert!(ptr::eq(view.diff(), model.diff()));
            assert_eq!(view.layout, DiffLayout::SideBySide);
            assert!(!view.sections[0].side_rows.is_empty());
            assert_eq!(view.revision(), 0);
            assert_eq!((view.search_matches(), view.search_position()), (2, 1));
            assert!(!view.loading.running());
            assert!(view.sections[0].prepared);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn search_highlights_both_versions_and_wraps_in_each_layout() -> Result<()> {
        for mode in [DiffLayout::Unified, DiffLayout::SideBySide] {
            let view = DiffView::new(DiffModel::from_texts(
                "alpha\nold needle\nsame needle\n",
                "alpha\nnew needle\nsame needle\n",
            ))
            .with_layout(mode);
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
            let current = harness.canopy.style().resolve(view_search::CURRENT);
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
                DiffLayout::Unified => Point::new(7, 2),
                DiffLayout::SideBySide => Point::new(33, 1),
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

    /// Build a view of three titled sections: a diff, a message, and an
    /// added file.
    fn sectioned(layout: DiffLayout) -> DiffView {
        let mut view = DiffView::new(DiffModel::from_texts("", "")).with_layout(layout);
        view.set_sections(vec![
            DiffSection::new(DiffModel::from_texts(
                "alpha\nold needle\n",
                "alpha\nnew needle\n",
            ))
            .with_title("a.txt"),
            DiffSection::message("Binary file").with_title("b.bin"),
            DiffSection::new(DiffModel::from_texts("", "added needle\n")).with_title("c.txt"),
        ]);
        view
    }

    /// Return the screen rows without their trailing blanks.
    fn trimmed(harness: &Harness) -> Vec<String> {
        harness
            .tbuf()
            .lines()
            .iter()
            .map(|line| line.trim_end().to_owned())
            .collect()
    }

    #[test]
    fn a_text_section_shows_its_lines_as_written() -> Result<()> {
        let mut view = DiffView::new(DiffModel::from_texts("", ""));
        view.set_sections(vec![
            DiffSection::text("Fix the long thing\n\nA body line that wraps.\nSigned: me\n")
                .with_title("Message"),
            DiffSection::new(DiffModel::from_texts("", "added needle\n")).with_title("c.txt"),
        ]);
        assert_eq!(view.message(), None, "text is no message");
        assert_eq!(
            view.search_text(),
            "\n\n\nadded needle",
            "text has no lines to search"
        );
        let mut harness = Harness::builder(view).size(20, 10).build()?;
        harness.render()?;
        assert_eq!(
            trimmed(&harness),
            [
                " Message",
                "Fix the long thing",
                "",
                "A body line that",
                "wraps.",
                "Signed: me",
                "",
                " c.txt",
                "+1 added needle",
                ""
            ],
            "each line starts a row, and wraps to the view"
        );
        let text = harness.canopy.style().resolve("diff_view/text");
        assert_eq!(
            harness.buf().get(Point::new(0, 1)).expect("text").style.fg,
            text.fg.solid_color().expect("a solid colour"),
        );
        let message = harness.canopy.style().resolve("diff_view/message");
        assert_ne!(text.fg, message.fg, "text does not read as a message");
        harness.with_root_widget(|view: &mut DiffView| {
            assert_eq!(view.section_starts(20), [0, 7]);
        });
        Ok(())
    }

    #[test]
    fn sections_show_one_after_the_other_under_their_titles() -> Result<()> {
        let mut harness = Harness::builder(sectioned(DiffLayout::Unified))
            .size(30, 11)
            .build()?;
        harness.render()?;
        assert_eq!(
            trimmed(&harness),
            [
                " a.txt",
                " 1 alpha",
                "-2 old needle",
                "+2 new needle",
                "",
                " b.bin",
                "Binary file",
                "",
                " c.txt",
                "+1 added needle",
                ""
            ]
        );
        let title = harness.canopy.style().resolve("diff_view/title");
        for row in [0, 5, 8] {
            assert_eq!(
                harness
                    .buf()
                    .get(Point::new(29, row))
                    .expect("a title")
                    .style
                    .bg,
                title.bg.solid_color().expect("a solid band"),
                "the title of row {row} is a band across the view"
            );
        }
        harness.with_root_widget(|view: &mut DiffView| {
            assert_eq!(view.section_starts(30), [0, 5, 8]);
            assert_eq!(
                view.rows().len(),
                3,
                "the queries of one diff read the first"
            );
        });
        Ok(())
    }

    #[test]
    fn paired_sections_keep_the_divider_to_the_rows_of_their_diffs() -> Result<()> {
        let mut harness = Harness::builder(sectioned(DiffLayout::SideBySide))
            .size(31, 11)
            .build()?;
        harness.render()?;
        assert_eq!(
            trimmed(&harness),
            [
                " a.txt",
                " 1 alpha       │ 1 alpha",
                "-2 old needle  │+2 new needle",
                "",
                " b.bin",
                "Binary file",
                "",
                " c.txt",
                "               │+1 added needle",
                "               │",
                "               │"
            ]
        );
        harness.with_root_widget(|view: &mut DiffView| {
            assert_eq!(view.section_starts(31), [0, 4, 7]);
        });
        Ok(())
    }

    #[test]
    fn search_and_change_movement_take_the_sections_as_one_text() -> Result<()> {
        for (mode, heads, found) in [
            (DiffLayout::Unified, [2, 5, 8], Point::new(9, 9)),
            (DiffLayout::SideBySide, [2, 4, 7], Point::new(25, 8)),
        ] {
            let mut harness = Harness::builder(sectioned(mode)).size(31, 11).build()?;
            harness.render()?;
            harness.with_root_widget_context(|view: &mut DiffView, ctx| {
                assert_eq!(
                    view.search_text(),
                    "\nalpha\nold needle\nnew needle\n\n\n\n\nadded needle",
                    "a title reads as an empty line, and a message as none"
                );
                assert_eq!(
                    view.change_heads(31),
                    heads,
                    "a change, a message's title, and the title over a first change"
                );
                view.set_matches(
                    ctx,
                    vec![
                        TextRange::new(TextPosition::new(8, 6), TextPosition::new(8, 12)),
                        TextRange::new(TextPosition::new(2, 4), TextPosition::new(2, 10)),
                        // A range in a title matches nothing.
                        TextRange::new(TextPosition::new(5, 0), TextPosition::new(5, 1)),
                    ],
                );
                assert_eq!((view.search_matches(), view.search_position()), (2, 1));
                view.search_next(ctx, 1);
                Ok(())
            })?;
            harness.render()?;
            let current = harness.canopy.style().resolve(view_search::CURRENT);
            assert_eq!(
                harness.buf().get(found).expect("the match").style.bg,
                current.bg.solid_color().expect("solid search style"),
                "{mode:?}: the match in the last section, at {found:?}: {:?}",
                harness.tbuf().lines()
            );
        }
        Ok(())
    }

    #[test]
    fn a_section_prepares_its_highlighters_only_in_view() -> Result<()> {
        let mut harness = Harness::builder(sectioned(DiffLayout::Unified))
            .size(30, 4)
            .build()?;
        harness.render()?;
        let prepared = |harness: &mut Harness| {
            harness.with_root_widget(|view: &mut DiffView| {
                view.sections
                    .iter()
                    .map(|section| section.prepared)
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(prepared(&mut harness), [true, false, false]);
        harness.with_root_widget_context(|view: &mut DiffView, ctx| {
            view.next_change(ctx);
            view.next_change(ctx);
            Ok(())
        })?;
        harness.render()?;
        assert_eq!(prepared(&mut harness), [true, true, true]);
        Ok(())
    }

    #[test]
    fn replacing_a_section_keeps_the_others() -> Result<()> {
        let mut harness = Harness::builder(sectioned(DiffLayout::Unified))
            .size(30, 11)
            .build()?;
        harness.render()?;
        harness.with_root_widget(|view: &mut DiffView| {
            let revision = view.revision();
            let arrived =
                DiffSection::new(DiffModel::from_texts("one\n", "one\ntwo\n")).with_title("b.txt");
            assert!(view.set_section(1, arrived));
            assert_ne!(view.revision(), revision, "the text changed");
            assert!(
                view.sections[0].prepared,
                "the first section keeps its state"
            );
            assert!(!view.sections[1].prepared);
            assert_eq!(view.section_starts(30), [0, 5, 9]);
            assert!(
                !view.set_section(3, DiffSection::message("none")),
                "the view holds three sections"
            );
        });
        harness.render()?;
        assert_eq!(
            trimmed(&harness)[5..8],
            [" b.txt", " 1 one", "+2 two"],
            "the section shows its diff"
        );
        Ok(())
    }
}
