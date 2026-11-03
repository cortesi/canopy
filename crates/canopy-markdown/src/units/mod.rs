//! Edit units: the source lines that an editor of one block changes.
//!
//! An edit unit holds the whole source lines of one block of a
//! [`DocumentSnapshot`], so it keeps the markers of the block's quotes and
//! lists. Blocks whose lines overlap make one unit. Text with no block, such
//! as a link reference definition or a lone block id, has no unit. Units come
//! from the document alone, not from its layout.
//!
//! A unit's range starts at the start of its first line and ends at the end
//! of the text of its last line, before that line's terminator. So a splice
//! of a unit never splits a CRLF pair. The terminators inside a unit are part
//! of it.

use std::ops::Range;

use crate::{
    document::{BlockKind, DocumentSnapshot, ParseLimits, line_text_end, parse_document},
    source_store::SourceStore,
};

/// The kind of block that an edit unit holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditKind {
    /// A paragraph. Its own text follows the markers of its first line.
    Paragraph,
    /// The text of a list item. Its own text follows the item's marker and
    /// any task box.
    ListItem,
    /// The text of a footnote definition. Its own text follows the label.
    Footnote,
    /// A heading of `#` marks. Its own text lies between the marks and any
    /// closing marks.
    AtxHeading,
    /// A heading over a line of `=` or `-`. Its own text is the text lines,
    /// without the underline.
    SetextHeading,
    /// A callout or alert of only its marker line. Its own text is the title.
    CalloutTitle,
    /// Code between fences. Its own text is the lines between the fences.
    FencedCode,
    /// Code indented four columns. Its own text is the code lines.
    IndentedCode,
    /// A table.
    Table,
    /// A thematic rule.
    Rule,
    /// An HTML block.
    Html,
    /// Front matter, which reads as a table when it holds simple
    /// properties.
    FrontMatter,
    /// An image alone in its paragraph.
    Image,
    /// An Obsidian `![[note]]` embed alone in its paragraph.
    Embed,
    /// Text the reader shows as source: a receiving construct, or text past
    /// a parse bound.
    Source,
    /// Blocks that share a line, as one unit.
    Merged,
}

/// The whole source lines of one block, or of blocks that share lines.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EditUnit {
    /// UTF-8 source bytes from the start of the first line to the end of
    /// the text of the last line.
    pub range: Range<usize>,
    /// The bytes that a spoken replace changes, within `range`, or `None`
    /// for a unit with no own text. [`replace_own_text`] keeps the
    /// structure around them.
    pub own: Option<Range<usize>>,
    /// The kind of block the unit holds.
    pub kind: EditKind,
    /// The indices of the unit's blocks in [`DocumentSnapshot::blocks`].
    pub blocks: Range<usize>,
}

/// A replacement of one source range.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Splice {
    /// The UTF-8 source bytes to replace.
    pub range: Range<usize>,
    /// The text that takes their place.
    pub text: String,
}

impl Splice {
    /// Return `source` with this splice applied.
    ///
    /// # Panics
    ///
    /// Panics if the range lies outside `source` or off its character
    /// boundaries.
    pub fn apply(&self, source: &str) -> String {
        let mut out = String::with_capacity(source.len() + self.text.len());
        out.push_str(&source[..self.range.start]);
        out.push_str(&self.text);
        out.push_str(&source[self.range.end..]);
        out
    }
}

/// Return the edit units of `document`, in source order.
pub fn edit_units(document: &DocumentSnapshot) -> Vec<EditUnit> {
    let text = document.source.text();
    let mut units: Vec<EditUnit> = Vec::new();
    for (index, block) in document.blocks.iter().enumerate() {
        let lines = block_lines(&text, &block.source);
        let syntax = document.syntax(index);
        if let Some(last) = units.last_mut()
            && lines.start <= last.range.end
        {
            last.range.end = last.range.end.max(lines.end);
            last.blocks.end = index + 1;
            last.kind = EditKind::Merged;
            last.own = None;
            continue;
        }
        units.push(EditUnit {
            own: syntax
                .and_then(|syntax| syntax.own.clone())
                .map(|own| inside(&text, own, &lines)),
            kind: syntax.map_or(EditKind::Source, |syntax| syntax.kind),
            range: lines,
            blocks: index..index + 1,
        });
    }
    units
}

