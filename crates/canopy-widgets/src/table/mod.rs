//! Immutable keyed table snapshots and their shared, clipped grid renderer.

use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    mem::size_of,
    ops::Range,
    sync::Arc,
};

use canopy::{
    error::{Error, Result},
    geom::{Line, Point, Rect, Size},
    render::Render,
    style::{Style, StyleRules, themes::Palette},
    text,
};
use unicode_segmentation::UnicodeSegmentation;

/// A stable dataset, row, or column identity. Labels are independent of keys.
pub type TableKey = Arc<str>;

/// A style over a half-open UTF-8 byte range of a cell's original text.
#[derive(Clone, Debug)]
pub struct TableSpan {
    /// Bytes of the full value.
    pub range: Range<usize>,
    /// Presentation only, with no action metadata.
    pub style: Style,
}

/// A full cell value and optional presentation spans.
#[derive(Clone, Debug)]
pub struct TableCell {
    /// Full unmodified cell value.
    text: Arc<str>,
    /// Validated presentation spans over the full value.
    spans: Arc<[TableSpan]>,
}
impl TableCell {
    /// Construct a plain value, preserving every byte for detail and copy.
    pub fn plain(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            spans: Arc::from([]),
        }
    }
    /// Validate ordered, nonoverlapping style spans on UTF-8 boundaries.
    pub fn styled(text: impl Into<Arc<str>>, spans: Vec<TableSpan>) -> Result<Self> {
        let text = text.into();
        let mut end = 0;
        for span in &spans {
            if span.range.start < end
                || span.range.start > span.range.end
                || span.range.end > text.len()
                || !text.is_char_boundary(span.range.start)
                || !text.is_char_boundary(span.range.end)
            {
                return Err(Error::Invalid(
                    "table style spans must be ordered UTF-8 byte ranges".into(),
                ));
            }
            end = span.range.end;
        }
        Ok(Self {
            text,
            spans: spans.into(),
        })
    }
    /// Return the full, unmodified value.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Share the full value without copying its bytes on the input path.
    pub fn shared_text(&self) -> Arc<str> {
        self.text.clone()
    }
    /// Return validated presentation spans.
    pub fn spans(&self) -> &[TableSpan] {
        &self.spans
    }
}
impl From<String> for TableCell {
    fn from(value: String) -> Self {
        Self::plain(value)
    }
}
impl From<&str> for TableCell {
    fn from(value: &str) -> Self {
        Self::plain(value)
    }
}

/// Horizontal placement within a column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TableAlignment {
    #[default]
    /// Align at the start.
    Left,
    /// Center short values.
    Center,
    /// Align at the end.
    Right,
}

/// One ordered column. Duplicate labels are permitted.
#[derive(Clone, Debug)]
pub struct TableColumn {
    /// Stable identity within the column axis.
    pub key: TableKey,
    /// Full label.
    pub label: TableCell,
    /// Placement of labels and body values.
    pub alignment: TableAlignment,
}
impl TableColumn {
    /// Construct a left-aligned column.
    pub fn new(key: impl Into<TableKey>, label: impl Into<TableCell>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            alignment: TableAlignment::Left,
        }
    }
}
/// One ordered keyed row, containing exactly one cell per column.
#[derive(Clone, Debug)]
pub struct TableRow {
    /// Stable identity within the row axis.
    pub key: TableKey,
    /// Full cell values, in column order.
    pub cells: Vec<TableCell>,
}
impl TableRow {
    /// Construct a row. The dataset validates its cell count.
    pub fn new(key: impl Into<TableKey>, cells: Vec<TableCell>) -> Self {
        Self {
            key: key.into(),
            cells,
        }
    }
}

