#![expect(
    clippy::multiple_inherent_impl,
    reason = "TermBuf methods are split between cell storage and motion."
)]

use std::{collections::BTreeMap, ops::Range};

use crate::{
    core::text,
    error::{Error, Result},
    geom::{Line, Point, Rect, Size},
    render::RenderBackend,
    style::ResolvedStyle,
};

/// A frame as styled text, for renderers outside the terminal.
mod capture;
/// Motion records and cursor painting.
mod motion;
pub use capture::{CaptureRun, CaptureStyle, ScreenCapture};
pub use motion::MotionStyle;

/// NULL character constant.
const NULL: char = '\0';

/// Maximum per-line shift to consider when diffing.
const MAX_LINE_SHIFT: usize = 8;
/// Maximum per-row shift to consider when diffing.
const MAX_ROW_SHIFT: usize = 4;

/// Limits for a materialized visible render target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderLimits {
    /// Maximum visible render-target width.
    pub max_width: u32,
    /// Maximum visible render-target height.
    pub max_height: u32,
    /// Maximum total number of materialized terminal cells.
    pub max_cells: usize,
}

impl RenderLimits {
    /// Construct explicit visible render-target limits.
    pub const fn new(max_width: u32, max_height: u32, max_cells: usize) -> Self {
        Self {
            max_width,
            max_height,
            max_cells,
        }
    }

    /// Validate a visible target size and return its exact cell count.
    pub fn cell_count(self, size: Size) -> Result<usize> {
        if size.w > self.max_width {
            return Err(Error::RenderWidthLimit {
                requested: size.w,
                limit: self.max_width,
            });
        }
        if size.h > self.max_height {
            return Err(Error::RenderHeightLimit {
                requested: size.h,
                limit: self.max_height,
            });
        }
        let width = usize::try_from(size.w).map_err(|_| Error::RenderCellCountOverflow {
            width: size.w,
            height: size.h,
        })?;
        let height = usize::try_from(size.h).map_err(|_| Error::RenderCellCountOverflow {
            width: size.w,
            height: size.h,
        })?;
        let cells = width
            .checked_mul(height)
            .ok_or(Error::RenderCellCountOverflow {
                width: size.w,
                height: size.h,
            })?;
        if cells > self.max_cells {
            return Err(Error::RenderCellLimit {
                requested: cells,
                limit: self.max_cells,
            });
        }
        Ok(cells)
    }
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self::new(2048, 2048, 1_000_000)
    }
}

/// A terminal cell with glyph and style.
#[derive(Clone, Debug)]
pub struct Cell {
    /// Base glyph character.
    pub ch: char,
    /// Additional grapheme characters stored with the base glyph.
    pub suffix: String,
    /// ResolvedStyle applied to the cell.
    pub style: ResolvedStyle,
    /// True when this cell continues a wide glyph from the previous column.
    pub continuation: bool,
}

impl PartialEq for Cell {
    /// Compare the cheap fields first, since a frame diff compares every cell.
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.ch == other.ch
            && self.continuation == other.continuation
            && self.style == other.style
            && (self.suffix.is_empty() && other.suffix.is_empty() || self.suffix == other.suffix)
    }
}

impl Cell {
    /// Construct a cell containing a single glyph.
    fn new(ch: char, style: ResolvedStyle) -> Self {
        Self {
            ch,
            suffix: String::new(),
            style,
            continuation: false,
        }
    }

    /// Construct a continuation cell for a wide glyph.
    fn continuation(style: ResolvedStyle) -> Self {
        Self {
            ch: NULL,
            suffix: String::new(),
            style,
            continuation: true,
        }
    }