/// Parse `text` as a complete document with the default parse limits, and
/// return its edit units. A text past a limit reads as one source unit.
pub fn parse_units(text: &str) -> Vec<EditUnit> {
    match SourceStore::static_with_limit(text, usize::MAX) {
        Ok(store) => edit_units(&parse_document(
            &store.snapshot(),
            &ParseLimits::default(),
            None,
        )),
        Err(_) => Vec::new(),
    }
}

/// Return the index of the unit whose range holds `byte`, counting the end
/// of a unit's last line as inside it. With none, return the first unit
/// after `byte`, or else the last unit before it. Return `None` only when
/// `units` is empty.
pub fn unit_at(units: &[EditUnit], byte: usize) -> Option<usize> {
    let next = units.partition_point(|unit| unit.range.end < byte);
    if next < units.len() {
        Some(next)
    } else {
        units.len().checked_sub(1)
    }
}

/// Return the splice that puts `replacement` in place of the own text of
/// `unit` in `source`, keeping the structure of its block. Return `None`
/// for a unit with no own text.
///
/// Line breaks in `replacement` take the line terminator of the unit. Each
/// new line of a paragraph, list item, or footnote takes the prefix of the
/// block's continuation lines: its quote markers, and blanks in place of its
/// list marker. A heading or callout title turns line breaks into spaces.
/// Each new line of code takes the prefix of the block's code lines. Line
/// breaks at the end of `replacement` are dropped, except in code.
pub fn replace_own_text(source: &str, unit: &EditUnit, replacement: &str) -> Option<Splice> {
    let own = unit.own.clone()?;
    let term = line_terminator(source, &unit.range);
    let text = match unit.kind {
        EditKind::AtxHeading | EditKind::SetextHeading | EditKind::CalloutTitle => {
            separated(source, &own, lines(trim_breaks(replacement)).join(" "))
        }
        EditKind::FencedCode | EditKind::IndentedCode => {
            let opening = unit.range.start..line_text_end(source, unit.range.start);
            let prefix = if unit.kind == EditKind::FencedCode {
                let line = &source[opening.clone()];
                let fence = line.find(['`', '~']).unwrap_or(line.len());
                continuation(&line[..fence])
            } else {
                continuation(&source[line_start(source, own.start)..own.start])
            };
            let body = lines(replacement).join(&format!("{term}{prefix}"));
            // A fence of no lines opens its first after its opening line.
            if own.is_empty() && own.start == opening.end && unit.kind == EditKind::FencedCode {
                if body.is_empty() {
                    String::new()
                } else {
                    format!("{term}{prefix}{body}")
                }
            } else {
                body
            }
        }
        _ => {
            let prefix = continuation(&source[line_start(source, own.start)..own.start]);
            separated(
                source,
                &own,
                lines(trim_breaks(replacement)).join(&format!("{term}{prefix}")),
            )
        }
    };
    Some(Splice { range: own, text })
}

/// The place of a new block in a text, and the text around its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewBlock {
    /// The byte where the block goes.
    pub(crate) at: usize,
    /// The text before the block's lines: the breaks that start a block.
    pub(crate) lead: String,
    /// The text that the block's editor opens with: the marker of a new list
    /// item, or nothing.
    pub(crate) seed: String,
    /// The prefix of the block's first line.
    first: String,
    /// The prefix of each line of the block after its first: the markers of
    /// its quotes, and blanks in place of the markers of its list items.
    prefix: String,
    /// The text after the block's lines: a blank line when a line follows
    /// with no blank line between, so that line stays out of the block.
    trail: String,
    /// The line terminator of the block's lines.
    terminator: &'static str,
    /// For a new list item, the paragraph that a text with no list marker
    /// makes instead, in the containers of the item.
    paragraph: Option<Box<Self>>,
}

impl NewBlock {
    /// Return a new block at the end of `source`, outside every container.
    pub(crate) fn at_end(source: &str) -> Self {
        let at = source.len();
        let terminator = line_terminator(source, &(at..at));
        let lead = if source.is_empty() || source.ends_with(&terminator.repeat(2)) {
            String::new()
        } else if source.ends_with(['\n', '\r']) {
            terminator.to_owned()
        } else {
            terminator.repeat(2)
        };
        Self {
            at,
            lead,
            seed: String::new(),
            first: String::new(),
            prefix: String::new(),
            trail: String::new(),
            terminator,
            paragraph: None,
        }
    }

