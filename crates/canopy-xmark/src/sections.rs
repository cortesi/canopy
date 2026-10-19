//! Sections: the part of a note that a link's or an embed's anchor names.

use alloc::{string::String, vec::Vec};
use core::ops::Range;

use crate::{Event, Options, Parser, Tag, TagEnd, heading_slug, hide_comments};

/// A heading, as a section starts at it.
struct Heading {
    /// Its level, 1 to 6.
    level: u8,
    /// Its id.
    id: String,
    /// Its text.
    text: String,
    /// Its source range.
    range: Range<usize>,
}

/// Return the source range of the section of `text` that `anchor` names, as
/// Obsidian reads it with [`Options::obsidian`].
///
/// A `^id` names the block that holds that block id, the first such block.
/// Any other anchor names a heading: by its id, by the id it would take, or
/// by its text without regard to case. A heading's section runs to the next
/// heading of the same or a higher level, or to the end.
#[must_use]
pub fn section(text: &str, anchor: &str) -> Option<Range<usize>> {
    let anchor = anchor.trim();
    if anchor.is_empty() {
        return None;
    }
    let (headings, blocks) = outline(text);
    if let Some(id) = anchor.strip_prefix('^') {
        return blocks
            .into_iter()
            .find(|(block, _)| block == id)
            .map(|(_, range)| range);
    }
    let slug = heading_slug(anchor);
    let at = headings.iter().position(|heading| {
        heading.id == anchor
            || heading.id == slug
            || heading.text.trim().eq_ignore_ascii_case(anchor)
    })?;
    let heading = &headings[at];
    let end = headings[at + 1..]
        .iter()
        .find(|next| next.level <= heading.level)
        .map_or(text.len(), |next| next.range.start);
    Some(heading.range.start..end)
}

/// The headings of `text`, and its block ids with the ranges of the blocks
/// they name.
fn outline(text: &str) -> (Vec<Heading>, Vec<(String, Range<usize>)>) {
    let options = Options::obsidian();
    let hidden = hide_comments(text, options);
    let mut headings = Vec::new();
    let mut blocks = Vec::new();
    // The open blocks, and the last block closed at each depth.
    let mut open: Vec<Range<usize>> = Vec::new();
    let mut closed: Vec<Option<Range<usize>>> = Vec::new();
    let mut heading: Option<Heading> = None;
    // Whether inline content came last: a block id after it ends a block,
    // and one after a block stands between blocks.
    let mut inline = false;
    for (event, range) in Parser::new_ext(&hidden, options).into_offset_iter() {
        let block = match &event {
            Event::Start(tag) => is_block(tag),
            Event::End(end) => is_block_end(*end),
            Event::BlockId(_) => inline,
            _ => false,
        };
        let after_inline = inline;
        inline = !block;
        match event {
            Event::Start(tag) => {
                if let Tag::Heading { level, id, .. } = &tag {
                    heading = Some(Heading {
                        level: *level as u8,
                        id: id.as_deref().unwrap_or_default().into(),
                        text: String::new(),
                        range: range.clone(),
                    });
                }
                if is_block(&tag) {
                    open.push(range);
                    closed.truncate(open.len());
                }
            }
            Event::End(end) => {
                if let (TagEnd::Heading(_), Some(done)) = (end, heading.take()) {
                    headings.push(done);
                }
                if is_block_end(end)
                    && let Some(block) = open.pop()
                {
                    closed.resize(open.len() + 1, None);
                    closed[open.len()] = Some(block);
                }
            }
            Event::Text(part) | Event::Code(part) => {
                if let Some(heading) = &mut heading {
                    heading.text.push_str(&part);
                }
            }
            Event::BlockId(id) => {
                // Inside a block, an id names it; between blocks, the block
                // before it.
                let named = if after_inline {
                    open.last().cloned()
                } else {
                    closed.get(open.len()).cloned().flatten()
                };
                if let Some(named) = named
                    && !blocks.iter().any(|(known, _)| *known == *id)
                {
                    blocks.push((id.into_string(), named));
                }
            }
            _ => {}
        }
    }
    (headings, blocks)
}

/// Whether `tag` opens a block that a block id can name.
fn is_block(tag: &Tag<'_>) -> bool {
    !matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Highlight
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
            | Tag::Embed(_)
    )
}

/// Whether `end` closes a block that [`is_block`] opens.
fn is_block_end(end: TagEnd) -> bool {
    !matches!(
        end,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Highlight
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
            | TagEnd::Embed
    )
}

#[cfg(test)]
mod tests {
    use super::section;

    const NOTE: &str = "# Note\n\nIntro.\n\n## Plans\n\nShip it. ^ship\n\n### Detail\n\nMore.\n\n## Later\n\nNot this.\n\n- one\n- two ^two\n\n^list\n";

    #[test]
    fn a_heading_section_runs_to_the_next_heading_as_high() {
        let range = section(NOTE, "Plans").expect("a section");
        assert!(NOTE[range.clone()].starts_with("## Plans"));
        assert!(NOTE[range].ends_with("More.\n\n"));
        for anchor in ["plans", "PLANS"] {
            assert_eq!(section(NOTE, anchor), section(NOTE, "Plans"));
        }
        assert_eq!(section(NOTE, "Missing"), None);
    }

    #[test]
    fn a_block_id_names_its_block_or_the_one_before_it() {
        let range = section(NOTE, "^ship").expect("a block");
        assert_eq!(&NOTE[range], "Ship it. ^ship\n");
        let range = section(NOTE, "^list").expect("a block");
        assert_eq!(&NOTE[range], "- one\n- two ^two\n\n");
        let range = section(NOTE, "^two").expect("a block");
        assert_eq!(&NOTE[range], "- two ^two\n\n");
    }
}