    /// Return true when the cell is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.ch == NULL && self.suffix.is_empty() && !self.continuation
    }

    /// Return a display character for tests and debugging.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn display_char(&self) -> char {
        if self.is_empty() || self.continuation {
            NULL
        } else {
            self.ch
        }
    }

    /// Append this cell's renderable text to the output buffer.
    fn push_text(&self, out: &mut String) {
        if self.continuation {
            return;
        }
        if self.is_empty() {
            out.push(' ');
            return;
        }
        out.push(self.ch);
        out.push_str(&self.suffix);
    }

    /// Return the rendered terminal cell width.
    fn rendered_width(&self) -> usize {
        if self.continuation {
            return 0;
        }
        if self.is_empty() {
            return 1;
        }
        if self.suffix.is_empty() {
            if self.ch.is_ascii_graphic() || self.ch == ' ' {
                return 1;
            }
            let mut encoded = [0; 4];
            return text::grapheme_width(self.ch.encode_utf8(&mut encoded));
        }

        text::grapheme_width(&self.rendered_text())
    }

    /// Return this cell's rendered text.
    pub fn rendered_text(&self) -> String {
        let mut out = String::new();
        self.push_text(&mut out);
        out
    }
}

/// Reject characters that cannot be represented by a single canonical cell.
fn validate_cell_character(ch: char) -> Result<()> {
    if ch == NULL {
        return Ok(());
    }
    let mut encoded = [0; 4];
    let width = text::grapheme_width(ch.encode_utf8(&mut encoded));
    if width != 1 {
        return Err(Error::InvalidCellCharacter { ch, width });
    }
    Ok(())
}

/// A 2D terminal buffer of styled cells.
#[derive(Clone, Debug, PartialEq)]
pub struct TermBuf {
    /// Buffer size in cells.
    pub(crate) size: Size,
    /// Backing cell storage.
    pub(crate) cells: Vec<Cell>,
    /// Moving graphemes, keyed by base cell index.
    motion: BTreeMap<usize, motion::MotionRecord>,
}

impl TermBuf {
    /// Return the cells in row-major order.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Construct a buffer filled with the given character and style.
    pub fn new(size: impl Into<Size>, ch: char, style: ResolvedStyle) -> Result<Self> {
        Self::new_with_limits(size, ch, style, RenderLimits::default())
    }