    /// Return a new block after `unit` of `document`, in the quotes and list
    /// items of the unit's first block, with a blank line between them.
    pub(crate) fn after(document: &DocumentSnapshot, unit: &EditUnit) -> Self {
        let source = document.source.text();
        let at = unit.range.end;
        let terminator = line_terminator(&source, &unit.range);
        let prefix = block_prefix(document, unit.blocks.start);
        let blank = prefix.trim_end_matches([' ', '\t']);
        let next = source[at..]
            .strip_prefix(terminator)
            .map(|rest| &rest[..line_text_end(rest, 0)]);
        let trail = match next {
            Some(line) if !is_blank(line) => {
                format!("{terminator}{blank}")
            }
            _ => String::new(),
        };
        Self {
            at,
            lead: format!("{terminator}{blank}{terminator}"),
            seed: String::new(),
            first: prefix.clone(),
            prefix,
            trail,
            terminator,
            paragraph: None,
        }
    }

    /// Return a new item after the list item that `unit` of `document` is the
    /// text of, at the same level: after the item's nested blocks, with the
    /// item's marker, the next number of an ordered list, and an open task
    /// box after a task. The block's text starts with that marker, its seed.
    /// A blank line sets it apart in a loose list. A text with no marker is a
    /// paragraph there instead, in the containers of the item: after a list
    /// that it splits, in the item that holds a nested list, or in a quote,
    /// with blank lines around it. Return `None` for a unit that is no list
    /// item's text.
    pub(crate) fn item_after(document: &DocumentSnapshot, unit: &EditUnit) -> Option<Self> {
        if unit.kind != EditKind::ListItem {
            return None;
        }
        let item = document.syntax(unit.blocks.start)?.item.clone()?;
        let source = document.source.text();
        let start = item.range.start;
        if start > source.len() || !source.is_char_boundary(start) {
            return None;
        }
        let rest = &source[start..];
        let width = marker_width(rest)?;
        let seed = format!(
            "{}{}{}",
            next_marker(&rest[..width]),
            marker_gap(&rest[width..]),
            if task_box(&rest[width..]) { "[ ] " } else { "" }
        );
        // The containers of the item: the text before its marker, with
        // blanks in place of the markers of the items it lies in.
        let first = continuation(&source[line_start(&source, start)..start]);
        let prefix = format!("{first}{}", " ".repeat(seed.chars().count()));
        // The item ends with the lines of its last block.
        let at = document.blocks[unit.blocks.start..]
            .iter()
            .take_while(|block| block.source.start < item.range.end)
            .map(|block| block_lines(&source, &block.source).end)
            .max()
            .unwrap_or(unit.range.end)
            .max(unit.range.end);
        let terminator = line_terminator(&source, &unit.range);
        let blank = first.trim_end_matches([' ', '\t']).to_owned();
        let next = source[at..]
            .strip_prefix(terminator)
            .map(|rest| &rest[..line_text_end(rest, 0)]);
        // A line that follows takes a blank line, so it stays out of a
        // paragraph, and apart from an item of a loose list.
        let apart = match next {
            Some(line) if !is_blank(line) => format!("{terminator}{blank}"),
            _ => String::new(),
        };
        let paragraph = Self {
            at,
            lead: format!("{terminator}{blank}{terminator}"),
            seed: String::new(),
            first: first.clone(),
            prefix: first.clone(),
            trail: apart.clone(),
            terminator,
            paragraph: None,
        };
        let (lead, trail) = if item.loose {
            (format!("{terminator}{blank}{terminator}"), apart)
        } else {
            (terminator.to_owned(), String::new())
        };
        Some(Self {
            at,
            lead,
            seed,
            first,
            prefix,
            trail,
            terminator,
            paragraph: Some(Box::new(paragraph)),
        })
    }

    /// Return the paragraph that `text` makes in place of a new item, when
    /// its first line has no list marker.
    fn paragraph_of(&self, text: &str) -> Option<&Self> {
        self.paragraph
            .as_deref()
            .filter(|_| !starts_item(lines(text)[0]))
    }

    /// Return the source byte where the block of `text` starts, after its
    /// lead.
    pub(crate) fn start(&self, text: &str) -> usize {
        let block = self.paragraph_of(text).unwrap_or(self);
        block.at + block.lead.len()
    }

