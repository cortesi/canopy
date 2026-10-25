//! A read-only table that flows into its width, as a document shows a table.
//!
//! [`TableReader`] lays its table out as a [`FlowTable`], the layout that a
//! Markdown document gives an embedded table: columns keep their natural
//! widths where they fit, and prose wraps where they do not. A table wider
//! than the view shows the leading columns that fit, names them in a label
//! above the table, and pages sideways through the rest. The label and the
//! header stay at the top while the rows scroll beneath them.
//!
//! The reader searches as every searchable view does, by the conventions of
//! [`view_search`](crate::view_search): the host installs the matches that
//! its matcher finds in the cells, with
//! [`cell_matches`](crate::regex_search::cell_matches), and the reader marks
//! them, steps between them, centres the current one, and marks their rows
//! on a scrollbar.
//!
//! In interact mode the reader selects one cell, and the navigation intents
//! move the selection instead of the rows: a cell at a time, a page of rows
//! at a time, or to the first or last row. The rows and the columns follow
//! the selection.

use std::{
    cell::{Cell, Ref, RefCell},
    sync::Arc,
};

use canopy::{
    ChangeOutcome, Context, EventOutcome, NodeName, Register, Setup, ViewContext, Widget,
    derive_commands,
    error::Result,
    geom::{Line, Point, Rect, Size},
    input::NavIntent,
    layout::{
        CanvasContext, Layout, MeasureConstraints, Measurement, RevealAlign, ScrollAxis,
        ScrollDirection, ScrollMark, ScrollOp,
    },
    render::Render,
    status::StatusItem,
};

use crate::{
    table::{
        CellMark, CellRange, FlowOptions, FlowTable, TableData, TableGlyphs, TableOverlay,
        style_rules,
    },
    view_search,
};

/// Cells of margin on each side of the table, when the view has room.
const PAD: u32 = 1;

/// Return the width the table fits in a view `width` cells wide.
fn inner(width: u32) -> u32 {
    if width > PAD * 2 {
        width - PAD * 2
    } else {
        width
    }
}

/// A flowed table laid out for one width and first column.
struct Laid {
    /// The width the flow fits.
    width: u32,
    /// The first column the flow shows.
    first: usize,
    /// The flow.
    flow: Arc<FlowTable>,
    /// The label that names the columns shown, when the flow cuts any.
    label: Option<String>,
}

impl Laid {
    /// Rows the label takes above the table.
    fn label_rows(&self) -> u32 {
        u32::from(self.label.is_some())
    }

    /// Rows that stay at the top while the body scrolls: the label, the
    /// header, and its rule.
    fn pinned(&self) -> u32 {
        self.label_rows() + self.flow.row_line(Some(0)).unwrap_or(0)
    }

    /// Return the canvas row of the first line of a body row, or of the
    /// header for `None`.
    fn row(&self, row: Option<usize>) -> u32 {
        self.label_rows() + self.flow.row_line(row).unwrap_or(0)
    }
}

/// What a reveal of the reader shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reveal {
    /// The current search match, centred.
    Match,
    /// The selected cell's row, beneath the pinned rows.
    Cursor,
}

/// Style role of the selected cell.
const CURSOR: &str = "table/cursor";

/// A read-only, searchable table that flows into its width.
pub struct TableReader {
    /// The table shown.
    data: Arc<TableData>,
    /// The first column shown.
    first: Cell<usize>,
    /// A column to bring into view once the reader knows its width.
    show_column: Cell<Option<usize>>,
    /// The layout for the last width, rebuilt when the width or the first
    /// column changes.
    laid: RefCell<Option<Laid>>,
    /// Search matches in reading order.
    matches: Vec<CellRange>,
    /// The index of the current match.
    current: Option<usize>,
    /// The selected cell in interact mode, by body row and data column.
    cursor: Option<(usize, usize)>,
    /// What the next reveal shows.
    reveal: Cell<Reveal>,
}

#[derive_commands]
impl TableReader {
    /// Construct a reader of `data`.
    pub fn new(data: Arc<TableData>) -> Self {
        Self {
            data,
            first: Cell::new(0),
            show_column: Cell::new(None),
            laid: RefCell::new(None),
            matches: Vec::new(),
            current: None,
            cursor: None,
            reveal: Cell::new(Reveal::Match),
        }
    }

    /// Return the table shown.
    pub fn data(&self) -> &Arc<TableData> {
        &self.data
    }