    /// Construct a buffer with explicit visible render-target limits.
    pub(crate) fn new_with_limits(
        size: impl Into<Size>,
        ch: char,
        style: ResolvedStyle,
        limits: RenderLimits,
    ) -> Result<Self> {
        let size = size.into();
        validate_cell_character(ch)?;
        let cell = Cell::new(ch, style);
        let count = limits.cell_count(size)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| Error::RenderAllocation { cells: count })?;
        cells.resize(count, cell);
        Ok(Self {
            size,
            cells,
            motion: BTreeMap::new(),
        })
    }

    /// Construct a buffer as `new_with_limits` does, painting over `spare`
    /// when it has the same size, to save its allocation.
    pub(crate) fn reuse_with_limits(
        spare: Option<Self>,
        size: Size,
        ch: char,
        style: ResolvedStyle,
        limits: RenderLimits,
    ) -> Result<Self> {
        match spare {
            Some(mut buf) if buf.size == size => {
                validate_cell_character(ch)?;
                limits.cell_count(size)?;
                buf.cells.fill(Cell::new(ch, style));
                buf.motion.clear();
                Ok(buf)
            }
            _ => Self::new_with_limits(size, ch, style, limits),
        }
    }

    /// Return the buffer size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Return the buffer bounds as a rectangle.
    pub fn rect(&self) -> Rect {
        self.size.rect()
    }

    /// Convert a point into a cell index.
    fn idx(&self, p: Point) -> Option<usize> {
        if !self.rect().contains_point(p) {
            return None;
        }
        let width = usize::try_from(self.size.w).ok()?;
        let x = usize::try_from(p.x).ok()?;
        let y = usize::try_from(p.y).ok()?;
        y.checked_mul(width)?.checked_add(x)
    }

    /// Return the cells of row `y`.
    fn row(&self, y: u32) -> &[Cell] {
        let width = self.size.w as usize;
        &self.cells[y as usize * width..(y as usize + 1) * width]
    }

    /// Return the absolute cell range of the complete grapheme at a cell index.
    fn grapheme_span(&self, index: usize) -> Option<Range<usize>> {
        let width = usize::try_from(self.size.w).ok()?;
        if width == 0 || index >= self.cells.len() {
            return None;
        }
        let row_start = index / width * width;
        let row_end = row_start.saturating_add(width).min(self.cells.len());
        let (start, end) = grapheme_range(&self.cells[row_start..row_end], index - row_start, 1)?;
        Some(row_start + start..row_start + end)
    }

    /// Prepare a cell for writing, clearing overlapping wide glyphs and motion.
    fn clear_for_write(&mut self, index: usize, style: ResolvedStyle) {
        // A single-cell glyph will be replaced directly by the caller.
        // Only a wide glyph can leave cells outside the write behind.
        if !self.cells[index].continuation
            && !self
                .cells
                .get(index + 1)
                .is_some_and(|cell| cell.continuation)
        {
            self.clear_motion(index);
            return;
        }
        if let Some(range) = self.grapheme_span(index) {
            self.clear_motion(range.start);
            self.cells[range].fill(Cell::new(NULL, style));
        }
    }

    /// Prepare a destination range for writing, clearing overlapping wide
    /// glyphs.
    fn clear_graphemes(&mut self, start: usize, width: usize, style: ResolvedStyle) {
        for index in start..start.saturating_add(width) {
            self.clear_for_write(index, style);
        }
    }

    /// Write a single-cell character at a specific point.
    pub(crate) fn put(&mut self, p: Point, ch: char, style: ResolvedStyle) -> Result<()> {
        validate_cell_character(ch)?;
        let Some(index) = self.idx(p) else {
            return Ok(());
        };
        self.clear_for_write(index, style);
        self.cells[index] = Cell::new(ch, style);
        Ok(())
    }

    /// Write a grapheme cluster and return its terminal cell width.
    pub(crate) fn put_grapheme(
        &mut self,
        p: Point,
        grapheme: &str,
        style: ResolvedStyle,
    ) -> Result<usize> {
        let width = text::grapheme_width(grapheme);
        if width == 0 {
            return Ok(0);
        }
        // A control character stands alone in its grapheme. A line break
        // measures one column, but a terminal would move its cursor instead
        // of drawing it, so the column paints blank.
        let grapheme = if grapheme.chars().any(char::is_control) {
            " "
        } else {
            grapheme
        };
        let Some(index) = self.idx(p) else {
            return Ok(0);
        };
        let available = usize::try_from(self.size.w.saturating_sub(p.x)).unwrap_or(0);
        if width > available {
            return Ok(0);
        }
        self.clear_graphemes(index, width, style);
        let mut chars = grapheme.chars();
        let ch = chars.next().unwrap_or(' ');
        let suffix: String = chars.collect();
        self.cells[index] = Cell {
            ch,
            suffix,
            style,
            continuation: false,
        };
        for offset in 1..width {
            self.cells[index + offset] = Cell::continuation(style);
        }
        Ok(width)
    }

    /// Fill a rectangle with a glyph and style.
    pub fn fill(&mut self, style: &ResolvedStyle, r: Rect, ch: char) -> Result<()> {
        validate_cell_character(ch)?;
        let Some(isec) = self.rect().intersect(r) else {
            return Ok(());
        };
        let width = self.size.w as usize;
        let cell = Cell::new(ch, *style);
        for y in isec.tl.y..isec.tl.y.saturating_add(isec.h) {
            let start = y as usize * width + isec.tl.x as usize;
            let end = start + isec.w as usize;
            // Only a wide glyph across an edge reaches outside the row, and
            // the fill overwrites the rest.
            self.clear_for_write(start, *style);
            self.clear_for_write(end - 1, *style);
            self.clear_motion_range(start..end);
            self.cells[start..end].fill(cell.clone());
        }
        Ok(())
    }

    /// Fill a rectangle, resolving the style separately for each cell.
    pub(crate) fn fill_with(
        &mut self,
        r: Rect,
        ch: char,
        style_at: impl Fn(Point) -> ResolvedStyle,
    ) -> Result<()> {
        validate_cell_character(ch)?;
        if let Some(isec) = self.rect().intersect(r) {
            let end_y = isec.tl.y.saturating_add(isec.h);
            let end_x = isec.tl.x.saturating_add(isec.w);
            for y in isec.tl.y..end_y {
                let row_start = y as usize * self.size.w as usize;
                for x in isec.tl.x..end_x {
                    let point = Point { x, y };
                    let style = style_at(point);
                    let index = row_start + x as usize;
                    self.clear_for_write(index, style);
                    self.cells[index] = Cell::new(ch, style);
                }
            }
        }
        Ok(())
    }

    /// Restyle the complete grapheme at a cell without changing its text.
    pub(crate) fn restyle_grapheme(&mut self, location: Point, style: ResolvedStyle) {
        let Some(idx) = self.idx(location) else {
            return;
        };
        let Some(range) = self.grapheme_span(idx) else {
            return;
        };
        self.clear_motion(range.start);
        for cell in &mut self.cells[range] {
            cell.style = style;
        }
    }

    /// Draw text clipped to the given line.
    pub fn text(&mut self, style: &ResolvedStyle, l: Line, txt: &str) -> Result<()> {
        self.text_with(l, txt, |_| *style)
    }

    /// Write text along a line, resolving the style separately for each cell.
    ///
    /// The text is clipped to the line and padded with spaces to the line's
    /// width.
    pub(crate) fn text_with(
        &mut self,
        l: Line,
        txt: &str,
        style_at: impl Fn(Point) -> ResolvedStyle,
    ) -> Result<()> {
        let Some(isec) = self.rect().intersect(l.rect()) else {
            return Ok(());
        };
        let offset = isec.tl.x.saturating_sub(l.tl.x) as usize;
        let max = isec.w as usize;
        let (out, _) = text::slice_by_columns(txt, offset, max);
        let mut col = 0usize;
        let mut x = isec.tl.x;

        for grapheme in text::graphemes(out) {
            let width = text::grapheme_width(grapheme);
            if width == 0 {
                continue;
            }
            if col + width > max {
                break;
            }

            let point = Point { x, y: isec.tl.y };
            self.put_grapheme(point, grapheme, style_at(point))?;
            x = x.saturating_add(u32::try_from(width).unwrap_or(u32::MAX));
            col = col.saturating_add(width);
        }

        for i in col..max {
            let point = Point {
                x: isec
                    .tl
                    .x
                    .saturating_add(u32::try_from(i).unwrap_or(u32::MAX)),
                y: isec.tl.y,
            };
            self.put(point, ' ', style_at(point))?;
        }
        Ok(())
    }

    /// Get a cell by position.
    pub fn get(&self, p: Point) -> Option<&Cell> {
        self.idx(p).map(|i| &self.cells[i])
    }

    /// Validate the canonical base-plus-continuation cell representation.
    fn validate_canonical(&self) -> Result<()> {
        let width = usize::try_from(self.size.w)
            .map_err(|_| Error::Internal("terminal buffer width does not fit usize".into()))?;
        if width == 0 {
            return if self.cells.is_empty() {
                Ok(())
            } else {
                Err(Error::Internal(
                    "zero-width terminal buffer contains cells".into(),
                ))
            };
        }
        let mut rows = self.cells.chunks_exact(width);
        for (row_index, row) in rows.by_ref().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                if cell.continuation {
                    let valid_base =
                        x.checked_sub(1)
                            .and_then(|base| row.get(base))
                            .is_some_and(|base| {
                                !base.continuation
                                    && base.rendered_width() == 2
                                    && base.style == cell.style
                            });
                    if !valid_base {
                        return Err(Error::Internal(format!(
                            "orphan terminal continuation at ({x}, {row_index})"
                        )));
                    }
                    continue;
                }
                let rendered_width = cell.rendered_width();
                if rendered_width == 0 {
                    return Err(Error::Internal(format!(
                        "zero-width terminal base at ({x}, {row_index})"
                    )));
                }
                if rendered_width == 2 && !row.get(x + 1).is_some_and(|next| next.continuation) {
                    return Err(Error::Internal(format!(
                        "wide terminal base lacks continuation at ({x}, {row_index})"
                    )));
                }
            }
        }
        if !rows.remainder().is_empty() {
            return Err(Error::Internal(
                "terminal buffer cell count is not divisible by its width".into(),
            ));
        }
        Ok(())
    }

    /// Return the rendered screen as rows of cell strings.
    pub fn rows(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::with_capacity(self.size.h as usize);
        for y in 0..self.size.h {
            let mut row = Vec::with_capacity(self.size.w as usize);
            for x in 0..self.size.w {
                let cell = self
                    .get(Point { x, y })
                    .expect("buffer coordinates should always be valid");
                row.push(cell.rendered_text());
            }
            rows.push(row);
        }
        rows
    }

    /// Return the rendered screen as newline-joined plain text.
    pub fn screen_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.size.h {
            if y > 0 {
                out.push('\n');
            }
            for x in 0..self.size.w {
                let cell = self
                    .get(Point { x, y })
                    .expect("buffer coordinates should always be valid");
                cell.push_text(&mut out);
            }
        }
        out
    }

    /// Diff this terminal buffer against a previous state, emitting changes
    /// to the provided render backend. The caller flushes the backend.
    pub fn emit_diff<R: RenderBackend>(&self, prev: &Self, backend: &mut R) -> Result<()> {
        prev.validate_canonical()?;
        self.emit_diff_after(prev, backend)
    }

    /// Diff this buffer against `prev`, a buffer that an earlier emission
    /// validated, so only this buffer needs validation.
    pub(crate) fn emit_diff_after<R: RenderBackend>(
        &self,
        prev: &Self,
        backend: &mut R,
    ) -> Result<()> {
        self.validate_canonical()?;
        if self.size != prev.size {
            return self.emit(backend);
        }
        // Whether each row differs from the row the terminal shows there.
        let mut dirty: Vec<bool> = (0..self.size.h)
            .map(|y| self.row(y) != prev.row(y))
            .collect();
        if !dirty.contains(&true) {
            return Ok(());
        }
        let scroll = backend
            .supports_line_shift()
            .then(|| detect_scroll(self, prev, &dirty))
            .flatten();
        let scroll = match scroll {
            Some((scroll, reused)) => {
                backend.shift_lines(scroll.top, scroll.bottom, scroll.shift)?;
                for y in scroll.top..=scroll.bottom {
                    dirty[y as usize] = true;
                }
                for y in reused {
                    dirty[y as usize] = false;
                }
                Some(scroll)
            }
            None => None,
        };
        let width = self.size.w as usize;
        let can_shift = backend.supports_char_shift();
        let mut out = Emission::default();
        for y in 0..self.size.h {
            let row_start = y as usize * width;
            let row_end = row_start + width;
            let current_row = &self.cells[row_start..row_end];
            let source = scroll.map_or(Some(y), |scroll| scroll.source(y));
            let Some(source) = source else {
                // The scroll exposed the row, so all of it repaints.
                render_line_range(backend, current_row, y, 0, width, &mut out)?;
                continue;
            };
            if !dirty[y as usize] {
                continue;
            }
            let prev_row = prev.row(source);

            if can_shift
                && let Some(shift) = detect_line_shift(current_row, prev_row, MAX_LINE_SHIFT)
            {
                // `detect_line_shift` only reports shifts inside the row width.
                let gap = shift.unsigned_abs() as usize;
                backend.shift_chars(Point { x: 0, y }, shift)?;
                if shift > 0 {
                    render_line_range(backend, current_row, y, 0, gap, &mut out)?;
                } else {
                    let start = width.saturating_sub(gap);
                    render_line_range(backend, current_row, y, start, gap, &mut out)?;
                }
                continue;
            }

            let mut x = 0usize;
            while x < width {
                if current_row[x] == prev_row[x] {
                    x += 1;
                    continue;
                }

                let start_x = grapheme_start(current_row, x);
                let style = &current_row[start_x].style;
                let mut end_x = x;
                while end_x < width {
                    let cell = &current_row[end_x];
                    if cell == &prev_row[end_x] || cell.style != *style {
                        break;
                    }
                    end_x += 1;
                }
                end_x = grapheme_end(current_row, end_x);

                if end_x <= start_x {
                    x += 1;
                    continue;
                }

                render_line_range(backend, current_row, y, start_x, end_x - start_x, &mut out)?;
                x = end_x;
            }
        }
        Ok(())
    }

    /// Emit this terminal buffer in full to the provided backend,
    /// batching runs of text with the same style. The caller flushes the
    /// backend.
    pub fn emit<R: RenderBackend>(&self, backend: &mut R) -> Result<()> {
        self.validate_canonical()?;
        let width = self.size.w as usize;
        let mut out = Emission::default();
        for y in 0..self.size.h {
            let row_start = y as usize * width;
            let row_end = row_start + width;
            let row = &self.cells[row_start..row_end];
            render_line_range(backend, row, y, 0, width, &mut out)?;
        }
        Ok(())
    }
}

