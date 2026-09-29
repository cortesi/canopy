//! Large text in a built-in pixel font, three rows high.

use std::time::{Duration, Instant};

use canopy::{
    Context, NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Align, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::{NodeWakeHandle, PollLifetime},
    style::roles,
};

/// Pixel rows of each glyph. Half blocks draw two pixel rows in each cell
/// row, so a glyph fills three cell rows.
const PIXEL_ROWS: usize = 5;

/// Cell rows of one line of text.
const LINE_ROWS: u32 = 3;

/// Pixel rows that the cell rows of one line show: the glyph and one blank
/// row below it.
const WINDOW: usize = LINE_ROWS as usize * 2;

/// Blank pixel columns between two glyphs.
const GAP: u32 = 1;

/// Time that a changed character takes to roll.
const ROLL: Duration = Duration::from_millis(200);

/// Time between two frames of a roll.
const ROLL_FRAME: Duration = Duration::from_millis(33);

/// The pixel rows of one glyph, top first: `#` is a set pixel.
type Glyph = [&'static str; PIXEL_ROWS];

/// The glyph that stands in for a character the font lacks.
const UNKNOWN: Glyph = ["##.", "..#", ".#.", "...", ".#."];

/// One pixel column of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Column {
    /// Run that paints the column.
    run: usize,
    /// Pixels, top first.
    pixels: [bool; WINDOW],
    /// Character of the column and the column within its glyph. A gap
    /// between glyphs has none.
    slot: Option<(char, u32)>,
}

/// A roll from the columns on screen to the columns of new text.
#[derive(Debug, Clone)]
struct Roll {
    /// Columns of each line on screen when the text changed.
    from: Vec<Vec<Column>>,
    /// Start of the roll: the first render after the change.
    started: Option<Instant>,
}

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
///
/// When the text changes and [`ViewContext::motion_active`] is true, the
/// changed characters roll over 200 ms: the old glyph moves up out of its
/// cells, and the new glyph moves in from below. The position of the roll
/// comes from [`ViewContext::now`], so a slow repaint skips frames and never
/// slows the roll. A change during a roll starts a new roll from the glyphs
/// on screen. Old and new text align on the right, so a number that gains a
/// digit rolls it in on the left. A hidden widget, or motion that turns off,
/// ends the roll at rest.
pub struct BigText {
    /// Runs of text: the style path that paints each run, and its text.
    runs: Vec<(String, String)>,
    /// Horizontal placement of each line in the area.
    align: Align,
    /// Columns of each line at the last render.
    shown: Vec<Vec<Column>>,
    /// Roll in progress.
    roll: Option<Roll>,
    /// Handle that starts polling when the text changes.
    wake: Option<NodeWakeHandle>,
}

