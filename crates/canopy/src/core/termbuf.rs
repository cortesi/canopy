use std::mem;

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    core::text,
    cursor,
    error::Result,
    geom::{Expanse, Frame, Line, Point, Rect},
    render::RenderBackend,
    style::{Attr, AttrSet, Color, Style},
};

/// NULL character constant.
const NULL: char = '\0';

/// Maximum per-line shift to consider when diffing.
const MAX_LINE_SHIFT: usize = 8;
/// Maximum per-row shift to consider when diffing.
const MAX_ROW_SHIFT: usize = 4;

/// A terminal cell with glyph and style.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Base glyph character.
    pub ch: char,
    /// Additional grapheme characters stored with the base glyph.
    pub suffix: String,
    /// Style applied to the cell.
    pub style: Style,
    /// True when this cell continues a wide glyph from the previous column.
    pub continuation: bool,
}

impl Cell {
    /// Construct a cell containing a single glyph.
    fn new(ch: char, style: Style) -> Self {
        Self {
            ch,
            suffix: String::new(),
            style,
            continuation: false,
        }
    }

    /// Construct an empty cell.
    fn empty(style: Style) -> Self {
        Self {
            ch: NULL,
            suffix: String::new(),
            style,
            continuation: false,
        }
    }

    /// Construct a continuation cell for a wide glyph.
    fn continuation(style: Style) -> Self {
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
}

/// A 2D terminal buffer of styled cells.
#[derive(Clone, Debug)]
pub struct TermBuf {
    /// Buffer size in cells.
    pub(crate) size: Expanse,
    /// Backing cell storage.
    pub(crate) cells: Vec<Cell>,
}

impl TermBuf {
    /// Construct a buffer filled with the given character and style.
    pub fn new(size: impl Into<Expanse>, ch: char, style: Style) -> Self {
        let size = size.into();
        let cell = Cell::new(ch, style);
        Self {
            size,
            cells: vec![cell; size.area() as usize],
        }
    }
    /// Create an empty TermBuf filled with NULL characters.
    pub fn empty_with_style(size: impl Into<Expanse>, style: Style) -> Self {
        let size = size.into();
        let cell = Cell::empty(style);
        Self {
            size,
            cells: vec![cell; size.area() as usize],
        }
    }

    /// Create an empty TermBuf filled with NULL characters.
    pub fn empty(size: impl Into<Expanse>) -> Self {
        let default_style = Style {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        };
        Self::empty_with_style(size, default_style)
    }

    /// Copy non-empty cells from a rectangle of another TermBuf into this one
    pub fn copy(&mut self, src: &Self, rect: Rect) {
        if src.size != self.size {
            return;
        }

        // Intersect the rectangle with our bounds
        if let Some(isec) = self.rect().intersect(&rect) {
            for y in isec.tl.y..isec.tl.y + isec.h {
                for x in isec.tl.x..isec.tl.x + isec.w {
                    let p = Point { x, y };
                    if let Some(cell) = src.get(p)
                        && !cell.is_empty()
                        && let Some(i) = self.idx(p)
                    {
                        self.cells[i] = cell.clone();
                    }
                }
            }
        }
    }

    /// Copy non-empty cells from a source TermBuf into a destination rectangle
    pub fn copy_to_rect(&mut self, src: &Self, dest_rect: Rect) {
        // The source buffer represents content to be placed at dest_rect
        // We need to map from source coordinates to destination coordinates

        // Intersect the destination rectangle with our bounds
        if let Some(clipped_dest) = self.rect().intersect(&dest_rect) {
            // Calculate the offset into the source buffer based on clipping
            let src_offset_x = (clipped_dest.tl.x - dest_rect.tl.x) as i32;
            let src_offset_y = (clipped_dest.tl.y - dest_rect.tl.y) as i32;

            // Copy the visible portion
            for dy in 0..clipped_dest.h {
                for dx in 0..clipped_dest.w {
                    let src_x = (dx as i32 + src_offset_x) as u32;
                    let src_y = (dy as i32 + src_offset_y) as u32;
                    let src_p = Point { x: src_x, y: src_y };

                    if let Some(cell) = src.get(src_p)
                        && !cell.is_empty()
                    {
                        let dest_p = Point {
                            x: clipped_dest.tl.x + dx,
                            y: clipped_dest.tl.y + dy,
                        };
                        if let Some(i) = self.idx(dest_p) {
                            self.cells[i] = cell.clone();
                        }
                    }
                }
            }
        }
    }

