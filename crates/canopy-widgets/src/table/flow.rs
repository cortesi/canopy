//! Width-fitted table previews that wrap long values within their columns.
//!
//! A flowed table lays out every row and never scrolls sideways. Columns that
//! fit keep their natural widths. When they do not, short and single-word
//! columns, such as keys, states, and dates, keep their full widths, and prose
//! columns give up room and wrap at word boundaries. Prose never narrows below
//! a readable width: when the columns cannot all fit, the flow keeps the
//! leading columns that do, and marks its broken right margin. Once any row
//! wraps, rules separate the rows, so each stays distinct from its neighbours.

use std::{iter, mem::size_of, ops::Range, sync::Arc};

use canopy::{
    error::{Error, Result},
    geom::{Line, Point, Rect},
    render::Render,
    text,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{TableCell, TableData, TableGlyphs, display_control};

/// The widest word kept whole when columns are short of room; a longer word
/// breaks between graphemes.
const WORD_CAP: u32 = 24;
/// A column at most this wide keeps its natural width while prose wraps.
const COMPACT_WIDTH: u32 = 16;
/// The narrowest prose column. Prose that would need narrower columns, or
/// more lines than its budget, makes the flow drop trailing columns instead.
const PROSE_MIN: u32 = 12;
/// The widest floor a prose column claims to hold its values in their line
/// budget; longer values end in an ellipsis.
const PROSE_MAX: u32 = 40;
/// Cells between columns: padding, separator, padding.
const GAP: u32 = 3;
/// Cells of the broken right margin: padding and the mark.
const MARGIN: u32 = 2;
/// Bytes of each value measured and wrapped. A flow shows a bounded number of
/// lines, so a longer value ends in an ellipsis.
const CELL_BYTES: usize = 16 * 1024;

/// Bounds of a flowed table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlowOptions {
    /// Total width, separators and any margin mark included.
    pub width: u32,
    /// Most lines one value takes; a longer value ends in an ellipsis.
    pub cell_lines: u32,
    /// Separator, rule, and substitution glyphs.
    pub glyphs: TableGlyphs,
}

/// One line of a wrapped value.
#[derive(Clone, Debug)]
pub(super) struct Fragment {
    /// Bytes of the full value shown on this line.
    pub(super) bytes: Range<usize>,
    /// Display cells of those bytes.
    pub(super) width: u32,
    /// Whether the value continues past this, its last shown line.
    pub(super) cut: bool,
}

/// One display line of a flowed table.
#[derive(Debug)]
enum FlowLine {
    /// A line of the header, or of the body row at an index, with the
    /// fragment of each shown column that reaches it.
    Cells {
        /// Body row index; `None` for the header.
        row: Option<usize>,
        /// Fragment per shown column, in column order.
        fragments: Vec<Option<Fragment>>,
    },
    /// A rule under the header.
    HeaderRule,
    /// A rule between body rows.
    Rule,
}

/// A table fitted to a width, with its values wrapped into lines. The flow
/// retains exactly the snapshot it laid out.
#[derive(Debug)]
pub struct FlowTable {
    /// Snapshot laid out by this flow.
    data: Arc<TableData>,
    /// Glyphs used for separators, rules, and substitutions.
    glyphs: TableGlyphs,
    /// Widths of the shown leading columns.
    widths: Vec<u32>,
    /// First cell of each shown column.
    starts: Vec<u32>,
    /// Width of the shown columns, separators included.
    width: u32,
    /// Whether trailing columns are cut, so the right margin is marked.
    cut: bool,
    /// Display lines, header first.
    lines: Vec<FlowLine>,
    /// Conservative owned bytes of this layout.
    derived_bytes: usize,
}

/// An unbreakable run of a value, or a forced line break.
#[derive(Clone, Debug)]
enum Token {
    /// Graphemes between spaces, with the spaces before them on their line.
    Word {
        /// Bytes of the word.
        bytes: Range<usize>,
        /// Display cells of the word.
        width: u32,
        /// Display cells of the spaces before the word.
        gap: u32,
    },
    /// A newline in the value.
    Break(usize),
}

