//! Source-addressed Markdown semantics, independent of widgets and geometry.
use std::{
    collections::{HashMap, HashSet},
    iter, mem,
    ops::Range,
    slice,
    sync::Arc,
};

pub use pulldown_cmark::Alignment;
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag};

use crate::source_store::SourceSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Bounds on semantic tree construction and rich table adaptation.
pub struct ParseLimits {
    /// Maximum parser events consumed during semantic construction.
    pub max_nodes: usize,
    /// Maximum nested parser containers and inline constructs.
    pub max_depth: usize,
    /// Maximum cells admitted to one rich table.
    pub max_table_cells: usize,
}
impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_nodes: 100_000,
            max_depth: 128,
            max_table_cells: 100_000,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// The readable payload represented by one reading unit.
pub enum UnitKind {
    /// Readable prose, including literal provisional syntax.
    Prose,
    /// Parser-normalized code, selectable as one card.
    Code,
    /// A completed table with normalized cell values.
    Table,
    /// Exact source retained for inspection or literal presentation.
    Source,
    /// A logical separator with no decoration in copied text.
    Separator,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Semantic emphasis independent of terminal geometry and palette.
pub enum InlineRole {
    /// Emphasized inline text.
    Emphasis,
    /// Strong inline emphasis.
    Strong,
    /// Parser-normalized code, selectable as one card.
    Code,
    /// Struck-through inline text.
    Strike,
    /// A parser link or recognized bare HTTP(S) URL.
    Link,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// A UTF-8 byte range in a reading unit and its semantic role.
pub struct InlineSpan {
    /// UTF-8 byte range in the addressed readable text.
    pub range: Range<usize>,
    /// Semantic text role used during presentation.
    pub role: InlineRole,
    /// Associated actionable object, when admitted.
    pub object: Option<u64>,
}
#[derive(Clone, Debug)]
/// One source-addressed payload in document reading order.
pub struct ReadingUnit {
    /// Opaque identity that is never recycled within its source epoch.
    pub id: u64,
    /// Original UTF-8 source range for this construct.
    pub source: Range<usize>,
    /// Semantic construct or payload category.
    pub kind: UnitKind,
    /// Readable text, excluding generated terminal decoration.
    pub text: String,
    /// Whether selection includes this entire payload.
    pub atomic: bool,
    /// Semantic spans whose offsets address this readable payload.
    pub spans: Vec<InlineSpan>,
    /// UTF-8 offsets of LF bytes that wrap as prose when displayed.
    pub soft_breaks: Vec<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A UTF-8 byte offset in one identified reading unit.
pub struct ReadingPosition {
    /// Identity of the associated reading unit.
    pub unit_id: u64,
    /// UTF-8 byte offset within the unit readable payload.
    pub offset: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Two reading endpoints; either selection direction is valid.
pub struct ReadingRange {
    /// The selection anchor endpoint.
    pub start: ReadingPosition,
    /// The selection head endpoint.
    pub end: ReadingPosition,
}
#[derive(Clone, Debug)]
/// Parser-normalized code payload and its original fence information.
pub struct CodeBlock {
    /// The complete parser-decoded fence info string.
    pub info: String,
    /// Normalized code bytes, preserving content whitespace and final LF.
    pub payload: Arc<str>,
    /// Whether the trailing code block is still receiving content.
    pub receiving: bool,
}
#[derive(Clone, Debug)]
/// One readable Markdown cell with source-derived column identity.
pub struct TableCell {
    /// Stable source-derived key within the containing table.
    pub key: u64,
    /// Original cell bytes, absent for synthetic missing cells.
    pub source: Option<Range<usize>>,
    /// Readable text, excluding generated terminal decoration.
    pub text: String,
}
#[derive(Clone, Debug)]
/// One body row with stable source-derived identity.
pub struct TableRow {
    /// Stable source-derived key within the containing table.
    pub key: u64,
    /// Ordered cells, padded to the header column count.
    pub cells: Vec<TableCell>,
}
#[derive(Clone, Debug)]
/// Parser-normalized table data without terminal geometry.
pub struct MarkdownTable {
    /// Parser column alignments in header order.
    pub alignments: Vec<Alignment>,
    /// Whether source rows contain excess cells omitted by the parser.
    pub ignored_excess: bool,
    /// Ordered header cells that define column keys.
    pub headers: Vec<TableCell>,
    /// Ordered body rows, excluding the header.
    pub rows: Vec<TableRow>,
}
#[derive(Clone, Debug)]
/// The interpreted construct associated with one reading unit.
pub enum BlockKind {
    /// An ordinary paragraph or tight-list text leaf.
    Paragraph,
    /// A heading indexed by the outline and fragment resolver.
    Heading {
        /// Heading level from one through six.
        level: u8,
    },
    /// Parser-normalized code, selectable as one card.
    Code(CodeBlock),
    /// A completed table with normalized cell values.
    Table(MarkdownTable),
    /// Exact source retained for inspection or literal presentation.
    Source {
        /// Why the block shows as source rather than rendered, or empty for
        /// source that always shows as itself, such as HTML.
        reason: String,
    },
    /// A thematic rule without readable decoration.
    Rule,
    /// A labeled footnote block or reference.
    Footnote {
        /// The readable label or footnote identifier.
        label: String,
    },
    /// An image alone in its paragraph, which a host may supply.
    Image(ImageBlock),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// An image alone in its paragraph: `![alt](path)`, or Obsidian's
/// `![[path]]` with an optional `|width` or `|widthxheight` size in pixels.
pub struct ImageBlock {
    /// The image's destination, and its alt text as the label.
    pub link: Link,
    /// The width the source asks for, in pixels.
    pub width: Option<u32>,
    /// The height the source asks for, in pixels.
    pub height: Option<u32>,
}
#[derive(Clone, Debug)]
/// One semantic leaf with source and container metadata.
pub struct Block {
    /// Opaque identity that is never recycled within its source epoch.
    pub id: u64,
    /// Original UTF-8 source range for this construct.
    pub source: Range<usize>,
    /// Semantic construct or payload category.
    pub kind: BlockKind,
    /// Identity of the associated reading unit.
    pub unit_id: u64,
    /// List and quote container nesting depth.
    pub depth: usize,
    /// Number of enclosing quote rails.
    pub quote_depth: usize,
    /// Uppercase alert label on the first leaf of an alert quote.
    pub quote_kind: Option<String>,
    /// Quote rails that continue past this block into the next, because
    /// both lie in the same quotes.
    pub quote_join: usize,
    /// The alert quotes this block lies in, outermost first.
    pub alerts: Vec<Alert>,
    /// A generated bullet or parser-derived ordered marker.
    pub list_marker: Option<String>,
    /// Whether this leaf remains open at the receiving boundary.
    pub provisional: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// A readable label and its original destination and title.
pub struct Link {
    /// The readable label or footnote identifier.
    pub label: String,
    /// The original parser-decoded URL destination.
    pub destination: String,
    /// The optional parser-decoded link title.
    pub title: String,
    /// Whether the source wrote a `[[target|label]]` wikilink, whose
    /// destination names a host document rather than a URL.
    pub wiki: bool,
    /// Whether the source gave the label: always for a Markdown link, and
    /// for a wikilink only after a `|`. An unlabelled wikilink shows its
    /// target as Obsidian does, `Note › Heading`.
    pub labelled: bool,
}
impl Link {
    /// Classify the destination. A wikilink is always local to the host;
    /// other destinations resolve as [`resolve_link`] does.
    pub fn resolve(&self, base: Option<&url::Url>) -> ResolvedLink {
        if !self.wiki {
            return resolve_link(&self.destination, base);
        }
        if self.destination.chars().any(char::is_control) {
            return ResolvedLink::Unavailable("Link contains control characters".into());
        }
        let (path, fragment) = match self.destination.split_once('#') {
            Some((path, fragment)) => (path.trim(), Some(fragment.trim())),
            None => (self.destination.trim(), None),
        };
        match (path.is_empty(), fragment) {
            (true, Some(fragment)) => ResolvedLink::Fragment(fragment.to_owned()),
            (true, None) => ResolvedLink::Unavailable("Wikilink names no document".into()),
            (false, _) => ResolvedLink::Local(LocalLink {
                path: path.to_owned(),
                fragment: fragment.map(str::to_owned),
                wiki: true,
            }),
        }
    }
}
#[derive(Clone, Debug)]
/// An explicit interaction target outside provisional content.
pub enum ObjectKind {
    /// A parser link or recognized bare HTTP(S) URL.
    Link(Link),
    /// Parser-normalized code, selectable as one card.
    Code,
    /// A completed table with normalized cell values.
    Table,
    /// Exact source retained for inspection or literal presentation.
    Source,
    /// A labeled footnote block or reference.
    Footnote {
        /// The readable label or footnote identifier.
        label: String,
    },
}
#[derive(Clone, Debug)]
/// One source-addressed interaction target within a document epoch.
pub struct Object {
    /// Opaque identity that is never recycled within its source epoch.
    pub id: u64,
    /// Original UTF-8 source range for this construct.
    pub source: Range<usize>,
    /// Identity of the associated reading unit.
    pub unit_id: u64,
    /// Semantic construct or payload category.
    pub kind: ObjectKind,
}
#[derive(Clone, Debug)]
/// One heading, its source identity, and its disambiguated fragment.
pub struct OutlineEntry {
    /// Opaque identity that is never recycled within its source epoch.
    pub id: u64,
    /// Identity of the associated reading unit.
    pub unit_id: u64,
    /// Original UTF-8 source range for this construct.
    pub source: Range<usize>,
    /// Heading level from one through six.
    pub level: u8,
    /// Readable text, excluding generated terminal decoration.
    pub text: String,
    /// Lowercase heading fragment with source-order duplicate suffixes.
    pub slug: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// An Obsidian block id, `^id`, and the block it marks.
pub struct BlockAnchor {
    /// The id, without its `^`.
    pub id: String,
    /// The source of the marked block.
    pub source: Range<usize>,
}
#[derive(Clone, Debug)]
/// Immutable semantic interpretation of one accepted source revision.
pub struct DocumentSnapshot {
    /// Accepted bytes and lifecycle interpreted by this snapshot.
    pub source: SourceSnapshot,
    /// Construction bounds used for this snapshot.
    pub limits: ParseLimits,
    /// Semantic leaves in source order.
    pub blocks: Vec<Block>,
    /// Readable payloads in the same order as blocks.
    pub units: Vec<ReadingUnit>,
    /// Explicit targets admitted outside the provisional region.
    pub objects: Vec<Object>,
    /// Heading index in source order.
    pub outline: Vec<OutlineEntry>,
    /// Obsidian block ids in source order, each naming the block it marks.
    /// The first block with an id keeps it.
    pub block_anchors: Vec<BlockAnchor>,
    /// The trailing source region awaiting a closing boundary.
    pub provisional: Option<Range<usize>>,
    /// Whether a derived-work bound required a source-card fallback.
    pub degraded: bool,
    /// Active source-role identities retained for the next publication.
    identities: HashMap<(usize, String, usize), u64>,
    /// Constant-time lookup from unit identity to reading-order index.
    unit_index: HashMap<u64, usize>,
    /// Worker-computed semantic allocation estimate, excluding accepted source.
    estimated_bytes: usize,
}
impl DocumentSnapshot {
    /// Conservative owned semantic-model bytes, excluding accepted source and
    /// flattened parser input. This worker-computed observation is constant
    /// time.
    pub fn derived_bytes(&self) -> usize {
        self.estimated_bytes
    }

