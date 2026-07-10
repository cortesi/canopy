use std::mem;

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    core::text,
    cursor,
    error::{Error, Result},
    geom::{FrameRects, Line, Point, Rect, Size},
    render::RenderBackend,
    style::{Attr, AttrSet, Color, ResolvedStyle},
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
    /// Create an empty TermBuf filled with NULL characters.
    pub fn empty_with_style(size: impl Into<Size>, style: ResolvedStyle) -> Result<Self> {
        Self::empty_with_style_and_limits(size, style, RenderLimits::default())
    }

    /// Create an empty buffer with explicit visible render-target limits.
    pub fn empty_with_style_and_limits(
        size: impl Into<Size>,
        style: ResolvedStyle,
        limits: RenderLimits,
    ) -> Result<Self> {
        let size = size.into();
        let cell = Cell::empty(style);
        let count = limits.cell_count(size)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| Error::RenderAllocation { cells: count })?;
        cells.resize(count, cell);
        Ok(Self { size, cells })
    }

    /// Create an empty TermBuf filled with NULL characters.
    pub fn empty(size: impl Into<Size>) -> Result<Self> {
        let default_style = ResolvedStyle {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        };
        Self::empty_with_style(size, default_style)
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
        validate_cell_character(ch)?;
        if let Some(isec) = self.rect().intersect(&r) {
            let end_y = isec.tl.y.saturating_add(isec.h);
            let end_x = isec.tl.x.saturating_add(isec.w);
            for y in isec.tl.y..end_y {
                for x in isec.tl.x..end_x {
                    self.put(Point { x, y }, ch, *style)?;
                }
            }
        }
        Ok(())
    }

    /// Fill all empty cells with the given character and style.
    pub fn fill_empty(&mut self, ch: char, style: &ResolvedStyle) -> Result<()> {
        validate_cell_character(ch)?;
        for i in 0..self.cells.len() {
            if self.cells[i].is_empty() {
                self.cells[i] = if ch == NULL {
                    Cell::empty(*style)
                } else {
                    Cell::new(ch, *style)
                };
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

    /// Fill the frame outline with a glyph and style.
    pub fn solid_frame(&mut self, style: &ResolvedStyle, f: FrameRects, ch: char) -> Result<()> {
        self.fill(style, f.top, ch)?;
        self.fill(style, f.left, ch)?;
        self.fill(style, f.right, ch)?;
        self.fill(style, f.bottom, ch)?;
        self.fill(style, f.topleft, ch)?;
        self.fill(style, f.topright, ch)?;
        self.fill(style, f.bottomleft, ch)?;
        self.fill(style, f.bottomright, ch)
    }

    /// Draw text clipped to the given line.
    pub fn text(&mut self, style: &ResolvedStyle, l: Line, txt: &str) -> Result<()> {
        if let Some(isec) = self.rect().intersect(&l.rect()) {
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

                self.put_grapheme(Point { x, y: isec.tl.y }, grapheme, *style)?;
                x = x.saturating_add(u32::try_from(width).unwrap_or(u32::MAX));
                col = col.saturating_add(width);
            }

            for i in col..max {
                self.put(
                    Point {
                        x: isec
                            .tl
                            .x
                            .saturating_add(u32::try_from(i).unwrap_or(u32::MAX)),
                        y: isec.tl.y,
                    },
                    ' ',
                    *style,
                )?;
            }
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
        if backend.supports_line_shift()
            && let Some(shift) = detect_row_shift(self, prev, MAX_ROW_SHIFT)
        {
            let last_row = self.size.h.saturating_sub(1);
            backend.shift_lines(0, last_row, shift)?;
            let width = self.size.w as usize;
            let count = shift.unsigned_abs();
            if shift > 0 {
                for y in 0..count {
                    let row_start = y as usize * width;
                    let row_end = row_start + width;
                    let row = &self.cells[row_start..row_end];
                    render_line_range(backend, row, y, 0, width)?;
                }
            } else if shift < 0 {
                let start = self.size.h.saturating_sub(count);
                for y in start..self.size.h {
                    let row_start = y as usize * width;
                    let row_end = row_start + width;
                    let row = &self.cells[row_start..row_end];
                    render_line_range(backend, row, y, 0, width)?;
                }
            }
            backend.flush()?;
            return Ok(());
        }
        if backend.supports_line_shift()
            && let Some((rect, shift)) = detect_inner_shift(self, prev, MAX_ROW_SHIFT)
        {
            let top = rect.tl.y;
            let bottom = rect.tl.y + rect.h - 1;
            backend.shift_lines(top, bottom, shift)?;
            let width = self.size.w as usize;
            let count = shift.unsigned_abs();
            let start_x = rect.tl.x as usize;
            let len = rect.w as usize;
            if shift > 0 {
                for y in rect.tl.y..rect.tl.y + count {
                    let row_start = y as usize * width;
                    let row_end = row_start + width;
                    let row = &self.cells[row_start..row_end];
                    render_line_range(backend, row, y, start_x, len)?;
                }
            } else if shift < 0 {
                let start = rect.tl.y + rect.h - count;
                for y in start..rect.tl.y + rect.h {
                    let row_start = y as usize * width;
                    let row_end = row_start + width;
                    let row = &self.cells[row_start..row_end];
                    render_line_range(backend, row, y, start_x, len)?;
                }
            }
            backend.flush()?;
            return Ok(());
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

/// Check whether two buffers are identical up to a vertical shift.
fn detect_row_shift(current: &TermBuf, prev: &TermBuf, max_shift: usize) -> Option<i32> {
    let height = current.size.h as i32;
    if height == 0 || height != prev.size.h as i32 {
        return None;
    }

    let max = max_shift.min(height.saturating_sub(2) as usize);
    if max == 0 {
        return None;
    }

    for shift in 1..=max {
        let shift = shift as i32;
        if buffer_matches_shift(current, prev, shift) {
            return Some(shift);
        }
        if buffer_matches_shift(current, prev, -shift) {
            return Some(-shift);
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

/// Determine whether two buffers match for a given vertical shift.
fn buffer_matches_shift(current: &TermBuf, prev: &TermBuf, shift: i32) -> bool {
    let height = current.size.h as i32;
    let width = current.size.w as usize;
    if shift == 0 || shift.unsigned_abs() as i32 >= height {
        return false;
    }

    if shift > 0 {
        for y in shift..height {
            let row = y as usize * width;
            let prev_row = (y - shift) as usize * width;
            if current.cells[row..row + width] != prev.cells[prev_row..prev_row + width] {
                return false;
            }
        }
    } else {
        let limit = height + shift;
        for y in 0..limit {
            let row = y as usize * width;
            let prev_row = (y - shift) as usize * width;
            if current.cells[row..row + width] != prev.cells[prev_row..prev_row + width] {
                return false;
            }
        }
    }
    true
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

#[cfg(test)]
mod tests {
    use proptest::{prelude::*, test_runner::TestCaseResult};
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;

    use super::*;
    use crate::{
        backend::crossterm::CrosstermRender,
        buf,
        core::{testing::model::trace_result, text::grapheme_width},
        geom::Line,
        style::{AttrSet, Color, PartialStyle},
        testing::buf::BufTest,
    };

    fn def_style() -> ResolvedStyle {
        ResolvedStyle {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        }
    }

    fn buf_from_rows(rows: &[&str]) -> TermBuf {
        let height = rows.len() as u32;
        let width = rows.first().map(|row| row.len()).unwrap_or(0) as u32;
        let style = def_style();
        let mut tb = TermBuf::new(Size::new(width, height), ' ', style)
            .expect("test render target should allocate");
        for (y, row) in rows.iter().enumerate() {
            tb.text(&style, Line::new(0, y as u32, width), row)
                .expect("test buffer mutation should succeed");
        }
        tb
    }

    #[test]
    fn basic_fill() {
        let mut tb = TermBuf::new(Size::new(4, 2), ' ', def_style())
            .expect("test render target should allocate");
        tb.fill(&def_style(), Rect::new(1, 0, 2, 2), 'x')
            .expect("test buffer mutation should succeed");

        BufTest::new(&tb).assert_matches(buf![
            " xx "
            " xx "
        ]);
    }

    #[test]
    fn allocation_limits_are_checked_before_reservation() {
        let style = def_style();
        assert!(matches!(
            TermBuf::new_with_limits(Size::new(5, 1), ' ', style, RenderLimits::new(4, 4, 16),),
            Err(Error::RenderWidthLimit { .. })
        ));
        assert!(matches!(
            TermBuf::new_with_limits(Size::new(2, 5), ' ', style, RenderLimits::new(5, 4, 20),),
            Err(Error::RenderHeightLimit { .. })
        ));
        assert!(matches!(
            TermBuf::new_with_limits(Size::new(4, 4), ' ', style, RenderLimits::new(4, 4, 15),),
            Err(Error::RenderCellLimit { .. })
        ));
        assert!(matches!(
            TermBuf::new_with_limits(
                Size::new(u32::MAX, u32::MAX),
                ' ',
                style,
                RenderLimits::new(u32::MAX, u32::MAX, usize::MAX),
            ),
            Err(Error::RenderAllocation { .. } | Error::RenderCellCountOverflow { .. })
        ));
    }

    #[test]
    fn single_cell_apis_reject_non_cell_characters() {
        let style = def_style();
        assert!(matches!(
            TermBuf::new(Size::new(1, 1), '界', style),
            Err(Error::InvalidCellCharacter { width: 2, .. })
        ));
        let mut buf = TermBuf::empty(Size::new(3, 3)).expect("test render target should allocate");
        assert!(matches!(
            buf.fill(&style, Rect::new(0, 0, 1, 1), '界'),
            Err(Error::InvalidCellCharacter { width: 2, .. })
        ));
        assert!(matches!(
            buf.fill_empty('\u{0301}', &style),
            Err(Error::InvalidCellCharacter { width: 0, .. })
        ));
        assert!(matches!(
            buf.solid_frame(&style, FrameRects::new(buf.rect(), 1), '界'),
            Err(Error::InvalidCellCharacter { width: 2, .. })
        ));
    }

    #[test]
    fn grapheme_replacement_clears_every_touched_grapheme() -> Result<()> {
        let style = def_style();
        let mut buf = TermBuf::new(Size::new(4, 1), '.', style)?;
        assert_eq!(buf.put_grapheme(Point { x: 1, y: 0 }, "界", style)?, 2);
        buf.put(Point { x: 2, y: 0 }, 'x', style)?;
        assert!(buf.get(Point { x: 1, y: 0 }).is_some_and(Cell::is_empty));
        assert_eq!(buf.get(Point { x: 2, y: 0 }).map(|cell| cell.ch), Some('x'));

        buf.put(Point { x: 1, y: 0 }, 'a', style)?;
        buf.put(Point { x: 2, y: 0 }, 'b', style)?;
        buf.put_grapheme(Point { x: 1, y: 0 }, "界", style)?;
        assert_eq!(
            buf.get(Point { x: 1, y: 0 }).map(|cell| cell.ch),
            Some('界')
        );
        assert!(
            buf.get(Point { x: 2, y: 0 })
                .is_some_and(|cell| cell.continuation)
        );
        buf.validate_canonical()
    }

    #[test]
    fn zero_width_and_right_clipped_graphemes_are_no_ops() -> Result<()> {
        let style = def_style();
        let mut buf = TermBuf::new(Size::new(2, 1), '.', style)?;
        assert_eq!(
            buf.put_grapheme(Point { x: 0, y: 0 }, "\u{0301}", style)?,
            0
        );
        assert_eq!(buf.screen_text(), "..");
        assert_eq!(buf.put_grapheme(Point { x: 1, y: 0 }, "界", style)?, 0);
        assert_eq!(buf.screen_text(), "..");
        buf.validate_canonical()
    }

    #[test]
    fn cursor_overlay_styles_complete_graphemes() -> Result<()> {
        let style = def_style();
        let mut buf = TermBuf::empty(Size::new(2, 1))?;
        buf.put_grapheme(Point::zero(), "界", style)?;
        buf.overlay_cursor(Point { x: 1, y: 0 }, cursor::CursorShape::Block);

        let base = buf.get(Point::zero()).expect("missing wide base");
        let continuation = buf.get(Point { x: 1, y: 0 }).expect("missing continuation");
        assert_eq!(base.ch, '界');
        assert!(continuation.continuation);
        assert_eq!(base.style, continuation.style);
        assert_eq!(base.style.fg, style.bg);
        assert_eq!(base.style.bg, style.fg);
        buf.validate_canonical()
    }

    #[test]
    fn rendering_rejects_noncanonical_buffers() -> Result<()> {
        let mut buf = TermBuf::empty(Size::new(1, 1))?;
        buf.cells[0] = Cell::continuation(def_style());
        let mut backend = RecBackend::new();
        assert!(matches!(buf.render(&mut backend), Err(Error::Invariant(_))));

        let mut ragged = TermBuf::empty(Size::new(2, 1))?;
        ragged.cells.pop();
        assert!(matches!(
            ragged.render(&mut backend),
            Err(Error::Invariant(_))
        ));
        Ok(())
    }

    #[test]
    fn canonical_buffer_renders_through_crossterm_backend() -> Result<()> {
        let style = def_style();
        let mut buf = TermBuf::new(Size::new(4, 1), ' ', style)?;
        buf.text(&style, Line::new(0, 0, 4), "a界")?;
        buf.overlay_cursor(Point { x: 2, y: 0 }, cursor::CursorShape::Underscore);
        buf.render(&mut CrosstermRender::default())
    }

    #[test]
    fn text_write() {
        let mut tb = TermBuf::new(Size::new(5, 1), ' ', def_style())
            .expect("test render target should allocate");
        tb.text(&def_style(), Line::new(0, 0, 5), "hi")
            .expect("test buffer mutation should succeed");

        BufTest::new(&tb).assert_matches(buf!["hi   "]);
    }

    #[test]
    fn text_handles_combining_and_wide_graphemes() {
        let style = def_style();
        let mut tb =
            TermBuf::new(Size::new(12, 1), ' ', style).expect("test render target should allocate");
        tb.text(&style, Line::new(0, 0, 12), "A\u{0301}界👩‍💻B")
            .expect("test buffer mutation should succeed");

        let first = tb.get(Point { x: 0, y: 0 }).expect("missing cell");
        assert!(
            first.suffix.contains('\u{0301}'),
            "expected combining mark stored with base glyph"
        );

        for x in 0..tb.size().w {
            let cell = tb.get(Point { x, y: 0 }).expect("missing cell");
            if cell.continuation || cell.is_empty() {
                continue;
            }
            let mut glyph = String::new();
            glyph.push(cell.ch);
            glyph.push_str(&cell.suffix);
            let width = grapheme_width(&glyph);
            if width == 2 {
                let next = tb
                    .get(Point { x: x + 1, y: 0 })
                    .expect("missing continuation cell");
                assert!(
                    next.continuation,
                    "expected continuation after wide glyph at column {x}"
                );
            }
        }
    }

    #[test]
    fn solid_frame_draw() {
        let mut tb = TermBuf::new(Size::new(4, 4), ' ', def_style())
            .expect("test render target should allocate");
        let f = FrameRects::new(Rect::new(0, 0, 4, 4), 1);
        tb.solid_frame(&def_style(), f, '#')
            .expect("test buffer mutation should succeed");

        BufTest::new(&tb).assert_matches(buf![
            "####"
            "#  #"
            "#  #"
            "####"
        ]);
    }

    struct RecBackend {
        ops: Vec<String>,
    }

    impl RecBackend {
        fn new() -> Self {
            Self { ops: Vec::new() }
        }
    }

    impl RenderBackend for RecBackend {
        fn style(&mut self, s: &ResolvedStyle) -> Result<()> {
            self.ops.push(format!("style {s:?}"));
            Ok(())
        }

        fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
            self.ops.push(format!("text {} {} {}", loc.x, loc.y, txt));
            Ok(())
        }

        fn supports_char_shift(&self) -> bool {
            false
        }

        fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            Ok(())
        }
    }

    struct ShiftBackend {
        shift: Option<i32>,
        text_ops: usize,
    }

    impl ShiftBackend {
        fn new() -> Self {
            Self {
                shift: None,
                text_ops: 0,
            }
        }
    }

    impl RenderBackend for ShiftBackend {
        fn style(&mut self, _s: &ResolvedStyle) -> Result<()> {
            Ok(())
        }

        fn text(&mut self, _loc: Point, _txt: &str) -> Result<()> {
            self.text_ops += 1;
            Ok(())
        }

        fn supports_char_shift(&self) -> bool {
            false
        }

        fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
            Ok(())
        }

        fn supports_line_shift(&self) -> bool {
            true
        }

        fn shift_lines(&mut self, _top: u32, _bottom: u32, count: i32) -> Result<()> {
            self.shift = Some(count);
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RegionShiftBackend {
        shift: Option<(u32, u32, i32)>,
        text_ops: usize,
    }

    impl RenderBackend for RegionShiftBackend {
        fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
            Ok(())
        }

        fn text(&mut self, _loc: Point, _txt: &str) -> Result<()> {
            self.text_ops += 1;
            Ok(())
        }

        fn supports_char_shift(&self) -> bool {
            false
        }

        fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
            Ok(())
        }

        fn supports_line_shift(&self) -> bool {
            true
        }

        fn shift_lines(&mut self, top: u32, bottom: u32, count: i32) -> Result<()> {
            self.shift = Some((top, bottom, count));
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            Ok(())
        }
    }

    struct ReplayBackend {
        size: Size,
        rows: Vec<Vec<char>>,
        char_shift: bool,
        line_shift: bool,
        wide_as_narrow: bool,
    }

    impl ReplayBackend {
        fn blank(size: Size) -> Self {
            Self {
                size,
                rows: vec![vec![' '; size.w as usize]; size.h as usize],
                char_shift: true,
                line_shift: true,
                wide_as_narrow: false,
            }
        }

        fn blank_with_narrow_wide(size: Size) -> Self {
            Self {
                wide_as_narrow: true,
                ..Self::blank(size)
            }
        }

        fn screen_text(&self) -> String {
            self.rows
                .iter()
                .map(|row| row.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    impl RenderBackend for ReplayBackend {
        fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
            Ok(())
        }

        fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
            let y = loc.y as usize;
            if y >= self.rows.len() {
                return Ok(());
            }
            let mut x = loc.x as usize;
            for grapheme in txt.graphemes(true) {
                let width = if self.wide_as_narrow {
                    1
                } else {
                    grapheme_width(grapheme)
                };
                if x < self.rows[y].len() {
                    let ch = grapheme.chars().next().unwrap_or(' ');
                    self.rows[y][x] = ch;
                }
                x = x.saturating_add(width);
            }
            Ok(())
        }

        fn supports_char_shift(&self) -> bool {
            self.char_shift
        }

        fn shift_chars(&mut self, loc: Point, count: i32) -> Result<()> {
            let y = loc.y as usize;
            let start = loc.x as usize;
            if y >= self.rows.len() || start >= self.rows[y].len() || count == 0 {
                return Ok(());
            }

            let width = self.rows[y].len();
            if count > 0 {
                let count = count as usize;
                for x in (start..width).rev() {
                    self.rows[y][x] = x
                        .checked_sub(count)
                        .filter(|source| *source >= start)
                        .map_or(' ', |source| self.rows[y][source]);
                }
            } else {
                let count = (-count) as usize;
                for x in start..width {
                    let source = x.saturating_add(count);
                    self.rows[y][x] = if source < width {
                        self.rows[y][source]
                    } else {
                        ' '
                    };
                }
            }
            Ok(())
        }

        fn supports_line_shift(&self) -> bool {
            self.line_shift
        }

        fn shift_lines(&mut self, top: u32, bottom: u32, count: i32) -> Result<()> {
            let top = top as usize;
            let bottom = bottom.min(self.size.h.saturating_sub(1)) as usize;
            if top > bottom || count == 0 {
                return Ok(());
            }
            let original = self.rows.clone();
            if count > 0 {
                let count = count as usize;
                for y in (top..=bottom).rev() {
                    self.rows[y] = y
                        .checked_sub(count)
                        .filter(|source| *source >= top)
                        .map_or(vec![' '; self.size.w as usize], |source| {
                            original[source].clone()
                        });
                }
            } else {
                let count = (-count) as usize;
                for y in top..=bottom {
                    let source = y.saturating_add(count);
                    self.rows[y] = if source <= bottom {
                        original[source].clone()
                    } else {
                        vec![' '; self.size.w as usize]
                    };
                }
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            for row in &mut self.rows {
                row.fill(' ');
            }
            Ok(())
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct ModelCell {
        grapheme: Option<String>,
        style: ResolvedStyle,
        continuation: bool,
    }

    impl ModelCell {
        fn space(style: ResolvedStyle) -> Self {
            Self {
                grapheme: Some(" ".into()),
                style,
                continuation: false,
            }
        }

        fn empty(style: ResolvedStyle) -> Self {
            Self {
                grapheme: None,
                style,
                continuation: false,
            }
        }

        fn displayed(&self) -> &str {
            if self.continuation {
                ""
            } else {
                self.grapheme.as_deref().unwrap_or(" ")
            }
        }
    }

    #[derive(Clone, Debug)]
    struct ModelBuffer {
        size: Size,
        cells: Vec<ModelCell>,
    }

    impl ModelBuffer {
        fn new(size: Size, style: ResolvedStyle) -> Self {
            let count = usize::try_from(size.w)
                .ok()
                .and_then(|width| {
                    usize::try_from(size.h)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .expect("generated model dimensions should fit");
            Self {
                size,
                cells: vec![ModelCell::space(style); count],
            }
        }

        fn width(grapheme: &str) -> usize {
            UnicodeWidthStr::width(grapheme).min(2)
        }

        fn index(&self, point: Point) -> Option<usize> {
            if point.x >= self.size.w || point.y >= self.size.h {
                return None;
            }
            let width = usize::try_from(self.size.w).ok()?;
            usize::try_from(point.y)
                .ok()?
                .checked_mul(width)?
                .checked_add(usize::try_from(point.x).ok()?)
        }

        fn grapheme_range(&self, index: usize) -> (usize, usize) {
            let width = usize::try_from(self.size.w).expect("generated width should fit");
            let row_start = index / width * width;
            let row_end = row_start.saturating_add(width).min(self.cells.len());
            let mut start = index;
            while start > row_start && self.cells[start].continuation {
                start -= 1;
            }
            let mut end = start.saturating_add(1);
            while end < row_end && self.cells[end].continuation {
                end += 1;
            }
            (start, end)
        }

        fn clear_at(&mut self, index: usize, style: ResolvedStyle) {
            let (start, end) = self.grapheme_range(index);
            self.cells[start..end].fill(ModelCell::empty(style));
        }

        fn put_grapheme(&mut self, point: Point, grapheme: &str, style: ResolvedStyle) {
            let width = Self::width(grapheme);
            if width == 0 {
                return;
            }
            let Some(index) = self.index(point) else {
                return;
            };
            let available = usize::try_from(self.size.w.saturating_sub(point.x)).unwrap_or(0);
            if width > available {
                return;
            }
            for offset in 0..width {
                self.clear_at(index + offset, style);
            }
            self.cells[index] = ModelCell {
                grapheme: Some(grapheme.into()),
                style,
                continuation: false,
            };
            for offset in 1..width {
                self.cells[index + offset] = ModelCell {
                    grapheme: None,
                    style,
                    continuation: true,
                };
            }
        }

        fn fill(&mut self, rect: Rect, ch: char, style: ResolvedStyle) {
            let Some(rect) = self.size.rect().intersect(&rect) else {
                return;
            };
            for y in rect.tl.y..rect.tl.y.saturating_add(rect.h) {
                for x in rect.tl.x..rect.tl.x.saturating_add(rect.w) {
                    self.put_grapheme(Point { x, y }, &ch.to_string(), style);
                }
            }
        }

        fn text(&mut self, line: Line, text: &str, style: ResolvedStyle) {
            let Some(line) = self.size.rect().intersect(&line.rect()) else {
                return;
            };
            let mut x = line.tl.x;
            let mut used = 0usize;
            let available = usize::try_from(line.w).unwrap_or(usize::MAX);
            for grapheme in text.graphemes(true) {
                let width = Self::width(grapheme);
                if width == 0 {
                    continue;
                }
                if used.saturating_add(width) > available {
                    break;
                }
                self.put_grapheme(Point { x, y: line.tl.y }, grapheme, style);
                x = x.saturating_add(u32::try_from(width).unwrap_or(u32::MAX));
                used = used.saturating_add(width);
            }
            for offset in used..usize::try_from(line.w).unwrap_or(usize::MAX) {
                self.put_grapheme(
                    Point {
                        x: line
                            .tl
                            .x
                            .saturating_add(u32::try_from(offset).unwrap_or(u32::MAX)),
                        y: line.tl.y,
                    },
                    " ",
                    style,
                );
            }
        }

        fn overlay_cursor(&mut self, point: Point, shape: cursor::CursorShape) {
            let Some(index) = self.index(point) else {
                return;
            };
            if self.cells[index].grapheme.is_none() && !self.cells[index].continuation {
                self.cells[index].grapheme = Some(" ".into());
            }
            let (start, end) = self.grapheme_range(index);
            for cell in &mut self.cells[start..end] {
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

        fn assert_matches(&self, actual: &TermBuf) -> TestCaseResult {
            prop_assert_eq!(actual.size(), self.size);
            prop_assert_eq!(actual.cells.len(), self.cells.len());
            for (actual, expected) in actual.cells.iter().zip(&self.cells) {
                prop_assert_eq!(actual.rendered_text(), expected.displayed());
                prop_assert_eq!(actual.style, expected.style);
                prop_assert_eq!(actual.continuation, expected.continuation);
            }
            Ok(())
        }
    }

    struct ModelBackend {
        model: ModelBuffer,
        style: ResolvedStyle,
    }

    impl RenderBackend for ModelBackend {
        fn style(&mut self, style: &ResolvedStyle) -> Result<()> {
            self.style = *style;
            Ok(())
        }

        fn text(&mut self, location: Point, text: &str) -> Result<()> {
            let mut x = location.x;
            for grapheme in text.graphemes(true) {
                let width = ModelBuffer::width(grapheme);
                self.model
                    .put_grapheme(Point { x, y: location.y }, grapheme, self.style);
                x = x.saturating_add(u32::try_from(width).unwrap_or(u32::MAX));
            }
            Ok(())
        }

        fn supports_char_shift(&self) -> bool {
            false
        }

        fn shift_chars(&mut self, _location: Point, _count: i32) -> Result<()> {
            Err(Error::Invariant(
                "model backend does not support character shifts".into(),
            ))
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            self.model = ModelBuffer::new(self.model.size, self.style);
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    enum BufferOperation {
        Grapheme {
            x: u32,
            y: u32,
            grapheme: &'static str,
            alternate_style: bool,
        },
        Fill {
            rect: Rect,
            ch: char,
            alternate_style: bool,
        },
        Text {
            line: Line,
            text: &'static str,
            alternate_style: bool,
        },
        Cursor {
            x: u32,
            y: u32,
            shape: cursor::CursorShape,
        },
        Resize {
            width: u32,
            height: u32,
        },
    }

    fn buffer_operation_strategy() -> impl Strategy<Value = BufferOperation> {
        prop_oneof![
            (
                0u32..8,
                0u32..5,
                prop::sample::select(vec!["a", "界", "👩‍💻", "A\u{0301}", "\u{0301}", " "]),
                any::<bool>(),
            )
                .prop_map(|(x, y, grapheme, alternate_style)| {
                    BufferOperation::Grapheme {
                        x,
                        y,
                        grapheme,
                        alternate_style,
                    }
                }),
            (
                0u32..8,
                0u32..5,
                0u32..8,
                0u32..5,
                prop::sample::select(vec![' ', '.', 'x']),
                any::<bool>(),
            )
                .prop_map(|(x, y, width, height, ch, alternate_style)| {
                    BufferOperation::Fill {
                        rect: Rect::new(x, y, width, height),
                        ch,
                        alternate_style,
                    }
                }),
            (
                0u32..8,
                0u32..5,
                0u32..8,
                prop::sample::select(vec!["", "a界", "👩‍💻b", "A\u{0301}", "界界", "\u{0301}a"]),
                any::<bool>(),
            )
                .prop_map(|(x, y, width, text, alternate_style)| {
                    BufferOperation::Text {
                        line: Line::new(x, y, width),
                        text,
                        alternate_style,
                    }
                }),
            (
                0u32..8,
                0u32..5,
                prop::sample::select(vec![
                    cursor::CursorShape::Block,
                    cursor::CursorShape::Line,
                    cursor::CursorShape::Underscore,
                ]),
            )
                .prop_map(|(x, y, shape)| BufferOperation::Cursor { x, y, shape }),
            (0u32..7, 0u32..5)
                .prop_map(|(width, height)| BufferOperation::Resize { width, height }),
        ]
    }

    fn apply_buffer_operation(
        actual: &mut TermBuf,
        model: &mut ModelBuffer,
        operation: &BufferOperation,
        styles: [ResolvedStyle; 2],
    ) -> Result<()> {
        let style_for = |alternate| styles[usize::from(alternate)];
        match operation {
            BufferOperation::Grapheme {
                x,
                y,
                grapheme,
                alternate_style,
            } => {
                let style = style_for(*alternate_style);
                actual.put_grapheme(Point { x: *x, y: *y }, grapheme, style)?;
                model.put_grapheme(Point { x: *x, y: *y }, grapheme, style);
            }
            BufferOperation::Fill {
                rect,
                ch,
                alternate_style,
            } => {
                let style = style_for(*alternate_style);
                actual.fill(&style, *rect, *ch)?;
                model.fill(*rect, *ch, style);
            }
            BufferOperation::Text {
                line,
                text,
                alternate_style,
            } => {
                let style = style_for(*alternate_style);
                actual.text(&style, *line, text)?;
                model.text(*line, text, style);
            }
            BufferOperation::Cursor { x, y, shape } => {
                let point = Point { x: *x, y: *y };
                actual.overlay_cursor(point, *shape);
                model.overlay_cursor(point, *shape);
            }
            BufferOperation::Resize { width, height } => {
                let size = Size::new(*width, *height);
                *actual = TermBuf::new(size, ' ', styles[0])?;
                *model = ModelBuffer::new(size, styles[0]);
            }
        }
        Ok(())
    }

    #[test]
    fn diff_no_change() {
        let style = def_style();
        let tb1 =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        let tb2 =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        let mut be = RecBackend::new();
        tb2.diff(&tb1, &mut be).unwrap();
        assert!(be.ops.is_empty());
    }

    proptest! {
        #[test]
        fn generated_grapheme_operations_remain_canonical_and_replayable(
            operations in prop::collection::vec(buffer_operation_strategy(), 0..48),
        ) {
            let base_style = def_style();
            let mut alternate_style = base_style;
            alternate_style.fg = Color::Red;
            alternate_style.bg = Color::Blue;
            let styles = [base_style, alternate_style];
            let initial_size = Size::new(4, 2);
            let mut actual = TermBuf::new(initial_size, ' ', base_style)?;
            let mut model = ModelBuffer::new(initial_size, base_style);

            for (index, operation) in operations.iter().enumerate() {
                let result = (|| {
                    let previous_actual = actual.clone();
                    let previous_model = model.clone();
                    apply_buffer_operation(&mut actual, &mut model, operation, styles)?;
                    actual.validate_canonical()?;
                    model.assert_matches(&actual)?;

                    let replay_model = if previous_actual.size() == actual.size() {
                        previous_model
                    } else {
                        ModelBuffer::new(actual.size(), base_style)
                    };
                    let mut backend = ModelBackend {
                        model: replay_model,
                        style: base_style,
                    };
                    actual.diff(&previous_actual, &mut backend)?;
                    backend.model.assert_matches(&actual)
                })();
                trace_result(result, &operations, index)?;
            }
        }
    }

    #[test]
    fn diff_vertical_shift_uses_scroll() {
        let prev = buf_from_rows(&["aaa", "bbb", "ccc"]);
        let cur = buf_from_rows(&["xxx", "aaa", "bbb"]);
        let mut be = ShiftBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.shift, Some(1));
        assert_eq!(be.text_ops, 1);
    }

    #[test]
    fn diff_vertical_shift_uses_scroll_interior() {
        let prev = buf_from_rows(&["#####", "#abc#", "#def#", "#ghi#", "#####"]);
        let cur = buf_from_rows(&["#####", "#xxx#", "#abc#", "#def#", "#####"]);
        let mut be = RegionShiftBackend::default();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.shift, Some((1, 3, 1)));
        assert_eq!(be.text_ops, 1);
    }

    #[test]
    fn diff_single_run() {
        let style = def_style();
        let prev =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        let mut cur =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        cur.text(&style, Line::new(0, 0, 3), "ab")
            .expect("test buffer mutation should succeed");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.ops.len(), 2);
        assert_eq!(be.ops[0], format!("style {style:?}"));
        assert_eq!(be.ops[1], "text 0 0 ab");
    }

    #[test]
    fn diff_style_changes() {
        let style1 = def_style();
        let mut style2 = style1;
        style2.fg = Color::Red;

        let prev =
            TermBuf::new(Size::new(2, 1), ' ', style1).expect("test render target should allocate");
        let mut cur =
            TermBuf::new(Size::new(2, 1), ' ', style1).expect("test render target should allocate");
        cur.fill(&style2, Rect::new(0, 0, 1, 1), 'a')
            .expect("test buffer mutation should succeed");
        cur.fill(&style1, Rect::new(1, 0, 1, 1), 'b')
            .expect("test buffer mutation should succeed");

        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();

        assert_eq!(be.ops.len(), 4);
        assert_eq!(be.ops[0], format!("style {style2:?}"));
        assert_eq!(be.ops[1], "text 0 0 a");
        assert_eq!(be.ops[2], format!("style {style1:?}"));
        assert_eq!(be.ops[3], "text 1 0 b");
    }

    #[test]
    fn diff_multi_line() {
        let style = def_style();
        let prev =
            TermBuf::new(Size::new(3, 2), ' ', style).expect("test render target should allocate");
        let mut cur =
            TermBuf::new(Size::new(3, 2), ' ', style).expect("test render target should allocate");
        cur.fill(&style, Rect::new(0, 1, 2, 1), 'x')
            .expect("test buffer mutation should succeed");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.ops.len(), 2);
        assert_eq!(be.ops[0], format!("style {style:?}"));
        assert_eq!(be.ops[1], "text 0 1 xx");
    }

    #[test]
    fn render_whole_buffer() {
        let style = def_style();
        let mut tb =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        tb.text(&style, Line::new(0, 0, 3), "ab")
            .expect("test buffer mutation should succeed");
        let mut be = RecBackend::new();
        tb.render(&mut be).unwrap();
        assert_eq!(
            be.ops,
            vec![format!("style {style:?}"), "text 0 0 ab ".to_string(),]
        );
    }

    #[test]
    fn render_repositions_after_wide_graphemes() {
        let style = def_style();
        let mut tb =
            TermBuf::new(Size::new(8, 1), ' ', style).expect("test render target should allocate");
        tb.text(&style, Line::new(0, 0, 7), "a界bc")
            .expect("test buffer mutation should succeed");
        tb.fill(&style, Rect::new(7, 0, 1, 1), '|')
            .expect("test buffer mutation should succeed");

        let mut backend = ReplayBackend::blank_with_narrow_wide(Size::new(8, 1));
        tb.render(&mut backend).unwrap();

        assert_eq!(backend.screen_text(), "a界 bc  |");
    }

    #[test]
    fn text_overwrites_stale_wide_continuation_cells() {
        let style = def_style();
        let mut tb =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        tb.text(&style, Line::new(0, 0, 3), "界a")
            .expect("test buffer mutation should succeed");
        BufTest::new(&tb).assert_matches(buf!["界Xa"]);

        tb.text(&style, Line::new(0, 0, 3), "b")
            .expect("test buffer mutation should succeed");
        BufTest::new(&tb).assert_matches(buf!["b  "]);
    }

    #[test]
    fn text_clips_wide_grapheme_without_partial_cell() {
        let style = def_style();
        let mut tb = TermBuf::empty(Size::new(1, 1)).expect("test render target should allocate");
        tb.text(&style, Line::new(0, 0, 1), "界")
            .expect("test buffer mutation should succeed");

        let cell = tb.get(Point { x: 0, y: 0 }).expect("missing cell");
        assert_eq!(cell.ch, ' ');
        assert!(!cell.continuation);
    }

    #[test]
    fn diff_size_change_rerender() {
        let style = def_style();
        let prev =
            TermBuf::new(Size::new(2, 1), ' ', style).expect("test render target should allocate");
        let mut cur =
            TermBuf::new(Size::new(3, 1), ' ', style).expect("test render target should allocate");
        cur.text(&style, Line::new(0, 0, 3), "abc")
            .expect("test buffer mutation should succeed");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(
            be.ops,
            vec![format!("style {style:?}"), "text 0 0 abc".to_string(),]
        );
    }

    #[test]
    fn contains_text() {
        let mut tb = TermBuf::new(Size::new(10, 3), ' ', def_style())
            .expect("test render target should allocate");
        tb.text(&def_style(), Line::new(0, 0, 10), "hello")
            .expect("test buffer mutation should succeed");
        tb.text(&def_style(), Line::new(0, 1, 10), "world")
            .expect("test buffer mutation should succeed");

        let bt = BufTest::new(&tb);
        assert!(bt.contains_text("hello"));
        assert!(bt.contains_text("world"));
        assert!(!bt.contains_text("goodbye"));
    }

    #[test]
    fn contains_text_style() {
        let mut tb = TermBuf::new(Size::new(10, 3), ' ', def_style())
            .expect("test render target should allocate");

        // Add text with different styles
        let mut red_style = def_style();
        red_style.fg = Color::Red;

        let mut blue_style = def_style();
        blue_style.fg = Color::Blue;

        tb.text(&red_style, Line::new(0, 0, 5), "hello")
            .expect("test buffer mutation should succeed");
        tb.text(&blue_style, Line::new(5, 0, 5), "world")
            .expect("test buffer mutation should succeed");
        tb.text(&def_style(), Line::new(0, 1, 10), "test line")
            .expect("test buffer mutation should succeed");

        // Test with foreground color partial style
        assert!(BufTest::new(&tb).contains_text_style("hello", &PartialStyle::fg(Color::Red)));
        assert!(!BufTest::new(&tb).contains_text_style("world", &PartialStyle::fg(Color::Red)));

        assert!(BufTest::new(&tb).contains_text_style("world", &PartialStyle::fg(Color::Blue)));
        assert!(!BufTest::new(&tb).contains_text_style("hello", &PartialStyle::fg(Color::Blue)));

        // Test with empty partial style (matches any style)
        let partial_any = PartialStyle::default();
        assert!(BufTest::new(&tb).contains_text_style("hello", &partial_any));
        assert!(BufTest::new(&tb).contains_text_style("world", &partial_any));
        assert!(BufTest::new(&tb).contains_text_style("test", &partial_any));

        // Test with multiple style attributes
        let partial_white_bg = PartialStyle::fg(Color::White).with_bg(Color::Black);
        assert!(BufTest::new(&tb).contains_text_style("test", &partial_white_bg));
    }

    #[test]
    fn contains_text_fg_compat() {
        use crate::style::solarized;
        let mut tb = TermBuf::new(Size::new(10, 1), ' ', def_style())
            .expect("test render target should allocate");

        let mut blue_style = def_style();
        blue_style.fg = solarized::BLUE;

        tb.text(&blue_style, Line::new(0, 0, 3), "two")
            .expect("test buffer mutation should succeed");

        // Test the old method
        assert!(BufTest::new(&tb).contains_text_fg("two", solarized::BLUE));

        // Test that it works the same as contains_text_style
        assert!(BufTest::new(&tb).contains_text_style("two", &PartialStyle::fg(solarized::BLUE)));
    }

    #[test]
    fn empty_constructor_uses_canonical_empty_cells() {
        let empty = TermBuf::empty(Size::new(5, 3)).expect("test render target should allocate");
        assert_eq!(empty.size(), Size::new(5, 3));
        BufTest::new(&empty).assert_matches(buf![
            "XXXXX"
            "XXXXX"
            "XXXXX"
        ]);
    }

    #[test]
    fn contains_text_style_builders() {
        use crate::style::Attr;
        let mut tb = TermBuf::new(Size::new(10, 2), ' ', def_style())
            .expect("test render target should allocate");

        // Create styles with different attributes
        let mut bold_red = def_style();
        bold_red.fg = Color::Red;
        bold_red.attrs = AttrSet::new(Attr::Bold);

        let mut italic_blue = def_style();
        italic_blue.fg = Color::Blue;
        italic_blue.attrs = AttrSet::new(Attr::Italic);

        tb.text(&bold_red, Line::new(0, 0, 4), "bold")
            .expect("test buffer mutation should succeed");
        tb.text(&italic_blue, Line::new(0, 1, 6), "italic")
            .expect("test buffer mutation should succeed");

        // Test using builder methods
        assert!(BufTest::new(&tb).contains_text_style("bold", &PartialStyle::fg(Color::Red)));
        assert!(BufTest::new(&tb).contains_text_style("italic", &PartialStyle::fg(Color::Blue)));

        // Test with attributes
        assert!(
            BufTest::new(&tb)
                .contains_text_style("bold", &PartialStyle::attrs(AttrSet::new(Attr::Bold)))
        );
        assert!(
            BufTest::new(&tb)
                .contains_text_style("italic", &PartialStyle::attrs(AttrSet::new(Attr::Italic)))
        );

        // Test chaining
        let bold_red_style = PartialStyle::fg(Color::Red).with_attrs(AttrSet::new(Attr::Bold));
        assert!(BufTest::new(&tb).contains_text_style("bold", &bold_red_style));

        // Test that it doesn't match wrong combinations
        let italic_red = PartialStyle::fg(Color::Red).with_attrs(AttrSet::new(Attr::Italic));
        assert!(!BufTest::new(&tb).contains_text_style("bold", &italic_red));
    }

    #[test]
    fn test_fill_empty() {
        // Create an empty buffer
        let mut tb = TermBuf::empty(Size::new(5, 3)).expect("test render target should allocate");

        // Verify all cells are NULL initially using buf macro
        BufTest::new(&tb).assert_matches(buf![
            "XXXXX"
            "XXXXX"
            "XXXXX"
        ]);

        // Add some content to part of the buffer
        tb.text(&def_style(), Line::new(1, 1, 3), "ABC")
            .expect("test buffer mutation should succeed");

        // Verify the content before fill_empty
        BufTest::new(&tb).assert_matches(buf![
            "XXXXX"
            "XABCX"
            "XXXXX"
        ]);

        // Fill empty cells with a specific character and style
        let mut fill_style = def_style();
        fill_style.fg = Color::Red;
        tb.fill_empty('.', &fill_style)
            .expect("test buffer mutation should succeed");

        // Check that the buffer now has dots where there were NULLs
        BufTest::new(&tb).assert_matches(buf![
            "....."
            ".ABC."
            "....."
        ]);

        // Verify specific style properties
        assert_eq!(tb.get(Point { x: 0, y: 0 }).unwrap().style.fg, Color::Red);
        assert_eq!(tb.get(Point { x: 1, y: 1 }).unwrap().style.fg, Color::White);
    }
}
