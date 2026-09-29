//! Chart primitives: value scales, stacked bars and columns drawn to an eighth
//! of a cell, and a braille dot canvas.
//!
//! A stacked bar or column is a run of [`Segment`]s. Each segment takes its
//! color from the foreground paint of its style path. Where one segment ends
//! inside a cell and the next begins, the cell shows a partial block: the
//! first segment is its foreground and the next its background, so a stack
//! stays smooth at an eighth of a cell. A cell shows at most two colors. When
//! more segments meet in one cell, the cell splits where it puts the fewest
//! eighths in a wrong color, and each side takes the color of its largest
//! part. On a tie, the larger color gets more of the cell.
//!
//! Cells that no segment fills are blank on the ground: the background of the
//! empty style path under the current layers. A track instead fills the rest
//! of a bar with its own color. A gradient paint resolves over the whole bar
//! or column, so its color at a cell tells how far along the cell is.

use canopy::{
    error::Result,
    geom::{Line, Point, Rect, Size},
    render::Render,
    style::{AttrSet, Mix, Paint, Style},
};

/// Eighths in one cell.
const EIGHTHS: u32 = 8;

/// Left blocks, by the eighths of the cell that they fill.
const LEFT: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Lower blocks, by the eighths of the cell that they fill.
const LOWER: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Braille dot bits, by dot row and dot column within a cell.
const DOT_BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// First code point of the braille patterns.
const BRAILLE_BASE: u32 = 0x2800;

/// A linear map from values onto cells, at an eighth of a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    /// The value at the far end of the scale.
    max: f64,
}

impl Scale {
    /// Returns a scale from zero to `max`. A `max` that is not a positive
    /// finite number gives a scale from zero to one.
    pub fn linear(max: f64) -> Self {
        let max = if max.is_finite() && max > 0.0 {
            max
        } else {
            1.0
        };
        Self { max }
    }

    /// Returns a scale from zero to `max` rounded up to 1, 2, or 5 times a
    /// power of ten, so that its ends and ticks read as round numbers. At the
    /// ends of the floating point range, where the rounded top is not a
    /// positive finite number, the scale keeps the top of [`Self::linear`].
    pub fn nice(max: f64) -> Self {
        let max = Self::linear(max).max;
        let power = 10_f64.powf(max.log10().floor());
        let fraction = max / power;
        // The tolerance keeps a round top such as 200 from rounding up to 500
        // after the division.
        let step = [1.0, 2.0, 5.0]
            .into_iter()
            .find(|step| fraction <= step * (1.0 + 1e-9))
            .unwrap_or(10.0);
        let nice = step * power;
        if nice.is_finite() && nice > 0.0 {
            Self { max: nice }
        } else {
            Self { max }
        }
    }

    /// Returns the value at the far end of the scale.
    pub fn max(&self) -> f64 {
        self.max
    }

    /// Returns the length of `value` in eighths of a cell, on a run of
    /// `cells` cells. A value that is not above zero has no length, and a
    /// value above the top of the scale fills the run.
    pub fn eighths(&self, value: f64, cells: u32) -> u32 {
        let span = cells.saturating_mul(EIGHTHS);
        if value.is_nan() || value <= 0.0 {
            return 0;
        }
        let length = (value / self.max * f64::from(span)).round();
        if length >= f64::from(span) {
            span
        } else {
            // The length is a whole number from zero to `span`, so it fits.
            length as u32
        }
    }

    /// Returns `count` evenly spaced values from zero to the top of the
    /// scale, both ends included. A count below two returns the two ends.
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        let count = count.max(2);
        let last = (count - 1) as f64;
        // The fraction comes first, so a top near the largest float does not
        // overflow on the way.
        (0..count)
            .map(|index| self.max * (index as f64 / last))
            .collect()
    }
}

/// One part of a stacked bar or column: a value and the style path whose
/// foreground paint colors it.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// The value of the part. A value that is not above zero draws nothing.
    pub value: f64,
    /// The style path of the part's color.
    pub style: String,
}

impl Segment {
    /// Returns a segment of `value` in the color of `style`.
    pub fn new(value: f64, style: impl Into<String>) -> Self {
        Self {
            value,
            style: style.into(),
        }
    }
}

/// A change of the segment colors of a column: toward the ground to set it
/// back, or toward the text color to bring it forward. The ground does not
/// change, so the empty cells of the column and the empty part of a cell stay
/// as they are. A tinted segment resolves its paint at rest, so it does not
/// move.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Tint {
    /// The colors as they are.
    #[default]
    None,
    /// Mix toward the ground, from 0.0, which keeps the colors, to 1.0, which
    /// hides the column.
    Mute(f32),
    /// Mix toward the foreground of the empty style path, from 0.0, which
    /// keeps the colors, to 1.0, which draws the column in that color.
    Light(f32),
}