/// A validated immutable snapshot. Construction indexes each axis once.
#[derive(Debug)]
pub struct TableData {
    /// Stable dataset identity.
    dataset: TableKey,
    /// Monotonic revision within the dataset.
    revision: u64,
    /// Validated columns in display order.
    columns: Vec<TableColumn>,
    /// Validated rectangular body rows.
    rows: Vec<TableRow>,
    /// Constant-time lookup of stable row identities.
    row_index: HashMap<TableKey, usize>,
    /// Constant-time lookup of stable column identities.
    column_index: HashMap<TableKey, usize>,
    /// Shared row-axis membership in display order.
    row_keys: Arc<[TableKey]>,
    /// Shared column-axis membership in display order.
    column_keys: Arc<[TableKey]>,
}
impl TableData {
    /// Validate unique keys and rectangular body data.
    pub fn new(
        dataset: impl Into<TableKey>,
        revision: u64,
        columns: Vec<TableColumn>,
        rows: Vec<TableRow>,
    ) -> Result<Self> {
        let row_keys: Arc<[TableKey]> = rows.iter().map(|row| row.key.clone()).collect();
        let column_keys: Arc<[TableKey]> = columns.iter().map(|col| col.key.clone()).collect();
        let row_index: HashMap<_, _> = row_keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        let column_index: HashMap<_, _> = column_keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        if row_index.len() != rows.len() || column_index.len() != columns.len() {
            return Err(Error::Invalid(
                "table keys must be unique within each axis".into(),
            ));
        }
        if rows.iter().any(|row| row.cells.len() != columns.len()) {
            return Err(Error::Invalid(
                "each table row must have one cell per column".into(),
            ));
        }
        if rows.len() > u32::MAX as usize {
            return Err(Error::Invalid("table has too many rows".into()));
        }
        Ok(Self {
            dataset: dataset.into(),
            revision,
            columns,
            rows,
            row_index,
            column_index,
            row_keys,
            column_keys,
        })
    }
    /// Start a plain table with stable numeric column keys, independent of
    /// labels.
    pub fn builder(
        dataset: impl Into<TableKey>,
        revision: u64,
        labels: impl IntoIterator<Item = impl Into<TableCell>>,
    ) -> TableBuilder {
        TableBuilder {
            dataset: dataset.into(),
            revision,
            columns: labels
                .into_iter()
                .enumerate()
                .map(|(i, label)| TableColumn::new(i.to_string(), label))
                .collect(),
            rows: Vec::new(),
        }
    }
    /// Dataset identity.
    pub fn dataset(&self) -> &TableKey {
        &self.dataset
    }
    /// Data revision, increasing within a dataset.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Ordered columns.
    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }
    /// Ordered rows.
    pub fn rows(&self) -> &[TableRow] {
        &self.rows
    }
    /// Find a stable row key without scanning the dataset.
    pub fn row_index(&self, key: &str) -> Option<usize> {
        self.row_index.get(key).copied()
    }
    /// Find a stable column key without scanning the dataset.
    pub fn column_index(&self, key: &str) -> Option<usize> {
        self.column_index.get(key).copied()
    }
    /// Shared axis keys, suitable for select-all without per-cell records.
    pub fn row_keys(&self) -> Arc<[TableKey]> {
        self.row_keys.clone()
    }
    /// Shared column keys.
    pub fn column_keys(&self) -> Arc<[TableKey]> {
        self.column_keys.clone()
    }
    /// Resolve a keyed cell in this snapshot.
    pub fn cell(&self, cell: &CellKey) -> Option<&TableCell> {
        Some(&self.rows[self.row_index(&cell.row)?].cells[self.column_index(&cell.column)?])
    }
    /// Return the key of an indexed body cell.
    pub fn cell_key(&self, row: usize, column: usize) -> Option<CellKey> {
        Some(CellKey {
            row: self.rows.get(row)?.key.clone(),
            column: self.columns.get(column)?.key.clone(),
        })
    }
}
/// Convenience builder for plain keyed rows.
pub struct TableBuilder {
    /// Identity for the snapshot under construction.
    dataset: TableKey,
    /// Revision for the snapshot under construction.
    revision: u64,
    /// Columns collected from the supplied labels.
    columns: Vec<TableColumn>,
    /// Keyed rows awaiting rectangular validation.
    rows: Vec<TableRow>,
}
impl TableBuilder {
    /// Append one keyed row in display order.
    #[must_use]
    pub fn row(
        mut self,
        key: impl Into<TableKey>,
        values: impl IntoIterator<Item = impl Into<TableCell>>,
    ) -> Self {
        self.rows.push(TableRow::new(
            key,
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }
    /// Validate and freeze the snapshot.
    pub fn build(self) -> Result<TableData> {
        TableData::new(self.dataset, self.revision, self.columns, self.rows)
    }
}

/// A body cell's stable keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellKey {
    /// Row identity.
    pub row: TableKey,
    /// Column identity.
    pub column: TableKey,
}

/// Explicit Cartesian membership. Storage grows by axes, never by cells.
#[derive(Clone, Debug, Default)]
pub struct TableSelection {
    /// Explicit captured row membership.
    rows: Arc<[TableKey]>,
    /// Explicit captured column membership.
    columns: Arc<[TableKey]>,
    /// Shared constant-time row membership lookup.
    row_set: Arc<HashSet<TableKey>>,
    /// Shared constant-time column membership lookup.
    column_set: Arc<HashSet<TableKey>>,
}
impl TableSelection {
    /// Construct explicit membership, removing duplicate keys.
    pub fn new(rows: Arc<[TableKey]>, columns: Arc<[TableKey]>) -> Self {
        fn axis(keys: Arc<[TableKey]>) -> (Arc<[TableKey]>, Arc<HashSet<TableKey>>) {
            let set: HashSet<_> = keys.iter().cloned().collect();
            let keys = if set.len() == keys.len() {
                keys
            } else {
                let mut seen = HashSet::new();
                keys.iter()
                    .filter(|key| seen.insert((*key).clone()))
                    .cloned()
                    .collect()
            };
            (keys, Arc::new(set))
        }
        let (rows, row_set) = axis(rows);
        let (columns, column_set) = axis(columns);
        Self {
            row_set,
            column_set,
            rows,
            columns,
        }
    }
    /// Selected row keys in their captured order.
    pub fn rows(&self) -> &[TableKey] {
        &self.rows
    }
    /// Selected column keys in their captured order.
    pub fn columns(&self) -> &[TableKey] {
        &self.columns
    }
    /// Whether either membership axis is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() || self.columns.is_empty()
    }
    /// Whether a keyed cell belongs to the selection.
    pub fn contains(&self, row: &str, column: &str) -> bool {
        self.row_set.contains(row) && self.column_set.contains(column)
    }
    /// Intersect membership with a new publication.
    pub(crate) fn reconcile(&mut self, data: &TableData) {
        *self = Self::new(
            self.rows
                .iter()
                .filter(|key| data.row_index(key).is_some())
                .cloned()
                .collect(),
            self.columns
                .iter()
                .filter(|key| data.column_index(key).is_some())
                .cloned()
                .collect(),
        );
        if self.is_empty() {
            *self = Self::default();
        }
    }
}

