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
    document::{DocumentSnapshot, ParseLimits, line_text_end, parse_document},
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
    let term = terminator(source, &unit.range);
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
fn terminator(source: &str, range: &Range<usize>) -> &'static str {
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