    /// Count model-owned payloads after parsing and identity reconciliation.
    fn estimate_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>();
        for allocation in [
            self.blocks.capacity().saturating_mul(size_of::<Block>()),
            self.units
                .capacity()
                .saturating_mul(size_of::<ReadingUnit>()),
            self.objects.capacity().saturating_mul(size_of::<Object>()),
            self.outline
                .capacity()
                .saturating_mul(size_of::<OutlineEntry>()),
            self.block_anchors
                .capacity()
                .saturating_mul(size_of::<BlockAnchor>()),
            self.identities.capacity().saturating_mul(
                2 * (size_of::<((usize, String, usize), u64)>() + size_of::<usize>()),
            ),
            self.unit_index
                .capacity()
                .saturating_mul(2 * (size_of::<(u64, usize)>() + size_of::<usize>())),
        ] {
            bytes = bytes.saturating_add(allocation);
        }
        for unit in &self.units {
            bytes = bytes
                .saturating_add(unit.text.capacity())
                .saturating_add(
                    unit.spans
                        .capacity()
                        .saturating_mul(size_of::<InlineSpan>()),
                )
                .saturating_add(
                    unit.soft_breaks
                        .capacity()
                        .saturating_mul(size_of::<usize>()),
                );
        }
        for block in &self.blocks {
            bytes = bytes
                .saturating_add(block.list_marker.as_ref().map_or(0, String::capacity))
                .saturating_add(block.quote_kind.as_ref().map_or(0, String::capacity));
            match &block.kind {
                BlockKind::Code(code) => {
                    bytes = bytes
                        .saturating_add(code.info.capacity())
                        .saturating_add(code.payload.len())
                        .saturating_add(2 * size_of::<usize>())
                }
                BlockKind::Table(table) => {
                    bytes = bytes
                        .saturating_add(
                            table
                                .alignments
                                .capacity()
                                .saturating_mul(size_of::<Alignment>()),
                        )
                        .saturating_add(
                            table
                                .headers
                                .capacity()
                                .saturating_mul(size_of::<TableCell>()),
                        )
                        .saturating_add(
                            table.rows.capacity().saturating_mul(size_of::<TableRow>()),
                        );
                    for cell in &table.headers {
                        bytes = bytes.saturating_add(cell.text.capacity());
                    }
                    for row in &table.rows {
                        bytes = bytes.saturating_add(
                            row.cells.capacity().saturating_mul(size_of::<TableCell>()),
                        );
                        for cell in &row.cells {
                            bytes = bytes.saturating_add(cell.text.capacity());
                        }
                    }
                }
                BlockKind::Source { reason } => bytes = bytes.saturating_add(reason.capacity()),
                BlockKind::Footnote { label } => bytes = bytes.saturating_add(label.capacity()),
                _ => {}
            }
        }
        for object in &self.objects {
            match &object.kind {
                ObjectKind::Link(link) => {
                    bytes = bytes
                        .saturating_add(link.label.capacity())
                        .saturating_add(link.destination.capacity())
                        .saturating_add(link.title.capacity())
                }
                ObjectKind::Footnote { label } => bytes = bytes.saturating_add(label.capacity()),
                _ => {}
            }
        }
        for heading in &self.outline {
            bytes = bytes
                .saturating_add(heading.text.capacity())
                .saturating_add(heading.slug.capacity());
        }
        for (_, role, _) in self.identities.keys() {
            bytes = bytes.saturating_add(role.capacity());
        }
        bytes
    }
    /// Look up one reading unit by its opaque identity.
    pub fn unit(&self, id: u64) -> Option<&ReadingUnit> {
        self.units.get(self.unit_position(id)?)
    }
    /// Reading-order index of a unit identity.
    pub(crate) fn unit_position(&self, id: u64) -> Option<usize> {
        self.unit_index.get(&id).copied()
    }
    /// Materialize the complete rendered copy projection.
    pub fn readable_text(&self) -> String {
        let mut text = String::new();
        for (i, unit) in self.units.iter().enumerate() {
            if i > 0 {
                text.push_str(self.separator(i));
            }
            text.push_str(&unit.text);
        }
        text
    }
    /// Return the readable separator before one source-ordered unit.
    pub(crate) fn separator(&self, index: usize) -> &'static str {
        if matches!(self.blocks[index].kind, BlockKind::Rule) {
            return "";
        }
        let previous = self.blocks[..index]
            .iter()
            .rev()
            .find(|block| !matches!(block.kind, BlockKind::Rule));
        match previous {
            None => "",
            Some(previous)
                if self.blocks[index].list_marker.is_some() && previous.list_marker.is_some() =>
            {
                "\n"
            }
            Some(_) => "\n\n",
        }
    }
    /// Materialize a displayed selection, expanding atomic cards.
    pub fn copy_range(&self, range: &ReadingRange) -> Option<String> {
        let a = self.unit_position(range.start.unit_id)?;
        let b = self.unit_position(range.end.unit_id)?;
        let (start, end) = if (a, range.start.offset) <= (b, range.end.offset) {
            ((a, range.start.offset), (b, range.end.offset))
        } else {
            ((b, range.end.offset), (a, range.start.offset))
        };
        let mut out = String::new();
        for index in start.0..=end.0 {
            let unit = &self.units[index];
            let from = if index == start.0 && !unit.atomic {
                start.1.min(unit.text.len())
            } else {
                0
            };
            let to = if index == end.0 && !unit.atomic {
                end.1.min(unit.text.len())
            } else {
                unit.text.len()
            };
            if index > start.0 {
                out.push_str(self.separator(index));
            }
            out.push_str(unit.text.get(from..to)?);
        }
        Some(out)
    }
    /// Restore by identity, then containing source span, then the nearest
    /// source neighbor.
    pub fn resolve_anchor(
        &self,
        id: u64,
        source_start: usize,
        offset: usize,
    ) -> Option<ReadingPosition> {
        let unit = self
            .unit(id)
            .or_else(|| self.units.iter().find(|u| u.source.contains(&source_start)))
            .or_else(|| self.units.iter().find(|u| u.source.start >= source_start))
            .or_else(|| self.units.last())?;
        let mut at = offset.min(unit.text.len());
        while !unit.text.is_char_boundary(at) {
            at -= 1;
        }
        Some(ReadingPosition {
            unit_id: unit.id,
            offset: at,
        })
    }
}

/// The supported syntax is CommonMark with these explicit extensions.
pub fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_WIKILINKS
}

#[derive(Clone, Debug)]
/// A bounded parser tree node with its original event range.
struct Node {
    /// Original UTF-8 event source span.
    range: Range<usize>,
    /// Owning construct tag for branch nodes.
    tag: Option<Tag<'static>>,
    /// Leaf event after adjacent text events have been merged.
    event: Option<Event<'static>>,
    /// Nested parser nodes in source order.
    children: Vec<Self>,
}
/// Build a bounded source-addressed parser tree and collect the source spans
/// of the reference definitions that the parser retained.
fn tree(text: &str, limits: &ParseLimits) -> Option<(Vec<Node>, Vec<Range<usize>>)> {
    let mut roots = Vec::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut count = 0;
    let events = Parser::new_ext(text, parser_options()).into_offset_iter();
    let definitions = events
        .reference_definitions()
        .iter()
        .map(|(_, definition)| definition.span.clone())
        .collect();
    for (event, range) in events {
        count += 1;
        if count > limits.max_nodes {
            return None;
        }
        match event {
            Event::Start(tag) => {
                if stack.len() >= limits.max_depth {
                    return None;
                }
                stack.push(Node {
                    range,
                    tag: Some(tag.into_static()),
                    event: None,
                    children: Vec::new(),
                });
            }
            Event::End(_) => {
                let node = stack.pop()?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            event => {
                let node = Node {
                    range,
                    tag: None,
                    event: Some(event.into_static()),
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    push_merged(&mut parent.children, node);
                } else {
                    push_merged(&mut roots, node);
                }
            }
        }
    }
    Some((roots, definitions))
}
/// Join adjacent text events before detecting bare URLs.
fn push_merged(nodes: &mut Vec<Node>, node: Node) {
    if let Some(last) = nodes.last_mut()
        && let (Some(Event::Text(before)), Some(Event::Text(after))) = (&last.event, &node.event)
    {
        let mut merged = String::with_capacity(before.len() + after.len());
        merged.push_str(before);
        merged.push_str(after);
        last.event = Some(Event::Text(merged.into()));
        last.range.end = node.range.end;
        return;
    }
    nodes.push(node);
}
/// The semantic tone of an alert, which sets its colour.
///
/// GitHub defines five alert kinds, and other tools, such as Obsidian,
/// MkDocs, Docusaurus, and reStructuredText, use overlapping vocabularies.
/// Each common label takes the tone of the GitHub kind closest in meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlertTone {
    /// Information worth noticing: note, info, todo, abstract, summary,
    /// tldr, quote, cite, and example.
    Note,
    /// Advice or success: tip, hint, success, check, and done.
    Tip,
    /// Information needed to succeed: important.
    Important,
    /// Something that needs attention: warning, attention, question, help,
    /// and faq.
    Warning,
    /// Risk or failure: caution, danger, error, failure, fail, missing, and
    /// bug.
    Caution,
}

impl AlertTone {
    /// Return the tone of an alert label, ignoring case, or nothing for a
    /// label no common vocabulary uses.
    #[must_use]
    pub fn of(label: &str) -> Option<Self> {
        Some(match label.to_ascii_lowercase().as_str() {
            "note" | "info" | "todo" | "abstract" | "summary" | "tldr" | "quote" | "cite"
            | "example" => Self::Note,
            "tip" | "hint" | "success" | "check" | "done" => Self::Tip,
            "important" => Self::Important,
            "warning" | "attention" | "question" | "help" | "faq" => Self::Warning,
            "caution" | "danger" | "error" | "failure" | "fail" | "missing" | "bug" => {
                Self::Caution
            }
            _ => return None,
        })
    }
}

/// An alert quote a block lies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Alert {
    /// The alert's tone.
    pub tone: AlertTone,
    /// The alert quote's rail, counting quote rails from the outermost.
    pub depth: usize,
}

/// Return the label of an alert marker that leads a quote, such as `info` in
/// `> [!info]`, and remove the marker line from `children`.
///
/// A marker stands alone on the quote's first line, as GitHub requires; an
/// Obsidian fold sign may follow it. Only a label with a tone counts, so an
/// unknown marker stays text.
fn take_alert_marker(children: &mut Vec<Node>) -> Option<String> {
    let paragraph = children
        .first_mut()
        .filter(|node| matches!(node.tag, Some(Tag::Paragraph)))?;
    let Some(Event::Text(text)) = paragraph
        .children
        .first()
        .and_then(|node| node.event.as_ref())
    else {
        return None;
    };
    let label = text
        .trim_end()
        .trim_end_matches(['+', '-'])
        .strip_prefix("[!")?
        .strip_suffix(']')?;
    AlertTone::of(label)?;
    let label = label.to_owned();
    paragraph.children.remove(0);
    if matches!(
        paragraph
            .children
            .first()
            .and_then(|node| node.event.as_ref()),
        Some(Event::SoftBreak | Event::HardBreak)
    ) {
        paragraph.children.remove(0);
    }
    if paragraph.children.is_empty() {
        children.remove(0);
    }
    Some(label)
}

#[derive(Clone, Default)]
/// Container metadata inherited by semantic leaves.
struct Context {
    /// List and quote nesting depth.
    depth: usize,
    /// Number of quote rails.
    quote_depth: usize,
    /// Optional first-leaf alert label.
    quote_kind: Option<String>,
    /// Source start of each enclosing quote, outermost first.
    quotes: Vec<usize>,
    /// The alert quotes the leaf lies in, outermost first.
    alerts: Vec<Alert>,
    /// Generated list marker for the containing item.
    marker: Option<String>,
    /// Source start of the enclosing construct.
    enclosing: usize,
    /// Label of the containing footnote definition.
    footnote: Option<String>,
}
#[derive(Clone)]
/// One interpreted leaf and its enclosing source context.
struct Leaf {
    /// The parser node interpreted as one reading unit.
    node: Node,
    /// Inherited list, quote, and footnote metadata.
    context: Context,
    /// For a paragraph of only a block id, the source of the block before it,
    /// which the id names. The paragraph itself shows nothing.
    names: Option<Range<usize>>,
}
/// Flatten semantic leaves while retaining container metadata.
fn collect(nodes: &[Node], context: &Context, out: &mut Vec<Leaf>) {
    for (index, node) in nodes.iter().enumerate() {
        match node.tag.as_ref() {
            Some(Tag::List(number)) => {
                for (index, item) in node.children.iter().enumerate() {
                    let mut c = context.clone();
                    c.depth += 1;
                    c.marker = Some(number.map_or_else(
                        || {
                            match c.depth {
                                1 => "•",
                                2 => "◦",
                                _ => "▪",
                            }
                            .into()
                        },
                        |n| format!("{}.", n.saturating_add(index as u64)),
                    ));
                    c.enclosing = node.range.start;
                    collect(slice::from_ref(item), &c, out);
                }
            }
            Some(Tag::Item) => {
                let start = out.len();
                let mut c = context.clone();
                c.enclosing = node.range.start;
                let mut inline = Vec::new();
                for child in &node.children {
                    if is_block(child) {
                        flush_inline(&mut inline, &c, out);
                        collect(slice::from_ref(child), &c, out);
                    } else {
                        inline.push(child.clone());
                    }
                }
                flush_inline(&mut inline, &c, out);
                // Only the first leaf of this item owns its marker. Nested
                // lists retain their own markers; quotes retain this item's
                // list depth even though they add container depth.
                let list_depth = c.depth.saturating_sub(c.quote_depth);
                for leaf in out[start..]
                    .iter_mut()
                    .filter(|leaf| {
                        leaf.context.depth.saturating_sub(leaf.context.quote_depth) == list_depth
                    })
                    .skip(1)
                {
                    leaf.context.marker = None;
                }
            }
            Some(Tag::BlockQuote(kind)) => {
                let start = out.len();
                let mut c = context.clone();
                c.quote_depth += 1;
                c.depth += 1;
                c.enclosing = node.range.start;
                c.quotes.push(node.range.start);
                // The parser knows GitHub's five kinds; other common labels
                // lead the quote as a marker line, which the alert drops.
                let mut children = node.children.clone();
                let label = match kind {
                    Some(kind) => Some(format!("{kind:?}")),
                    None => take_alert_marker(&mut children),
                };
                collect(&children, &c, out);
                let Some((label, tone)) =
                    label.and_then(|label| AlertTone::of(&label).map(|tone| (label, tone)))
                else {
                    continue;
                };
                if let Some(first) = out.get_mut(start) {
                    first.context.quote_kind = Some(label.to_uppercase());
                }
                let alert = Alert {
                    tone,
                    depth: c.quote_depth,
                };
                // Nested alerts collected first, so this one goes in front.
                for leaf in &mut out[start..] {
                    leaf.context.alerts.insert(0, alert);
                }
            }
            Some(Tag::FootnoteDefinition(label)) => {
                let mut c = context.clone();
                c.footnote = Some(label.to_string());
                c.enclosing = node.range.start;
                collect(&node.children, &c, out);
            }
            _ if is_block(node) => out.push(Leaf {
                node: node.clone(),
                context: context.clone(),
                names: index
                    .checked_sub(1)
                    .filter(|_| {
                        matches!(node.tag, Some(Tag::Paragraph))
                            && block_id(&plain(&node.children)).is_some()
                    })
                    .map(|previous| nodes[previous].range.clone()),
            }),
            _ => {
                if node.tag.is_some() {
                    collect(&node.children, context, out);
                } else if !matches!(node.event, Some(Event::SoftBreak | Event::HardBreak)) {
                    out.push(Leaf {
                        node: node.clone(),
                        context: context.clone(),
                        names: None,
                    });
                }
            }
        }
    }
}
/// Wrap a tight-list inline run as one prose leaf.
fn flush_inline(inline: &mut Vec<Node>, context: &Context, out: &mut Vec<Leaf>) {
    if inline.is_empty() {
        return;
    }
    let range = inline.first().unwrap().range.start..inline.last().unwrap().range.end;
    out.push(Leaf {
        node: Node {
            range,
            tag: Some(Tag::Paragraph),
            event: None,
            children: mem::take(inline),
        },
        context: context.clone(),
        names: None,
    });
}
/// Identify parser constructs that establish reading-unit boundaries.
fn is_block(node: &Node) -> bool {
    matches!(
        node.tag,
        Some(
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::CodeBlock(_)
                | Tag::Table(_)
                | Tag::HtmlBlock
                | Tag::MetadataBlock(_)
                | Tag::List(_)
                | Tag::BlockQuote(_)
                | Tag::FootnoteDefinition(_)
        )
    ) || matches!(node.event, Some(Event::Rule))
}

/// Recognize a complete blank line after the first line, across CR, LF, and
/// CRLF endings. Lines of only spaces and tabs are blank.
fn blank_boundary(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut first = true;
    while let Some(len) = bytes[start..]
        .iter()
        .position(|byte| matches!(byte, b'\n' | b'\r'))
    {
        let end = start + len;
        if !first
            && bytes[start..end]
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t'))
        {
            return true;
        }
        first = false;
        start = end
            + if bytes[end..].starts_with(b"\r\n") {
                2
            } else {
                1
            };
    }
    false
}
/// Recognize a complete CR, LF, or CRLF source line.
fn complete_line(text: &str) -> bool {
    text.ends_with('\n') || text.ends_with('\r')
}
/// A parser-recognized closing fence ends before its line terminator. EOF
/// remains provisional until a terminator arrives, since an append can turn
/// the apparent fence into code. Container-boundary closure ends after the
/// preceding line terminator and is not an explicit fence.
fn fence_closed(node: &Node, text: &str) -> bool {
    text.as_bytes()
        .get(node.range.end)
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
        && !text[..node.range.end].ends_with(['\n', '\r'])
}
/// Apply explicit HTML terminators and blank-line closure rules.
fn html_closed(raw: &str) -> bool {
    let lower = raw.trim_start().to_ascii_lowercase();
    let delimiter = if lower.starts_with("<!--") {
        Some("-->")
    } else if lower.starts_with("<?") {
        Some("?>")
    } else if lower.starts_with("<![cdata[") {
        Some("]]>")
    } else if lower.starts_with("<!") {
        Some(">")
    } else if lower.starts_with("<script") {
        Some("</script>")
    } else if lower.starts_with("<pre") {
        Some("</pre>")
    } else if lower.starts_with("<style") {
        Some("</style>")
    } else if lower.starts_with("<textarea") {
        Some("</textarea>")
    } else {
        None
    };
    delimiter.is_some_and(|delimiter| lower.contains(delimiter)) || blank_boundary(raw)
}
// RefDefs retains only the winning definition. Recover invisible duplicate
// blocks from source lines as well, so the last duplicate cannot disappear
// while receiving.
/// Recover invisible duplicate definition blocks omitted from RefDefs.
fn definition_records(
    text: &str,
    leaves: &[Leaf],
    mut definitions: Vec<Range<usize>>,
) -> Vec<Range<usize>> {
    let mut offset = 0;
    let mut leaf_index = 0;
    let mut starts: HashSet<_> = definitions.iter().map(|range| range.start).collect();
    for line in text.split_inclusive('\n') {
        let mut content = line.trim_start_matches(' ');
        // Container prefixes are syntax; the label retains its absolute source
        // position.
        while let Some(rest) = content.strip_prefix('>') {
            content = rest.trim_start_matches(' ');
        }
        let start = offset + line.len() - content.len();
        while leaf_index < leaves.len() && leaves[leaf_index].node.range.end <= start {
            leaf_index += 1;
        }
        let painted = leaves
            .get(leaf_index)
            .is_some_and(|leaf| leaf.node.range.contains(&start));
        if !painted
            && content.starts_with('[')
            && !content.starts_with("[^")
            && content.find("]:").is_some_and(|end| end > 1)
            && !starts.contains(&start)
        {
            let following = leaves
                .get(leaf_index)
                .filter(|leaf| leaf.node.range.start > start)
                .map_or(text.len(), |leaf| leaf.node.range.start);
            definitions.push(start..following);
            starts.insert(start);
        }
        offset += line.len();
    }
    definitions
}
/// Classify the final source leaf or invisible definition while receiving.
fn provisional_region(
    text: &str,
    leaves: &[Leaf],
    definitions: &[Range<usize>],
) -> Option<Range<usize>> {
    let leaf = leaves.iter().max_by_key(|l| l.node.range.start);
    let definition = definitions.iter().max_by_key(|r| r.start);
    if let Some(def) = definition.filter(|r| leaf.is_none_or(|l| r.start > l.node.range.start)) {
        let after = &text[def.end.min(text.len())..];
        // A definition can gain a title on the following line until a blank
        // line or another block.
        if blank_boundary(&text[def.start..]) || !after.trim().is_empty() {
            return None;
        }
        return Some(def.start..text.len());
    }
    let leaf = leaf?;
    let raw = &text[leaf.node.range.start..];
    let closed = match leaf.node.tag.as_ref() {
        Some(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => fence_closed(&leaf.node, text),
        Some(Tag::CodeBlock(CodeBlockKind::Indented)) => {
            blank_boundary(&text[leaf.node.range.end..])
        }
        Some(Tag::Heading { .. }) => complete_line(raw),
        Some(Tag::Table(_)) => false,
        Some(Tag::HtmlBlock) => html_closed(raw),
        Some(Tag::MetadataBlock(_)) => raw
            .lines()
            .skip(1)
            .any(|line| matches!(line.trim(), "---" | "...")),
        _ if matches!(leaf.node.event, Some(Event::Rule)) => complete_line(raw),
        _ => blank_boundary(raw),
    };
    (!closed).then_some(leaf.node.range.start..text.len())
}

/// Worker-local semantic construction and identity reconciliation.
struct Builder<'a> {
    /// The semantic snapshot under construction.
    document: DocumentSnapshot,
    /// Latest published identity baseline from this epoch.
    previous: Option<&'a DocumentSnapshot>,
    /// Bounds controlling rich semantic adaptation.
    limits: &'a ParseLimits,
    /// Enclosing quotes of the last block pushed.
    quotes: Vec<usize>,
}
impl Builder<'_> {
    /// Index a block id, unless an earlier block already holds it.
    fn anchor(&mut self, id: String, source: Range<usize>) {
        if !self
            .document
            .block_anchors
            .iter()
            .any(|anchor| anchor.id == id)
        {
            self.document.block_anchors.push(BlockAnchor { id, source });
        }
    }
    /// Index a trailing block marker on `payload` and drop it from the
    /// readable text, its spans, and its breaks.
    fn strip_block_id(
        &mut self,
        payload: &mut String,
        spans: &mut [InlineSpan],
        soft_breaks: &mut Vec<usize>,
        source: &Range<usize>,
    ) {
        let Some((at, id)) = trailing_block_id(payload) else {
            return;
        };
        self.anchor(id.to_owned(), source.clone());
        payload.truncate(at);
        for span in spans {
            span.range.start = span.range.start.min(at);
            span.range.end = span.range.end.min(at);
        }
        soft_breaks.retain(|offset| *offset < at);
    }
    /// Reuse the latest active identity or allocate a never-recycled one.
    fn id(&mut self, start: usize, role: &str, enclosing: usize) -> u64 {
        let key = (start, role.to_owned(), enclosing);
        if let Some(id) = self.document.identities.get(&key) {
            return *id;
        }
        let id = self
            .previous
            .filter(|p| p.source.epoch == self.document.source.epoch)
            .and_then(|p| p.identities.get(&key))
            .copied()
            .unwrap_or_else(|| self.document.source.allocate_id());
        self.document.identities.insert(key, id);
        id
    }
    /// Adapt one parser leaf to a readable unit and admitted objects.
    fn leaf(&mut self, leaf: &Leaf, text: &str) {
        let range = leaf.node.range.clone();
        let provisional = self
            .document
            .provisional
            .as_ref()
            .is_some_and(|p| range.start >= p.start);
        let raw = &text[range.clone()];
        let context = &leaf.context;
        let mut spans = Vec::new();
        let mut links = Vec::new();
        let mut soft_breaks = Vec::new();
        let literal = provisional
            && matches!(
                leaf.node.tag,
                Some(Tag::Table(_) | Tag::HtmlBlock | Tag::MetadataBlock(_))
            )
            || provisional
                && matches!(
                    leaf.node.tag,
                    Some(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
                )
                && !raw.contains(['\n', '\r']);
        let (mut kind, unit_kind, payload, atomic) = if literal {
            (
                BlockKind::Source {
                    reason: "Receiving syntax".into(),
                },
                UnitKind::Prose,
                text[range.start..].to_owned(),
                false,
            )
        } else {
            match leaf.node.tag.as_ref() {
                Some(Tag::CodeBlock(code)) => {
                    let payload = plain(&leaf.node.children);
                    let info = match code {
                        CodeBlockKind::Fenced(info) => info.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    (
                        BlockKind::Code(CodeBlock {
                            info,
                            payload: Arc::from(payload.as_str()),
                            receiving: provisional,
                        }),
                        UnitKind::Code,
                        payload,
                        true,
                    )
                }
                Some(Tag::Table(alignments)) => {
                    if count_cells(&leaf.node) > self.limits.max_table_cells {
                        self.document.degraded = true;
                        (
                            BlockKind::Source {
                                reason: "Table cell limit".into(),
                            },
                            UnitKind::Source,
                            raw.to_owned(),
                            true,
                        )
                    } else {
                        let table = self.table(&leaf.node, alignments, context.enclosing, text);
                        let payload = table_text(&table);
                        (BlockKind::Table(table), UnitKind::Table, payload, true)
                    }
                }
                // The terminal shows HTML and front matter as source, so
                // their cards need no explanation.
                Some(Tag::HtmlBlock | Tag::MetadataBlock(_)) => (
                    BlockKind::Source {
                        reason: String::new(),
                    },
                    UnitKind::Source,
                    raw.to_owned(),
                    true,
                ),
                _ if matches!(leaf.node.event, Some(Event::Rule)) => {
                    (BlockKind::Rule, UnitKind::Separator, String::new(), false)
                }
                _ if lone_image(&leaf.node).is_some() => {
                    let image = lone_image(&leaf.node).expect("checked");
                    // It reads, and copies, as an inline image does.
                    let payload = image_text(&image.link.label, &image.link.destination);
                    // The image is one whole payload, its alt text.
                    (BlockKind::Image(image), UnitKind::Prose, payload, true)
                }
                _ => {
                    let mut payload = inline(
                        &leaf.node.children,
                        &mut spans,
                        &mut links,
                        &mut soft_breaks,
                    );
                    self.strip_block_id(&mut payload, &mut spans, &mut soft_breaks, &range);
                    let kind = match leaf.node.tag.as_ref() {
                        Some(Tag::Heading { level, .. }) => BlockKind::Heading {
                            level: *level as u8,
                        },
                        _ => BlockKind::Paragraph,
                    };
                    (kind, UnitKind::Prose, payload, false)
                }
            }
        };
        if matches!(kind, BlockKind::Paragraph)
            && let Some(label) = &context.footnote
        {
            kind = BlockKind::Footnote {
                label: label.clone(),
            };
        }
        let role = match &kind {
            BlockKind::Paragraph => "paragraph",
            BlockKind::Heading { .. } => "heading",
            BlockKind::Code(_) => "code",
            BlockKind::Table(_) => "table",
            BlockKind::Source { .. } if !atomic => "literal",
            BlockKind::Source { .. } => "source",
            BlockKind::Rule => "rule",
            BlockKind::Footnote { .. } => "footnote",
            BlockKind::Image(_) => "image",
        };
        let id = self.id(range.start, role, context.enclosing);
        let unit_id = self.id(range.start, &format!("unit:{role}"), context.enclosing);
        if !provisional {
            if matches!(
                kind,
                BlockKind::Code(_)
                    | BlockKind::Table(_)
                    | BlockKind::Source { .. }
                    | BlockKind::Image(_)
            ) {
                let object_id = self.id(range.start, &format!("object:{role}"), context.enclosing);
                self.document.objects.push(Object {
                    id: object_id,
                    source: range.clone(),
                    unit_id,
                    kind: match &kind {
                        BlockKind::Code(_) => ObjectKind::Code,
                        BlockKind::Table(_) => ObjectKind::Table,
                        // An image follows as its link does.
                        BlockKind::Image(image) => ObjectKind::Link(image.link.clone()),
                        _ => ObjectKind::Source,
                    },
                });
            }
            let mut bare_sources: HashMap<(usize, usize), Vec<Range<usize>>> = HashMap::new();
            for mut pending in links {
                if let Some(index) = pending.bare_index {
                    let positions = bare_sources
                        .entry((pending.source.start, pending.source.end))
                        .or_insert_with(|| bare_url_ranges(&text[pending.source.clone()]));
                    if let Some(found) = positions.get(index) {
                        pending.source =
                            pending.source.start + found.start..pending.source.start + found.end;
                        pending.identity_offset = 0;
                    }
                }
                let object_id = self.id(
                    pending.source.start,
                    &format!("link:{}", pending.identity_offset),
                    range.start,
                );
                if let Some(span) = spans.get_mut(pending.span) {
                    span.object = Some(object_id);
                }
                self.document.objects.push(Object {
                    id: object_id,
                    source: pending.source,
                    unit_id,
                    kind: pending.kind,
                });
            }
        }
        if let BlockKind::Heading { level } = &kind {
            self.document.outline.push(OutlineEntry {
                id,
                unit_id,
                source: range.clone(),
                level: *level,
                text: payload.clone(),
                slug: String::new(),
            });
        }
        if let Some(last) = self.document.blocks.last_mut() {
            last.quote_join = self
                .quotes
                .iter()
                .zip(&context.quotes)
                .take_while(|(a, b)| a == b)
                .count();
        }
        self.quotes.clone_from(&context.quotes);
        self.document.blocks.push(Block {
            id,
            source: range.clone(),
            kind,
            unit_id,
            depth: context.depth,
            quote_depth: context.quote_depth,
            quote_kind: context.quote_kind.clone(),
            quote_join: 0,
            alerts: context.alerts.clone(),
            list_marker: context.marker.clone(),
            provisional,
        });
        self.document
            .unit_index
            .insert(unit_id, self.document.units.len());
        self.document.units.push(ReadingUnit {
            id: unit_id,
            source: range,
            kind: unit_kind,
            text: payload,
            atomic,
            spans,
            soft_breaks,
        });
    }
    /// Adapt parser table structure without splitting source on pipes.
    fn table(
        &mut self,
        node: &Node,
        alignments: &[Alignment],
        enclosing: usize,
        text: &str,
    ) -> MarkdownTable {
        let columns = alignments.len();
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        let mut ignored_excess = false;
        for row in &node.children {
            if !matches!(row.tag, Some(Tag::TableHead | Tag::TableRow)) {
                continue;
            }
            if let Some(last) = row
                .children
                .iter()
                .filter(|n| matches!(n.tag, Some(Tag::TableCell)))
                .nth(columns.saturating_sub(1))
            {
                let tail = text.get(last.range.end..row.range.end).unwrap_or("").trim();
                if !tail.is_empty() && tail != "|" {
                    ignored_excess = true;
                }
            }
            let mut cells = Vec::new();
            for (index, cell) in row
                .children
                .iter()
                .filter(|n| matches!(n.tag, Some(Tag::TableCell)))
                .take(columns)
                .enumerate()
            {
                let source = (!cell.range.is_empty()).then(|| cell.range.clone());
                let key = self.id(
                    if matches!(row.tag, Some(Tag::TableHead)) {
                        cell.range.start
                    } else {
                        node.range.start
                    },
                    &format!("column:{index}"),
                    enclosing,
                );
                cells.push(TableCell {
                    key,
                    source,
                    text: plain(&cell.children),
                });
            }
            while cells.len() < columns {
                let index = cells.len();
                let key = self.id(node.range.start, &format!("column:{index}"), enclosing);
                cells.push(TableCell {
                    key,
                    source: None,
                    text: String::new(),
                });
            }
            if matches!(row.tag, Some(Tag::TableHead)) {
                headers = cells;
            } else {
                let key = self.id(row.range.start, "row", node.range.start);
                rows.push(TableRow { key, cells });
            }
        }
        // Column identity belongs to the header, including synthetic missing
        // values.
        for row in &mut rows {
            for (cell, header) in row.cells.iter_mut().zip(&headers) {
                cell.key = header.key;
            }
        }
        MarkdownTable {
            alignments: alignments.to_vec(),
            ignored_excess,
            headers,
            rows,
        }
    }
    /// Preserve all accepted bytes when semantic construction reaches a bound.
    fn source_fallback(&mut self, text: &str, reason: &str) {
        let range = 0..text.len();
        let id = self.id(0, "source", 0);
        let unit_id = self.id(0, "unit:source", 0);
        self.document.blocks.push(Block {
            id,
            source: range.clone(),
            kind: BlockKind::Source {
                reason: reason.into(),
            },
            unit_id,
            depth: 0,
            quote_depth: 0,
            quote_kind: None,
            quote_join: 0,
            alerts: Vec::new(),
            list_marker: None,
            provisional: false,
        });
        self.document
            .unit_index
            .insert(unit_id, self.document.units.len());
        self.document.units.push(ReadingUnit {
            id: unit_id,
            source: range.clone(),
            kind: UnitKind::Source,
            text: text.into(),
            atomic: true,
            spans: Vec::new(),
            soft_breaks: Vec::new(),
        });
        if self.document.source.lifecycle.is_complete() {
            let object_id = self.id(0, "object:source", 0);
            self.document.objects.push(Object {
                id: object_id,
                source: range,
                unit_id,
                kind: ObjectKind::Source,
            });
        }
    }
}

/// An inline target awaiting provisional admission and object identity.
struct PendingLink {
    /// Original inline source span.
    source: Range<usize>,
    /// Index of the corresponding readable style span.
    span: usize,
    /// Occurrence discriminator when exact bare source mapping is unavailable.
    identity_offset: usize,
    /// Bare URL occurrence within one merged text event.
    bare_index: Option<usize>,
    /// The admitted interaction target payload.
    kind: ObjectKind,
}
/// Project parsed inline text and retain semantic spans.
fn inline(
    nodes: &[Node],
    spans: &mut Vec<InlineSpan>,
    links: &mut Vec<PendingLink>,
    soft_breaks: &mut Vec<usize>,
) -> String {
    let mut out = String::new();
    append_inline(nodes, &mut out, spans, links, soft_breaks, false);
    out
}
/// An image's readable text: its label, then its destination unless the
/// label already names it, as an unlabelled embed's does.
fn image_text(label: &str, destination: &str) -> String {
    if label.is_empty() || label == destination {
        destination.to_owned()
    } else {
        format!("{label} ({destination})")
    }
}
/// The image a paragraph holds alone, beside whitespace.
fn lone_image(node: &Node) -> Option<ImageBlock> {
    if !matches!(node.tag, Some(Tag::Paragraph)) {
        return None;
    }
    let mut rest = node.children.iter().filter(|child| {
        !matches!(child.event, Some(Event::SoftBreak | Event::HardBreak))
            && !matches!(&child.event, Some(Event::Text(text)) if text.trim().is_empty())
    });
    let image = rest.next()?;
    if rest.next().is_some() {
        return None;
    }
    let Some(Tag::Image {
        link_type,
        dest_url,
        title,
        ..
    }) = &image.tag
    else {
        return None;
    };
    let wiki = matches!(link_type, LinkType::WikiLink { .. });
    let label = plain(&image.children);
    // An Obsidian image's label after `|` is its size when it reads as one.
    let size = wiki
        .then(|| {
            let (width, height) = match label.trim().split_once('x') {
                Some((width, height)) => (width, Some(height)),
                None => (label.trim(), None),
            };
            Some((
                width.parse::<u32>().ok()?,
                height.map(str::parse::<u32>).transpose().ok()?,
            ))
        })
        .flatten();
    let alt = if size.is_some() || (wiki && label.trim() == dest_url.as_ref()) {
        wiki_label(dest_url)
    } else {
        label
    };
    Some(ImageBlock {
        link: Link {
            label: alt,
            destination: dest_url.to_string(),
            title: title.to_string(),
            wiki,
            labelled: true,
        },
        width: size.map(|(width, _)| width),
        height: size.and_then(|(_, height)| height),
    })
}
/// The id of a whole-text Obsidian block marker, `^id`.
fn block_id(text: &str) -> Option<String> {
    let id = text.trim().strip_prefix('^')?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .then(|| id.to_owned())
}
/// A trailing Obsidian block marker, ` ^id`: where the text before it ends,
/// and the id.
fn trailing_block_id(text: &str) -> Option<(usize, &str)> {
    let trimmed = text.trim_end();
    let caret = trimmed.rfind('^')?;
    let id = &trimmed[caret + 1..];
    let before = &trimmed[..caret];
    let valid = !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && before.ends_with(char::is_whitespace)
        && !before.trim().is_empty();
    valid.then(|| (before.trim_end().len(), id))
}
/// The label Obsidian shows for an unlabelled wikilink: its target without a
/// `.md` extension, with its anchor after `›`.
fn wiki_label(target: &str) -> String {
    let (path, anchor) = match target.split_once('#') {
        Some((path, anchor)) => (path.trim(), Some(anchor.trim())),
        None => (target.trim(), None),
    };
    let path = path
        .len()
        .checked_sub(3)
        .filter(|at| path.is_char_boundary(*at) && path[*at..].eq_ignore_ascii_case(".md"))
        .map_or(path, |at| &path[..at]);
    match anchor {
        Some(anchor) if path.is_empty() => anchor.to_owned(),
        Some(anchor) => format!("{path} › {anchor}"),
        None => path.to_owned(),
    }
}
/// Append readable inline content without interpreting terminal controls.
fn append_inline(
    nodes: &[Node],
    out: &mut String,
    spans: &mut Vec<InlineSpan>,
    links: &mut Vec<PendingLink>,
    soft_breaks: &mut Vec<usize>,
    exclude_bare: bool,
) {
    for node in nodes {
        let start = out.len();
        if let Some(tag) = &node.tag {
            let excluded = exclude_bare || matches!(tag, Tag::Link { .. } | Tag::Image { .. });
            let child_spans = spans.len();
            append_inline(&node.children, out, spans, links, soft_breaks, excluded);
            // An unlabelled wikilink shows its target as Obsidian does.
            if let Tag::Link {
                link_type: LinkType::WikiLink { has_pothole: false },
                dest_url,
                ..
            } = tag
            {
                out.truncate(start);
                spans.truncate(child_spans);
                soft_breaks.retain(|at| *at < start);
                out.push_str(&wiki_label(dest_url));
            }
            if let Tag::Image { dest_url, .. } = tag {
                let label = out[start..].to_owned();
                out.truncate(start);
                out.push_str(&image_text(&label, dest_url));
            }
            let role = match tag {
                Tag::Emphasis => Some(InlineRole::Emphasis),
                Tag::Strong => Some(InlineRole::Strong),
                Tag::Strikethrough => Some(InlineRole::Strike),
                Tag::Link { .. } => Some(InlineRole::Link),
                _ => None,
            };
            if let Some(role) = role {
                let span = spans.len();
                spans.push(InlineSpan {
                    range: start..out.len(),
                    role,
                    object: None,
                });
                if let Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    ..
                } = tag
                {
                    links.push(PendingLink {
                        source: node.range.clone(),
                        span,
                        identity_offset: 0,
                        bare_index: None,
                        kind: ObjectKind::Link(Link {
                            label: out[start..].into(),
                            destination: dest_url.to_string(),
                            title: title.to_string(),
                            wiki: matches!(link_type, LinkType::WikiLink { .. }),
                            labelled: !matches!(
                                link_type,
                                LinkType::WikiLink { has_pothole: false }
                            ),
                        }),
                    });
                }
            }
        } else {
            match &node.event {
                Some(Event::Text(text)) => {
                    out.push_str(text);
                    if !exclude_bare {
                        for (bare_index, found) in bare_url_ranges(text).into_iter().enumerate() {
                            let span = spans.len();
                            let label = text[found.clone()].to_owned();
                            spans.push(InlineSpan {
                                range: start + found.start..start + found.end,
                                role: InlineRole::Link,
                                object: None,
                            });
                            links.push(PendingLink {
                                source: node.range.clone(),
                                span,
                                identity_offset: found.start,
                                bare_index: Some(bare_index),
                                kind: ObjectKind::Link(Link {
                                    label: label.clone(),
                                    destination: label,
                                    title: String::new(),
                                    wiki: false,
                                    labelled: true,
                                }),
                            });
                        }
                    }
                }
                Some(Event::InlineHtml(text)) => out.push_str(text),
                Some(Event::Code(text)) => {
                    out.push_str(text);
                    spans.push(InlineSpan {
                        range: start..out.len(),
                        role: InlineRole::Code,
                        object: None,
                    });
                }
                Some(Event::SoftBreak) => {
                    soft_breaks.push(out.len());
                    out.push('\n');
                }
                Some(Event::HardBreak) => out.push('\n'),
                Some(Event::TaskListMarker(done)) => {
                    out.push_str(if *done { "[x] " } else { "[ ] " })
                }
                Some(Event::FootnoteReference(label)) => {
                    out.push('[');
                    out.push_str(label);
                    out.push(']');
                    let span = spans.len();
                    spans.push(InlineSpan {
                        range: start..out.len(),
                        role: InlineRole::Link,
                        object: None,
                    });
                    links.push(PendingLink {
                        source: node.range.clone(),
                        span,
                        identity_offset: 0,
                        bare_index: None,
                        kind: ObjectKind::Footnote {
                            label: label.to_string(),
                        },
                    });
                }
                _ => {}
            }
        }
    }
}
/// Project readable payload without retaining interaction metadata.
fn plain(nodes: &[Node]) -> String {
    inline(nodes, &mut Vec::new(), &mut Vec::new(), &mut Vec::new())
}
/// Count parser table cells before rich adaptation.
fn count_cells(node: &Node) -> usize {
    usize::from(matches!(node.tag, Some(Tag::TableCell)))
        + node.children.iter().map(count_cells).sum::<usize>()
}
/// Join normalized cell values using tabs and logical LF rows.
fn table_text(table: &MarkdownTable) -> String {
    let mut out = String::new();
    for (index, cells) in iter::once(&table.headers)
        .chain(table.rows.iter().map(|r| &r.cells))
        .enumerate()
    {
        if index > 0 {
            out.push('\n');
        }
        for (column, cell) in cells.iter().enumerate() {
            if column > 0 {
                out.push('\t');
            }
            out.push_str(&cell.text);
        }
    }
    out
}

/// Parse one accepted revision. Limits degrade accepted content to recoverable
/// source cards.
pub fn parse_document(
    source: &SourceSnapshot,
    limits: &ParseLimits,
    previous: Option<&DocumentSnapshot>,
) -> DocumentSnapshot {
    let text = source.text();
    let mut builder = Builder {
        document: DocumentSnapshot {
            source: source.clone(),
            limits: *limits,
            blocks: Vec::new(),
            units: Vec::new(),
            objects: Vec::new(),
            outline: Vec::new(),
            block_anchors: Vec::new(),
            provisional: None,
            degraded: false,
            identities: HashMap::new(),
            unit_index: HashMap::new(),
            estimated_bytes: 0,
        },
        previous,
        limits,
        quotes: Vec::new(),
    };
    let Some((initial, definitions)) = tree(&text, limits) else {
        builder.document.degraded = true;
        builder.source_fallback(&text, "Semantic node or depth limit");
        builder.document.estimated_bytes = builder.document.estimate_bytes();
        return builder.document;
    };
    let mut leaves = Vec::new();
    collect(&initial, &Context::default(), &mut leaves);
    let definitions = definition_records(&text, &leaves, definitions);
    if !source.lifecycle.is_complete() {
        builder.document.provisional = provisional_region(&text, &leaves, &definitions);
    }
    if let Some(region) = &builder.document.provisional {
        let excluded: Vec<_> = definitions
            .iter()
            .filter(|r| r.start >= region.start)
            .cloned()
            .collect();
        if !excluded.is_empty() {
            let mut bytes = text.as_bytes().to_vec();
            for range in excluded {
                for byte in &mut bytes[range] {
                    if *byte != b'\n' && *byte != b'\r' {
                        *byte = b' ';
                    }
                }
            }
            let masked =
                String::from_utf8(bytes).expect("offset-preserving ASCII masks preserve UTF-8");
            if let Some((nodes, _)) = tree(&masked, limits) {
                leaves.clear();
                collect(&nodes, &Context::default(), &mut leaves);
            }
            let range = region.clone();
            let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let quote_depth = text[line_start..range.start]
                .chars()
                .filter(|ch| *ch == '>')
                .count();
            leaves.push(Leaf {
                node: Node {
                    range,
                    tag: Some(Tag::HtmlBlock),
                    event: None,
                    children: Vec::new(),
                },
                names: None,
                context: Context {
                    depth: quote_depth,
                    quote_depth,
                    enclosing: line_start,
                    ..Context::default()
                },
            });
        }
    }
    leaves.sort_by_key(|leaf| leaf.node.range.start);
    for leaf in &leaves {
        // A paragraph of only a block id marks the block before it, and
        // shows nothing itself.
        if let Some(named) = &leaf.names {
            if let Some(id) = block_id(&plain(&leaf.node.children)) {
                builder.anchor(id, named.clone());
            }
            continue;
        }
        builder.leaf(leaf, &text);
    }
    let mut slugs = HashMap::<String, usize>::new();
    let mut used_slugs = HashSet::new();
    for heading in &mut builder.document.outline {
        let base = heading_slug(&heading.text);
        let count = slugs.entry(base.clone()).or_default();
        loop {
            let slug = if *count == 0 {
                base.clone()
            } else {
                format!("{base}-{count}")
            };
            *count += 1;
            if used_slugs.insert(slug.clone()) {
                heading.slug = slug;
                break;
            }
        }
    }
    builder.document.estimated_bytes = builder.document.estimate_bytes();
    builder.document
}

/// Lowercase Unicode letters/digits, retain `_` and `-`, replace whitespace
/// with `-`. Punctuation is omitted; repeated headings gain source-order `-1`,
/// `-2` suffixes.
pub fn heading_slug(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '-') {
                Some(c)
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// Recognize HTTP(S) URLs in ordinary text, balancing parentheses and trimming
/// punctuation.
pub fn bare_url_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let boundary = text[..at]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '*' | '_' | '~'));
        if boundary && (rest.starts_with("https://") || rest.starts_with("http://")) {
            let mut end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '\u{0}'))
                .unwrap_or(rest.len());
            let opens = rest[..end].bytes().filter(|byte| *byte == b'(').count();
            let mut closes = rest[..end].bytes().filter(|byte| *byte == b')').count();
            loop {
                let url = &rest[..end];
                let Some(last) = url.chars().next_back() else {
                    break;
                };
                if matches!(
                    last,
                    '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '*' | '_' | '~'
                ) || last == ')' && closes > opens
                {
                    end -= last.len_utf8();
                    if last == ')' {
                        closes -= 1;
                    }
                } else {
                    break;
                }
            }
            if url::Url::parse(&rest[..end]).is_ok() {
                out.push(at..at + end);
                at += end;
                continue;
            }
        }
        at += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// A destination classified without filesystem inference or shell