    /// Return the source text that puts a block of `text` here, or an empty
    /// text when `text` holds nothing past the seed. The first line takes the
    /// first prefix and each other line the prefix, and a blank line takes
    /// it without its trailing blanks. Line breaks at the end of `text` are
    /// dropped.
    pub(crate) fn text(&self, text: &str) -> String {
        if let Some(paragraph) = self.paragraph_of(text) {
            return paragraph.text(text);
        }
        let body = text.strip_prefix(self.seed.as_str()).unwrap_or(text);
        if body.trim().is_empty() {
            return String::new();
        }
        let lines: Vec<String> = lines(trim_breaks(text))
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let prefix = if index == 0 {
                    &self.first
                } else {
                    &self.prefix
                };
                if line.trim().is_empty() {
                    prefix.trim_end_matches([' ', '\t']).to_owned()
                } else {
                    format!("{prefix}{line}")
                }
            })
            .collect();
        format!("{}{}{}", self.lead, lines.join(self.terminator), self.trail)
    }
}

/// Return the width of the list marker at the start of `text`: a bullet of
/// `-`, `*`, or `+`, or up to nine digits with a `.` or `)`.
fn marker_width(text: &str) -> Option<usize> {
    if text.starts_with(['-', '*', '+']) {
        return Some(1);
    }
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    ((1..=9).contains(&digits) && text[digits..].starts_with(['.', ')'])).then_some(digits + 1)
}

/// Return whether `line` starts with a list marker after its indent: a
/// marker with a blank or nothing after it.
fn starts_item(line: &str) -> bool {
    let line = line.trim_start_matches([' ', '\t']);
    marker_width(line).is_some_and(|width| {
        let rest = &line[width..];
        rest.is_empty() || rest.starts_with([' ', '\t'])
    })
}

/// Return the marker of the item after one with `marker`: the same bullet,
/// or the next number with the same delimiter.
fn next_marker(marker: &str) -> String {
    let (number, delimiter) = marker.split_at(marker.len() - 1);
    match number.parse::<u64>() {
        Ok(number) => format!("{}{delimiter}", number.saturating_add(1)),
        Err(_) => marker.to_owned(),
    }
}

/// Return the blanks between a marker and the item's text, from `text`
/// after the marker: the item's own, from one to four, or one space.
fn marker_gap(text: &str) -> String {
    let gap: String = text.chars().take_while(|c| *c == ' ').take(4).collect();
    let rest = &text[gap.len()..];
    if gap.is_empty() || rest.is_empty() || rest.starts_with(['\n', '\r']) {
        " ".to_owned()
    } else {
        gap
    }
}

/// Return whether `text` after a marker starts with a task box.
fn task_box(text: &str) -> bool {
    let rest = text.trim_start_matches([' ', '\t']);
    ["[ ]", "[x]", "[X]"].iter().any(|box_| {
        rest.strip_prefix(box_)
            .is_some_and(|after| after.is_empty() || after.starts_with([' ', '\t', '\n', '\r']))
    })
}

/// Return the splice that deletes `unit` of `document`, with the blank lines
/// that set it apart on one side, so the blocks around it stay apart. With
/// blank lines on both sides, the side with fewer quote markers stays, as it
/// sets apart more blocks. A blank line on one side alone stays. With text on
/// both sides, a blank line of the unit's quotes takes its place, except for
/// a list item right after another, as the next line then has a marker of
/// its own. The first unit of a text goes with the blank lines after it, and
/// the last with those before it. A line of only quote markers is blank.
/// `units` are the units of `document`, and `index` the unit's place in them.
pub(crate) fn delete_unit(
    document: &DocumentSnapshot,
    units: &[EditUnit],
    index: usize,
) -> Option<Splice> {
    let unit = units.get(index)?;
    let source = document.source.text();
    let Range { start, end } = unit.range;
    let following = next_text_start(&source, end);
    let previous = previous_text_end(&source, start);
    let range = match previous {
        None => start..following,
        Some(previous) if following == source.len() => previous..end,
        Some(previous) => match (
            blank_run(&source, previous, start),
            blank_run(&source, end, following),
        ) {
            (Some(before), Some(after)) if before <= after => start..following,
            (Some(_), Some(_)) => previous..end,
            (None, None)
                if !(unit.kind == EditKind::ListItem
                    && index
                        .checked_sub(1)
                        .and_then(|before| units.get(before))
                        .is_some_and(|before| {
                            before.kind == EditKind::ListItem && before.range.end == previous
                        })) =>
            {
                let prefix = block_prefix(document, unit.blocks.start);
                return Some(Splice {
                    range: start..end,
                    text: prefix.trim_end_matches([' ', '\t']).to_owned(),
                });
            }
            _ => start..end + terminator_at(&source, end),
        },
    };
    Some(Splice {
        range,
        text: String::new(),
    })
}