    /// Return the buffer size.
    pub fn size(&self) -> Expanse {
        self.size
    }

    /// Return the buffer bounds as a rectangle.
    pub fn rect(&self) -> Rect {
        self.size.rect()
    }

    /// Convert a point into a cell index.
    fn idx(&self, p: Point) -> Option<usize> {
        if self.rect().contains_point(p) {
            Some(p.y as usize * self.size.w as usize + p.x as usize)
        } else {
            None
        }
    }

    /// Write a cell at a specific point.
    pub(crate) fn put(&mut self, p: Point, ch: char, style: Style) {
        if let Some(i) = self.idx(p) {
            self.cells[i] = Cell::new(ch, style);
        }
    }

    /// Write a grapheme cluster at a specific point.
    fn put_grapheme(&mut self, p: Point, grapheme: &str, style: Style) {
        if let Some(i) = self.idx(p) {
            let mut chars = grapheme.chars();
            let ch = chars.next().unwrap_or(' ');
            let suffix: String = chars.collect();
            self.cells[i] = Cell {
                ch,
                suffix,
                style,
                continuation: false,
            };
        }
    }

    /// Write a continuation cell for a wide glyph.
    fn put_continuation(&mut self, p: Point, style: Style) {
        if let Some(i) = self.idx(p) {
            self.cells[i] = Cell::continuation(style);
        }
    }

    /// Fill a rectangle with a glyph and style.
    pub fn fill(&mut self, style: &Style, r: Rect, ch: char) {
        if let Some(isec) = self.rect().intersect(&r) {
            for y in isec.tl.y..isec.tl.y + isec.h {
                for x in isec.tl.x..isec.tl.x + isec.w {
                    self.put(Point { x, y }, ch, style.clone());
                }
            }
        }
    }

    /// Fill all empty cells with the given character and style.
    pub fn fill_empty(&mut self, ch: char, style: &Style) {
        for i in 0..self.cells.len() {
            if self.cells[i].is_empty() {
                self.cells[i] = Cell::new(ch, style.clone());
            }
        }
    }

    /// Overlay a cursor on a cell by adjusting its style.
    pub fn overlay_cursor(&mut self, location: Point, shape: cursor::CursorShape) {
        let Some(idx) = self.idx(location) else {
            return;
        };
        let mut cell = self.cells[idx].clone();
        match shape {
            cursor::CursorShape::Underscore => {
                cell.style.attrs = cell.style.attrs.with(Attr::Underline);
            }
            cursor::CursorShape::Block | cursor::CursorShape::Line => {
                mem::swap(&mut cell.style.fg, &mut cell.style.bg);
            }
        }
        if cell.is_empty() || cell.continuation {
            cell.ch = ' ';
            cell.suffix.clear();
            cell.continuation = false;
        }
        self.cells[idx] = cell;
    }

    /// Fill the frame outline with a glyph and style.
    pub fn solid_frame(&mut self, style: &Style, f: Frame, ch: char) {
        self.fill(style, f.top, ch);
        self.fill(style, f.left, ch);
        self.fill(style, f.right, ch);
        self.fill(style, f.bottom, ch);
        self.fill(style, f.topleft, ch);
        self.fill(style, f.topright, ch);
        self.fill(style, f.bottomleft, ch);
        self.fill(style, f.bottomright, ch);
    }