/// Separator and control-substitution glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TableGlyphs {
    /// Column boundary.
    pub separator: char,
    /// A newline inside a value.
    pub newline: char,
    /// A tab inside a value.
    pub tab: char,
    /// Other terminal controls.
    pub control: char,
}
impl TableGlyphs {
    /// Thin Unicode separators and visible whitespace substitutions.
    pub const UNICODE: Self = Self {
        separator: '│',
        newline: '↵',
        tab: '⇥',
        control: '�',
    };
    /// Portable ASCII substitutions.
    pub const ASCII: Self = Self {
        separator: '|',
        newline: ' ',
        tab: ' ',
        control: '?',
    };
}
impl Default for TableGlyphs {
    fn default() -> Self {
        Self::UNICODE
    }
}

/// Deterministic width preparation. The default samples at most 64 rows and
/// 64 KiB of text across all columns. Large snapshots belong on a host worker.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableLayoutOptions {
    /// Exact widths, or bounded automatic measurement when empty.
    pub widths: Vec<u32>,
    /// Minimum automatic width, at least one cell.
    pub min_width: u32,
    /// Maximum preferred automatic width.
    pub max_width: u32,
    /// Maximum initial body rows measured.
    pub sample_rows: usize,
    /// Maximum bytes measured across labels and body cells.
    pub sample_bytes: usize,
    /// Shared grid glyphs.
    pub glyphs: TableGlyphs,
}
impl Default for TableLayoutOptions {
    fn default() -> Self {
        Self {
            widths: Vec::new(),
            min_width: 3,
            max_width: 40,
            sample_rows: 64,
            sample_bytes: 65536,
            glyphs: TableGlyphs::default(),
        }
    }
}