/// interpolation.
pub enum ResolvedLink {
    /// A local heading fragment requiring no external host action.
    Fragment(String),
    /// A structured HTTP(S) URL eligible for an explicit host Open.
    External(url::Url),
    /// A document of the host's own, which only the host can find.
    Local(LocalLink),
    /// A destination that requires a base URL or explicit host handler.
    Unavailable(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// A link to a document of the host's own: a relative or absolute path, a
/// `file:` URL, or a wikilink. The reader never touches the filesystem; the
/// host decides what the path names.
pub struct LocalLink {
    /// The percent-decoded path, as written, without its fragment.
    pub path: String,
    /// The percent-decoded heading or block anchor after `#`, if any.
    pub fragment: Option<String>,
    /// Whether a wikilink named the document. Its path may omit an
    /// extension, and the host may resolve it by name.
    pub wiki: bool,
}
/// Resolve without filesystem inference. Only control-free HTTP(S) URLs can
/// open by default. Without a base URL, a relative path or `file:` URL is
/// local to the host.
pub fn resolve_link(destination: &str, base: Option<&url::Url>) -> ResolvedLink {
    if destination.chars().any(char::is_control) {
        return ResolvedLink::Unavailable("URL contains control characters".into());
    }
    if let Some(fragment) = destination.strip_prefix('#') {
        return ResolvedLink::Fragment(fragment.to_owned());
    }
    let parsed = match (url::Url::parse(destination), base) {
        (Err(url::ParseError::RelativeUrlWithoutBase), Some(base)) => base.join(destination),
        (Err(url::ParseError::RelativeUrlWithoutBase), None) => return local_link(destination),
        (parsed, _) => parsed,
    };
    match parsed {
        Ok(url) if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() => {
            ResolvedLink::External(url)
        }
        Ok(url) if url.scheme() == "file" => local_link(&format!(
            "{}{}",
            url.path(),
            url.fragment().map_or(String::new(), |f| format!("#{f}"))
        )),
        Ok(_) => ResolvedLink::Unavailable("URL scheme needs a host handler".into()),
        Err(_) => ResolvedLink::Unavailable("Relative URL needs a valid host base URL".into()),
    }
}
/// Split a local destination into its decoded path and fragment.
fn local_link(destination: &str) -> ResolvedLink {
    let decode = |text: &str| {
        percent_encoding::percent_decode_str(text)
            .decode_utf8_lossy()
            .into_owned()
    };
    let (path, fragment) = match destination.split_once('#') {
        Some((path, fragment)) => (path, Some(decode(fragment))),
        None => (destination, None),
    };
    let path = decode(path.split_once('?').map_or(path, |(path, _)| path));
    if path.is_empty() {
        return ResolvedLink::Unavailable("Link names no document".into());
    }
    ResolvedLink::Local(LocalLink {
        path,
        fragment,
        wiki: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_store::{FinishOutcome, SourceStore};
    fn complete(text: &str) -> DocumentSnapshot {
        parse_document(
            &SourceStore::static_text(text).unwrap().snapshot(),
            &ParseLimits::default(),
            None,
        )
    }
    fn receiving(text: &str) -> DocumentSnapshot {
        let mut s = SourceStore::stream();
        s.append(text).unwrap();
        parse_document(&s.snapshot(), &ParseLimits::default(), None)
    }
    fn assert_spans(doc: &DocumentSnapshot) {
        let text = doc.source.text();
        for range in doc
            .blocks
            .iter()
            .map(|b| &b.source)
            .chain(doc.units.iter().map(|u| &u.source))
            .chain(doc.objects.iter().map(|o| &o.source))
        {
            assert!(range.start <= range.end && range.end <= text.len());
            assert!(text.is_char_boundary(range.start));
            assert!(text.is_char_boundary(range.end));
        }
        for unit in &doc.units {
            for span in &unit.spans {
                assert!(unit.text.get(span.range.clone()).is_some());
            }
        }
    }
    #[test]
    fn independent_readable_and_exact_source_goldens() {
        let text = "AT &amp; T **bold**.\r\n\r\n# Heading\r\n\r\n![cat](img/cat.png)\r\n";
        let doc = complete(text);
        assert_eq!(
            doc.readable_text(),
            "AT & T bold.\n\nHeading\n\ncat (img/cat.png)"
        );
        assert_eq!(&*doc.source.text(), text);
        assert_eq!(doc.outline[0].slug, "heading");
        assert_spans(&doc);
        let first = &doc.units[0];
        assert_eq!(
            first
                .spans
                .iter()
                .find(|s| s.role == InlineRole::Strong)
                .map(|s| &first.text[s.range.clone()]),
            Some("bold")
        );
        let range = ReadingRange {
            start: ReadingPosition {
                unit_id: first.id,
                offset: 0,
            },
            end: ReadingPosition {
                unit_id: first.id,
                offset: 6,
            },
        };
        assert_eq!(doc.copy_range(&range).as_deref(), Some("AT & T"));
    }
    #[test]
    fn parser_table_grammar_missing_and_ignored_cells() {
        let text = "| A | B |\n| - | - |\n| a\\|b | `c` | ignored |\n| short |\n";
        let doc = complete(text);
        assert_eq!(doc.readable_text(), "A\tB\na|b\tc\nshort\t");
        let BlockKind::Table(table) = &doc.blocks[0].kind else {
            panic!("table");
        };
        assert!(table.ignored_excess);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[1].cells[1].text, "");
        assert_eq!(table.rows[1].cells[1].source, None);
        assert_eq!(table.rows[0].cells[0].key, table.headers[0].key);
        assert_spans(&doc);
        let bad = complete("A | B\n---\nx | y\n");
        assert!(
            !bad.blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::Table(_)))
        );
    }
    #[test]
    fn receiving_tables_are_entire_literal_until_a_following_block_or_finish() {
        for tail in ["", "\n", "\n\n", "\n\n\n"] {
            let text = format!("A | B\n- | -\nx | y{tail}");
            let doc = receiving(&text);
            assert_eq!(doc.readable_text(), text);
            assert!(doc.objects.is_empty());
            assert!(!doc.units[0].atomic);
        }
        let text = "A | B\n- | -\nx | y\n\nafter\n";
        let doc = receiving(text);
        assert!(matches!(doc.blocks[0].kind, BlockKind::Table(_)));
        assert!(
            doc.objects
                .iter()
                .any(|o| matches!(o.kind, ObjectKind::Table))
        );
        let doc = complete("A | B\n- | -\nx | y");
        assert_eq!(doc.readable_text(), "A\tB\nx\ty");
    }
    #[test]
    fn references_admit_only_after_their_closing_boundary() {
        let prefix = "[label][r]\n\n[r]: https://example.com";
        for tail in ["", "\n", "\n  \"title\""] {
            let text = format!("{prefix}{tail}");
            let doc = receiving(&text);
            assert!(doc.objects.is_empty(), "{text:?}: {:?}", doc.objects);
            assert_eq!(doc.units[0].text, "[label][r]");
            assert_eq!(
                doc.units.last().unwrap().text,
                format!("[r]: https://example.com{tail}")
            );
        }
        for tail in ["\n\n", "\n\nnext"] {
            let doc = receiving(&format!("{prefix}{tail}"));
            let link = doc
                .objects
                .iter()
                .find_map(|o| {
                    if let ObjectKind::Link(link) = &o.kind {
                        Some(link)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(link.label, "label");
            assert_eq!(link.destination, "https://example.com");
        }
        let doc = complete(&format!("{prefix}\n\n[r]: https://second.invalid\n"));
        assert!(doc.objects.iter().any(
            |o| matches!(&o.kind,ObjectKind::Link(link) if link.destination=="https://example.com")
        ));
    }
    #[test]
    fn code_closure_payload_and_atomic_copy() {
        let doc = receiving("```rust\r\nx\r\n```\r\n");
        assert_eq!(doc.provisional, None);
        let BlockKind::Code(code) = &doc.blocks[0].kind else {
            panic!("code");
        };
        assert_eq!(&*code.payload, "x\n");
        assert!(!code.receiving);
        let unit = &doc.units[0];
        let range = ReadingRange {
            start: ReadingPosition {
                unit_id: unit.id,
                offset: 1,
            },
            end: ReadingPosition {
                unit_id: unit.id,
                offset: 1,
            },
        };
        assert_eq!(doc.copy_range(&range).as_deref(), Some("x\n"));
        assert!(receiving("```ru").objects.is_empty());
        assert_eq!(receiving("```ru").units[0].text, "```ru");
        let doc = receiving("```ru\nx");
        assert!(matches!(&doc.blocks[0].kind,BlockKind::Code(code) if code.receiving));
        assert!(doc.objects.is_empty());
        assert!(
            matches!(&complete("```\n").blocks[0].kind,BlockKind::Code(code) if code.payload.is_empty())
        );
    }
    #[test]
    fn receiving_fences_require_parser_closure_and_a_complete_closing_line() {
        for text in [
            "```\nx\n```",
            "```\nx\n    ```\n",
            "```\nx\n> ```\n",
            "> ```\n> x\n>     ```\n",
        ] {
            let doc = receiving(text);
            assert!(doc.provisional.is_some(), "{text:?}");
            assert!(doc.objects.is_empty(), "{text:?}");
            assert!(matches!(&doc.blocks[0].kind, BlockKind::Code(code) if code.receiving));
            assert_eq!(&*doc.source.text(), text);
        }
        for text in [
            "```\nx\n```\n",
            "```\nx\n   ```\n",
            "~~~\nx\n~~~~\r\n",
            "> > ```\n> > x\n> > ```\n",
            "- ```\n  x\n  ```\n",
        ] {
            let doc = receiving(text);
            assert_eq!(doc.provisional, None, "{text:?}");
            assert!(matches!(&doc.blocks[0].kind, BlockKind::Code(code) if !code.receiving));
            assert!(
                doc.objects
                    .iter()
                    .any(|object| matches!(object.kind, ObjectKind::Code))
            );
        }
        let mut source = SourceStore::stream();
        source.append("```\nx\n```").unwrap();
        let first = parse_document(&source.snapshot(), &ParseLimits::default(), None);
        assert!(first.objects.is_empty());
        source.append("suffix\n").unwrap();
        let next = parse_document(&source.snapshot(), &ParseLimits::default(), Some(&first));
        assert!(next.objects.is_empty());
        assert_eq!(next.units[0].text, "x\n```suffix\n");
        source.finish(FinishOutcome::Completed);
        let finished = parse_document(&source.snapshot(), &ParseLimits::default(), Some(&next));
        assert!(finished.provisional.is_none());
        assert_eq!(
            finished.readable_text(),
            complete("```\nx\n```suffix\n").readable_text()
        );
    }
    #[test]
    fn identities_grow_reclassify_and_never_recycle() {
        let mut source = SourceStore::stream();
        source.append("same\n\ntext").unwrap();
        let first = parse_document(&source.snapshot(), &ParseLimits::default(), None);
        source.append(" grows").unwrap();
        let second = parse_document(&source.snapshot(), &ParseLimits::default(), Some(&first));
        assert_eq!(first.units[0].id, second.units[0].id);
        assert_eq!(first.units[1].id, second.units[1].id);
        source.append("\n----\n").unwrap();
        let third = parse_document(&source.snapshot(), &ParseLimits::default(), Some(&second));
        assert_ne!(second.units[1].id, third.units[1].id);
        assert_eq!(first.units[0].id, third.units[0].id);
        let independently_rejected =
            parse_document(&source.snapshot(), &ParseLimits::default(), None);
        assert_ne!(third.units[1].id, independently_rejected.units[1].id);
    }
    #[test]
    fn whitespace_only_lines_are_blank_boundaries() {
        for text in ["a\r\nb", "a\n  ", "\nb\n", "a\r"] {
            assert!(!blank_boundary(text), "{text:?}");
        }
        for text in ["a\n\n", "a\r\n\r\n", "a\r\r", "a\n \t\n", "a\r\n  \r\nb"] {
            assert!(blank_boundary(text), "{text:?}");
        }
        let doc = receiving("see https://example.com\n \t\n");
        assert_eq!(doc.provisional, None);
        assert!(
            doc.objects
                .iter()
                .any(|object| matches!(object.kind, ObjectKind::Link(_)))
        );
        assert!(
            receiving("see https://example.com\n  ")
                .provisional
                .is_some()
        );
    }
    #[test]
    fn short_fixtures_all_utf8_splits_and_codepoint_appends_finish_like_static() {
        let fixtures = [
            "",
            "é\r\n\t終😀",
            "Title\n===\n",
            "[x][r]\n\n[r]: /target\n\n",
            "| A | B |\n| - | - |\n| x | y |\n",
            "```luau\nprint('é')\n```",
            "- **a**\n- b\n",
            "e\u{301} &amp; `x`\n",
        ];
        for text in fixtures {
            let expected = complete(text).readable_text();
            for split in (0..=text.len()).filter(|i| text.is_char_boundary(*i)) {
                let mut source = SourceStore::stream();
                let mut prior = None;
                for chunk in [&text[..split], "", &text[split..]] {
                    source.append(chunk).unwrap();
                    let next =
                        parse_document(&source.snapshot(), &ParseLimits::default(), prior.as_ref());
                    assert_spans(&next);
                    prior = Some(next);
                }
                source.finish(FinishOutcome::Completed);
                let doc =
                    parse_document(&source.snapshot(), &ParseLimits::default(), prior.as_ref());
                assert_eq!(doc.readable_text(), expected, "{text:?} split {split}");
                assert_eq!(&*doc.source.text(), text);
            }
            let mut source = SourceStore::stream();
            let mut previous = None;
            for ch in text.chars() {
                source.append(ch.encode_utf8(&mut [0; 4])).unwrap();
                let doc = parse_document(
                    &source.snapshot(),
                    &ParseLimits::default(),
                    previous.as_ref(),
                );
                assert_spans(&doc);
                previous = Some(doc);
            }
            source.finish(FinishOutcome::Aborted("interrupted".into()));
            assert_eq!(
                parse_document(
                    &source.snapshot(),
                    &ParseLimits::default(),
                    previous.as_ref()
                )
                .readable_text(),
                expected
            );
        }
    }
    #[test]
    fn limits_degrade_without_dropping_accepted_source() {
        let text = "one\n\ntwo\n";
        let source = SourceStore::static_text(text).unwrap();
        for max_nodes in [0, 1, 2] {
            let doc = parse_document(
                &source.snapshot(),
                &ParseLimits {
                    max_nodes,
                    ..Default::default()
                },
                None,
            );
            assert!(doc.degraded);
            assert_eq!(doc.readable_text(), text);
        }
        let source = SourceStore::static_text("A | B\n- | -\nx | y").unwrap();
        let doc = parse_document(
            &source.snapshot(),
            &ParseLimits {
                max_table_cells: 3,
                ..Default::default()
            },
            None,
        );
        assert!(doc.degraded);
        assert_eq!(doc.readable_text(), &*source.snapshot().text());
    }
    #[test]
    fn heading_slug_duplicates_links_and_url_policy() {
        let doc = complete(
            "# Héllo, World!\n\n# Héllo, World!\n\nhttps://example.com/a_(b). `https://code.invalid`\n",
        );
        assert_eq!(
            doc.outline
                .iter()
                .map(|h| h.slug.as_str())
                .collect::<Vec<_>>(),
            ["héllo-world", "héllo-world-1"]
        );
        let links: Vec<_> = doc
            .objects
            .iter()
            .filter_map(|o| {
                if let ObjectKind::Link(l) = &o.kind {
                    Some(l.destination.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(links, ["https://example.com/a_(b)"]);
        let base = url::Url::parse("https://example.com/docs/").unwrap();
        assert!(
            matches!(resolve_link("../a",Some(&base)),ResolvedLink::External(url) if url.as_str()=="https://example.com/a")
        );
        assert!(matches!(resolve_link("#part",None),ResolvedLink::Fragment(s) if s=="part"));
        for value in ["javascript:alert(1)", "https://example.com/\n"] {
            assert!(matches!(
                resolve_link(value, None),
                ResolvedLink::Unavailable(_)
            ));
        }
    }
    #[test]
    fn wikilinks_and_relative_paths_resolve_to_host_documents() {
        let doc = complete(
            "[[work/verber|Verber]] [[Note#Plans]] [[#Local Part]] [m](my%20note.md#a%20b)\n",
        );
        let links: Vec<_> = doc
            .objects
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::Link(link) => Some(link.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            links
                .iter()
                .map(|l| (l.label.as_str(), l.wiki))
                .collect::<Vec<_>>(),
            [
                ("Verber", true),
                ("Note › Plans", true),
                ("Local Part", true),
                ("m", false)
            ]
        );
        let local = |path: &str, fragment: Option<&str>, wiki| {
            ResolvedLink::Local(LocalLink {
                path: path.into(),
                fragment: fragment.map(Into::into),
                wiki,
            })
        };
        let resolved: Vec<_> = links.iter().map(|l| l.resolve(None)).collect();
        assert_eq!(
            resolved,
            [
                local("work/verber", None, true),
                local("Note", Some("Plans"), true),
                ResolvedLink::Fragment("Local Part".into()),
                local("my note.md", Some("a b"), false),
            ]
        );
        assert_eq!(
            resolve_link("/tmp/file", None),
            local("/tmp/file", None, false)
        );
        assert_eq!(
            resolve_link("file:///tmp/a%20b.md#x", None),
            local("/tmp/a b.md", Some("x"), false)
        );
        // A base URL still makes relative paths URLs, but not wikilinks.
        let base = url::Url::parse("https://example.com/docs/").unwrap();
        assert!(matches!(
            links[3].resolve(Some(&base)),
            ResolvedLink::External(_)
        ));
        assert_eq!(
            links[0].resolve(Some(&base)),
            local("work/verber", None, true)
        );
    }
    #[test]
    fn block_ids_mark_their_blocks_and_leave_the_readable_text() {
        let text = "Para one ^p1\n\n- item a ^li\n- item b\n\n^list\n\n| a |\n| - |\n| 1 |\n\n^tab\n\n## Head ^h\n\nx^y and ^bad! and ^p1 dup ^p1\n";
        let doc = complete(text);
        let ids: Vec<(&str, &str)> = doc
            .block_anchors
            .iter()
            .map(|anchor| {
                (
                    anchor.id.as_str(),
                    text[anchor.source.clone()].lines().next().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            ids,
            [
                ("p1", "Para one ^p1"),
                ("li", "item a ^li"),
                ("list", "- item a ^li"),
                ("tab", "| a |"),
                ("h", "## Head ^h"),
            ]
        );
        let readable: Vec<&str> = doc.units.iter().map(|unit| unit.text.as_str()).collect();
        assert!(readable.contains(&"Para one"));
        assert!(readable.contains(&"item a"));
        assert!(
            !readable
                .iter()
                .any(|text| *text == "^list" || *text == "^tab")
        );
        // Only a whole trailing marker counts; the first holder of an id keeps it.
        assert!(readable.contains(&"x^y and ^bad! and ^p1 dup"));
        assert_eq!(doc.outline[0].text, "Head");
        assert_eq!(doc.outline[0].slug, "head");
    }
    #[test]
    fn an_image_alone_in_its_paragraph_is_an_image_block() {
        let doc = complete(
            "![Diagram](a.png)\n\n![[b.png|300]]\n\n![[c.png|300x200]]\n\n![[d.png|Caption]]\n\nText ![inline](e.png) here.\n",
        );
        let images: Vec<_> = doc
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                BlockKind::Image(image) => Some((
                    image.link.label.as_str(),
                    image.link.destination.as_str(),
                    image.link.wiki,
                    image.width,
                    image.height,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            images,
            [
                ("Diagram", "a.png", false, None, None),
                ("b.png", "b.png", true, Some(300), None),
                ("c.png", "c.png", true, Some(300), Some(200)),
                ("Caption", "d.png", true, None, None),
            ]
        );
        // An image block is a link object, and reads as its alt text.
        assert!(
            doc.objects
                .iter()
                .any(|o| matches!(&o.kind, ObjectKind::Link(link) if link.destination == "a.png"))
        );
        assert!(
            doc.units
                .iter()
                .any(|unit| unit.text == "Diagram (a.png)" && unit.atomic)
        );
        // An image in running text keeps its text form.
        assert!(
            doc.units
                .iter()
                .any(|unit| unit.text == "Text inline (e.png) here.")
        );
    }
    #[test]
    fn heading_slugs_disambiguate_generated_suffixes_and_literal_headings() {
        for (text, expected) in [
            ("# A\n# A\n# A-1\n# A\n", vec!["a", "a-1", "a-1-1", "a-2"]),
            ("# A-1\n# A\n# A\n", vec!["a-1", "a", "a-2"]),
        ] {
            let doc = complete(text);
            assert_eq!(
                doc.outline
                    .iter()
                    .map(|heading| heading.slug.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    #[test]
    fn rules_contribute_no_decoration_or_extra_copy_separators() {
        let doc = complete("---\n\none\n\n---\n\ntwo\n\n---\n");
        assert_eq!(doc.readable_text(), "one\n\ntwo");
        let range = ReadingRange {
            start: ReadingPosition {
                unit_id: doc.units[0].id,
                offset: 0,
            },
            end: ReadingPosition {
                unit_id: doc.units.last().unwrap().id,
                offset: 0,
            },
        };
        assert_eq!(doc.copy_range(&range).as_deref(), Some("one\n\ntwo"));
    }
    #[test]
    fn table_reference_changes_values_under_existing_keys() {
        let mut source = SourceStore::stream();
        source
            .append("A | B\n- | -\n[x][r] | y\n\nafter\n\n")
            .unwrap();
        let first = parse_document(&source.snapshot(), &ParseLimits::default(), None);
        source.append("[r]: https://example.com\n\n").unwrap();
        let next = parse_document(&source.snapshot(), &ParseLimits::default(), Some(&first));
        let (BlockKind::Table(a), BlockKind::Table(b)) =
            (&first.blocks[0].kind, &next.blocks[0].kind)
        else {
            panic!("tables");
        };
        assert_eq!(first.blocks[0].id, next.blocks[0].id);
        assert_eq!(a.rows[0].key, b.rows[0].key);
        assert_eq!(a.headers[0].key, b.headers[0].key);
        assert_eq!(a.rows[0].cells[0].text, "[x][r]");
        assert_eq!(b.rows[0].cells[0].text, "x");
    }
    #[test]
    fn limits_at_n_minus_one_n_and_n_plus_one() {
        let source = SourceStore::static_text("a").unwrap().snapshot();
        for (max_nodes, degraded) in [(2, true), (3, false), (4, false)] {
            assert_eq!(
                parse_document(
                    &source,
                    &ParseLimits {
                        max_nodes,
                        ..Default::default()
                    },
                    None
                )
                .degraded,
                degraded
            );
        }
        for (max_depth, degraded) in [(0, true), (1, false), (2, false)] {
            assert_eq!(
                parse_document(
                    &source,
                    &ParseLimits {
                        max_depth,
                        ..Default::default()
                    },
                    None
                )
                .degraded,
                degraded
            );
        }
        let source = SourceStore::static_text("A | B\n- | -\nx | y")
            .unwrap()
            .snapshot();
        for (max_table_cells, degraded) in [(3, true), (4, false), (5, false)] {
            assert_eq!(
                parse_document(
                    &source,
                    &ParseLimits {
                        max_table_cells,
                        ..Default::default()
                    },
                    None
                )
                .degraded,
                degraded
            );
        }
    }

    #[test]
    fn duplicate_trailing_definition_stays_visible_without_changing_first_destination() {
        let text =
            "[x][r]\n\n[r]: https://first.invalid\n\n[r]: https://second.invalid\n  \"title\"";
        let doc = receiving(text);
        assert_eq!(
            doc.units.last().unwrap().text,
            "[r]: https://second.invalid\n  \"title\""
        );
        let links: Vec<_> = doc
            .objects
            .iter()
            .filter_map(|o| {
                if let ObjectKind::Link(link) = &o.kind {
                    Some(link.destination.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(links, ["https://first.invalid"]);
    }

    #[test]
    fn inline_html_breaks_alerts_and_ordered_lists_have_explicit_semantics() {
        let doc = complete(
            "hello <b>world</b>\nsoft  \nhard\n\n> [!NOTE]\n> caution\n\n3. three\n4. four\n",
        );
        assert_eq!(doc.units[0].text, "hello <b>world</b>\nsoft\nhard");
        assert_eq!(doc.units[0].soft_breaks, [18]);
        assert_eq!(doc.blocks[1].quote_kind.as_deref(), Some("NOTE"));
        assert_eq!(doc.blocks[1].quote_depth, 1);
        assert_eq!(doc.blocks[2].list_marker.as_deref(), Some("3."));
        assert_eq!(doc.blocks[3].list_marker.as_deref(), Some("4."));
    }
    #[test]
    fn common_alert_labels_take_the_tone_of_the_closest_github_kind() {
        use super::{Alert, AlertTone};

        for (label, tone) in [
            ("NOTE", AlertTone::Note),
            ("info", AlertTone::Note),
            ("Tip", AlertTone::Tip),
            ("success", AlertTone::Tip),
            ("IMPORTANT", AlertTone::Important),
            ("question", AlertTone::Warning),
            ("danger", AlertTone::Caution),
            ("bug", AlertTone::Caution),
        ] {
            assert_eq!(AlertTone::of(label), Some(tone), "{label}");
        }
        assert_eq!(AlertTone::of("unknown"), None);

        let doc = complete("> [!info]-\n> body text\n>\n> second\n");
        assert_eq!(doc.blocks[0].quote_kind.as_deref(), Some("INFO"));
        assert_eq!(doc.units[0].text, "body text", "the marker line goes");
        let alert = [Alert {
            tone: AlertTone::Note,
            depth: 1,
        }];
        assert_eq!(doc.blocks[0].alerts, alert);
        assert_eq!(doc.blocks[1].alerts, alert, "every block of the alert");
        assert_eq!(doc.blocks[1].quote_kind, None, "only the first is labelled");

        let doc = complete("> [!WARNING]\n> careful\n");
        assert_eq!(doc.blocks[0].quote_kind.as_deref(), Some("WARNING"));
        assert_eq!(
            doc.blocks[0].alerts.first().map(|alert| alert.tone),
            Some(AlertTone::Warning)
        );

        let doc = complete("> [!unknown]\n> stays text\n");
        assert_eq!(doc.blocks[0].alerts, []);
        assert!(doc.units[0].text.starts_with("[!unknown]"));

        let doc = complete("> [!tip]\n> outer\n>\n> > [!danger]\n> > inner\n");
        let tones: Vec<_> = doc
            .blocks
            .iter()
            .map(|block| {
                block
                    .alerts
                    .iter()
                    .map(|alert| (alert.tone, alert.depth))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            tones,
            [
                vec![(AlertTone::Tip, 1)],
                vec![(AlertTone::Tip, 1), (AlertTone::Caution, 2)],
            ],
            "a nested alert keeps its own tone, and the outer rail keeps its"
        );
    }
    #[test]
    fn quote_join_counts_the_quotes_a_block_shares_with_the_next() {
        let doc = complete("> a\n>\n> > b\n>\n> c\n\n> d\n\ne\n");
        let joins: Vec<_> = doc.blocks.iter().map(|block| block.quote_join).collect();
        assert_eq!(joins, [1, 1, 0, 0, 0]);
    }
    #[test]
    fn loose_list_continuations_do_not_repeat_item_markers() {
        let doc = complete(
            "1. first\n\n   continuation\n\n   - nested\n\n     nested continuation\n\n   > quoted\n   >\n   > quote continuation\n\n2. next\n",
        );
        assert_eq!(
            doc.blocks
                .iter()
                .map(|block| block.list_marker.as_deref())
                .collect::<Vec<_>>(),
            [Some("1."), None, Some("◦"), None, None, None, Some("2.")]
        );
        assert_eq!(doc.blocks[1].depth, doc.blocks[0].depth);
        assert_eq!(doc.blocks[3].depth, doc.blocks[2].depth);
        assert_eq!(doc.blocks[4].quote_depth, 1);
        assert_eq!(doc.blocks[5].quote_depth, 1);
    }
    #[test]
    fn explicit_html_and_nested_quote_fences_close_before_finish() {
        for html in [
            "<!-- comment -->",
            "<?instruction?>",
            "<![CDATA[x]]>",
            "<!DOCTYPE html>",
            "<SCRIPT>\nx\n</SCRIPT>",
        ] {
            assert_eq!(receiving(html).provisional, None, "{html}");
        }
        let doc = receiving("> > ```rust\n> > x\n> > ```\n");
        assert_eq!(doc.provisional, None);
        assert!(matches!(&doc.blocks[0].kind,BlockKind::Code(code) if &*code.payload=="x\n"));
        let doc = receiving("> [r]: https://example.com");
        assert_eq!(doc.blocks[0].quote_depth, 1);
        assert!(doc.objects.is_empty());
    }
    #[test]
    fn bare_url_object_spans_are_exact_and_distinct() {
        let text = "&amp; https://example.com/a_(b). https://example.com/a_(b)\n";
        let doc = complete(text);
        assert_eq!(doc.objects.len(), 2);
        assert_ne!(doc.objects[0].id, doc.objects[1].id);
        for object in &doc.objects {
            assert_eq!(&text[object.source.clone()], "https://example.com/a_(b)");
        }
    }
}
