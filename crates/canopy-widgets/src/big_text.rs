//! Large text in a built-in pixel font, three rows high.

use canopy::{
    NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Align, Layout, MeasureConstraints, Measurement},
    render::Render,
    style::roles,
};

/// Pixel rows of each glyph. Half blocks draw two pixel rows in each cell
/// row, so a glyph fills three cell rows.
const PIXEL_ROWS: usize = 5;

/// Cell rows of one line of text.
const LINE_ROWS: u32 = 3;

/// Blank pixel columns between two glyphs.
const GAP: u32 = 1;

/// The pixel rows of one glyph, top first: `#` is a set pixel.
type Glyph = [&'static str; PIXEL_ROWS];

/// The glyph that stands in for a character the font lacks.
const UNKNOWN: Glyph = ["##.", "..#", ".#.", "...", ".#."];

/// Large text drawn in a built-in pixel font with half blocks.
///
/// The font is five pixels high, and most glyphs are three pixels wide: M and
/// W take five, N takes four, and narrow marks such as `.` take one. Each line
/// of text is three rows high. Text that does not fit the area is cut at its
/// right and bottom edges. The font has the letters A to Z, the
/// digits, common punctuation, and basic math: `+ - × ÷ = < > ≤ ≥ % ° …`.
/// Lowercase letters draw as capitals, and a character without a glyph draws
/// as `?`. A newline starts a new line, one blank row below the last.
///
/// The text is a list of runs, each with the style path that paints it, so
/// one value can show a number and a dimmer unit. `BigText` pushes the
/// `big_text` layer, and a plain run paints the `text` part.
pub struct BigText {
    /// Runs of text: the style path that paints each run, and its text.
    runs: Vec<(String, String)>,
    /// Horizontal placement of each line in the area.
    align: Align,
}

impl BigText {
    /// Constructs large text of one run in the `text` part.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            runs: vec![(roles::TEXT.to_owned(), text.into())],
            align: Align::Start,
        }
    }

    /// Replaces the text with runs, each painted by its own style path.
    #[must_use]
    pub fn with_runs<S, T>(mut self, runs: impl IntoIterator<Item = (S, T)>) -> Self
    where
        S: Into<String>,
        T: Into<String>,
    {
        self.set_runs(runs);
        self
    }

    /// Places each line at the start, the center, or the end of the area.
    #[must_use]
    pub fn with_align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Replaces the text with one run in the `text` part.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.runs = vec![(roles::TEXT.to_owned(), text.into())];
    }

    /// Replaces the text with runs, each painted by its own style path.
    pub fn set_runs<S, T>(&mut self, runs: impl IntoIterator<Item = (S, T)>)
    where
        S: Into<String>,
        T: Into<String>,
    {
        self.runs = runs
            .into_iter()
            .map(|(style, text)| (style.into(), text.into()))
            .collect();
    }

    /// Returns the text of all runs.
    pub fn text(&self) -> String {
        self.runs.iter().map(|(_, text)| text.as_str()).collect()
    }

    /// Returns the size in cells that `text` takes.
    pub fn size_of(text: &str) -> Size {
        let lines = text.split('\n').collect::<Vec<_>>();
        let width = lines
            .iter()
            .map(|line| line_width(line.chars()))
            .max()
            .unwrap_or(0);
        Size::new(width, rows_for(lines.len()))
    }

    /// Returns the rows of cells that draw `text`, without trailing spaces.
    pub fn rows_of(text: &str) -> Vec<String> {
        let mut rows = Vec::new();
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                rows.push(String::new());
            }
            let columns = columns(line.chars().map(|ch| (0, ch)));
            for row in 0..LINE_ROWS as usize {
                let text: String = columns
                    .iter()
                    .map(|(_, pixels)| cell(pixels, row))
                    .collect();
                rows.push(text.trim_end().to_owned());
            }
        }
        rows
    }

    /// Returns the lines of the runs, each a list of characters with the
    /// index of their run.
    fn lines(&self) -> Vec<Vec<(usize, char)>> {
        let mut lines = vec![Vec::new()];
        for (index, (_, text)) in self.runs.iter().enumerate() {
            for ch in text.chars() {
                if ch == '\n' {
                    lines.push(Vec::new());
                } else if let Some(line) = lines.last_mut() {
                    line.push((index, ch));
                }
            }
        }
        lines
    }
}