/// A pure prepared grid, retaining exactly the snapshot it measured.
#[derive(Debug)]
pub struct PreparedTable {
    /// Exact immutable snapshot measured by this geometry.
    data: Arc<TableData>,
    /// Preparation options retained with the geometry.
    options: TableLayoutOptions,
    /// Prepared cell widths in column order.
    widths: Vec<u32>,
    /// Column prefix sums for binary hit lookup.
    starts: Vec<u32>,
    /// Total body canvas width including separators.
    width: u32,
    /// Hash of every prepared geometry input.
    geometry_key: u64,
    /// Text bytes inspected by bounded width sampling.
    sampled_bytes: usize,
    /// Precomputed retained geometry allocation estimate.
    derived_bytes: usize,
}
/// Prepare widths once. Explicit widths require one positive width per column.
pub fn prepare(data: Arc<TableData>, options: TableLayoutOptions) -> Result<PreparedTable> {
    if options.min_width == 0
        || options.max_width < options.min_width
        || (!options.widths.is_empty()
            && (options.widths.len() != data.columns.len() || options.widths.contains(&0)))
    {
        return Err(Error::Invalid("invalid table widths".into()));
    }
    for glyph in [
        options.glyphs.separator,
        options.glyphs.newline,
        options.glyphs.tab,
        options.glyphs.control,
    ] {
        if text::width(&glyph.to_string()) != 1 || glyph.is_control() {
            return Err(Error::Invalid(
                "table glyphs must occupy one terminal cell".into(),
            ));
        }
    }
    let mut sampled_bytes = 0;
    let mut widths = vec![options.min_width; data.columns.len()];
    if options.widths.is_empty() {
        let mut measure = |cell: &TableCell, column: usize| {
            let remaining = options.sample_bytes.saturating_sub(sampled_bytes);
            let mut end = remaining.min(cell.text.len());
            while !cell.text.is_char_boundary(end) {
                end -= 1;
            }
            sampled_bytes += end;
            let width = display_width(&cell.text[..end], options.max_width);
            widths[column] = widths[column].max(width).min(options.max_width);
        };
        for (i, col) in data.columns.iter().enumerate() {
            measure(&col.label, i);
        }
        for row in data.rows.iter().take(options.sample_rows) {
            for (i, cell) in row.cells.iter().enumerate() {
                measure(cell, i);
            }
        }
    } else {
        widths.clone_from(&options.widths);
    }
    let mut starts = Vec::with_capacity(widths.len());
    let mut width = 0u32;
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            width = width
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("table width overflow".into()))?;
        }
        starts.push(width);
        width = width
            .checked_add(*w)
            .ok_or_else(|| Error::Invalid("table width overflow".into()))?;
    }
    let mut hash = DefaultHasher::new();
    options.hash(&mut hash);
    widths.hash(&mut hash);
    data.rows.len().hash(&mut hash);
    for col in data.columns() {
        col.alignment.hash(&mut hash);
    }
    let derived_bytes = size_of::<PreparedTable>()
        .saturating_add(widths.capacity().saturating_mul(size_of::<u32>()))
        .saturating_add(starts.capacity().saturating_mul(size_of::<u32>()))
        .saturating_add(options.widths.capacity().saturating_mul(size_of::<u32>()));
    Ok(PreparedTable {
        data,
        options,
        widths,
        starts,
        width,
        geometry_key: hash.finish(),
        sampled_bytes,
        derived_bytes,
    })
}
/// Return whether a character requires a safe one-cell substitution.
pub(crate) fn display_control(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}
/// Measure the one-line representation, stopping at the supplied width cap.
fn display_width(value: &str, cap: u32) -> u32 {
    let mut width = 0u32;
    for g in value.graphemes(true) {
        width = width.saturating_add(if g.chars().any(display_control) {
            1
        } else {
            u32::try_from(text::grapheme_width(g)).unwrap_or(u32::MAX)
        });
        if width >= cap {
            return cap;
        }
    }
    width
}