    /// Show `data` from its first column, dropping the search matches and
    /// the selected cell, which address the table they were found in. The
    /// scroll offset stays, so a host that shows a new table scrolls to its
    /// top.
    pub fn set_data(&mut self, data: Arc<TableData>) {
        self.data = data;
        self.first.set(0);
        self.show_column.set(None);
        self.laid.replace(None);
        self.matches.clear();
        self.current = None;
        self.cursor = None;
    }

    /// Return the first column shown.
    pub fn first_column(&self) -> usize {
        self.first.get()
    }

    /// Show the columns from `delta` columns past the first one shown,
    /// within the table.
    /// @param delta Columns to move; negative values move left.
    #[command]
    pub fn shift_columns(&mut self, ctx: &mut dyn Context, delta: i32) {
        // The layout settles any column waiting to show before the shift.
        // A shift past the last column the view needs shows nothing new.
        if !self.laid(ctx.view().content.w).flow.cut() && delta > 0 {
            return;
        }
        let first = self.first.get().saturating_add_signed(delta as isize);
        self.set_first_column(first);
    }

    /// Scroll the rows by `op`. A page is the height of the rows that show
    /// beneath the pinned label and header, less one row.
    pub fn scroll(&mut self, ctx: &mut dyn Context, op: ScrollOp) -> ChangeOutcome {
        let view = ctx.view();
        let op = match op {
            ScrollOp::Pages(direction, count) => {
                let body = view
                    .content
                    .h
                    .saturating_sub(self.laid(view.content.w).pinned());
                ScrollOp::Lines(direction, body.saturating_sub(1).max(1) * count)
            }
            op => op,
        };
        ctx.scroll(op)
    }

    /// Highlight `matches`, which arrive in reading order: header cells
    /// first, then each row's cells from left to right. The first match at
    /// or below the top of the rows becomes current, and the reader reveals
    /// it.
    pub fn set_matches(&mut self, ctx: &mut dyn Context, matches: Vec<CellRange>) {
        self.matches = matches;
        let view = ctx.view();
        self.current = {
            let laid = self.laid(view.content.w);
            // Rows scroll beneath the pinned rows, so the first row in view
            // is past them, but at the top the header itself is in view.
            let top = match view.scroll.y {
                0 => 0,
                scroll => scroll + laid.pinned(),
            };
            view_search::first_at_or_below(
                self.matches.iter().map(|found| laid.row(found.row)),
                top,
            )
        };
        self.reveal(ctx);
    }

    /// Highlight `matches`, which arrive as for [`Self::set_matches`],
    /// without moving the view, as when the table reloads beneath a held
    /// search. The first match at or after `current`, the match that was
    /// current before, becomes current; with no `current`, no match is.
    pub fn restore_matches(&mut self, matches: Vec<CellRange>, current: Option<&CellRange>) {
        self.current = current.and_then(|current| {
            let at = (current.cell(), current.bytes.start);
            let index = matches.partition_point(|found| (found.cell(), found.bytes.start) < at);
            (!matches.is_empty()).then(|| index % matches.len())
        });
        self.matches = matches;
    }

    /// Return the current search match.
    pub fn current_match(&self) -> Option<&CellRange> {
        self.current.and_then(|index| self.matches.get(index))
    }

    /// Make another search match current and reveal it. While the reader
    /// selects a cell, a match in the rows selects its cell.
    /// @param delta Matches to move; negative values move backward.
    #[command]
    pub fn search_next(&mut self, ctx: &mut dyn Context, delta: i32) {
        self.current = view_search::step(self.current, self.matches.len(), delta);
        self.reveal(ctx);
    }

    /// Remove the search and its highlights.
    #[command]
    pub fn clear_search(&mut self, _ctx: &mut dyn Context) {
        self.matches.clear();
        self.current = None;
    }

    /// Select a cell, so the navigation intents move the selection instead
    /// of the rows. The selection starts at the current search match's cell
    /// when it shows in the rows, and otherwise at the first cell in view. A
    /// table with no rows has no cell to select.
    #[command]
    pub fn interact(&mut self, ctx: &mut dyn Context) {
        if self.cursor.is_some() || self.data.rows().is_empty() {
            return;
        }
        let cell = {
            let view = ctx.view();
            let laid = self.laid(view.content.w);
            // The rows show from the first whose top lies beneath the pinned
            // rows.
            let top = laid.flow.row_line(Some(0)).unwrap_or(0) + view.scroll.y;
            let bottom = top + view.content.h.saturating_sub(laid.pinned());
            // Before layout gives the view a height, every row past the top
            // counts as in view.
            let shows = |row: usize| {
                laid.flow
                    .row_line(Some(row))
                    .is_some_and(|line| line >= top && (line < bottom || bottom <= top))
            };
            let found = self
                .current_match()
                .and_then(|found| Some((found.row?, found.column)))
                .filter(|&(row, column)| shows(row) && laid.flow.columns().contains(&column));
            found.unwrap_or_else(|| {
                let row = laid.flow.row_at(top).unwrap_or(0);
                let row = if shows(row) {
                    row
                } else {
                    (row + 1).min(self.data.rows().len() - 1)
                };
                (row, self.first.get())
            })
        };
        self.select(ctx, cell);
    }