/// The tokens of a value, and the widths that bound its column.
#[derive(Debug, Default)]
pub(super) struct Measure {
    /// Words and breaks of the measured bytes.
    tokens: Vec<Token>,
    /// Widest line without wrapping.
    pub(super) natural: u32,
    /// Widest word.
    word: u32,
    /// Whether any line holds two words.
    breaks: bool,
    /// Whether measurement stopped before the end of the value.
    partial: bool,
}

/// Display cells of one grapheme; a control takes one substitute cell.
pub(super) fn cells(g: &str) -> u32 {
    if g.chars().any(display_control) {
        1
    } else {
        u32::try_from(text::grapheme_width(g)).unwrap_or(u32::MAX)
    }
}

/// End the open word at a byte, carrying the spaces before it.
fn close(m: &mut Measure, word: &mut Option<(usize, u32)>, at: usize, gap: &mut u32) {
    if let Some((start, width)) = word.take() {
        m.tokens.push(Token::Word {
            bytes: start..at,
            width,
            gap: *gap,
        });
        *gap = 0;
    }
}

/// Split a value into words and breaks within the measured byte bound.
pub(super) fn measure(value: &str) -> Measure {
    let mut end = value.len().min(CELL_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut m = Measure {
        partial: end < value.len(),
        ..Measure::default()
    };
    let (mut line, mut gap, mut words) = (0u32, 0u32, 0usize);
    let mut word: Option<(usize, u32)> = None;
    for (i, g) in value[..end].grapheme_indices(true) {
        match g {
            "\n" | "\r" | "\r\n" => {
                close(&mut m, &mut word, i, &mut gap);
                m.tokens.push(Token::Break(i));
                (line, gap, words) = (0, 0, 0);
            }
            " " => {
                close(&mut m, &mut word, i, &mut gap);
                gap = gap.saturating_add(1);
            }
            _ => {
                let w = cells(g);
                let width = match &mut word {
                    Some((_, width)) => width,
                    None => {
                        words += 1;
                        m.breaks |= words > 1;
                        if words > 1 || line > 0 {
                            line = line.saturating_add(gap);
                        }
                        &mut word.insert((i, 0)).1
                    }
                };
                *width = width.saturating_add(w);
                line = line.saturating_add(w);
                m.word = m.word.max(*width);
                m.natural = m.natural.max(line);
            }
        }
    }
    close(&mut m, &mut word, end, &mut gap);
    m
}

/// Wrap measured tokens into at most `lines` fragments of `width` cells.
pub(super) fn wrap(value: &str, m: &Measure, width: u32, lines: u32) -> Vec<Fragment> {
    let lines = lines.max(1) as usize;
    let mut out: Vec<Fragment> = Vec::new();
    let mut current: Option<Fragment> = None;
    let mut cut = m.partial;
    'tokens: for token in &m.tokens {
        if out.len() >= lines {
            cut = true;
            break;
        }
        match token {
            Token::Break(at) => out.push(current.take().unwrap_or(Fragment {
                bytes: *at..*at,
                width: 0,
                cut: false,
            })),
            Token::Word {
                bytes,
                width: w,
                gap,
            } => {
                if let Some(line) = &mut current
                    && line.width + gap + w <= width
                {
                    line.bytes.end = bytes.end;
                    line.width += gap + w;
                    continue;
                }
                if let Some(line) = current.take() {
                    out.push(line);
                }
                if *w <= width {
                    current = Some(Fragment {
                        bytes: bytes.clone(),
                        width: *w,
                        cut: false,
                    });
                    continue;
                }
                // A word wider than its column breaks between graphemes.
                let mut piece = Fragment {
                    bytes: bytes.start..bytes.start,
                    width: 0,
                    cut: false,
                };
                for (i, g) in value[bytes.clone()].grapheme_indices(true) {
                    let at = bytes.start + i;
                    let gw = cells(g);
                    if piece.width + gw > width && piece.width > 0 {
                        if out.len() + 1 >= lines {
                            out.push(piece);
                            cut = true;
                            break 'tokens;
                        }
                        out.push(piece);
                        piece = Fragment {
                            bytes: at..at,
                            width: 0,
                            cut: false,
                        };
                    }
                    piece.bytes.end = at + g.len();
                    piece.width += gw;
                }
                current = Some(piece);
            }
        }
    }
    if let Some(line) = current {
        if out.len() < lines {
            out.push(line);
        } else {
            cut = true;
        }
    }
    if cut && let Some(last) = out.last_mut() {
        // Room for the ellipsis comes from the end of the last line.
        while last.width + 1 > width && !last.bytes.is_empty() {
            let (i, g) = value[last.bytes.clone()]
                .grapheme_indices(true)
                .next_back()
                .expect("nonempty fragment");
            last.bytes.end = last.bytes.start + i;
            last.width -= cells(g);
        }
        last.cut = true;
    }
    out
}