/// The scroll window and fixed decorations of a grid paint or hit query.
#[derive(Clone, Copy, Debug)]
pub struct TableWindow {
    /// Body canvas offset. Header follows x, gutter follows y.
    pub offset: Point,
    /// Include a fixed one-row header.
    pub header: bool,
    /// Include fixed row numbers.
    pub row_numbers: bool,
}
impl Default for TableWindow {
    fn default() -> Self {
        Self {
            offset: Point::ZERO,
            header: true,
            row_numbers: false,
        }
    }
}
/// A hit names data only. Separators and empty space return no hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableHit {
    /// Column label.
    Header(usize),
    /// Row-number gutter.
    Row(usize),
    /// Body cell.
    Cell {
        /// Row index.
        row: usize,
        /// Column index.
        column: usize,
    },
}
/// Visible-work counters returned by grid painting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TablePaintStats {
    /// Painted body cells.
    pub cells: usize,
    /// Painted labels.
    pub labels: usize,
    /// Visible columns visited after the binary prefix-sum lookup.
    pub columns_visited: usize,
    /// Body-row lookups, including an end-of-data probe when present.
    pub rows_visited: usize,
}
impl PreparedTable {
    /// Snapshot retained by this geometry.
    pub fn data(&self) -> &Arc<TableData> {
        &self.data
    }
    /// Prepared column widths.
    pub fn widths(&self) -> &[u32] {
        &self.widths
    }
    /// Glyph preset retained by this prepared snapshot.
    pub fn glyphs(&self) -> TableGlyphs {
        self.options.glyphs
    }
    /// Stable key for these layout options and measured widths.
    pub fn geometry_key(&self) -> u64 {
        self.geometry_key
    }
    /// Total body canvas dimensions, excluding fixed chrome.
    pub fn size(&self) -> Size {
        Size::new(self.width, self.data.rows.len() as u32)
    }
    /// Conservative retained geometry bytes, computed during preparation.
    /// This excludes the immutable TableData model and its full cell payloads.
    pub fn derived_bytes(&self) -> usize {
        self.derived_bytes
    }
    /// Bytes inspected for automatic measurement.
    pub fn sampled_bytes(&self) -> usize {
        self.sampled_bytes
    }
    /// Fixed gutter width, excluding the body grid.
    pub fn gutter_width(&self) -> u32 {
        self.data.rows.len().max(1).to_string().len() as u32 + 1
    }
    /// Body rectangle for one indexed cell.
    pub fn cell_rect(&self, row: usize, column: usize) -> Option<Rect> {
        if row >= self.data.rows.len() {
            return None;
        }
        Some(Rect::new(
            *self.starts.get(column)?,
            u32::try_from(row).ok()?,
            *self.widths.get(column)?,
            1,
        ))
    }
    /// Resolve a canvas column with a binary prefix-sum lookup.
    pub fn column_at(&self, x: u32) -> Option<usize> {
        let i = self
            .starts
            .partition_point(|start| *start <= x)
            .checked_sub(1)?;
        (x < self.starts[i] + self.widths[i]).then_some(i)
    }
    /// Hit a grid using the same offsets and decoration widths as paint.
    pub fn hit_test(&self, area: Rect, window: TableWindow, point: Point) -> Option<TableHit> {
        if !area.contains_point(point) {
            return None;
        }
        let x = point.x - area.tl.x;
        let y = point.y - area.tl.y;
        let gutter = if window.row_numbers {
            self.gutter_width().min(area.w)
        } else {
            0
        };
        if window.header && y == 0 {
            return self
                .column_at(x.checked_sub(gutter)?.saturating_add(window.offset.x))
                .map(TableHit::Header);
        }
        let row = y
            .checked_sub(u32::from(window.header))?
            .saturating_add(window.offset.y) as usize;
        if row >= self.data.rows.len() {
            return None;
        }
        if x < gutter {
            return Some(TableHit::Row(row));
        }
        Some(TableHit::Cell {
            row,
            column: self.column_at((x - gutter).saturating_add(window.offset.x))?,
        })
    }
    /// Paint only the visible row/column window. No per-cell nodes or hit
    /// rectangles exist.
    pub fn paint(
        &self,
        render: &mut Render,
        area: Rect,
        window: TableWindow,
        cursor: Option<&CellKey>,
        selection: &TableSelection,
    ) -> Result<TablePaintStats> {
        let mut stats = TablePaintStats::default();
        if area.w == 0 || area.h == 0 {
            return Ok(stats);
        }
        render.fill("table", area, ' ')?;
        let gutter = if window.row_numbers {
            self.gutter_width().min(area.w)
        } else {
            0
        };
        let body = Rect::new(area.tl.x + gutter, area.tl.y, area.w - gutter, area.h);
        // A gutter-only paint has no body columns to visit.
        let columns = if body.w == 0 {
            0..0
        } else {
            let from = self
                .starts
                .partition_point(|start| *start <= window.offset.x)
                .saturating_sub(1);
            from..self.widths.len()
        };
        for column in columns {
            let start = self.starts[column];
            let first = start.saturating_sub(u32::from(column > 0));
            if first >= window.offset.x.saturating_add(body.w) {
                break;
            }
            if start + self.widths[column] <= window.offset.x {
                continue;
            }
            stats.columns_visited += 1;
            if window.header {
                self.paint_cell(
                    render,
                    body,
                    window.offset.x,
                    body.tl.y,
                    column,
                    &self.data.columns[column].label,
                    "table/header",
                )?;
                stats.labels += usize::from(start < window.offset.x.saturating_add(body.w));
            }
            let rows = area.h.saturating_sub(u32::from(window.header));
            for y in 0..rows {
                let row = window.offset.y.saturating_add(y) as usize;
                stats.rows_visited += 1;
                let Some(data_row) = self.data.rows.get(row) else {
                    break;
                };
                let selected = selection.contains(&data_row.key, &self.data.columns[column].key);
                let active = cursor.is_some_and(|key| {
                    key.row == data_row.key && key.column == self.data.columns[column].key
                });
                let style = if active {
                    "table/cursor"
                } else if selected {
                    "table/selection"
                } else {
                    "table/cell"
                };
                self.paint_cell(
                    render,
                    body,
                    window.offset.x,
                    body.tl.y + u32::from(window.header) + y,
                    column,
                    &data_row.cells[column],
                    style,
                )?;
                stats.cells += usize::from(start < window.offset.x.saturating_add(body.w));
            }
        }
        if gutter > 0 {
            for y in u32::from(window.header)..area.h {
                let row = window.offset.y.saturating_add(y - u32::from(window.header));
                if row as usize >= self.data.rows.len() {
                    break;
                }
                render.text(
                    "table/gutter",
                    Line::new(area.tl.x, area.tl.y + y, gutter),
                    &format!("{} ", row + 1),
                )?;
            }
        }
        Ok(stats)
    }
    #[expect(clippy::too_many_arguments, reason = "one clipped cell paint")]
    /// Paint one visible clipped cell, including its preceding separator.
    fn paint_cell(
        &self,
        render: &mut Render,
        area: Rect,
        offset: u32,
        y: u32,
        column: usize,
        cell: &TableCell,
        path: &str,
    ) -> Result<()> {
        let start = self.starts[column];
        let width = self.widths[column];
        let mut glyph = [0; 4];
        if column > 0 && start > offset && start - 1 < offset.saturating_add(area.w) {
            render.text(
                "table/separator",
                Line::new(area.tl.x + (start - 1 - offset), y, 1),
                self.options.glyphs.separator.encode_utf8(&mut glyph),
            )?;
        }
        let left = start.max(offset);
        let right = (start + width).min(offset.saturating_add(area.w));
        if right <= left {
            return Ok(());
        }
        let line = Rect::new(area.tl.x + (left - offset), y, right - left, 1);
        render.fill(path, line, ' ')?;
        let measured = display_width(&cell.text, width);
        let padding = match self.data.columns[column].alignment {
            TableAlignment::Left => 0,
            TableAlignment::Center => (width - measured) / 2,
            TableAlignment::Right => width - measured,
        };
        let base = render.resolve_style(path);
        let mut x = start + padding;
        for (byte, g) in cell.text.grapheme_indices(true) {
            if x >= start + width || x >= offset.saturating_add(area.w) {
                break;
            }
            let g: &str = if g.chars().any(display_control) {
                match g {
                    "\n" | "\r" | "\r\n" => self.options.glyphs.newline,
                    "\t" => self.options.glyphs.tab,
                    _ => self.options.glyphs.control,
                }
                .encode_utf8(&mut glyph)
            } else {
                g
            };
            let cells = text::grapheme_width(g) as u32;
            if x >= offset
                && x.saturating_add(cells) <= start + width
                && x.saturating_add(cells) <= offset.saturating_add(area.w)
            {
                let point = Point {
                    x: area.tl.x + (x - offset),
                    y,
                };
                let mut style = base.clone();
                // Spans are ordered and disjoint, so their ends ascend.
                let span = cell.spans[cell.spans.partition_point(|span| span.range.end <= byte)..]
                    .first()
                    .filter(|span| span.range.contains(&byte));
                if let Some(span) = span {
                    let overlay = render.apply_effects(span.style.clone());
                    style.fg = overlay.fg;
                    style.attrs = overlay.attrs;
                }
                render.put_grapheme(style.resolve_at(line, point), point, g)?;
            }
            x = x.saturating_add(cells);
        }
        Ok(())
    }
    /// Serialize selected body cells in current display order, with optional
    /// labels. The limit applies to UTF-8 output bytes; overflow returns no
    /// partial payload.
    pub fn tsv(
        &self,
        selection: &TableSelection,
        include_header: bool,
        limit: usize,
    ) -> Result<String> {
        let mut rows: Vec<_> = selection
            .rows
            .iter()
            .filter_map(|key| self.data.row_index(key))
            .collect();
        rows.sort_unstable();
        rows.dedup();
        let mut columns: Vec<_> = selection
            .columns
            .iter()
            .filter_map(|key| self.data.column_index(key))
            .collect();
        columns.sort_unstable();
        columns.dedup();
        let mut out = String::new();
        let mut write_row = |cells: &[&TableCell]| -> Result<()> {
            for (i, cell) in cells.iter().enumerate() {
                if i > 0 {
                    push_bounded(&mut out, "\t", limit)?;
                }
                let quoted = cell.text.contains(['\t', '\n', '\r', '"']);
                if quoted {
                    push_bounded(&mut out, "\"", limit)?;
                }
                for part in cell.text.split_inclusive('"') {
                    push_bounded(&mut out, part, limit)?;
                    if part.ends_with('"') {
                        push_bounded(&mut out, "\"", limit)?;
                    }
                }
                if quoted {
                    push_bounded(&mut out, "\"", limit)?;
                }
            }
            push_bounded(&mut out, "\n", limit)
        };
        if include_header && !columns.is_empty() {
            write_row(
                &columns
                    .iter()
                    .map(|i| &self.data.columns[*i].label)
                    .collect::<Vec<_>>(),
            )?;
        }
        if !columns.is_empty() {
            for row in rows {
                write_row(
                    &columns
                        .iter()
                        .map(|i| &self.data.rows[row].cells[*i])
                        .collect::<Vec<_>>(),
                )?;
            }
        }
        Ok(out)
    }
}
/// Append bytes only when the complete fragment fits the copy limit.
fn push_bounded(out: &mut String, text: &str, limit: usize) -> Result<()> {
    if text.len() > limit.saturating_sub(out.len()) {
        return Err(Error::Invalid(format!("table copy exceeds {limit} bytes")));
    }
    out.push_str(text);
    Ok(())
}

