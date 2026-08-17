//! Tests for the terminal buffer.

use proptest::{prelude::*, test_runner::TestCaseResult};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::*;
use crate::{
    backend::crossterm::CrosstermRender,
    buf,
    core::{testing::model::trace_result, text::grapheme_width},
    geom::Line,
    style::{AttrSet, Color, PartialStyle, StyleBuilder},
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
    let mut buf = TermBuf::new(Size::new(3, 3), '\0', def_style())
        .expect("test render target should allocate");
    assert!(matches!(
        buf.fill(&style, Rect::new(0, 0, 1, 1), '界'),
        Err(Error::InvalidCellCharacter { width: 2, .. })
    ));
    assert!(matches!(
        buf.fill(&style, Rect::new(0, 0, 1, 1), '\u{0301}'),
        Err(Error::InvalidCellCharacter { width: 0, .. })
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
    let mut buf = TermBuf::new(Size::new(2, 1), '\0', def_style())?;
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
    let mut buf = TermBuf::new(Size::new(1, 1), '\0', def_style())?;
    buf.cells[0] = Cell::continuation(def_style());
    let mut backend = RecBackend::new();
    assert!(matches!(buf.render(&mut backend), Err(Error::Invariant(_))));

    let mut ragged = TermBuf::new(Size::new(2, 1), '\0', def_style())?;
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

    fn flush(&mut self) -> Result<()> {
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
        let Some(rect) = self.size.rect().intersect(rect) else {
            return;
        };
        for y in rect.tl.y..rect.tl.y.saturating_add(rect.h) {
            for x in rect.tl.x..rect.tl.x.saturating_add(rect.w) {
                self.put_grapheme(Point { x, y }, &ch.to_string(), style);
            }
        }
    }

    fn text(&mut self, line: Line, text: &str, style: ResolvedStyle) {
        let Some(line) = self.size.rect().intersect(line.rect()) else {
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
        (0u32..7, 0u32..5).prop_map(|(width, height)| BufferOperation::Resize { width, height }),
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
    let mut tb = TermBuf::new(Size::new(1, 1), '\0', def_style())
        .expect("test render target should allocate");
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
    let partial_white_bg =
        PartialStyle::from(StyleBuilder::new().fg(Color::White).bg(Color::Black));
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
    let empty = TermBuf::new(Size::new(5, 3), '\0', def_style())
        .expect("test render target should allocate");
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
    let bold_red_style = PartialStyle::from(StyleBuilder::new().fg(Color::Red).attr(Attr::Bold));
    assert!(BufTest::new(&tb).contains_text_style("bold", &bold_red_style));

    // Test that it doesn't match wrong combinations
    let italic_red = PartialStyle::from(StyleBuilder::new().fg(Color::Red).attr(Attr::Italic));
    assert!(!BufTest::new(&tb).contains_text_style("bold", &italic_red));
}