/// One column of a [`ColumnChart`](crate::ColumnChart): a stack above the
/// axis, a stack below it, and the marks of the column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Column {
    /// The stack above the axis, which grows up.
    pub upper: Vec<Segment>,
    /// The stack below the axis, which grows down.
    pub lower: Vec<Segment>,
    /// A value above the axis that a dotted line marks, such as a limit. The
    /// line shows in the cells that the stack leaves empty, and hides when
    /// the value is past the top of the scale.
    pub reference: Option<f64>,
    /// A glyph in the marker lane above the chart.
    pub marker: Option<Marker>,
    /// Whether the column draws muted, behind the others.
    pub muted: bool,
}

/// A glyph in the marker lane of a column, with the style path of its color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// The glyph.
    pub glyph: char,
    /// The style path of its color.
    pub style: String,
    /// Rank among the markers of one cell when columns fit the width: the
    /// marker of the highest rank shows.
    pub rank: u8,
}

impl Marker {
    /// Returns a marker of `glyph` in the color of `style`, of rank zero.
    pub fn new(glyph: char, style: impl Into<String>) -> Self {
        Self {
            glyph,
            style: style.into(),
            rank: 0,
        }
    }

    /// Sets the rank of the marker.
    #[must_use]
    pub fn with_rank(mut self, rank: u8) -> Self {
        self.rank = rank;
        self
    }
}

/// The side of its area that a column grows from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// The column grows up from the bottom row.
    Bottom,
    /// The column grows down from the top row.
    Top,
}

/// The direction in which a run of cells fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    /// Left to right along a row.
    Right,
    /// Bottom to top along a column.
    Up,
    /// Top to bottom along a column.
    Down,
}

/// One tone of a run: a paint that fills the run up to an end.
struct Tone {
    /// End of the tone, in eighths from the start of the run.
    end: u32,
    /// Paint of the tone.
    paint: Paint,
    /// Whether the tone is the ground. A cell of ground alone is blank.
    ground: bool,
}

/// What fills a run after its segments.
enum Rest {
    /// The foreground paint of a track style.
    Track(Paint),
    /// The ground.
    Ground,
}

impl Tone {
    /// Returns whether two tones draw the same.
    fn same(&self, other: &Self) -> bool {
        self.ground == other.ground && self.paint == other.paint
    }
}

/// The two tones of one cell and where the first ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellTones {
    /// Index of the tone of the part of the cell nearest the start of the
    /// run.
    first: usize,
    /// Index of the tone of the rest of the cell.
    second: usize,
    /// Eighths of the cell in the first tone, from 1 to 8.
    split: u32,
}

/// Most pieces of one cell: each covers at least one eighth.
const MAX_PIECES: usize = EIGHTHS as usize;

/// The part of one tone within one cell: the tone's index, and the eighths
/// of the cell that it covers.
type Piece = (usize, u32);

/// Paints a stacked bar in `line`, from left to right.
///
/// The segments stack in order. The cells after the last segment show the
/// foreground paint of `track` when it is given, and the ground otherwise.
pub fn hbar(
    render: &mut Render<'_>,
    line: Line,
    scale: &Scale,
    segments: &[Segment],
    track: Option<&str>,
) -> Result<()> {
    let rest = match track {
        Some(track) => Rest::Track(render.resolve_style(track).fg),
        None => Rest::Ground,
    };
    let rect = Rect::new(line.tl.x, line.tl.y, line.w, 1);
    paint_run(render, rect, Fill::Right, scale, segments, rest, Tint::None)
}

/// Paints a stacked column in `rect`, growing from `base`. Each column of
/// cells in `rect` shows the same stack, and the cells beyond the stack show
/// the ground. `tint` changes every color of the column.
pub fn column(
    render: &mut Render<'_>,
    rect: Rect,
    base: Base,
    scale: &Scale,
    segments: &[Segment],
    tint: Tint,
) -> Result<()> {
    let fill = match base {
        Base::Bottom => Fill::Up,
        Base::Top => Fill::Down,
    };
    paint_run(render, rect, fill, scale, segments, Rest::Ground, tint)
}

/// Returns the ground paint: the background of the empty style path under the
/// current layers.
fn ground(render: &Render<'_>) -> Paint {
    render.resolve_style("").bg
}