/// Return the fewest quote markers on a blank line between the line whose
/// text ends at `from` and the line that starts at `to`, or `None` with no
/// line between them.
fn blank_run(source: &str, from: usize, to: usize) -> Option<usize> {
    let first = from + terminator_at(source, from);
    let last = to.checked_sub(terminator_before(source, to))?;
    (first <= last).then(|| {
        lines(&source[first..last])
            .iter()
            .map(|line| line.matches('>').count())
            .min()
            .unwrap_or(0)
    })
}

/// Return whether `line` is blank: it holds nothing but quote markers and
/// blanks.
fn is_blank(line: &str) -> bool {
    line.trim_start_matches(['>', ' ', '\t']).is_empty()
}

/// Return the length of the line terminator at byte `at` of `source`, or 0
/// where none starts.
fn terminator_at(source: &str, at: usize) -> usize {
    let rest = &source[at..];
    if rest.starts_with("\r\n") {
        2
    } else {
        usize::from(rest.starts_with(['\n', '\r']))
    }
}

/// Return the length of the line terminator that ends just before byte `at`
/// of `source`, a line start, or 0 at the start of the text.
fn terminator_before(source: &str, at: usize) -> usize {
    let before = &source[..at];
    if before.ends_with("\r\n") {
        2
    } else {
        usize::from(before.ends_with(['\n', '\r']))
    }
}

/// Return the end of the text of the last line before the line that starts
/// at `at` that is not blank, or `None` when every line before it is blank.
fn previous_text_end(source: &str, mut at: usize) -> Option<usize> {
    while at > 0 {
        let end = at - terminator_before(source, at);
        let start = line_start(source, end);
        if !is_blank(&source[start..end]) {
            return Some(end);
        }
        at = start;
    }
    None
}

/// Return the start of the first line after the line that ends at `at` that
/// is not blank, or the end of the text.
fn next_text_start(source: &str, mut at: usize) -> usize {
    loop {
        let width = terminator_at(source, at);
        if width == 0 {
            return source.len();
        }
        let start = at + width;
        let end = line_text_end(source, start);
        if !is_blank(&source[start..end]) {
            return start;
        }
        at = end;
    }
}

/// The indent of the lines of a footnote's body after its first.
const FOOTNOTE_INDENT: &str = "    ";

/// Return the prefix of the lines of a new block in the containers of block
/// `index`: the text before the block on its first line, with blanks in
/// place of list markers. A footnote label takes the indent of a footnote's
/// body. The block's own indent is not part of it: the indent of indented
/// code, and, outside lists, every blank after the last quote marker but
/// one.
fn block_prefix(document: &DocumentSnapshot, index: usize) -> String {
    let text = document.source.text();
    let Some(block) = document.blocks.get(index) else {
        return String::new();
    };
    let mut start = block.source.start.min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let line = &text[line_start(&text, start)..start];
    if let Some((label, rest)) = footnote_label(line) {
        return code_unindented(
            document,
            index,
            format!(
                "{}{FOOTNOTE_INDENT}{}",
                continuation(&line[..label]),
                continuation(&line[rest..])
            ),
        );
    }
    if block.depth > block.quote_depth {
        return code_unindented(document, index, continuation(line));
    }
    // Outside lists, the containers are quotes and footnote bodies.
    let quotes = line.rfind('>').map_or(0, |last| {
        let after = &line[last + 1..];
        last + 1
            + after
                .chars()
                .next()
                .filter(|c| matches!(c, ' ' | '\t'))
                .map_or(0, char::len_utf8)
    });
    // The indent of a footnote's body comes before its quotes.
    if quotes == 0 && matches!(block.kind, BlockKind::Footnote { .. }) {
        return FOOTNOTE_INDENT.to_owned();
    }
    continuation(&line[..quotes])
}

