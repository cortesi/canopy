use std::mem;

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    core::text,
    cursor,
    error::{Error, Result},
    geom::{Line, Point, Rect, Size},
    render::RenderBackend,
    style::{Attr, ResolvedStyle},
};

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
    pub(crate) fn cell_count(self, size: Size) -> Result<usize> {
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
#[derive(Clone, Debug, PartialEq)]
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

    /// Construct an empty cell.
    fn empty(style: ResolvedStyle) -> Self {
        Self {
            ch: NULL,
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
#[derive(Clone, Debug)]
pub struct TermBuf {
    /// Buffer size in cells.
    pub(crate) size: Size,
    /// Backing cell storage.
    pub(crate) cells: Vec<Cell>,
}

impl TermBuf {
    /// Construct a buffer filled with the given character and style.
    pub fn new(size: impl Into<Size>, ch: char, style: ResolvedStyle) -> Result<Self> {
        Self::new_with_limits(size, ch, style, RenderLimits::default())
    }

    /// Construct a buffer with explicit visible render-target limits.
    pub fn new_with_limits(
        size: impl Into<Size>,
        ch: char,
        style: ResolvedStyle,
        limits: RenderLimits,
    ) -> Result<Self> {
        let size = size.into();
        validate_cell_character(ch)?;
        let cell = if ch == NULL {
            Cell::empty(style)
        } else {
            Cell::new(ch, style)
        };
        let count = limits.cell_count(size)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| Error::RenderAllocation { cells: count })?;
        cells.resize(count, cell);
        Ok(Self { size, cells })
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

    /// Clear the complete grapheme occupying one cell index.
    fn clear_grapheme_at(&mut self, index: usize, style: ResolvedStyle) {
        let Ok(width) = usize::try_from(self.size.w) else {
            return;
        };
        if width == 0 || index >= self.cells.len() {
            return;
        }
        let row_start = index / width * width;
        let row_end = row_start.saturating_add(width).min(self.cells.len());
        let x = index - row_start;
        let range = grapheme_range(&self.cells[row_start..row_end], x, 1);
        if let Some((start, end)) = range {
            self.cells[row_start + start..row_start + end].fill(Cell::empty(style));
        }
    }

    /// Clear every complete grapheme touched by a destination cell range.
    fn clear_graphemes(&mut self, start: usize, width: usize, style: ResolvedStyle) {
        for index in start..start.saturating_add(width) {
            self.clear_grapheme_at(index, style);
        }
    }

    /// Write a single-cell character at a specific point.
    pub(crate) fn put(&mut self, p: Point, ch: char, style: ResolvedStyle) -> Result<()> {
        validate_cell_character(ch)?;
        let Some(index) = self.idx(p) else {
            return Ok(());
        };
        self.clear_grapheme_at(index, style);
        self.cells[index] = if ch == NULL {
            Cell::empty(style)
        } else {
            Cell::new(ch, style)
        };
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
        self.fill_with(r, ch, |_| *style)
    }

    /// Fill a rectangle, resolving the style separately for each cell.
    pub fn fill_with(
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
                for x in isec.tl.x..end_x {
                    let point = Point { x, y };
                    self.put(point, ch, style_at(point))?;
                }
            }
        }
        Ok(())
    }

    /// Overlay a cursor on a cell by adjusting its style.
    pub fn overlay_cursor(&mut self, location: Point, shape: cursor::CursorShape) {
        let Some(idx) = self.idx(location) else {
            return;
        };
        if self.cells[idx].is_empty() {
            self.cells[idx] = Cell::new(' ', self.cells[idx].style);
        }
        let Ok(width) = usize::try_from(self.size.w) else {
            return;
        };
        let row_start = idx / width * width;
        let row_end = row_start.saturating_add(width).min(self.cells.len());
        let Some((start, end)) =
            grapheme_range(&self.cells[row_start..row_end], idx - row_start, 1)
        else {
            return;
        };
        for cell in &mut self.cells[row_start + start..row_start + end] {
            match shape {
                cursor::CursorShape::Underscore => {
                    cell.style.attrs = cell.style.attrs.with(Attr::Underline);
                }
                cursor::CursorShape::Block | cursor::CursorShape::Line => {
                    mem::swap(&mut cell.style.fg, &mut cell.style.bg);
                }
            }
        }
    }

    /// Draw text clipped to the given line.
    pub fn text(&mut self, style: &ResolvedStyle, l: Line, txt: &str) -> Result<()> {
        self.text_with(l, txt, |_| *style)
    }

    /// Write text along a line, resolving the style separately for each cell.
    ///
    /// The text is clipped to the line and padded with spaces to the line's width.
    pub fn text_with(
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

        for grapheme in out.graphemes(true) {
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
            .map_err(|_| Error::Invariant("terminal buffer width does not fit usize".into()))?;
        if width == 0 {
            return if self.cells.is_empty() {
                Ok(())
            } else {
                Err(Error::Invariant(
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
                        return Err(Error::Invariant(format!(
                            "orphan terminal continuation at ({x}, {row_index})"
                        )));
                    }
                    continue;
                }
                let rendered_width = cell.rendered_width();
                if rendered_width == 0 {
                    return Err(Error::Invariant(format!(
                        "zero-width terminal base at ({x}, {row_index})"
                    )));
                }
                if rendered_width == 2 && !row.get(x + 1).is_some_and(|next| next.continuation) {
                    return Err(Error::Invariant(format!(
                        "wide terminal base lacks continuation at ({x}, {row_index})"
                    )));
                }
            }
        }
        if !rows.remainder().is_empty() {
            return Err(Error::Invariant(
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
        self.rows()
            .into_iter()
            .map(|row| row.concat())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Diff this terminal buffer against a previous state, emitting changes
    /// to the provided render backend.
    pub fn diff<R: RenderBackend>(&self, prev: &Self, backend: &mut R) -> Result<()> {
        self.validate_canonical()?;
        prev.validate_canonical()?;
        let mut wrote = false;
        if self.size != prev.size {
            return self.render(backend);
        }
        if backend.supports_line_shift() {
            let full = self.rect();
            if let Some(shift) = detect_row_shift_in_rect(self, prev, full, MAX_ROW_SHIFT) {
                return render_shifted_rect(self, backend, full, shift);
            }
            if let Some((rect, shift)) = detect_inner_shift(self, prev, MAX_ROW_SHIFT) {
                return render_shifted_rect(self, backend, rect, shift);
            }
        }
        let width = self.size.w as usize;
        let can_shift = backend.supports_char_shift();
        for y in 0..self.size.h {
            let row_start = y as usize * width;
            let row_end = row_start + width;
            let current_row = &self.cells[row_start..row_end];
            let prev_row = &prev.cells[row_start..row_end];

            if current_row == prev_row {
                continue;
            }

            if can_shift
                && let Some(shift) = detect_line_shift(current_row, prev_row, MAX_LINE_SHIFT)
            {
                let gap = if shift > 0 {
                    shift as usize
                } else {
                    (-shift) as usize
                };
                if gap > 0 && gap < width {
                    backend.shift_chars(Point { x: 0, y }, shift)?;
                    if shift > 0 {
                        render_line_range(backend, current_row, y, 0, gap)?;
                    } else {
                        let start = width.saturating_sub(gap);
                        render_line_range(backend, current_row, y, start, gap)?;
                    }
                    wrote = true;
                    continue;
                }
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

                render_line_range(backend, current_row, y, start_x, end_x - start_x)?;
                wrote = true;
                x = end_x;
            }
        }
        if wrote {
            backend.flush()?;
        }
        Ok(())
    }

    /// Render this terminal buffer in full using the provided backend,
    /// batching runs of text with the same style.
    pub fn render<R: RenderBackend>(&self, backend: &mut R) -> Result<()> {
        self.validate_canonical()?;
        let mut wrote = false;
        let width = self.size.w as usize;
        for y in 0..self.size.h {
            let row_start = y as usize * width;
            let row_end = row_start + width;
            let row = &self.cells[row_start..row_end];
            render_line_range(backend, row, y, 0, width)?;
            wrote = true;
        }
        if wrote {
            backend.flush()?;
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

/// Return the exclusive end of a range expanded to include trailing continuations.
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

/// Check whether two buffers have matching borders and a shifted interior.
fn detect_inner_shift(current: &TermBuf, prev: &TermBuf, max_shift: usize) -> Option<(Rect, i32)> {
    if current.size != prev.size {
        return None;
    }
    if current.size.w <= 2 || current.size.h <= 2 {
        return None;
    }
    if !borders_match(current, prev) {
        return None;
    }

    let rect = Rect::new(1, 1, current.size.w - 2, current.size.h - 2);
    let shift = detect_row_shift_in_rect(current, prev, rect, max_shift)?;
    Some((rect, shift))
}

/// Check that the outer border cells match between buffers.
fn borders_match(current: &TermBuf, prev: &TermBuf) -> bool {
    if current.size != prev.size {
        return false;
    }

    let width = current.size.w as usize;
    let height = current.size.h as usize;
    if width == 0 || height == 0 {
        return false;
    }

    let top = &current.cells[..width];
    let prev_top = &prev.cells[..width];
    if top != prev_top {
        return false;
    }

    if height > 1 {
        let bottom_start = (height - 1) * width;
        let bottom = &current.cells[bottom_start..bottom_start + width];
        let prev_bottom = &prev.cells[bottom_start..bottom_start + width];
        if bottom != prev_bottom {
            return false;
        }
    }

    if width > 1 && height > 2 {
        for row in 1..height - 1 {
            let row_start = row * width;
            let row_end = row_start + width - 1;
            if current.cells[row_start] != prev.cells[row_start]
                || current.cells[row_end] != prev.cells[row_end]
            {
                return false;
            }
        }
    }

    true
}

/// Check whether two buffers are identical up to a vertical shift within a rect.
fn detect_row_shift_in_rect(
    current: &TermBuf,
    prev: &TermBuf,
    rect: Rect,
    max_shift: usize,
) -> Option<i32> {
    let height = rect.h as i32;
    if rect.w == 0 || rect.h == 0 {
        return None;
    }

    let max = max_shift.min(height.saturating_sub(2) as usize);
    if max == 0 {
        return None;
    }

    for shift in 1..=max {
        let shift = shift as i32;
        if buffer_matches_shift_in_rect(current, prev, rect, shift) {
            return Some(shift);
        }
        if buffer_matches_shift_in_rect(current, prev, rect, -shift) {
            return Some(-shift);
        }
    }
    None
}

/// Shift the rows of `rect` on the backend and repaint the rows the shift exposed.
fn render_shifted_rect<R: RenderBackend>(
    buf: &TermBuf,
    backend: &mut R,
    rect: Rect,
    shift: i32,
) -> Result<()> {
    backend.shift_lines(rect.tl.y, rect.tl.y + rect.h - 1, shift)?;
    let width = buf.size.w as usize;
    let count = shift.unsigned_abs();
    let exposed = if shift > 0 {
        rect.tl.y..rect.tl.y + count
    } else {
        rect.tl.y + rect.h - count..rect.tl.y + rect.h
    };
    for y in exposed {
        let row_start = y as usize * width;
        let row = &buf.cells[row_start..row_start + width];
        render_line_range(backend, row, y, rect.tl.x as usize, rect.w as usize)?;
    }
    backend.flush()
}

/// Determine whether two buffers match for a given vertical shift within a rect.
fn buffer_matches_shift_in_rect(current: &TermBuf, prev: &TermBuf, rect: Rect, shift: i32) -> bool {
    if shift == 0 {
        return false;
    }

    let height = rect.h as i32;
    if shift.unsigned_abs() as i32 >= height {
        return false;
    }

    let width = current.size.w as usize;
    let rect_x = rect.tl.x as usize;
    let rect_w = rect.w as usize;
    if rect_x + rect_w > width {
        return false;
    }

    let rect_top = rect.tl.y as i32;
    let rect_bottom = rect_top + rect.h as i32;

    if shift > 0 {
        for y in rect_top + shift..rect_bottom {
            let row = y as usize * width;
            let prev_row = (y - shift) as usize * width;
            let cur_slice = &current.cells[row + rect_x..row + rect_x + rect_w];
            let prev_slice = &prev.cells[prev_row + rect_x..prev_row + rect_x + rect_w];
            if cur_slice != prev_slice {
                return false;
            }
        }
    } else {
        let limit = rect_bottom + shift;
        for y in rect_top..limit {
            let row = y as usize * width;
            let prev_row = (y - shift) as usize * width;
            let cur_slice = &current.cells[row + rect_x..row + rect_x + rect_w];
            let prev_slice = &prev.cells[prev_row + rect_x..prev_row + rect_x + rect_w];
            if cur_slice != prev_slice {
                return false;
            }
        }
    }
    true
}

/// Determine whether the current line matches the previous line shifted by `shift`.
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

/// Render a slice of a single line using style runs from the current buffer.
fn render_line_range<R: RenderBackend>(
    backend: &mut R,
    row: &[Cell],
    y: u32,
    start: usize,
    len: usize,
) -> Result<()> {
    let Some((start, end)) = grapheme_range(row, start, len) else {
        return Ok(());
    };

    let mut x = start;
    while x < end {
        x = render_styled_cells(backend, row, y, x, end)?;
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
) -> Result<usize> {
    let style = &row[start].style;
    let mut text = String::new();
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
        cell.push_text(&mut text);
        split_after_wide = width > 1;
        x += 1;
    }

    backend.style(style)?;
    if text.is_empty() {
        backend.text(Point { x: start as u32, y }, " ")?;
        return Ok(x.max(start + 1));
    }

    backend.text(Point { x: start as u32, y }, &text)?;
    Ok(x)
}

/// Tests for the terminal buffer.
#[cfg(test)]
mod tests;