impl Widget for BigText {
    fn layout(&self) -> Layout {
        Layout::column()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Self::size_of(&self.text()))
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("big_text");
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill(roles::TEXT, area, ' ')?;
        let bottom = area.tl.y.saturating_add(area.h);
        for (index, line) in self.lines().iter().enumerate() {
            let top = area
                .tl
                .y
                .saturating_add(LINE_ROWS.saturating_add(1) * index as u32);
            if top >= bottom {
                break;
            }
            let columns = columns(line.iter().copied());
            let width = columns.len() as u32;
            let offset = match self.align {
                Align::Start => 0,
                Align::Center => area.w.saturating_sub(width) / 2,
                Align::End => area.w.saturating_sub(width),
            };
            // The columns past the right edge of the area stay unpainted.
            let shown = &columns[..columns.len().min((area.w - offset) as usize)];
            let x = area.tl.x.saturating_add(offset);
            for row in 0..LINE_ROWS {
                let y = top.saturating_add(row);
                if y >= bottom {
                    break;
                }
                paint_row(render, &self.runs, shown, x, y, row)?;
            }
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("big_text")
    }
}

/// Returns the cell rows of `lines` lines of text, with a blank row between
/// two lines.
fn rows_for(lines: usize) -> u32 {
    let lines = u32::try_from(lines).unwrap_or(u32::MAX);
    (LINE_ROWS.saturating_add(1))
        .saturating_mul(lines)
        .saturating_sub(1)
}

/// Returns the width in cells of one line of text.
fn line_width(line: impl Iterator<Item = char>) -> u32 {
    let widths = line.map(|ch| glyph(ch)[0].len() as u32).collect::<Vec<_>>();
    let gaps = GAP * u32::try_from(widths.len().saturating_sub(1)).unwrap_or(0);
    widths.iter().sum::<u32>() + gaps
}

/// Returns the pixel columns of one line: for each column, the run that owns
/// it and its pixels, top first. The gap after a glyph belongs to its run.
fn columns(line: impl Iterator<Item = (usize, char)>) -> Vec<(usize, [bool; PIXEL_ROWS])> {
    let mut columns = Vec::new();
    for (position, (run, ch)) in line.enumerate() {
        if position > 0
            && let Some(&(previous, _)) = columns.last()
        {
            columns.extend((0..GAP).map(|_| (previous, [false; PIXEL_ROWS])));
        }
        let rows = glyph(ch);
        for x in 0..rows[0].len() {
            let mut pixels = [false; PIXEL_ROWS];
            for (y, row) in rows.iter().enumerate() {
                pixels[y] = row.as_bytes()[x] == b'#';
            }
            columns.push((run, pixels));
        }
    }
    columns
}

/// Returns the half block of cell row `row` of one pixel column.
fn cell(pixels: &[bool; PIXEL_ROWS], row: usize) -> char {
    let top = pixels.get(row * 2).copied().unwrap_or(false);
    let bottom = pixels.get(row * 2 + 1).copied().unwrap_or(false);
    match (top, bottom) {
        (true, true) => '█',
        (true, false) => '▀',
        (false, true) => '▄',
        (false, false) => ' ',
    }
}