impl BigText {
    /// Constructs large text of one run in the `text` part.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            runs: vec![(roles::TEXT.to_owned(), text.into())],
            align: Align::Start,
            shown: Vec::new(),
            roll: None,
            wake: None,
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
        self.set_runs([(roles::TEXT, text.into())]);
    }

    /// Replaces the text with runs, each painted by its own style path.
    pub fn set_runs<S, T>(&mut self, runs: impl IntoIterator<Item = (S, T)>)
    where
        S: Into<String>,
        T: Into<String>,
    {
        let before = self.text();
        self.runs = runs
            .into_iter()
            .map(|(style, text)| (style.into(), text.into()))
            .collect();
        // Text that never showed has nothing to roll from.
        if self.text() != before && !self.shown.is_empty() {
            self.roll = Some(Roll {
                from: self.shown.clone(),
                started: None,
            });
            if let Some(wake) = &self.wake {
                // An expired handle means that the widget left the tree, and
                // its next mount takes a new one.
                let _outcome = wake.wake();
            }
        }
    }

    /// Returns the text of all runs.
    pub fn text(&self) -> String {
        self.runs.iter().map(|(_, text)| text.as_str()).collect()
    }

    /// Returns whether a roll is in progress.
    #[cfg(test)]
    fn rolling(&self) -> bool {
        self.roll.is_some()
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
        let lines = text
            .split('\n')
            .map(|line| columns(line.chars().map(|ch| (0, ch))))
            .collect::<Vec<_>>();
        rows_of_columns(&lines)
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

    /// Returns the columns to draw at `now`, and ends a roll that finished
    /// or that motion no longer shows.
    fn frame(&mut self, now: Instant, motion: bool) -> Vec<Vec<Column>> {
        let target = self
            .lines()
            .into_iter()
            .map(|line| columns(line.into_iter()))
            .collect::<Vec<_>>();
        let Some(roll) = &mut self.roll else {
            return target;
        };
        let started = *roll.started.get_or_insert(now);
        let elapsed = now.saturating_duration_since(started);
        if !motion || elapsed >= ROLL {
            self.roll = None;
            return target;
        }
        let progress = elapsed.as_secs_f32() / ROLL.as_secs_f32();
        target
            .iter()
            .enumerate()
            .map(|(index, line)| {
                roll_line(
                    roll.from.get(index).map_or(&[][..], Vec::as_slice),
                    line,
                    progress,
                )
            })
            .collect()
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
        let frame = self.frame(ctx.now(), ctx.motion_active());
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            self.shown = frame;
            return Ok(());
        }
        render.fill(roles::TEXT, area, ' ')?;
        let bottom = area.tl.y.saturating_add(area.h);
        for (index, columns) in frame.iter().enumerate() {
            let top = area
                .tl
                .y
                .saturating_add(LINE_ROWS.saturating_add(1) * index as u32);
            if top >= bottom {
                break;
            }
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
        self.shown = frame;
        Ok(())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        let Some(roll) = &mut self.roll else {
            return Ok(None);
        };
        let area = ctx.view().view_rect_local();
        let hidden = area.w == 0 || area.h == 0;
        if hidden || !ctx.motion_active() {
            self.roll = None;
            return Ok(None);
        }
        // The poll repaints, and the render after the last frame ends the
        // roll. A widget out of view renders no frame, so the poll starts the
        // clock of the roll too.
        let started = *roll.started.get_or_insert(ctx.now());
        let ended = ctx.now().saturating_duration_since(started) >= ROLL;
        Ok((!ended).then_some(ROLL_FRAME))
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.wake = Some(ctx.wake_handle(PollLifetime::Node)?);
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

/// Returns the pixel columns of one line. The gap after a glyph belongs to
/// its run.
fn columns(line: impl Iterator<Item = (usize, char)>) -> Vec<Column> {
    let mut columns: Vec<Column> = Vec::new();
    for (position, (run, ch)) in line.enumerate() {
        if position > 0
            && let Some(previous) = columns.last().map(|column| column.run)
        {
            columns.extend((0..GAP).map(|_| Column {
                run: previous,
                pixels: [false; WINDOW],
                slot: None,
            }));
        }
        let rows = glyph(ch);
        for x in 0..rows[0].len() {
            let mut pixels = [false; WINDOW];
            for (y, row) in rows.iter().enumerate() {
                pixels[y] = row.as_bytes()[x] == b'#';
            }
            columns.push(Column {
                run,
                pixels,
                slot: Some((ch, x as u32)),
            });
        }
    }
    columns
}

/// Returns the cell rows of lines of columns, without trailing spaces.
fn rows_of_columns(lines: &[Vec<Column>]) -> Vec<String> {
    let mut rows = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            rows.push(String::new());
        }
        for row in 0..LINE_ROWS as usize {
            let text: String = line
                .iter()
                .map(|column| cell(&column.pixels, row))
                .collect();
            rows.push(text.trim_end().to_owned());
        }
    }
    rows
}

/// Returns one line of a roll at `progress`, from 0 to 1. The old columns
/// align with the new ones on the right. A column whose character and pixels
/// match stays, and every other column moves the old pixels up and the new
/// ones in from below. A column without an old column rolls in from blank.
fn roll_line(from: &[Column], to: &[Column], progress: f32) -> Vec<Column> {
    // Ease out, so a roll settles gently on its new glyph.
    let eased = 1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(3);
    let shift = ((eased * WINDOW as f32).floor() as usize).min(WINDOW);
    let skip = from.len().saturating_sub(to.len());
    let lead = to.len().saturating_sub(from.len());
    to.iter()
        .enumerate()
        .map(|(index, new)| {
            let old = index
                .checked_sub(lead)
                .and_then(|index| from.get(index + skip));
            if old.is_some_and(|old| old.pixels == new.pixels && old.slot == new.slot) {
                return *new;
            }
            let old = old.map_or([false; WINDOW], |old| old.pixels);
            let mut pixels = [false; WINDOW];
            for (row, pixel) in pixels.iter_mut().enumerate() {
                let source = row + shift;
                *pixel = if source < WINDOW {
                    old[source]
                } else {
                    new.pixels[source - WINDOW]
                };
            }
            Column { pixels, ..*new }
        })
        .collect()
}