/// Give each column its floor, then share the remaining room in proportion
/// to how far each column falls short of its natural width.
pub(super) fn distribute(natural: &[u32], floor: &[u32], room: u32) -> Vec<u32> {
    let mut widths = floor.to_vec();
    let extra = u64::from(room - floor.iter().sum::<u32>());
    let slack: Vec<u64> = natural
        .iter()
        .zip(floor)
        .map(|(n, f)| u64::from(n - f))
        .collect();
    let total: u64 = slack.iter().sum();
    if total == 0 {
        return widths;
    }
    let mut given = 0;
    let mut remainders = Vec::with_capacity(widths.len());
    for (i, s) in slack.iter().enumerate() {
        let share = extra * s / total;
        widths[i] += share as u32;
        given += share;
        remainders.push((extra * s % total, i));
    }
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, i) in remainders.into_iter().take((extra - given) as usize) {
        widths[i] += 1;
    }
    widths
}

/// Widths of columns fitted to `room`, or `None` when even their floors do
/// not fit.
fn fit(columns: &[Column], room: u32) -> Option<Vec<u32>> {
    let natural: Vec<u32> = columns.iter().map(|c| c.natural).collect();
    if natural.iter().sum::<u32>() <= room {
        return Some(natural);
    }
    // Each tier gives up more structure. Compact columns hold their widths
    // longest: prose first breaks its long words, then compact columns wrap
    // at their spaces. Prose never narrows below the width that holds its
    // values in their line budget.
    let tiers = [
        |c: &Column| if c.compact() { c.natural } else { c.prose() },
        |c: &Column| if c.compact() { c.natural } else { c.fit },
        |c: &Column| {
            if c.single || !c.compact() {
                c.natural.min(c.fit)
            } else {
                c.word.min(c.natural)
            }
        },
    ];
    tiers.iter().find_map(|tier| {
        let floor: Vec<u32> = columns.iter().map(tier).collect();
        (floor.iter().sum::<u32>() <= room).then(|| distribute(&natural, &floor, room))
    })
}

/// The measured bounds of one column.
#[derive(Clone, Copy, Debug)]
struct Column {
    /// Widest line without wrapping.
    natural: u32,
    /// Widest word.
    word: u32,
    /// Whether every value is a single word.
    single: bool,
    /// Narrowest width, within prose bounds, that holds every value in its
    /// line budget.
    fit: u32,
}
impl Column {
    /// Short columns, and columns of single words, read as structure.
    fn compact(self) -> bool {
        self.natural <= COMPACT_WIDTH || (self.single && self.natural <= WORD_CAP)
    }
    /// A prose column that keeps its words whole where it can.
    fn prose(self) -> u32 {
        self.fit.max(self.word.min(WORD_CAP)).min(self.natural)
    }
}