/// Paints a run of cells that fills in `fill` direction across `rect`: the
/// segments in order, then `rest` to the end of the run.
fn paint_run(
    render: &mut Render<'_>,
    rect: Rect,
    fill: Fill,
    scale: &Scale,
    segments: &[Segment],
    rest: Rest,
    tint: Tint,
) -> Result<()> {
    let cells = match fill {
        Fill::Right => rect.w,
        Fill::Up | Fill::Down => rect.h,
    };
    if cells == 0 || rect.w == 0 || rect.h == 0 {
        return Ok(());
    }
    let ground = ground(render);
    let tones = tones(render, scale, segments, cells, rest, &ground);
    let tint = match tint {
        Tint::Mute(amount) if amount > 0.0 => Some((ground, amount.min(1.0))),
        Tint::Light(amount) if amount > 0.0 => Some((render.resolve_style("").fg, amount.min(1.0))),
        _ => None,
    };
    let mut cursor = 0;
    // The style of the last pair of tones, which the next cells reuse while
    // the pair stays the same.
    let mut last: Option<((usize, usize), Style)> = None;
    for index in 0..cells {
        let cell = cell_tones(&tones, &mut cursor, index);
        let (glyph, fg, bg) = match fill {
            _ if cell.split == EIGHTHS && tones[cell.first].ground => (' ', cell.first, cell.first),
            _ if cell.split == EIGHTHS => ('█', cell.first, cell.first),
            Fill::Right => (LEFT[cell.split as usize], cell.first, cell.second),
            Fill::Up => (LOWER[cell.split as usize], cell.first, cell.second),
            // A downward fill covers the top of a cell, which the lower block
            // of the rest of the cell draws with its colors swapped.
            Fill::Down => (
                LOWER[(EIGHTHS - cell.split) as usize],
                cell.second,
                cell.first,
            ),
        };
        if last.as_ref().is_none_or(|(pair, _)| *pair != (fg, bg)) {
            let style = Style {
                fg: tones[fg].paint.clone(),
                bg: tones[bg].paint.clone(),
                attrs: AttrSet::default(),
            };
            last = Some(((fg, bg), style));
        }
        let Some((_, style)) = &last else {
            continue;
        };
        // The first cell of this step of the run, and how many cells across
        // the run it spans.
        let (x, y, across) = match fill {
            Fill::Right => (rect.tl.x.saturating_add(index), rect.tl.y, 1),
            Fill::Up => (
                rect.tl.x,
                rect.tl.y.saturating_add(cells - 1 - index),
                rect.w,
            ),
            Fill::Down => (rect.tl.x, rect.tl.y.saturating_add(index), rect.w),
        };
        // A tint changes the tones of the segments and leaves the ground.
        let (fg_ground, bg_ground) = (tones[fg].ground, tones[bg].ground);
        for offset in 0..across {
            let point = Point {
                x: x.saturating_add(offset),
                y,
            };
            if let Some((toward, amount)) = &tint {
                let toward = toward.resolve(rect, point);
                let tinted = |paint: &Paint, ground: bool| {
                    if ground {
                        paint.clone()
                    } else {
                        Paint::solid(paint.resolve(rect, point).mix(toward, *amount, Mix::Oklab))
                    }
                };
                let style = Style {
                    fg: tinted(&style.fg, fg_ground),
                    bg: tinted(&style.bg, bg_ground),
                    attrs: AttrSet::default(),
                };
                render.put_styled(&style, rect, point, glyph)?;
            } else {
                render.put_styled(style, rect, point, glyph)?;
            }
        }
    }
    Ok(())
}

/// Returns the tones of a run of `cells` cells: one for each segment with a
/// length, then `rest` to the end of the run. Each segment ends where the
/// running sum of the values ends, so rounding never adds up along the run.
/// Neighbors that draw the same join into one tone.
fn tones(
    render: &Render<'_>,
    scale: &Scale,
    segments: &[Segment],
    cells: u32,
    rest: Rest,
    ground: &Paint,
) -> Vec<Tone> {
    let span = cells.saturating_mul(EIGHTHS);
    let mut tones = Vec::with_capacity(segments.len() + 1);
    let mut sum = 0.0;
    let mut start = 0;
    for segment in segments {
        if segment.value > 0.0 {
            sum += segment.value;
        }
        let end = scale.eighths(sum, cells);
        if end > start {
            push_tone(
                &mut tones,
                Tone {
                    end,
                    paint: render.resolve_style(&segment.style).fg,
                    ground: false,
                },
            );
            start = end;
        }
    }
    if start < span {
        let rest = match rest {
            Rest::Track(paint) => Tone {
                end: span,
                paint,
                ground: false,
            },
            Rest::Ground => Tone {
                end: span,
                paint: ground.clone(),
                ground: true,
            },
        };
        push_tone(&mut tones, rest);
    }
    tones
}

/// Adds a tone to a run, or extends the last tone when both draw the same.
fn push_tone(tones: &mut Vec<Tone>, tone: Tone) {
    match tones.last_mut() {
        Some(last) if last.same(&tone) => last.end = tone.end,
        _ => tones.push(tone),
    }
}