    /// Stop selecting a cell, so the navigation intents move the rows again.
    #[command]
    pub fn leave_interact(&mut self, _ctx: &mut dyn Context) {
        self.cursor = None;
    }

    /// Return whether the reader selects a cell.
    #[command]
    pub fn interacting(&self) -> bool {
        self.cursor.is_some()
    }

    /// Return the selected cell, by body row and data column.
    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }

    /// Select `cell`, by body row and data column, within the table, or stop
    /// selecting with `None`. Nothing scrolls, as when a host restores the
    /// selection of a reloaded table.
    pub fn set_cursor(&mut self, cell: Option<(usize, usize)>) {
        self.cursor = cell
            .filter(|_| !self.data.rows().is_empty())
            .map(|cell| self.clamp(cell));
    }

    /// Return the text of the selected cell.
    #[command]
    pub fn selected_text(&self) -> Option<String> {
        let (row, column) = self.cursor?;
        Some(self.data.rows()[row].cells[column].text().to_owned())
    }

    /// Move the selected cell `rows` rows down and `columns` columns right,
    /// within the table.
    /// @param rows Rows to move; negative values move up.
    /// @param columns Columns to move; negative values move left.
    #[command]
    pub fn move_cursor(&mut self, ctx: &mut dyn Context, rows: i32, columns: i32) {
        let Some((row, column)) = self.cursor else {
            return;
        };
        let cell = (
            row.saturating_add_signed(rows as isize),
            column.saturating_add_signed(columns as isize),
        );
        self.select(ctx, cell);
    }

    /// Return the number of search matches.
    #[command]
    pub fn search_matches(&self) -> usize {
        self.matches.len()
    }

    /// Return the one-based position of the current search match.
    /// @return The position, or 0 when there is no current match.
    #[command]
    pub fn search_position(&self) -> usize {
        self.current.map_or(0, |index| index + 1)
    }

    /// Return `(row, column)` within the table's rows and columns.
    fn clamp(&self, (row, column): (usize, usize)) -> (usize, usize) {
        (
            row.min(self.data.rows().len().saturating_sub(1)),
            column.min(self.data.columns().len().saturating_sub(1)),
        )
    }

    /// Select `cell`, within the table, and bring it into view.
    fn select(&mut self, ctx: &mut dyn Context, cell: (usize, usize)) {
        let (row, column) = self.clamp(cell);
        self.cursor = Some((row, column));
        self.show_column.set(Some(column));
        self.reveal.set(Reveal::Cursor);
        ctx.reveal_anchor(RevealAlign::Nearest);
    }

    /// Move the selected cell by `pages` pages of rows, and the rows with
    /// it, so the selection keeps its place in the view.
    fn page_cursor(&mut self, ctx: &mut dyn Context, pages: i32) {
        let Some((row, column)) = self.cursor else {
            return;
        };
        let target = {
            let view = ctx.view();
            let laid = self.laid(view.content.w);
            let page = view.content.h.saturating_sub(laid.pinned());
            let page = i64::from(page.saturating_sub(1).max(1));
            let line = i64::from(laid.flow.row_line(Some(row)).unwrap_or(0));
            let first = i64::from(laid.flow.row_line(Some(0)).unwrap_or(0));
            let line = (line + page * i64::from(pages)).max(first);
            laid.flow.row_at(u32::try_from(line).unwrap_or(u32::MAX))
        };
        self.scroll(ctx, ScrollOp::pages(pages));
        self.select(ctx, (target.unwrap_or(0), column));
    }

    /// Move by `nav`, as the reader's navigation intents do: the selected
    /// cell while the reader selects one, and otherwise the rows, or the
    /// columns sideways.
    pub fn navigate(&mut self, ctx: &mut dyn Context, nav: NavIntent) {
        if self.cursor.is_some() {
            match nav {
                NavIntent::Left => self.move_cursor(ctx, 0, -1),
                NavIntent::Right => self.move_cursor(ctx, 0, 1),
                NavIntent::Up => self.move_cursor(ctx, -1, 0),
                NavIntent::Down => self.move_cursor(ctx, 1, 0),
                NavIntent::PageUp => self.page_cursor(ctx, -1),
                NavIntent::PageDown => self.page_cursor(ctx, 1),
                NavIntent::First => self.move_cursor(ctx, i32::MIN, 0),
                NavIntent::Last => self.move_cursor(ctx, i32::MAX, 0),
            }
            return;
        }
        match nav {
            NavIntent::Left => self.shift_columns(ctx, -1),
            NavIntent::Right => self.shift_columns(ctx, 1),
            NavIntent::Up => {
                self.scroll(ctx, ScrollOp::Lines(ScrollDirection::Up, 1));
            }
            NavIntent::Down => {
                self.scroll(ctx, ScrollOp::Lines(ScrollDirection::Down, 1));
            }
            NavIntent::PageUp => {
                self.scroll(ctx, ScrollOp::pages(-1));
            }
            NavIntent::PageDown => {
                self.scroll(ctx, ScrollOp::pages(1));
            }
            NavIntent::First => {
                self.scroll(ctx, ScrollOp::To(Point::ZERO));
            }
            NavIntent::Last => {
                self.scroll(ctx, ScrollOp::To(Point::new(0, u32::MAX)));
            }
        }
    }

    /// Show the columns from `first`, within the table.
    pub fn set_first_column(&mut self, first: usize) {
        self.show_column.set(None);
        self.start_at(first);
    }

    /// Lay the table out from column `first`, within the table.
    fn start_at(&self, first: usize) {
        let first = first.min(self.data.columns().len().saturating_sub(1));
        if first != self.first.get() {
            self.first.set(first);
            self.laid.replace(None);
        }
    }

    /// Show the current match's column and centre its row, once layout has
    /// settled the reader's width. While the reader selects a cell, a match
    /// in the rows selects its cell.
    fn reveal(&mut self, ctx: &mut dyn Context) {
        let Some(found) = self.current_match() else {
            return;
        };
        let (row, column) = (found.row, found.column);
        self.show_column.set(Some(column));
        // The header stays in view, so only a body row scrolls to show.
        if let Some(row) = row {
            if self.cursor.is_some() {
                self.cursor = Some((row, column));
            }
            self.reveal.set(Reveal::Match);
            ctx.reveal_anchor(view_search::REVEAL);
        }
    }

    /// Lay out from the first column that shows `column`: the column itself
    /// when it lies before the columns shown, and otherwise the first that
    /// still shows the columns before it that fit.
    fn bring_into_view(&self, width: u32, column: usize) {
        let first = self.first.get();
        if column < first {
            self.start_at(column);
            return;
        }
        let shows = |first: usize| self.prepare(width, first).columns().contains(&column);
        let cached = self
            .laid
            .borrow()
            .as_ref()
            .filter(|laid| laid.width == width && laid.first == first)
            .map(|laid| laid.flow.columns().contains(&column));
        if cached.unwrap_or_else(|| shows(first)) {
            return;
        }
        // A later first column leaves more room for the columns after it, so
        // the first that shows `column` is found by bisection.
        let (mut low, mut high) = (first + 1, column);
        while low < high {
            let middle = low + (high - low) / 2;
            if shows(middle) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        self.start_at(low);
    }

    /// Lay the table out to fit `width` from column `first`.
    fn prepare(&self, width: u32, first: usize) -> FlowTable {
        FlowTable::prepare(
            self.data.clone(),
            FlowOptions {
                width,
                cell_lines: u32::MAX,
                glyphs: TableGlyphs::UNICODE,
                first_column: first,
            },
        )
        .expect("the reader's glyphs are valid")
    }

    /// Return the layout for a view `width` cells wide, laying the table out
    /// again when the width or the first column has changed. A width of
    /// zero is one that layout has yet to settle, so a column waiting to
    /// show waits on.
    fn laid(&self, width: u32) -> Ref<'_, Laid> {
        let width = inner(width);
        if width > 0
            && let Some(column) = self.show_column.take()
        {
            self.bring_into_view(width, column);
        }
        let width = width.max(1);
        let first = self.first.get();
        let stale = self
            .laid
            .borrow()
            .as_ref()
            .is_none_or(|laid| laid.width != width || laid.first != first);
        if stale {
            let flow = self.prepare(width, first);
            let label = flow.label();
            self.laid.replace(Some(Laid {
                width,
                first,
                flow: Arc::new(flow),
                label,
            }));
        }
        Ref::map(self.laid.borrow(), |laid| {
            laid.as_ref().expect("the layout was just built")
        })
    }

    /// Return the marks that paint the search matches, and the selected
    /// cell's ground.
    fn overlay(&self) -> TableOverlay {
        let marks = self
            .matches
            .iter()
            .enumerate()
            .map(|(index, found)| CellMark {
                range: found.clone(),
                role: if Some(index) == self.current {
                    view_search::CURRENT
                } else {
                    view_search::MATCH
                },
            })
            .collect();
        let overlay = TableOverlay::new(None, marks);
        match self.cursor {
            Some((row, column)) => overlay.with_cell(Some(row), column, CURSOR),
            None => overlay,
        }
    }
}