/// Returns the half block of cell row `row` of one pixel column.
fn cell(pixels: &[bool; WINDOW], row: usize) -> char {
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
    columns: &[Column],
    x: u32,
    y: u32,
    row: u32,
) -> Result<()> {
    let mut start = 0;
    while start < columns.len() {
        let run = columns[start].run;
        let end = columns[start..]
            .iter()
            .position(|column| column.run != run)
            .map_or(columns.len(), |offset| start + offset);
        let text: String = columns[start..end]
            .iter()
            .map(|column| cell(&column.pixels, row as usize))
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
    use std::sync::Arc;

    use canopy::{
        ContextExt, NodeId,
        geom::Point,
        layout::{Edges, LayoutOverride},
        style::{Color, ResolvedStyle},
        testing::{ManualClock, harness::Harness},
    };

    use super::*;
    use crate::Scroll;

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

    /// Returns a harness of `text` whose clock the test moves, and the clock.
    fn clocked(text: &str, motion: bool) -> Result<(Harness, Arc<ManualClock>)> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = Harness::builder(BigText::new(text))
            .size(20, 3)
            .clock(Arc::clone(&clock))
            .motion(motion)
            .build()?;
        harness.render()?;
        Ok((harness, clock))
    }

    /// Returns the rows on screen, without trailing spaces.
    fn screen(harness: &Harness) -> Vec<String> {
        harness
            .tbuf()
            .lines()
            .iter()
            .map(|line| line.trim_end().to_owned())
            .collect()
    }

    /// Sets the text of the root widget.
    fn set(harness: &mut Harness, text: &str) {
        harness.with_root_widget(|widget: &mut BigText| widget.set_text(text));
    }

    /// Returns whether the root widget rolls.
    fn rolls(harness: &mut Harness) -> bool {
        harness.with_root_widget(|widget: &mut BigText| widget.rolling())
    }

    /// Moves the clock on and renders.
    fn after(harness: &mut Harness, clock: &ManualClock, millis: u64) -> Result<()> {
        clock.advance(Duration::from_millis(millis))?;
        harness.render()
    }

    #[test]
    fn a_changed_character_rolls_and_the_others_stay() -> Result<()> {
        let (mut harness, clock) = clocked("12", true)?;
        set(&mut harness, "13");
        harness.render()?;
        assert_eq!(
            screen(&harness),
            BigText::rows_of("12"),
            "the roll starts at rest"
        );
        assert!(rolls(&mut harness));
        after(&mut harness, &clock, 80)?;
        let middle = screen(&harness);
        let (old, new) = (BigText::rows_of("12"), BigText::rows_of("13"));
        for row in 0..3 {
            let prefix = |text: &str| text.chars().take(4).collect::<String>();
            assert_eq!(prefix(&middle[row]), prefix(&new[row]), "the 1 stays");
        }
        assert_ne!(middle, old);
        assert_ne!(middle, new);
        after(&mut harness, &clock, 120)?;
        assert_eq!(screen(&harness), new);
        assert!(!rolls(&mut harness), "the roll ends at 200 ms");
        Ok(())
    }

    #[test]
    fn the_roll_moves_the_old_glyph_up_and_the_new_one_in_from_below() {
        let from = columns("8".chars().map(|ch| (0, ch)));
        let to = columns("1".chars().map(|ch| (0, ch)));
        let rows = |progress| rows_of_columns(&[roll_line(&from, &to, progress)]);
        assert_eq!(rows(0.0), BigText::rows_of("8"));
        assert_eq!(rows(0.05), BigText::rows_of("8"), "less than one pixel row");
        // One pixel row up: the top row of the eight leaves, and the top row
        // of the one enters at the bottom.
        let first = rows(0.1);
        assert_eq!(first[0], "█▄█", "rows two and three of the eight");
        assert_eq!(first[2], " ▄", "the top row of the one");
        assert_eq!(rows(1.0), BigText::rows_of("1"));
    }

    #[test]
    fn a_change_during_a_roll_starts_from_the_glyphs_on_screen() -> Result<()> {
        let (mut harness, clock) = clocked("12", true)?;
        set(&mut harness, "13");
        harness.render()?;
        after(&mut harness, &clock, 60)?;
        let middle = screen(&harness);
        set(&mut harness, "14");
        harness.render()?;
        assert_eq!(screen(&harness), middle, "the new roll starts on screen");
        after(&mut harness, &clock, 150)?;
        assert!(rolls(&mut harness), "the new roll has its own 200 ms");
        after(&mut harness, &clock, 50)?;
        assert_eq!(screen(&harness), BigText::rows_of("14"));
        assert!(!rolls(&mut harness));
        Ok(())
    }

    #[test]
    fn a_longer_text_aligns_on_the_right_and_rolls_in_on_the_left() -> Result<()> {
        let (mut harness, clock) = clocked("9", true)?;
        set(&mut harness, "10");
        harness.render()?;
        let shifted = BigText::rows_of("9")
            .iter()
            .map(|row| format!("    {row}"))
            .collect::<Vec<_>>();
        assert_eq!(
            screen(&harness),
            shifted,
            "the old digit moves to the right"
        );
        after(&mut harness, &clock, 200)?;
        assert_eq!(screen(&harness), BigText::rows_of("10"));
        Ok(())
    }

    #[test]
    fn motion_that_turns_off_ends_the_roll_at_rest() -> Result<()> {
        let (mut harness, clock) = clocked("12", true)?;
        set(&mut harness, "34");
        harness.render()?;
        after(&mut harness, &clock, 50)?;
        assert_ne!(screen(&harness), BigText::rows_of("34"));
        harness.canopy.set_motion_live(false);
        harness.render()?;
        assert_eq!(screen(&harness), BigText::rows_of("34"));
        assert!(!rolls(&mut harness));
        Ok(())
    }

    #[test]
    fn without_motion_text_changes_at_once() -> Result<()> {
        let (mut harness, _clock) = clocked("12", false)?;
        set(&mut harness, "13");
        harness.render()?;
        assert_eq!(screen(&harness), BigText::rows_of("13"));
        assert!(!rolls(&mut harness));
        set(&mut harness, "13");
        assert!(!rolls(&mut harness), "the same text does not roll");
        Ok(())
    }

    #[test]
    fn polls_drive_a_roll_to_its_end() -> Result<()> {
        let (mut harness, clock) = clocked("12", true)?;
        set(&mut harness, "99");
        let mut frames = 0;
        harness.wait_until(Duration::from_secs(10), |harness| {
            frames += 1;
            clock.advance(ROLL_FRAME)?;
            Ok(!rolls(harness))
        })?;
        harness.render()?;
        assert_eq!(screen(&harness), BigText::rows_of("99"));
        assert!(frames >= 6, "the roll took {frames} frames");
        Ok(())
    }

    /// A blank widget.
    struct Blank;

    impl Widget for Blank {
        fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
            render.fill("", ctx.view().view_rect_local(), ' ')
        }
    }

    /// A root that scrolls a blank area and a text below it, so the blank
    /// area can push the text out of view.
    struct Below {
        /// The text to mount.
        text: Option<BigText>,
        /// The blank area.
        blank: Option<NodeId>,
    }

    impl Widget for Below {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let scroll: NodeId = ctx.add_child(ctx.node_id(), Scroll::vertical())?.into();
            let blank = ctx.add_child(scroll, Blank)?;
            self.blank = Some(blank.into());
            push(ctx, blank.into(), 0)?;
            let text = self.text.take().expect("text");
            ctx.add_child(scroll, text)?;
            Ok(())
        }
    }

    /// Sets the height of the blank area above the text.
    fn push(ctx: &mut dyn Context, blank: NodeId, rows: u32) -> Result<()> {
        ctx.set_layout_override(
            blank,
            LayoutOverride::new().flex_horizontal(1).fixed_height(rows),
        )
    }

    /// Sets the height of the blank area of a harness and renders.
    fn push_by(harness: &mut Harness, rows: u32) -> Result<()> {
        harness.with_root_widget_context(|below: &mut Below, ctx| {
            push(ctx, below.blank.expect("blank"), rows)
        })?;
        harness.render()
    }

    #[test]
    fn a_roll_out_of_view_ends_by_the_clock() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = Harness::builder(Below {
            text: Some(BigText::new("12")),
            blank: None,
        })
        .size(20, 3)
        .clock(Arc::clone(&clock))
        .motion(true)
        .build()?;
        harness.render()?;
        push_by(&mut harness, 3)?;
        harness.with_unique(|text: &mut BigText, _| {
            text.set_text("99");
            Ok(())
        })?;
        // Polls move the roll past its end, though no frame shows.
        let mut frames = 0;
        harness.wait_until(Duration::from_secs(10), |_| {
            frames += 1;
            clock.advance(ROLL_FRAME)?;
            Ok(frames >= 7)
        })?;
        // Then the polls stop.
        let mut turns = 0;
        let idle = harness.wait_until(Duration::from_millis(200), |_| {
            turns += 1;
            clock.advance(ROLL_FRAME)?;
            Ok(false)
        });
        assert!(idle.is_err());
        assert!(turns < 5, "{turns} turns after the end of the roll");
        // The text comes into view at rest.
        push_by(&mut harness, 0)?;
        assert_eq!(screen(&harness), BigText::rows_of("99"));
        assert!(!harness.with_unique(|text: &mut BigText, _| Ok(text.rolling()))?);
        Ok(())
    }
}