/// Return the first cell of the grapheme containing the provided cell index.
fn grapheme_start(row: &[Cell], mut x: usize) -> usize {
    while x > 0 && row[x].continuation {
        x -= 1;
    }
    x
}

/// Return the exclusive end of a range expanded to include trailing
/// continuations.
fn grapheme_end(row: &[Cell], mut x: usize) -> usize {
    x = x.min(row.len());
    while x < row.len() && row[x].continuation {
        x += 1;
    }
    x
}

/// Return a cell range expanded to whole graphemes.
fn grapheme_range(row: &[Cell], start: usize, len: usize) -> Option<(usize, usize)> {
    if len == 0 || start >= row.len() {
        return None;
    }

    let end = start.saturating_add(len).min(row.len());
    let start = grapheme_start(row, start);
    let end = grapheme_end(row, end);
    Some((start, end))
}

/// Check whether two lines are identical up to a horizontal shift.
fn detect_line_shift(current: &[Cell], prev: &[Cell], max_shift: usize) -> Option<i32> {
    let width = current.len();
    if width == 0 || width != prev.len() {
        return None;
    }

    let max = max_shift.min(width.saturating_sub(1));
    if max == 0 {
        return None;
    }

    for shift in 1..=max {
        if line_matches_shift(current, prev, shift as i32) {
            return Some(shift as i32);
        }
        if line_matches_shift(current, prev, -(shift as i32)) {
            return Some(-(shift as i32));
        }
    }
    None
}