/// Paints cell row `row` of a line at `y`, one text call for each stretch of
/// columns that one run owns.
fn paint_row(
    render: &mut Render<'_>,
    runs: &[(String, String)],
    columns: &[(usize, [bool; PIXEL_ROWS])],
    x: u32,
    y: u32,
    row: u32,
) -> Result<()> {
    let mut start = 0;
    while start < columns.len() {
        let run = columns[start].0;
        let end = columns[start..]
            .iter()
            .position(|(owner, _)| *owner != run)
            .map_or(columns.len(), |offset| start + offset);
        let text: String = columns[start..end]
            .iter()
            .map(|(_, pixels)| cell(pixels, row as usize))
            .collect();
        let line = Line::new(x.saturating_add(start as u32), y, (end - start) as u32);
        render.text(&runs[run].0, line, &text)?;
        start = end;
    }
    Ok(())
}

/// Returns the glyph of `ch`. Lowercase letters take the capital glyph, other
/// whitespace takes the space, and a character without a glyph takes `?`.
fn glyph(ch: char) -> Glyph {
    let ch = match ch {
        'a'..='z' => ch.to_ascii_uppercase(),
        '−' | '–' => '-',
        '‘' | '’' => '\'',
        '“' | '”' => '"',
        ch if ch.is_whitespace() => ' ',
        ch => ch,
    };
    match ch {
        ' ' => ["..", "..", "..", "..", ".."],
        '0' => ["###", "#.#", "#.#", "#.#", "###"],
        '1' => [".#.", "##.", ".#.", ".#.", "###"],
        '2' => ["###", "..#", "###", "#..", "###"],
        '3' => ["###", "..#", "###", "..#", "###"],
        '4' => ["#.#", "#.#", "###", "..#", "..#"],
        '5' => ["###", "#..", "###", "..#", "###"],
        '6' => ["###", "#..", "###", "#.#", "###"],
        '7' => ["###", "..#", "..#", "..#", "..#"],
        '8' => ["###", "#.#", "###", "#.#", "###"],
        '9' => ["###", "#.#", "###", "..#", "###"],
        'A' => [".#.", "#.#", "###", "#.#", "#.#"],
        'B' => ["##.", "#.#", "##.", "#.#", "##."],
        'C' => [".##", "#..", "#..", "#..", ".##"],
        'D' => ["##.", "#.#", "#.#", "#.#", "##."],
        'E' => ["###", "#..", "##.", "#..", "###"],
        'F' => ["###", "#..", "##.", "#..", "#.."],
        'G' => [".##", "#..", "#.#", "#.#", ".##"],
        'H' => ["#.#", "#.#", "###", "#.#", "#.#"],
        'I' => ["###", ".#.", ".#.", ".#.", "###"],
        'J' => ["..#", "..#", "..#", "#.#", ".#."],
        'K' => ["#.#", "#.#", "##.", "#.#", "#.#"],
        'L' => ["#..", "#..", "#..", "#..", "###"],
        'M' => ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'N' => ["#..#", "##.#", "#.##", "#..#", "#..#"],
        'O' => [".#.", "#.#", "#.#", "#.#", ".#."],
        'P' => ["##.", "#.#", "##.", "#..", "#.."],
        'Q' => [".#.", "#.#", "#.#", "##.", ".##"],
        'R' => ["##.", "#.#", "##.", "#.#", "#.#"],
        'S' => [".##", "#..", ".#.", "..#", "##."],
        'T' => ["###", ".#.", ".#.", ".#.", ".#."],
        'U' => ["#.#", "#.#", "#.#", "#.#", "###"],
        'V' => ["#.#", "#.#", "#.#", "#.#", ".#."],
        'W' => ["#...#", "#...#", "#.#.#", "##.##", "#...#"],
        'X' => ["#.#", "#.#", ".#.", "#.#", "#.#"],
        'Y' => ["#.#", "#.#", ".#.", ".#.", ".#."],
        'Z' => ["###", "..#", ".#.", "#..", "###"],
        '.' => [".", ".", ".", ".", "#"],
        ',' => ["..", "..", "..", ".#", "#."],
        ':' => [".", "#", ".", "#", "."],
        ';' => ["..", ".#", "..", ".#", "#."],
        '!' => ["#", "#", "#", ".", "#"],
        '?' => UNKNOWN,
        '\'' => ["#", "#", ".", ".", "."],
        '"' => ["#.#", "#.#", "...", "...", "..."],
        '-' => ["...", "...", "###", "...", "..."],
        '+' => ["...", ".#.", "###", ".#.", "..."],
        '=' => ["...", "###", "...", "###", "..."],
        '*' => ["#.#", ".#.", "#.#", "...", "..."],
        '×' => ["...", "#.#", ".#.", "#.#", "..."],
        '÷' => [".#.", "...", "###", "...", ".#."],
        '/' => ["..#", "..#", ".#.", "#..", "#.."],
        '\\' => ["#..", "#..", ".#.", "..#", "..#"],
        '%' => ["#.#", "..#", ".#.", "#..", "#.#"],
        '(' => [".#", "#.", "#.", "#.", ".#"],
        ')' => ["#.", ".#", ".#", ".#", "#."],
        '[' => ["##", "#.", "#.", "#.", "##"],
        ']' => ["##", ".#", ".#", ".#", "##"],
        '<' => ["..#", ".#.", "#..", ".#.", "..#"],
        '>' => ["#..", ".#.", "..#", ".#.", "#.."],
        '≤' => ["..#", ".#.", "..#", "...", "###"],
        '≥' => ["#..", ".#.", "#..", "...", "###"],
        '_' => ["...", "...", "...", "...", "###"],
        '#' => ["#.#", "###", "#.#", "###", "#.#"],
        '^' => [".#.", "#.#", "...", "...", "..."],
        '~' => ["....", ".#.#", "#.#.", "....", "...."],
        '|' => ["#", "#", "#", "#", "#"],
        '°' => ["##", "##", "..", "..", ".."],
        '·' => [".", ".", "#", ".", "."],
        '—' => ["....", "....", "####", "....", "...."],
        '…' => [".....", ".....", ".....", ".....", "#.#.#"],
        _ => UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        Context, ContextExt,
        geom::Point,
        layout::{Edges, LayoutOverride},
        style::{Color, ResolvedStyle},
        testing::harness::Harness,
    };

    use super::*;

    /// The digits of the font, as their cell rows.
    const DIGITS: [&str; 3] = [
        "█▀█ ▄█  ▀▀█ ▀▀█ █ █ █▀▀ █▀▀ ▀▀█ █▀█ █▀█",
        "█ █  █  █▀▀ ▀▀█ ▀▀█ ▀▀█ █▀█   █ █▀█ ▀▀█",
        "▀▀▀ ▀▀▀ ▀▀▀ ▀▀▀   ▀ ▀▀▀ ▀▀▀   ▀ ▀▀▀ ▀▀▀",
    ];

    #[test]
    fn every_glyph_has_five_rows_of_one_width() {
        let characters = ('A'..='Z')
            .chain('0'..='9')
            .chain(" .,:;!?'\"-+=*×÷/\\%()[]<>≤≥_#^~|°·—…".chars());
        for ch in characters {
            let rows = glyph(ch);
            let width = rows[0].len();
            assert!(width > 0, "{ch} has pixels");
            assert!(
                rows.iter().all(|row| row.len() == width),
                "{ch} has one width"
            );
            assert!(
                rows.iter()
                    .all(|row| row.bytes().all(|b| b == b'#' || b == b'.')),
                "{ch} has only pixels"
            );
            assert!(
                ch == ' ' || rows.concat().contains('#'),
                "{ch} is not blank"
            );
        }
    }

    #[test]
    fn digits_draw_in_three_rows_of_half_blocks() {
        assert_eq!(BigText::rows_of("0123456789"), DIGITS);
        assert_eq!(BigText::size_of("0123456789"), Size::new(39, 3));
    }

    #[test]
    fn lowercase_draws_as_capitals_and_unknown_characters_as_a_question() {
        assert_eq!(BigText::rows_of("abc"), BigText::rows_of("ABC"));
        assert_eq!(BigText::rows_of("€"), BigText::rows_of("?"));
        assert_eq!(BigText::rows_of("4−2"), BigText::rows_of("4-2"));
        assert_eq!(BigText::rows_of("a\tb"), BigText::rows_of("a b"));
    }

    #[test]
    fn wide_and_narrow_glyphs_keep_their_widths() {
        assert_eq!(BigText::size_of("M"), Size::new(5, 3));
        assert_eq!(BigText::size_of("1.5"), Size::new(3 + 1 + 1 + 1 + 3, 3));
        assert_eq!(
            BigText::rows_of("MW"),
            ["█▄ ▄█ █   █", "█ ▀ █ █▄▀▄█", "▀   ▀ ▀   ▀"]
        );
    }

    #[test]
    fn a_newline_starts_a_line_below_a_blank_row() {
        assert_eq!(BigText::size_of("AB\nC"), Size::new(7, 7));
        let rows = BigText::rows_of("A\nB");
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[3], "");
        assert_eq!(rows[4], "█▀▄");
        assert_eq!(BigText::size_of(""), Size::new(0, 3));
    }

    /// Unit color of the styled runs test.
    const UNIT: Color = Color::Rgb {
        r: 90,
        g: 90,
        b: 90,
    };

    /// Returns the glyph and the style of one cell.
    fn cell_at(harness: &Harness, x: u32, y: u32) -> (char, ResolvedStyle) {
        let cell = harness.buf().get(Point { x, y }).expect("cell");
        (cell.ch, cell.style)
    }

    #[test]
    fn runs_paint_in_their_own_styles() -> Result<()> {
        let text = BigText::new("").with_runs([("text", "4"), ("unit", "K")]);
        let mut harness = Harness::builder(text)
            .size(12, 3)
            .configure(|setup| {
                setup.widget_styles(|_palette, rules| {
                    rules.fg("big_text/unit", UNIT).apply();
                });
                Ok(())
            })
            .build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        assert_eq!(lines[0].trim_end(), "█ █ █ █");
        let (glyph, digit) = cell_at(&harness, 0, 0);
        assert_eq!(glyph, '█');
        assert_ne!(digit.fg, UNIT, "the digit takes the text part");
        let (glyph, unit) = cell_at(&harness, 4, 0);
        assert_eq!(glyph, '█');
        assert_eq!(unit.fg, UNIT, "the unit takes its own style");
        Ok(())
    }

    #[test]
    fn text_stays_within_its_area() -> Result<()> {
        // "88" is seven columns wide and three rows high, but the widget has
        // only five columns and two rows inside its padding.
        let mut harness = Harness::builder(Padded(Some(BigText::new("88"))))
            .size(7, 4)
            .build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        assert_eq!(lines[1], " █▀█ █ ", "the right padding stays blank");
        assert_eq!(lines[2], " █▀█ █ ");
        assert_eq!(lines[3], "       ", "the bottom padding stays blank");
        Ok(())
    }

    /// A root that holds its child inside one cell of padding.
    struct Padded(Option<BigText>);

    impl Widget for Padded {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
            render.fill("", ctx.view().view_rect_local(), ' ')
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let child = self.0.take().expect("child");
            let node = ctx.add_child(ctx.node_id(), child)?;
            let padded = LayoutOverride {
                padding: Some(Edges::all(1)),
                ..LayoutOverride::new().flex_horizontal(1).flex_vertical(1)
            };
            ctx.set_layout_override(node.into(), padded)
        }
    }

    #[test]
    fn lines_align_in_a_wider_area() -> Result<()> {
        let text = BigText::new("1\n11").with_align(Align::End);
        let mut harness = Harness::builder(text).size(9, 7).build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        assert_eq!(lines[0].trim_end(), "      ▄█");
        assert_eq!(lines[4].trim_end(), "  ▄█  ▄█");
        Ok(())
    }
}