/// Copy operation interpreted against the displayed snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableCopy {
    /// Current full value, without TSV framing.
    Cell,
    /// Explicit body membership.
    Selection,
    /// Rows intersecting the selection, or the cursor row.
    Rows,
    /// All rows, with labels exactly once.
    All,
}
/// Host-facing events. Clipboard access belongs to the host.
#[derive(Clone, Debug)]
pub enum TableEvent {
    /// One activation of a full cell value.
    Activate {
        /// Dataset identity.
        dataset: TableKey,
        /// Displayed revision.
        revision: u64,
        /// Activated keys.
        cell: CellKey,
    },
    /// One complete payload; never a truncated prefix.
    Copy {
        /// Dataset identity.
        dataset: TableKey,
        /// Displayed revision.
        revision: u64,
        /// Exact requested bytes.
        text: String,
    },
}
/// Interactive behavior and resource limits.
#[derive(Clone, Debug)]
pub struct TableViewOptions {
    /// Enable cursor input and focus.
    pub interactive: bool,
    /// Fixed row-number gutter.
    pub row_numbers: bool,
    /// Fixed header row.
    pub header: bool,
    /// Maximum pending host events.
    pub event_capacity: usize,
    /// Maximum aggregate pending event text bytes.
    pub event_bytes: usize,
    /// Maximum bytes in one copy.
    pub copy_bytes: usize,
}
impl Default for TableViewOptions {
    fn default() -> Self {
        Self {
            interactive: true,
            row_numbers: true,
            header: true,
            event_capacity: 32,
            event_bytes: 1024 * 1024,
            copy_bytes: 1024 * 1024,
        }
    }
}

/// Install the shared table palette roles, including standalone views and
/// previews. Pass this callback to `Setup::widget_styles` to follow theme
/// changes.
pub fn style_rules(palette: &Palette, rules: StyleRules<'_>) {
    use canopy::style::Attr;
    rules
        .fg("table", palette.fg)
        .bg("table", palette.bg)
        .bg("table/header", palette.panel_bg)
        .attr("table/header", Attr::Bold)
        .fg("table/gutter", palette.muted_fg)
        .fg("table/separator", palette.frame)
        .bg("table/selection", palette.selection_bg)
        .bg("table/cursor", palette.selection_bg)
        .attr("table/cursor", Attr::Underline)
        .attr("table/cursor", Attr::Bold)
        .fg("table/prompt", palette.accent)
        .bg("table/prompt", palette.panel_bg)
        .fg("table/scrollbar", palette.frame)
        .fg("table/scrollbar/thumb", palette.muted_fg)
        .apply();
}

#[cfg(test)]
mod tests;