/// Rows `top..=bottom` that the terminal scrolls by `shift` rows, positive
/// moving them down, before a diff repaints what still differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scroll {
    /// First row of the region.
    top: u32,
    /// Last row of the region.
    bottom: u32,
    /// Rows the region moves, positive moving them down.
    shift: i32,
}

impl Scroll {
    /// Return the previous row that terminal row `y` shows after the scroll:
    /// `None` for a row the scroll exposes, and `y` outside the region.
    fn source(self, y: u32) -> Option<u32> {
        if !(self.top..=self.bottom).contains(&y) {
            return Some(y);
        }
        let source = i64::from(y) - i64::from(self.shift);
        (i64::from(self.top)..=i64::from(self.bottom))
            .contains(&source)
            .then_some(source as u32)
    }
}

/// Find the scroll that lets the most changed rows show a previous row
/// whole, if it saves more rows than it costs, and the rows it lets show
/// their previous row.
///
/// A scroll costs the rows it exposes, and the unchanged rows inside its
/// region, which it moves away.
fn detect_scroll(
    current: &TermBuf,
    prev: &TermBuf,
    changed: &[bool],
) -> Option<(Scroll, Vec<u32>)> {
    let height = current.size.h;
    let mut best: Option<(usize, Scroll, Vec<u32>)> = None;
    for magnitude in 1..=MAX_ROW_SHIFT as i32 {
        for shift in [magnitude, -magnitude] {
            let reused: Vec<u32> = (0..height)
                .filter(|&y| {
                    let source = i64::from(y) - i64::from(shift);
                    changed[y as usize]
                        && (0..i64::from(height)).contains(&source)
                        && current.row(y) == prev.row(source as u32)
                })
                .collect();
            let (Some(&first), Some(&last)) = (reused.first(), reused.last()) else {
                continue;
            };
            let (top, bottom) = if shift > 0 {
                (first - shift.unsigned_abs(), last)
            } else {
                (first, last + shift.unsigned_abs())
            };
            let moved = (top..=bottom).filter(|&y| !changed[y as usize]).count();
            let cost = moved + shift.unsigned_abs() as usize;
            let saved = reused.len().saturating_sub(cost);
            if saved > best.as_ref().map_or(0, |(saved, ..)| *saved) {
                best = Some((saved, Scroll { top, bottom, shift }, reused));
            }
        }
    }
    best.map(|(_, scroll, reused)| (scroll, reused))
}