/// The narrowest width between the prose bounds at which no value of a
/// column needs more than `lines` lines. A value too long to fit at the
/// widest bound ends in an ellipsis at any width, so it sets no floor.
fn fit_width(values: &[(&str, &Measure)], natural: u32, lines: u32) -> u32 {
    let fits = |width: u32| {
        values
            .iter()
            .filter(|(_, m)| !m.partial && m.natural <= PROSE_MAX.saturating_mul(lines))
            .all(|(text, m)| {
                !wrap(text, m, width, lines)
                    .last()
                    .is_some_and(|fragment| fragment.cut)
            })
    };
    let (mut low, mut high) = (PROSE_MIN.min(natural), PROSE_MAX.min(natural));
    if low >= high || !fits(high) {
        return high.max(low);
    }
    // Greedy wrapping needs no more lines at a greater width.
    while low < high {
        let mid = low + (high - low) / 2;
        if fits(mid) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    low
}

impl FlowTable {
    /// Fit the header and every body row to `options.width`, keeping the
    /// leading columns that fit when not all do.
    pub fn prepare(data: Arc<TableData>, options: FlowOptions) -> Result<Self> {
        options.glyphs.validate()?;
        let count = data.columns.len();
        let cell = |row: Option<usize>, column: usize| match row {
            None => &data.columns[column].label,
            Some(row) => &data.rows[row].cells[column],
        };
        let grid: Vec<Vec<Measure>> = iter::once(None)
            .chain((0..data.rows.len()).map(Some))
            .map(|row| (0..count).map(|c| measure(&cell(row, c).text)).collect())
            .collect();
        let bounds: Vec<Column> = (0..count)
            .map(|c| {
                let column = || grid.iter().map(|line| &line[c]);
                let natural = column().map(|m| m.natural).max().unwrap_or(0).max(1);
                let values: Vec<(&str, &Measure)> = grid
                    .iter()
                    .enumerate()
                    .map(|(i, line)| (&*cell(i.checked_sub(1), c).text, &line[c]))
                    .collect();
                Column {
                    natural,
                    word: column().map(|m| m.word).max().unwrap_or(0),
                    single: column().all(|m| !m.breaks),
                    fit: fit_width(&values, natural, options.cell_lines),
                }
            })
            .collect();
        // Keep the most leading columns that fit sensibly; a lone column
        // that cannot fit breaks its words.
        let widths = (1..=count)
            .rev()
            .find_map(|shown| {
                let margin = if shown < count { MARGIN } else { 0 };
                let room = options
                    .width
                    .checked_sub((shown as u32 - 1) * GAP + margin)?;
                fit(&bounds[..shown], room)
            })
            .unwrap_or_else(|| {
                bounds
                    .first()
                    .map(|c| {
                        let margin = if count > 1 { MARGIN } else { 0 };
                        vec![c.natural.min(options.width.saturating_sub(margin).max(1))]
                    })
                    .unwrap_or_default()
            });
        let shown = widths.len();
        let mut starts = Vec::with_capacity(shown);
        let mut width = 0;
        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                width += GAP;
            }
            starts.push(width);
            width += w;
        }
        let blocks: Vec<(Option<usize>, Vec<Vec<Fragment>>)> = grid
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let row = i.checked_sub(1);
                let fragments = line[..shown]
                    .iter()
                    .enumerate()
                    .map(|(c, m)| wrap(&cell(row, c).text, m, widths[c], options.cell_lines))
                    .collect();
                (row, fragments)
            })
            .collect();
        let height =
            |fragments: &[Vec<Fragment>]| fragments.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let ruled = blocks[1..].iter().any(|(_, f)| height(f) > 1);
        let mut lines = Vec::new();
        for (row, fragments) in blocks {
            // Rules separate body rows once any of them wraps.
            if ruled && row.is_some_and(|row| row > 0) {
                lines.push(FlowLine::Rule);
            }
            for line in 0..height(&fragments) {
                lines.push(FlowLine::Cells {
                    row,
                    fragments: fragments.iter().map(|f| f.get(line).cloned()).collect(),
                });
            }
            // A heavy rule closes the header, matching the pitch of ruled
            // rows.
            if row.is_none() {
                lines.push(FlowLine::HeaderRule);
            }
        }
        let derived_bytes = size_of::<Self>()
            .saturating_add(lines.capacity() * size_of::<FlowLine>())
            .saturating_add(lines.len() * shown * size_of::<Option<Fragment>>())
            .saturating_add((widths.capacity() + starts.capacity()) * size_of::<u32>());
        Ok(Self {
            data,
            glyphs: options.glyphs,
            widths,
            starts,
            width,
            cut: shown < count,
            lines,
            derived_bytes,
        })
    }
    /// Display lines, header and rules included.
    pub fn height(&self) -> u32 {
        self.lines.len() as u32
    }
    /// Widths of the shown leading columns.
    pub fn widths(&self) -> &[u32] {
        &self.widths
    }
    /// Whether trailing columns are cut at the right margin.
    pub fn cut(&self) -> bool {
        self.cut
    }
    /// Conservative owned bytes, excluding the shared snapshot.
    pub fn derived_bytes(&self) -> usize {
        self.derived_bytes
    }
    /// Paint the lines from `offset.y` into `area`, starting `offset.x`
    /// cells into the table.
    pub fn paint(&self, render: &mut Render, area: Rect, offset: Point) -> Result<()> {
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill("table", area, ' ')?;
        let end = offset.x.saturating_add(area.w);
        let visible = |x: u32, w: u32| x >= offset.x && x.saturating_add(w) <= end;
        let at = |x: u32, y: u32| Point {
            x: area.tl.x + x - offset.x,
            y,
        };
        let mut glyph = [0; 4];
        let mut put = |render: &mut Render, path: &str, x: u32, y: u32, g: char| {
            if visible(x, 1) {
                render.text(path, Line::new(at(x, y).x, y, 1), g.encode_utf8(&mut glyph))?;
            }
            Ok::<_, Error>(())
        };
        for (i, line) in self
            .lines
            .iter()
            .skip(offset.y as usize)
            .take(area.h as usize)
            .enumerate()
        {
            let y = area.tl.y + i as u32;
            let (row, fragments) = match line {
                FlowLine::Rule | FlowLine::HeaderRule => {
                    let heavy = matches!(line, FlowLine::HeaderRule);
                    let (rule, cross) = if heavy {
                        (self.glyphs.header_rule, self.glyphs.header_cross)
                    } else {
                        (self.glyphs.rule, self.glyphs.cross)
                    };
                    for x in offset.x..self.width.min(end) {
                        let g = if self.starts.contains(&(x + GAP - 1)) {
                            cross
                        } else {
                            rule
                        };
                        put(render, "table/separator", x, y, g)?;
                    }
                    continue;
                }
                FlowLine::Cells { row, fragments } => (row, fragments),
            };
            let path = if row.is_some() {
                "table/cell"
            } else {
                "table/flow/header"
            };
            if self.cut {
                put(
                    render,
                    "table/more",
                    self.width + MARGIN - 1,
                    y,
                    self.glyphs.more,
                )?;
            }
            for (column, fragment) in fragments.iter().enumerate() {
                let (start, width) = (self.starts[column], self.widths[column]);
                if column > 0 {
                    put(
                        render,
                        "table/separator",
                        start - 2,
                        y,
                        self.glyphs.separator,
                    )?;
                }
                let (left, right) = (start.max(offset.x), (start + width).min(end));
                let Some(fragment) = fragment.as_ref().filter(|_| right > left) else {
                    continue;
                };
                let clip = Rect::new(at(left, y).x, y, right - left, 1);
                let cell: &TableCell = match row {
                    None => &self.data.columns[column].label,
                    Some(row) => &self.data.rows[*row].cells[column],
                };
                let used = fragment.width + u32::from(fragment.cut);
                let padding = self.data.columns[column].alignment.padding(width, used);
                let base = render.resolve_style(path);
                let mut x = start + padding;
                for (i, g) in cell.text[fragment.bytes.clone()].grapheme_indices(true) {
                    let byte = fragment.bytes.start + i;
                    let w = cells(g);
                    if visible(x, w) {
                        let style = cell.style_at(render, &base, byte);
                        let point = at(x, y);
                        let mut buf = [0; 4];
                        let g = self.glyphs.display(g, &mut buf);
                        render.put_grapheme(style.resolve_at(clip, point), point, g)?;
                    }
                    x += w;
                }
                if fragment.cut && visible(x, 1) {
                    let point = at(x, y);
                    let mut buf = [0; 4];
                    render.put_grapheme(
                        base.resolve_at(clip, point),
                        point,
                        self.glyphs.ellipsis.encode_utf8(&mut buf),
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{TableColumn, TableRow};

    fn data(labels: &[&str], rows: &[&[&str]]) -> Arc<TableData> {
        let columns = labels
            .iter()
            .enumerate()
            .map(|(i, label)| TableColumn::new(i.to_string(), *label))
            .collect();
        let rows = rows
            .iter()
            .enumerate()
            .map(|(i, cells)| {
                TableRow::new(i.to_string(), cells.iter().map(|c| (*c).into()).collect())
            })
            .collect();
        Arc::new(TableData::new("t", 1, columns, rows).unwrap())
    }
    fn flow(data: Arc<TableData>, width: u32) -> FlowTable {
        FlowTable::prepare(
            data,
            FlowOptions {
                width,
                cell_lines: 3,
                glyphs: TableGlyphs::UNICODE,
            },
        )
        .unwrap()
    }
    /// The text of each display line, shown cells joined by separators.
    fn text(flow: &FlowTable) -> Vec<String> {
        flow.lines
            .iter()
            .map(|line| match line {
                FlowLine::HeaderRule => "=".into(),
                FlowLine::Rule => "-".into(),
                FlowLine::Cells { row, fragments } => fragments
                    .iter()
                    .enumerate()
                    .map(|(c, f)| {
                        let cell = match row {
                            None => &flow.data.columns[c].label,
                            Some(r) => &flow.data.rows[*r].cells[c],
                        };
                        f.as_ref().map_or(String::new(), |f| {
                            let mut s = cell.text[f.bytes.clone()].to_string();
                            if f.cut {
                                s.push('…');
                            }
                            s
                        })
                    })
                    .collect::<Vec<_>>()
                    .join("|"),
            })
            .collect()
    }

    #[test]
    fn fitting_columns_keep_natural_widths_without_a_cap() {
        let note = "a long note that is well past forty cells of natural width";
        let flow = flow(data(&["Stage", "Notes"], &[&["Source", note]]), 100);
        assert_eq!(flow.widths(), &[6, note.len() as u32]);
        assert!(!flow.cut());
        assert_eq!(
            text(&flow),
            vec!["Stage|Notes", "=", &format!("Source|{note}")]
        );
    }

    #[test]
    fn prose_wraps_while_compact_columns_keep_their_width() {
        let flow = flow(
            data(
                &["ID", "State", "Notes"],
                &[
                    &["0001", "in progress", "alpha beta gamma delta epsilon"],
                    &["0002", "done", "short"],
                ],
            ),
            36,
        );
        // Two gaps of three cells leave fifteen for the prose.
        assert_eq!(flow.widths(), &[4, 11, 15]);
        assert_eq!(
            text(&flow),
            vec![
                "ID|State|Notes",
                "=",
                "0001|in progress|alpha beta",
                "||gamma delta",
                "||epsilon",
                "-",
                "0002|done|short",
            ]
        );
    }

    #[test]
    fn values_past_the_line_budget_end_in_an_ellipsis() {
        let long = "word ".repeat(40);
        let flow = flow(data(&["K", "V"], &[&["k", long.trim_end()]]), 30);
        assert_eq!(flow.widths(), &[1, 26]);
        let lines = text(&flow);
        assert_eq!(lines.len(), 5);
        assert!(lines[4].ends_with('…'), "{lines:?}");
    }

    #[test]
    fn newlines_break_lines_and_only_wrapped_rows_take_rules() {
        let wrapped = flow(data(&["A", "B"], &[&["x", "one\ntwo"], &["y", "z"]]), 40);
        assert_eq!(
            text(&wrapped),
            vec!["A|B", "=", "x|one", "|two", "-", "y|z"]
        );
        let flat = flow(data(&["A", "B"], &[&["x", "one"], &["y", "z"]]), 40);
        assert_eq!(text(&flat), vec!["A|B", "=", "x|one", "y|z"]);
    }

    #[test]
    fn every_row_is_laid_out() {
        let rows: Vec<[&str; 1]> = (0..50).map(|_| ["v"]).collect();
        let rows: Vec<&[&str]> = rows.iter().map(|r| &r[..]).collect();
        assert_eq!(flow(data(&["K"], &rows), 20).height(), 52);
    }

    #[test]
    fn trailing_columns_are_cut_before_prose_turns_absurdly_tall() {
        let prose = "a sentence of prose that needs room to stay readable";
        let flow = flow(
            data(
                &["Key", "One", "Two", "Three"],
                &[&["k", prose, prose, prose]],
            ),
            60,
        );
        // Three prose columns cannot each hold their value in three lines,
        // so the flow keeps two and marks the cut.
        assert!(flow.cut());
        assert_eq!(flow.widths().len(), 3);
        assert!(text(&flow).iter().all(|line| !line.ends_with('…')));
        assert!(flow.width + MARGIN <= 60);
    }

    #[test]
    fn a_lone_column_too_wide_for_its_room_breaks_its_words() {
        let flow = flow(
            data(&["Value"], &[&["abcdefghijklmnopqrstuvwxyz0123456789"]]),
            10,
        );
        assert_eq!(flow.widths(), &[10]);
        assert_eq!(
            text(&flow),
            vec!["Value", "=", "abcdefghij", "klmnopqrst", "uvwxyz012…"]
        );
    }
}