/// Returns the two tones of cell `index` of a run, and where the first ends.
/// The cells of a run come in order, and `cursor` holds the first tone that
/// can reach the next cell.
///
/// With more than two tones in the cell, the split goes where it puts the
/// fewest eighths in a wrong tone. Each side takes the tone that covers most
/// of it, and tones that draw the same count together. On a tie, the larger
/// of the two tones gets more of the cell.
fn cell_tones(tones: &[Tone], cursor: &mut usize, index: u32) -> CellTones {
    let start = index * EIGHTHS;
    let end = start + EIGHTHS;
    while tones[*cursor].end <= start {
        *cursor += 1;
    }
    let mut pieces: [Piece; MAX_PIECES] = [(0, 0); MAX_PIECES];
    let mut count = 0;
    let mut from = if *cursor == 0 {
        0
    } else {
        tones[*cursor - 1].end
    };
    for (offset, tone) in tones[*cursor..].iter().enumerate() {
        let (low, high) = (from.max(start), tone.end.min(end));
        if high > low {
            pieces[count] = (*cursor + offset, high - low);
            count += 1;
        }
        from = tone.end;
        if from >= end {
            break;
        }
    }
    let pieces = &pieces[..count];
    if let [(only, _)] = *pieces {
        return CellTones {
            first: only,
            second: only,
            split: EIGHTHS,
        };
    }
    // Each candidate is its misplaced eighths, the eighths of its larger
    // tone, and its tones.
    let mut best: Option<(u32, u32, CellTones)> = None;
    for boundary in 1..pieces.len() {
        let (left, right) = pieces.split_at(boundary);
        let (first, first_len) = largest(tones, left);
        let (second, second_len) = largest(tones, right);
        let split: u32 = left.iter().map(|(_, len)| len).sum();
        let misplaced = (split - first_len) + (EIGHTHS - split - second_len);
        let larger = if first_len >= second_len {
            split
        } else {
            EIGHTHS - split
        };
        let better = best.as_ref().is_none_or(|(cost, share, _)| {
            misplaced < *cost || (misplaced == *cost && larger > *share)
        });
        if better {
            best = Some((
                misplaced,
                larger,
                CellTones {
                    first,
                    second,
                    split,
                },
            ));
        }
    }
    best.map(|(_, _, tones)| tones)
        .expect("a cell with two or more tones has a boundary")
}

/// Returns the tone that covers most of `pieces`, with the eighths that it
/// covers. Tones that draw the same count together, and the first of equal
/// tones wins.
fn largest(tones: &[Tone], pieces: &[Piece]) -> (usize, u32) {
    let mut best = (pieces[0].0, 0);
    for &(tone, _) in pieces {
        let covered = pieces
            .iter()
            .filter(|(other, _)| tones[*other].same(&tones[tone]))
            .map(|(_, len)| len)
            .sum();
        if covered > best.1 {
            best = (tone, covered);
        }
    }
    best
}

/// A canvas of braille dots, two dots wide and four dots high in each cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Braille {
    /// Size of the canvas in cells.
    size: Size,
    /// Dot bits of each cell, row by row.
    cells: Vec<u8>,
}

impl Braille {
    /// Returns an empty canvas of `size` cells.
    pub fn new(size: Size) -> Self {
        let count = usize::try_from(u64::from(size.w) * u64::from(size.h)).unwrap_or(0);
        Self {
            size,
            cells: vec![0; count],
        }
    }

    /// Returns the size of the canvas in dots.
    pub fn dots(&self) -> Size {
        Size::new(self.size.w.saturating_mul(2), self.size.h.saturating_mul(4))
    }

    /// Clears every dot.
    pub fn clear(&mut self) {
        self.cells.fill(0);
    }

    /// Sets the dot at `point`, in dots from the top left. A point outside
    /// the canvas sets nothing.
    pub fn set(&mut self, point: Point) {
        let dots = self.dots();
        if point.x >= dots.w || point.y >= dots.h {
            return;
        }
        let index = (point.y / 4) as usize * self.size.w as usize + (point.x / 2) as usize;
        if let Some(cell) = self.cells.get_mut(index) {
            *cell |= DOT_BITS[(point.y % 4) as usize][(point.x % 2) as usize];
        }
    }