/// Return `prefix` without the indent of indented code, when block `index`
/// is indented code.
fn code_unindented(document: &DocumentSnapshot, index: usize, mut prefix: String) -> String {
    let indented = document
        .syntax(index)
        .is_some_and(|syntax| syntax.kind == EditKind::IndentedCode);
    if indented {
        if prefix.ends_with('\t') {
            prefix.pop();
        } else {
            let spaces = prefix.len() - prefix.trim_end_matches(' ').len();
            prefix.truncate(prefix.len() - spaces.min(4));
        }
    }
    prefix
}

/// Return where the first footnote label of `line` starts, and where the
/// text after the label and its blanks starts.
fn footnote_label(line: &str) -> Option<(usize, usize)> {
    let start = line.find("[^")?;
    let after = start + line[start..].find("]:")? + 2;
    let rest = line.len() - line[after..].trim_start_matches([' ', '\t']).len();
    Some((start, rest))
}

/// Return the bytes of the whole lines that hold `source`, from the start of
/// its first line to the end of the text of its last.
fn block_lines(text: &str, source: &Range<usize>) -> Range<usize> {
    let start = source.start.min(text.len());
    let mut end = source.end.clamp(start, text.len());
    // The block's terminator ends its last line, which holds the byte before.
    while end > start && matches!(text.as_bytes()[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    line_start(text, start)..line_text_end(text, end)
}

/// Return `own` moved inside `lines` and onto character boundaries.
fn inside(text: &str, own: Range<usize>, lines: &Range<usize>) -> Range<usize> {
    let clamp = |at: usize| {
        let mut at = at.clamp(lines.start, lines.end);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let start = clamp(own.start);
    start..clamp(own.end).max(start)
}

/// Return the start of the line that holds byte `at`: the byte after the
/// last line terminator before it, or the start of `text`.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind(['\n', '\r']).map_or(0, |found| found + 1)
}

/// Return the line terminator of the text in `range`: the first one in it,
/// or else the one after it, or else the first in `source`, or else LF.
pub(crate) fn line_terminator(source: &str, range: &Range<usize>) -> &'static str {
    let found = source[range.clone()]
        .find(['\n', '\r'])
        .map(|offset| range.start + offset)
        .or_else(|| (range.end < source.len()).then_some(range.end))
        .or_else(|| source.find(['\n', '\r']));
    match found.map(|at| &source.as_bytes()[at..]) {
        Some([b'\r', b'\n', ..]) => "\r\n",
        Some([b'\r', ..]) => "\r",
        _ => "\n",
    }
}

/// Split `text` into lines at CRLF, LF, and CR.
fn lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['\n', '\r']) {
        out.push(&rest[..at]);
        let width = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
        rest = &rest[at + width..];
    }
    out.push(rest);
    out
}

/// Return `text` without line terminators at its end.
fn trim_breaks(text: &str) -> &str {
    text.trim_end_matches(['\n', '\r'])
}

/// Return the prefix of a block's continuation lines from the text before
/// the own text on its first line. Quote markers and blanks stay, and every
/// other character, such as a list marker, a task box, or a footnote label,
/// becomes one space, so each column stays where it was.
fn continuation(prefix: &str) -> String {
    let mut bracket = false;
    prefix
        .chars()
        .map(|c| {
            let out = match c {
                '>' if !bracket => '>',
                ' ' | '\t' => c,
                _ => ' ',
            };
            match c {
                '[' => bracket = true,
                ']' => bracket = false,
                _ => {}
            }
            out
        })
        .collect()
}

/// Separate a non-empty `text` that replaces an empty own text from the
/// marks around it: a blank after a `#` or `]` before it, and before a
/// closing `#` after it.
fn separated(source: &str, own: &Range<usize>, mut text: String) -> String {
    if !own.is_empty() || text.is_empty() {
        return text;
    }
    if source[..own.start]
        .chars()
        .next_back()
        .is_some_and(|c| !matches!(c, ' ' | '\t' | '\n' | '\r'))
    {
        text.insert(0, ' ');
    }
    if source[own.end..].starts_with('#') {
        text.push(' ');
    }
    text
}

#[cfg(test)]
mod tests;