impl Widget for TableReader {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        constraints.clamp(Size::new(1, 1))
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let laid = self.laid(view.w);
        Size::new(view.w.max(1), laid.label_rows() + laid.flow.height().max(1))
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let area = view.view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        let laid = self.laid(view.content.w);
        let overlay = self.overlay();
        render.fill("table", area, ' ')?;
        let pad = (area.w - inner(area.w)) / 2;
        let area = Rect::new(area.tl.x + pad, area.tl.y, inner(area.w), area.h);
        let label = laid.label_rows().min(area.h);
        if let Some(text) = laid.label.as_ref().filter(|_| label > 0) {
            render.text("table/label", Line::new(area.tl.x, area.tl.y, area.w), text)?;
        }
        // The header stays at the top, and the rows scroll beneath it.
        let header = laid.pinned().saturating_sub(laid.label_rows());
        let header = header.min(area.h - label);
        let at = |rows: u32| Rect::new(area.tl.x, area.tl.y + rows, area.w, area.h - rows);
        laid.flow.paint(
            render,
            Rect::new(area.tl.x, area.tl.y + label, area.w, header),
            Point::ZERO,
            &overlay,
        )?;
        let body = label + header;
        if body < area.h {
            let top = laid.flow.row_line(Some(0)).unwrap_or(0) + view.scroll.y;
            laid.flow
                .paint(render, at(body), Point::new(0, top), &overlay)?;
        }
        Ok(())
    }

    fn reveal_anchor(&self, content: Size) -> Option<Rect> {
        let laid = self.laid(content.w);
        match self.reveal.get() {
            Reveal::Match => {
                let found = self.current_match()?;
                found.row?;
                Some(Rect::new(0, laid.row(found.row), 1, 1))
            }
            Reveal::Cursor => {
                let (row, _) = self.cursor?;
                let top = laid.row(Some(row));
                let end = if row + 1 < self.data.rows().len() {
                    laid.row(Some(row + 1))
                } else {
                    laid.label_rows() + laid.flow.height()
                };
                // The pinned rows cover the top of the view, so the row
                // shows only as far below the top as they reach.
                let pinned = laid.pinned();
                Some(Rect::new(0, top - pinned, 1, end - top + pinned))
            }
        }
    }

    fn scroll_marks(&self, axis: ScrollAxis, content: Size) -> Vec<ScrollMark> {
        if axis != ScrollAxis::Vertical {
            return Vec::new();
        }
        let laid = self.laid(content.w);
        view_search::marks(self.matches.iter().map(|found| laid.row(found.row)))
    }

    fn accepts_intent(&self, intent: &str, _ctx: &dyn ViewContext) -> bool {
        NavIntent::from_name(intent).is_some()
    }

    fn on_intent(&mut self, intent: &str, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Some(nav) = NavIntent::from_name(intent) else {
            return Ok(EventOutcome::Ignore);
        };
        self.navigate(ctx, nav);
        Ok(EventOutcome::Handle)
    }

    fn status(&self, _ctx: &dyn ViewContext) -> Vec<StatusItem> {
        let Some((row, column)) = self.cursor else {
            return Vec::new();
        };
        let label = self.data.columns()[column].label.text();
        vec![
            StatusItem::new("table.cell")
                .at_end()
                .priority(41)
                .span("label", format!("row {} · {label}", row + 1)),
        ]
    }

    fn name(&self) -> NodeName {
        NodeName::convert("table_reader")
    }
}

impl Register for TableReader {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.widget_styles(style_rules);
        setup.add_commands::<Self>()
    }
}

#[cfg(test)]
mod tests;