/// Determine whether the current line matches the previous line shifted by
/// `shift`.
fn line_matches_shift(current: &[Cell], prev: &[Cell], shift: i32) -> bool {
    let width = current.len();
    if width == 0 || width != prev.len() || shift == 0 {
        return false;
    }

    if shift > 0 {
        let shift = shift as usize;
        if shift >= width {
            return false;
        }
        current[shift..] == prev[..width - shift]
    } else {
        let shift = (-shift) as usize;
        if shift >= width {
            return false;
        }
        current[..width - shift] == prev[shift..]
    }
}

/// The state of one emission: the text of the run it is writing, and the
/// style the backend last took.
#[derive(Default)]
struct Emission {
    /// Text of the current run.
    text: String,
    /// Style the backend last took, which later runs in it need not repeat.
    style: Option<ResolvedStyle>,
}

/// Render a slice of a single line using style runs from the current buffer.
fn render_line_range<R: RenderBackend>(
    backend: &mut R,
    row: &[Cell],
    y: u32,
    start: usize,
    len: usize,
    out: &mut Emission,
) -> Result<()> {
    let Some((start, end)) = grapheme_range(row, start, len) else {
        return Ok(());
    };

    let mut x = start;
    while x < end {
        x = render_styled_cells(backend, row, y, x, end, out)?;
    }
    Ok(())
}

/// Render cells that share a style, splitting after wide graphemes.
fn render_styled_cells<R: RenderBackend>(
    backend: &mut R,
    row: &[Cell],
    y: u32,
    start: usize,
    end: usize,
    out: &mut Emission,
) -> Result<usize> {
    let style = &row[start].style;
    let text = &mut out.text;
    text.clear();
    let mut x = start;
    let mut split_after_wide = false;

    while x < end {
        let cell = &row[x];
        if split_after_wide {
            if cell.continuation {
                x += 1;
                continue;
            }
            break;
        }
        if cell.style != *style {
            break;
        }

        let width = cell.rendered_width();
        cell.push_text(text);
        split_after_wide = width > 1;
        x += 1;
    }

    if out.style != Some(*style) {
        backend.style(style)?;
        out.style = Some(*style);
    }
    let text = &out.text;
    if text.is_empty() {
        backend.text(Point { x: start as u32, y }, " ")?;
        return Ok(x.max(start + 1));
    }

    backend.text(Point { x: start as u32, y }, text)?;
    Ok(x)
}

/// Tests for the terminal buffer.
#[cfg(test)]
mod tests;