    /// Draw text clipped to the given line.
    pub fn text(&mut self, style: &Style, l: Line, txt: &str) {
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

                self.put_grapheme(Point { x, y: isec.tl.y }, grapheme, style.clone());
                for i in 1..width {
                    self.put_continuation(
                        Point {
                            x: x + i as u32,
                            y: isec.tl.y,
                        },
                        style.clone(),
                    );
                }
                x += width as u32;
                col += width;
            }

            for i in col..max {
                self.put(
                    Point {
                        x: isec.tl.x + i as u32,
                        y: isec.tl.y,
                    },
                    ' ',
                    style.clone(),
                );
            }
        }
    }

    /// Get a cell by position.
    pub fn get(&self, p: Point) -> Option<&Cell> {
        self.idx(p).map(|i| &self.cells[i])
    }
    /// Diff this terminal buffer against a previous state, emitting changes
    /// to the provided render backend.
    pub fn diff<R: RenderBackend>(&self, prev: &Self, backend: &mut R) -> Result<()> {
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

                let style = &current_row[x].style;
                let start_x = x;
                let mut text = String::new();
                while x < width {
                    let cell = &current_row[x];
                    if cell == &prev_row[x] || cell.style != *style {
                        break;
                    }
                    cell.push_text(&mut text);
                    x += 1;
                }
                backend.style(style)?;
                backend.text(
                    Point {
                        x: start_x as u32,
                        y,
                    },
                    &text,
                )?;
                wrote = true;
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
        let mut wrote = false;
        for y in 0..self.size.h {
            let mut x = 0;
            while x < self.size.w {
                let idx = y as usize * self.size.w as usize + x as usize;
                let cell = &self.cells[idx];
                let style = cell.style.clone();
                let start_x = x;
                let mut text = String::new();
                while x < self.size.w {
                    let idx2 = y as usize * self.size.w as usize + x as usize;
                    let ccell = &self.cells[idx2];
                    if ccell.style == style {
                        ccell.push_text(&mut text);
                        x += 1;
                    } else {
                        break;
                    }
                }
                backend.style(&style)?;
                backend.text(Point { x: start_x, y }, &text)?;
                wrote = true;
            }
        }
        if wrote {
            backend.flush()?;
        }
        Ok(())
    }
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

    let max = max_shift.min(height.saturating_sub(1) as usize);
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
    if len == 0 || start >= row.len() {
        return Ok(());
    }

    let end = start.saturating_add(len).min(row.len());
    let mut x = start;
    while x < end {
        let style = &row[x].style;
        let run_start = x;
        let mut text = String::new();
        while x < end && row[x].style == *style {
            row[x].push_text(&mut text);
            x += 1;
        }
        backend.style(style)?;
        backend.text(
            Point {
                x: run_start as u32,
                y,
            },
            &text,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buf,
        core::text::grapheme_width,
        geom::Line,
        style::{AttrSet, Color, PartialStyle},
        testing::buf::BufTest,
    };

    fn def_style() -> Style {
        Style {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        }
    }

    fn buf_from_rows(rows: &[&str]) -> TermBuf {
        let height = rows.len() as u32;
        let width = rows.first().map(|row| row.len()).unwrap_or(0) as u32;
        let style = def_style();
        let mut tb = TermBuf::new(Expanse::new(width, height), ' ', style.clone());
        for (y, row) in rows.iter().enumerate() {
            tb.text(&style, Line::new(0, y as u32, width), row);
        }
        tb
    }

    #[test]
    fn basic_fill() {
        let mut tb = TermBuf::new(Expanse::new(4, 2), ' ', def_style());
        tb.fill(&def_style(), Rect::new(1, 0, 2, 2), 'x');

        BufTest::new(&tb).assert_matches(buf![
            " xx "
            " xx "
        ]);
    }

    #[test]
    fn text_write() {
        let mut tb = TermBuf::new(Expanse::new(5, 1), ' ', def_style());
        tb.text(&def_style(), Line::new(0, 0, 5), "hi");

        BufTest::new(&tb).assert_matches(buf!["hi   "]);
    }

    #[test]
    fn text_handles_combining_and_wide_graphemes() {
        let style = def_style();
        let mut tb = TermBuf::new(Expanse::new(12, 1), ' ', style.clone());
        tb.text(&style, Line::new(0, 0, 12), "A\u{0301}界👩‍💻B");

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
        let mut tb = TermBuf::new(Expanse::new(4, 4), ' ', def_style());
        let f = Frame::new(Rect::new(0, 0, 4, 4), 1);
        tb.solid_frame(&def_style(), f, '#');

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
        fn style(&mut self, s: &Style) -> Result<()> {
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

        fn exit(&mut self, _code: i32) -> ! {
            unreachable!()
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
        fn style(&mut self, _s: &Style) -> Result<()> {
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

        fn exit(&mut self, _code: i32) -> ! {
            unreachable!()
        }

        fn reset(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diff_no_change() {
        let style = def_style();
        let tb1 = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        let tb2 = TermBuf::new(Expanse::new(3, 1), ' ', style);
        let mut be = RecBackend::new();
        tb2.diff(&tb1, &mut be).unwrap();
        assert!(be.ops.is_empty());
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
    fn diff_single_run() {
        let style = def_style();
        let prev = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        let mut cur = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        cur.text(&style, Line::new(0, 0, 3), "ab");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.ops.len(), 2);
        assert_eq!(be.ops[0], format!("style {style:?}"));
        assert_eq!(be.ops[1], "text 0 0 ab");
    }

    #[test]
    fn diff_style_changes() {
        let style1 = def_style();
        let mut style2 = style1.clone();
        style2.fg = Color::Red;

        let prev = TermBuf::new(Expanse::new(2, 1), ' ', style1.clone());
        let mut cur = TermBuf::new(Expanse::new(2, 1), ' ', style1.clone());
        cur.fill(&style2, Rect::new(0, 0, 1, 1), 'a');
        cur.fill(&style1, Rect::new(1, 0, 1, 1), 'b');

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
        let prev = TermBuf::new(Expanse::new(3, 2), ' ', style.clone());
        let mut cur = TermBuf::new(Expanse::new(3, 2), ' ', style.clone());
        cur.fill(&style, Rect::new(0, 1, 2, 1), 'x');
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.ops.len(), 2);
        assert_eq!(be.ops[0], format!("style {style:?}"));
        assert_eq!(be.ops[1], "text 0 1 xx");
    }

    #[test]
    fn render_whole_buffer() {
        let style = def_style();
        let mut tb = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        tb.text(&style, Line::new(0, 0, 3), "ab");
        let mut be = RecBackend::new();
        tb.render(&mut be).unwrap();
        assert_eq!(
            be.ops,
            vec![format!("style {style:?}"), "text 0 0 ab ".to_string(),]
        );
    }

    #[test]
    fn diff_size_change_rerender() {
        let style = def_style();
        let prev = TermBuf::new(Expanse::new(2, 1), ' ', style.clone());
        let mut cur = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        cur.text(&style, Line::new(0, 0, 3), "abc");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(
            be.ops,
            vec![format!("style {style:?}"), "text 0 0 abc".to_string(),]
        );
    }

    #[test]
    fn contains_text() {
        let mut tb = TermBuf::new(Expanse::new(10, 3), ' ', def_style());
        tb.text(&def_style(), Line::new(0, 0, 10), "hello");
        tb.text(&def_style(), Line::new(0, 1, 10), "world");

        let bt = BufTest::new(&tb);
        assert!(bt.contains_text("hello"));
        assert!(bt.contains_text("world"));
        assert!(!bt.contains_text("goodbye"));
    }

    #[test]
    fn contains_text_style() {
        let mut tb = TermBuf::new(Expanse::new(10, 3), ' ', def_style());

        // Add text with different styles
        let mut red_style = def_style();
        red_style.fg = Color::Red;

        let mut blue_style = def_style();
        blue_style.fg = Color::Blue;

        tb.text(&red_style, Line::new(0, 0, 5), "hello");
        tb.text(&blue_style, Line::new(5, 0, 5), "world");
        tb.text(&def_style(), Line::new(0, 1, 10), "test line");

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
        let mut tb = TermBuf::new(Expanse::new(10, 1), ' ', def_style());

        let mut blue_style = def_style();
        blue_style.fg = solarized::BLUE;

        tb.text(&blue_style, Line::new(0, 0, 3), "two");

        // Test the old method
        assert!(BufTest::new(&tb).contains_text_fg("two", solarized::BLUE));

        // Test that it works the same as contains_text_style
        assert!(BufTest::new(&tb).contains_text_style("two", &PartialStyle::fg(solarized::BLUE)));
    }

    #[test]
    fn test_empty_and_copy() {
        // Test empty constructor
        let empty = TermBuf::empty(Expanse::new(5, 3));
        assert_eq!(empty.size(), Expanse::new(5, 3));
        BufTest::new(&empty).assert_matches(buf![
            "XXXXX"
            "XXXXX"
            "XXXXX"
        ]);

        // Test copy functionality
        let mut src = TermBuf::new(Expanse::new(5, 3), ' ', def_style());
        src.text(&def_style(), Line::new(1, 1, 3), "ABC");

        BufTest::new(&src).assert_matches(buf![
            "     "
            " ABC "
            "     "
        ]);

        let mut dst = TermBuf::empty(Expanse::new(5, 3));
        dst.copy(&src, Rect::new(1, 1, 3, 1));

        // Check that only the text was copied (spaces are not copied)
        BufTest::new(&dst).assert_matches(buf![
            "XXXXX"
            "XABCX"
            "XXXXX"
        ]);

        // Test copy with partial rectangle
        let mut dst2 = TermBuf::empty(Expanse::new(5, 3));
        dst2.copy(&src, Rect::new(2, 1, 2, 1));

        BufTest::new(&dst2).assert_matches(buf![
            "XXXXX"
            "XXBCX"
            "XXXXX"
        ]);

        // Test copy with different sizes (should do nothing)
        let mut wrong_size = TermBuf::empty(Expanse::new(4, 3));
        wrong_size.copy(&src, Rect::new(0, 0, 5, 3));

        BufTest::new(&wrong_size).assert_matches(buf![
            "XXXX"
            "XXXX"
            "XXXX"
        ]);
    }

    #[test]
    fn contains_text_style_builders() {
        use crate::style::Attr;
        let mut tb = TermBuf::new(Expanse::new(10, 2), ' ', def_style());

        // Create styles with different attributes
        let mut bold_red = def_style();
        bold_red.fg = Color::Red;
        bold_red.attrs = AttrSet::new(Attr::Bold);

        let mut italic_blue = def_style();
        italic_blue.fg = Color::Blue;
        italic_blue.attrs = AttrSet::new(Attr::Italic);

        tb.text(&bold_red, Line::new(0, 0, 4), "bold");
        tb.text(&italic_blue, Line::new(0, 1, 6), "italic");

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
        let mut tb = TermBuf::empty(Expanse::new(5, 3));

        // Verify all cells are NULL initially using buf macro
        BufTest::new(&tb).assert_matches(buf![
            "XXXXX"
            "XXXXX"
            "XXXXX"
        ]);

        // Add some content to part of the buffer
        tb.text(&def_style(), Line::new(1, 1, 3), "ABC");

        // Verify the content before fill_empty
        BufTest::new(&tb).assert_matches(buf![
            "XXXXX"
            "XABCX"
            "XXXXX"
        ]);

        // Fill empty cells with a specific character and style
        let mut fill_style = def_style();
        fill_style.fg = Color::Red;
        tb.fill_empty('.', &fill_style);

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