    /// Sets the dots of a straight line from `from` to `to`, both included.
    /// The parts of the line outside the canvas set nothing: the line is cut
    /// to the canvas first, so a distant end costs no time.
    pub fn line(&mut self, from: Point, to: Point) {
        let Some((from, to)) = self.clip(from, to) else {
            return;
        };
        let (mut x, mut y) = (i64::from(from.x), i64::from(from.y));
        let (x1, y1) = (i64::from(to.x), i64::from(to.y));
        let dx = (x1 - x).abs();
        let dy = -(y1 - y).abs();
        let step_x = if x < x1 { 1 } else { -1 };
        let step_y = if y < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            // The coordinates stay between the two ends, so they fit.
            self.set(Point {
                x: x as u32,
                y: y as u32,
            });
            if x == x1 && y == y1 {
                return;
            }
            let double = 2 * error;
            if double >= dy {
                error += dy;
                x += step_x;
            }
            if double <= dx {
                error += dx;
                y += step_y;
            }
        }
    }

    /// Returns the part of the line from `from` to `to` within the canvas,
    /// with its ends rounded to dots, or `None` when the line misses it.
    fn clip(&self, from: Point, to: Point) -> Option<(Point, Point)> {
        let dots = self.dots();
        if dots.w == 0 || dots.h == 0 {
            return None;
        }
        let (x0, y0) = (f64::from(from.x), f64::from(from.y));
        let (dx, dy) = (f64::from(to.x) - x0, f64::from(to.y) - y0);
        let (right, bottom) = (f64::from(dots.w - 1), f64::from(dots.h - 1));
        // Liang–Barsky: each side of the canvas bounds the line parameter.
        let (mut low, mut high) = (0.0_f64, 1.0_f64);
        for (p, q) in [(-dx, x0), (dx, right - x0), (-dy, y0), (dy, bottom - y0)] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    low = low.max(t);
                } else {
                    high = high.min(t);
                }
            }
        }
        if low > high {
            return None;
        }
        // The cut ends lie within the canvas, so they round to dots in it.
        let point = |t: f64| Point {
            x: (x0 + t * dx).round().clamp(0.0, right) as u32,
            y: (y0 + t * dy).round().clamp(0.0, bottom) as u32,
        };
        Some((point(low), point(high)))
    }

    /// Returns the text of cell row `row`: a braille pattern for each cell
    /// with dots, and a space for each cell without.
    pub fn row_text(&self, row: u32) -> String {
        if row >= self.size.h {
            return String::new();
        }
        let width = self.size.w as usize;
        let start = row as usize * width;
        self.cells[start..start + width]
            .iter()
            .map(|&bits| match bits {
                0 => ' ',
                bits => char::from_u32(BRAILLE_BASE + u32::from(bits)).unwrap_or(' '),
            })
            .collect()
    }

    /// Paints the canvas with its top left cell at `origin`, in the style
    /// `style`.
    pub fn paint(&self, render: &mut Render<'_>, origin: Point, style: &str) -> Result<()> {
        for row in 0..self.size.h {
            let line = Line::new(origin.x, origin.y.saturating_add(row), self.size.w);
            render.text(style, line, &self.row_text(row))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        NodeName, ViewContext, Widget,
        geom::Point,
        layout::Layout,
        style::{Color, GradientSpec, GradientStop, ResolvedStyle},
        testing::harness::Harness,
    };

    use super::*;

    /// A function that paints into a render.
    type PaintFn = Box<dyn Fn(&mut Render<'_>) -> Result<()>>;

    /// A widget that runs one paint function when it renders.
    struct Painter {
        /// The paint function.
        paint: PaintFn,
    }

    impl Widget for Painter {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
            render.fill("", ctx.view().view_rect_local(), ' ')?;
            (self.paint)(render)
        }

        fn name(&self) -> NodeName {
            NodeName::convert("painter")
        }
    }

    /// Colors of the test segments and the track.
    const A: Color = Color::Rgb { r: 200, g: 0, b: 0 };
    /// Second segment color.
    const B: Color = Color::Rgb { r: 0, g: 200, b: 0 };
    /// Third segment color.
    const C: Color = Color::Rgb { r: 0, g: 0, b: 200 };
    /// Track color.
    const T: Color = Color::Rgb {
        r: 60,
        g: 60,
        b: 60,
    };

    /// Renders `paint` on a `width` × `height` screen with the test colors.
    fn painted(
        width: u32,
        height: u32,
        paint: impl Fn(&mut Render<'_>) -> Result<()> + 'static,
    ) -> Harness {
        let mut harness = Harness::builder(Painter {
            paint: Box::new(paint),
        })
        .size(width, height)
        .configure(|setup| {
            setup.widget_styles(|_palette, rules| {
                let ramp = GradientSpec::with_stops(
                    0.0,
                    vec![GradientStop::new(0.0, A), GradientStop::new(1.0, C)],
                );
                rules
                    .fg("a", A)
                    .fg("a2", A)
                    .fg("b", B)
                    .fg("c", C)
                    .fg("track", T)
                    .fg("ramp", Paint::gradient(ramp))
                    .apply();
            });
            Ok(())
        })
        .build()
        .expect("harness");
        harness.render().expect("render");
        harness
    }

    /// Returns the glyph and the resolved style of one cell.
    fn cell(harness: &Harness, x: u32, y: u32) -> (char, ResolvedStyle) {
        let cell = harness.buf().get(Point { x, y }).expect("cell");
        (cell.ch, cell.style)
    }

    /// Returns the text of one screen row, without trailing spaces.
    fn row(harness: &Harness, y: u32) -> String {
        harness.tbuf().lines()[y as usize].trim_end().to_owned()
    }

    #[test]
    fn a_linear_scale_needs_a_positive_top() {
        assert_eq!(Scale::linear(40.0).max(), 40.0);
        assert_eq!(Scale::linear(0.0).max(), 1.0);
        assert_eq!(Scale::linear(-3.0).max(), 1.0);
        assert_eq!(Scale::linear(f64::NAN).max(), 1.0);
        assert_eq!(Scale::linear(f64::INFINITY).max(), 1.0);
    }

    #[test]
    fn a_nice_scale_rounds_its_top_up_to_one_two_or_five() {
        let top = |max| Scale::nice(max).max();
        assert_eq!(top(0.7), 1.0);
        assert_eq!(top(1.0), 1.0);
        assert_eq!(top(1.2), 2.0);
        assert_eq!(top(3.0), 5.0);
        assert_eq!(top(7.0), 10.0);
        assert_eq!(top(200.0), 200.0);
        assert_eq!(top(272_000.0), 500_000.0);
        assert!((top(0.013) - 0.02).abs() < 1e-12);
    }

    #[test]
    fn eighths_round_and_stay_within_the_run() {
        let scale = Scale::linear(100.0);
        assert_eq!(scale.eighths(50.0, 10), 40);
        assert_eq!(scale.eighths(1.0, 10), 1, "0.8 eighths round up");
        assert_eq!(scale.eighths(0.5, 10), 0, "0.4 eighths round down");
        assert_eq!(scale.eighths(250.0, 10), 80, "a value above the top fills");
        assert_eq!(scale.eighths(-5.0, 10), 0);
        assert_eq!(scale.eighths(f64::NAN, 10), 0);
        assert_eq!(scale.eighths(100.0, 0), 0);
    }

    #[test]
    fn ticks_span_the_scale() {
        assert_eq!(Scale::linear(10.0).ticks(3), vec![0.0, 5.0, 10.0]);
        assert_eq!(Scale::linear(10.0).ticks(0), vec![0.0, 10.0]);
        let ticks = Scale::linear(f64::MAX).ticks(3);
        assert!(ticks.iter().all(|tick| tick.is_finite()), "{ticks:?}");
        assert_eq!(ticks[2], f64::MAX);
    }

    #[test]
    fn a_nice_scale_stays_finite_at_the_ends_of_the_float_range() {
        for max in [5e-324, f64::MIN_POSITIVE / 4.0, 1.1e308, f64::MAX] {
            let top = Scale::nice(max).max();
            assert!(top.is_finite() && top > 0.0, "{max} gives {top}");
            assert!(top >= max, "{max} gives {top}");
        }
    }

    #[test]
    fn a_bar_ends_on_an_eighth_block_over_its_track() {
        // 5 cells hold 40 eighths. 21 of 40 fill 2 cells and 5 eighths.
        let harness = painted(6, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 5),
                &Scale::linear(40.0),
                &[Segment::new(21.0, "a")],
                Some("track"),
            )
        });
        assert_eq!(row(&harness, 0), "██▋██");
        assert_eq!(cell(&harness, 0, 0).1.fg, A);
        let (glyph, style) = cell(&harness, 2, 0);
        assert_eq!(glyph, '▋');
        assert_eq!(
            (style.fg, style.bg),
            (A, T),
            "the segment ends over the track"
        );
        assert_eq!(cell(&harness, 4, 0).1.fg, T);
    }

    #[test]
    fn two_segments_share_a_cell_at_their_boundary() {
        // 3 + 3 eighths share the first cell, and the ground takes the rest.
        let harness = painted(2, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 2),
                &Scale::linear(16.0),
                &[Segment::new(3.0, "a"), Segment::new(3.0, "b")],
                None,
            )
        });
        let (glyph, style) = cell(&harness, 0, 0);
        assert_eq!(glyph, '▍');
        assert_eq!((style.fg, style.bg), (A, B));
        let (glyph, style) = cell(&harness, 1, 0);
        assert_eq!(glyph, ' ', "a cell of ground alone is blank");
        assert_ne!(style.bg, B);
    }

    #[test]
    fn three_segments_in_a_cell_split_where_the_fewest_eighths_move() {
        // One cell holds a 2, a 1, and a 5. Either split moves one eighth, and
        // the larger color, C, takes more of the cell.
        let harness = painted(1, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 1),
                &Scale::linear(8.0),
                &[
                    Segment::new(2.0, "a"),
                    Segment::new(1.0, "b"),
                    Segment::new(5.0, "c"),
                ],
                None,
            )
        });
        let (glyph, style) = cell(&harness, 0, 0);
        assert_eq!(glyph, '▎');
        assert_eq!((style.fg, style.bg), (A, C));
        // A 5, a 1, and a 2: now A is the larger color.
        let harness = painted(1, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 1),
                &Scale::linear(8.0),
                &[
                    Segment::new(5.0, "a"),
                    Segment::new(1.0, "b"),
                    Segment::new(2.0, "c"),
                ],
                None,
            )
        });
        let (glyph, style) = cell(&harness, 0, 0);
        assert_eq!(glyph, '▊');
        assert_eq!((style.fg, style.bg), (A, C));
    }

    #[test]
    fn segments_that_draw_the_same_count_together() {
        // A 2 and a 2 are one A of 4, so the cell splits A and B evenly and
        // misplaces the one eighth of C. Two paths of one color count as one.
        for first in ["a", "a2"] {
            let harness = painted(1, 1, move |r| {
                hbar(
                    r,
                    Line::new(0, 0, 1),
                    &Scale::linear(8.0),
                    &[
                        Segment::new(2.0, "a"),
                        Segment::new(2.0, first),
                        Segment::new(3.0, "b"),
                        Segment::new(1.0, "c"),
                    ],
                    None,
                )
            });
            let (glyph, style) = cell(&harness, 0, 0);
            assert_eq!(glyph, '▌', "with {first}");
            assert_eq!((style.fg, style.bg), (A, B), "with {first}");
        }
    }

    #[test]
    fn equal_segments_end_on_the_run_without_a_gap() {
        // Thirds of one cell end at 3, 5, and 8 eighths: nothing is left for
        // the ground, and the split falls where it misplaces least.
        let harness = painted(1, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 1),
                &Scale::linear(3.0),
                &[
                    Segment::new(1.0, "a"),
                    Segment::new(1.0, "b"),
                    Segment::new(1.0, "c"),
                ],
                None,
            )
        });
        let (glyph, style) = cell(&harness, 0, 0);
        assert_eq!(glyph, '▋');
        assert_eq!((style.fg, style.bg), (A, C));
    }

    #[test]
    fn a_gradient_spans_the_whole_bar_whatever_the_fill() {
        // A half bar and a full bar show the same colors where both fill.
        let harness = painted(8, 2, |r| {
            let scale = Scale::linear(8.0);
            hbar(
                r,
                Line::new(0, 0, 8),
                &scale,
                &[Segment::new(4.0, "ramp")],
                None,
            )?;
            hbar(
                r,
                Line::new(0, 1, 8),
                &scale,
                &[Segment::new(8.0, "ramp")],
                None,
            )
        });
        for x in 0..4 {
            assert_eq!(
                cell(&harness, x, 0).1.fg,
                cell(&harness, x, 1).1.fg,
                "at {x}"
            );
        }
        assert_ne!(cell(&harness, 0, 1).1.fg, cell(&harness, 7, 1).1.fg);
    }

    #[test]
    fn empty_and_negative_segments_draw_nothing() {
        let harness = painted(3, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 3),
                &Scale::linear(24.0),
                &[
                    Segment::new(0.0, "a"),
                    Segment::new(-4.0, "b"),
                    Segment::new(8.0, "c"),
                ],
                Some("track"),
            )
        });
        assert_eq!(cell(&harness, 0, 0).1.fg, C);
        assert_eq!(cell(&harness, 1, 0).1.fg, T);
    }

    #[test]
    fn a_column_grows_up_from_its_base() {
        // 3 rows hold 24 eighths: 12 of A, then 9 of B.
        let harness = painted(1, 3, |r| {
            column(
                r,
                Rect::new(0, 0, 1, 3),
                Base::Bottom,
                &Scale::linear(24.0),
                &[Segment::new(12.0, "a"), Segment::new(9.0, "b")],
                Tint::None,
            )
        });
        let bottom = cell(&harness, 0, 2);
        assert_eq!((bottom.0, bottom.1.fg), ('█', A));
        let middle = cell(&harness, 0, 1);
        assert_eq!(middle.0, '▄', "A fills the lower half of the middle row");
        assert_eq!((middle.1.fg, middle.1.bg), (A, B));
        let top = cell(&harness, 0, 0);
        assert_eq!(top.0, '▅', "B ends 5 eighths up the top row");
        assert_eq!(top.1.fg, B);
        assert_ne!(top.1.bg, B, "the ground is above the column");
    }

    #[test]
    fn a_column_grows_down_with_complementary_blocks() {
        // 2 rows: A fills 3 eighths from the top of the first row.
        let harness = painted(1, 2, |r| {
            column(
                r,
                Rect::new(0, 0, 1, 2),
                Base::Top,
                &Scale::linear(16.0),
                &[Segment::new(3.0, "a")],
                Tint::None,
            )
        });
        let (glyph, style) = cell(&harness, 0, 0);
        assert_eq!(glyph, '▅', "the lower 5 eighths show the ground");
        assert_eq!(style.bg, A, "the top 3 eighths show A");
        assert_ne!(style.fg, A);
        let (glyph, style) = cell(&harness, 0, 1);
        assert_eq!(glyph, ' ', "the second row is ground");
        assert_ne!(style.bg, A);
    }

    #[test]
    fn two_segments_meet_inside_a_downward_cell() {
        // A fills the top half of the first row, and B its lower half.
        let harness = painted(1, 2, |r| {
            column(
                r,
                Rect::new(0, 0, 1, 2),
                Base::Top,
                &Scale::linear(16.0),
                &[Segment::new(4.0, "a"), Segment::new(4.0, "b")],
                Tint::None,
            )
        });
        let (glyph, style) = cell(&harness, 0, 0);
        assert_eq!(glyph, '▄');
        assert_eq!((style.fg, style.bg), (B, A));
    }

    #[test]
    fn a_wide_column_repeats_its_stack_and_a_muted_one_fades() {
        let harness = painted(4, 1, |r| {
            let stack = [Segment::new(8.0, "a")];
            let scale = Scale::linear(8.0);
            column(
                r,
                Rect::new(0, 0, 2, 1),
                Base::Bottom,
                &scale,
                &stack,
                Tint::None,
            )?;
            column(
                r,
                Rect::new(2, 0, 2, 1),
                Base::Bottom,
                &scale,
                &stack,
                Tint::Mute(0.5),
            )
        });
        assert_eq!(cell(&harness, 0, 0).1.fg, A);
        assert_eq!(cell(&harness, 1, 0).1.fg, A);
        let muted = cell(&harness, 2, 0).1.fg;
        assert_ne!(muted, A, "a muted column mixes toward the ground");
        assert_eq!(cell(&harness, 3, 0).1.fg, muted);
    }

    #[test]
    fn a_light_tint_changes_the_tones_and_leaves_the_ground() {
        let harness = painted(2, 2, |r| {
            let stack = [Segment::new(8.0, "a")];
            let scale = Scale::linear(16.0);
            column(
                r,
                Rect::new(0, 0, 1, 2),
                Base::Bottom,
                &scale,
                &stack,
                Tint::None,
            )?;
            column(
                r,
                Rect::new(1, 0, 1, 2),
                Base::Bottom,
                &scale,
                &stack,
                Tint::Light(0.5),
            )
        });
        let (plain, lit) = (cell(&harness, 0, 1).1, cell(&harness, 1, 1).1);
        assert_eq!(plain.fg, A);
        assert_ne!(lit.fg, A, "the segment lightens");
        let (ground, lit_ground) = (cell(&harness, 0, 0).1, cell(&harness, 1, 0).1);
        assert_eq!(lit_ground.bg, ground.bg, "the ground above the stack stays");
    }

    #[test]
    fn a_zero_sized_area_draws_nothing() {
        let harness = painted(2, 1, |r| {
            hbar(
                r,
                Line::new(0, 0, 0),
                &Scale::linear(1.0),
                &[Segment::new(1.0, "a")],
                None,
            )?;
            column(
                r,
                Rect::new(0, 0, 0, 1),
                Base::Bottom,
                &Scale::linear(1.0),
                &[Segment::new(1.0, "a")],
                Tint::None,
            )
        });
        assert_ne!(cell(&harness, 0, 0).1.fg, A);
    }

    #[test]
    fn braille_dots_map_onto_patterns() {
        let mut canvas = Braille::new(Size::new(2, 1));
        assert_eq!(canvas.dots(), Size::new(4, 4));
        for y in 0..4 {
            for x in 0..2 {
                canvas.set(Point { x, y });
            }
        }
        canvas.set(Point { x: 3, y: 3 });
        canvas.set(Point { x: 9, y: 9 });
        assert_eq!(canvas.row_text(0), "⣿⢀");
        canvas.clear();
        assert_eq!(canvas.row_text(0), "  ");
    }

    #[test]
    fn a_braille_line_sets_every_dot_between_its_ends() {
        let mut canvas = Braille::new(Size::new(2, 1));
        canvas.line(Point { x: 0, y: 3 }, Point { x: 3, y: 0 });
        // Dots (0,3), (1,2), (2,1), (3,0).
        assert_eq!(canvas.row_text(0), "⡠⠊");
        let mut canvas = Braille::new(Size::new(1, 1));
        canvas.line(Point { x: 1, y: 0 }, Point { x: 1, y: 3 });
        assert_eq!(canvas.row_text(0), "⢸");
    }

    #[test]
    fn a_braille_line_is_cut_to_the_canvas() {
        let mut canvas = Braille::new(Size::new(1, 1));
        // A distant end costs no time, and the line keeps its course.
        canvas.line(Point { x: 0, y: 0 }, Point { x: u32::MAX, y: 0 });
        assert_eq!(canvas.row_text(0), "⠉");
        canvas.clear();
        canvas.line(Point { x: 10, y: 3 }, Point { x: 0, y: 3 });
        assert_eq!(canvas.row_text(0), "⣀", "a line from outside crosses");
        canvas.clear();
        canvas.line(Point { x: 5, y: 5 }, Point { x: 9, y: 9 });
        assert_eq!(canvas.row_text(0), " ", "a line outside draws nothing");
        let mut empty = Braille::new(Size::new(0, 0));
        empty.line(Point { x: 0, y: 0 }, Point { x: 3, y: 3 });
        empty.set(Point { x: 0, y: 0 });
        assert_eq!(empty.dots(), Size::new(0, 0));
    }

    #[test]
    fn a_braille_canvas_paints_its_rows() {
        let harness = painted(3, 2, |r| {
            let mut canvas = Braille::new(Size::new(2, 2));
            canvas.line(Point { x: 0, y: 0 }, Point { x: 0, y: 7 });
            canvas.paint(r, Point { x: 1, y: 0 }, "a")
        });
        assert_eq!(row(&harness, 0), " ⡇");
        assert_eq!(row(&harness, 1), " ⡇");
        assert_eq!(cell(&harness, 1, 0).1.fg, A);
    }
}
