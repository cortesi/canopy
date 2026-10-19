// Copyright 2017 Google Inc. All rights reserved.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

//! Tree-based two pass parser.

use alloc::{borrow::ToOwned, boxed::Box, collections::VecDeque, string::String, vec::Vec};
use core::{
    cell::Cell,
    cmp::{max, min},
    iter::FusedIterator,
    num::NonZeroUsize,
    ops::{Index, Range},
};
use std::collections::HashMap;

use unicase::UniCase;

use crate::{
    Alignment, BlockQuoteKind, Callout, CodeBlockKind, ContainerKind, Embed, EmbedKind, Event,
    HeadingLevel, Limits, LinkType, MetadataBlockKind, Options, Tag, TagEnd, autolink, comments,
    firstpass::run_first_pass,
    linklabel::{FootnoteLabel, LinkLabel, ReferenceLabel, scan_link_label_rest},
    links::{embed_size, wikilink_label},
    scanners::*,
    strings::CowStr,
    tree::{Tree, TreeIndex},
};

// Allowing arbitrary depth nested parentheses inside link destinations
// can create denial of service vulnerabilities if we're not careful.
// The simplest countermeasure is to limit their depth, which is
// explicitly allowed by the spec as long as the limit is at least 3:
// https://spec.commonmark.org/0.29/#link-destination
pub const LINK_MAX_NESTED_PARENS: usize = 32;

#[derive(Debug, Default, Clone)]
pub struct Item {
    pub start: usize,
    pub end: usize,
    pub body: ItemBody,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub enum ItemBody {
    // These are possible inline items, need to be resolved in second pass.

    // repeats, can_open, can_close
    MaybeEmphasis(usize, bool, bool),
    // can_open, can_close, brace context
    MaybeMath(bool, bool, u8),
    // quote byte, can_open, can_close
    MaybeSmartQuote(u8, bool, bool),
    MaybeCode(usize, bool), // number of backticks, preceded by backslash
    MaybeHtml,
    MaybeLinkOpen,
    // bool indicates whether or not the preceding section could be a reference
    MaybeLinkClose(bool),
    MaybeImage,

    // These are inline items after resolution.
    Emphasis,
    Strong,
    Strikethrough,
    Highlight,
    Superscript,
    Subscript,
    Math(CowIndex, bool), // true for display math
    Code(CowIndex),
    Link(LinkIndex),
    Image(LinkIndex),
    Embed(EmbedIndex),
    FootnoteReference(CowIndex),
    TaskListMarker(u8), // the character in the brackets, a space when open

    // These are also inline items.
    InlineHtml,
    OwnedInlineHtml(CowIndex),
    SynthesizeText(CowIndex),
    SynthesizeChar(char),
    Html,
    Text {
        backslash_escaped: bool,
    },
    BlockId,
    Hashtag,
    IgnoredCells(usize),
    SoftBreak,
    // true = is backlash
    HardBreak(bool),

    // Dummy node at the top of the tree - should not be used otherwise!
    #[default]
    Root,

    // These are block items.
    Paragraph,
    TightParagraph,
    Rule,
    Heading(HeadingLevel, Option<HeadingIndex>), // heading level
    FencedCodeBlock(CowIndex),
    IndentCodeBlock,
    HtmlBlock,
    BlockQuote(Option<BlockQuoteKind>),
    Callout(BlockQuoteKind, CalloutIndex),
    Container(u8, ContainerKind, CowIndex), /* (fence length, specific renderer, descriptor
                                             * used in renderer) */
    // is_tight|last_line_empty, list character, list start index
    List(Cell<ListFlags>, u8, u64),
    ListItem(usize), // indent level
    FootnoteDefinition(CowIndex),
    MetadataBlock(MetadataBlockKind),

    // Definition lists
    DefinitionList(Cell<ListFlags>),
    // gets turned into either a paragraph or a definition list title,
    // depending on whether there's a definition after it
    MaybeDefinitionListTitle,
    DefinitionListTitle,
    DefinitionListDefinition(usize),

    // Tables
    Table(AlignmentIndex),
    TableHead,
    TableRow,
    TableCell,
}

impl ItemBody {
    fn is_maybe_inline(&self) -> bool {
        use ItemBody::*;
        matches!(
            *self,
            MaybeEmphasis(..)
                | MaybeMath(..)
                | MaybeSmartQuote(..)
                | MaybeCode(..)
                | MaybeHtml
                | MaybeLinkOpen
                | MaybeLinkClose(..)
                | MaybeImage
        )
    }
    fn is_inline(&self) -> bool {
        use ItemBody::*;
        matches!(
            *self,
            MaybeEmphasis(..)
                | MaybeMath(..)
                | MaybeSmartQuote(..)
                | MaybeCode(..)
                | MaybeHtml
                | MaybeLinkOpen
                | MaybeLinkClose(..)
                | MaybeImage
                | Emphasis
                | Strong
                | Strikethrough
                | Highlight
                | Math(..)
                | Code(..)
                | Link(..)
                | Embed(..)
                | Image(..)
                | FootnoteReference(..)
                | TaskListMarker(..)
                | InlineHtml
                | OwnedInlineHtml(..)
                | SynthesizeText(..)
                | SynthesizeChar(..)
                | Html
                | Text { .. }
                | BlockId
                | Hashtag
                | SoftBreak
                | HardBreak(..)
        )
    }
}

bitflags::bitflags! {
    /// Option struct containing flags for enabling extra features
    /// that are not part of the CommonMark spec.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(crate) struct ListFlags: u8 {
        const IS_TIGHT = 1;
        const LAST_LINE_EMPTY = 1 << 1;
    }
}

#[derive(Debug)]
pub struct BrokenLink<'a> {
    pub span: core::ops::Range<usize>,
    pub link_type: LinkType,
    pub reference: CowStr<'a>,
}

/// Markdown event iterator.
pub struct Parser<'input, CB = DefaultParserCallbacks> {
    callbacks: CB,
    inner: ParserInner<'input>,
}

// Inner state for `Parser`, extracted so that it can remain generic over the
// callback without re-compiling complex logic for each instantiation of the
// generic type.
struct ParserInner<'input> {
    text: &'input str,
    options: Options,
    tree: Tree<Item>,
    allocs: Allocations<'input>,
    html_scan_guard: HtmlScanGuard,

    // https://github.com/pulldown-cmark/pulldown-cmark/issues/844
    // Consider this example:
    //
    //     [x]: xxx...
    //     [x]
    //     [x]
    //     [x]
    //
    // Which expands to this HTML:
    //
    //     <a href="xxx...">x</a>
    //     <a href="xxx...">x</a>
    //     <a href="xxx...">x</a>
    //
    // This is quadratic growth, because it's filling in the area of a square.
    // To prevent this, track how much it's expanded and limit it.
    link_ref_expansion_limit: usize,

    // used by inline passes. store them here for reuse
    inline_stack: InlineStack,
    link_stack: LinkStack,
    wikilink_stack: LinkStack,
    code_delims: CodeDelims,
    math_delims: MathDelims,

    /// Heading ids given so far, and how many repeats each base id had.
    heading_ids: HashMap<String, usize>,

    /// Bounds on the parse's work.
    limits: Limits,
    /// Whether the parse passed a bound, and stopped.
    limit_hit: bool,
    /// Events given so far.
    events: usize,
    /// Tags open.
    depth: usize,
    /// How each heading took its id, by its start, for a later resume.
    headings: Vec<(usize, HeadingKey)>,
    /// Inline footnotes read so far, which number their labels.
    inline_footnotes: usize,
}

/// How a heading took its id.
#[derive(Clone, Debug, PartialEq, Eq)]
enum HeadingKey {
    /// From its text, which gave this base id before any suffix.
    Base(String),
    /// From its own attributes.
    Explicit(String),
}

/// What a parse learned about a document, for parsing a later part of it on
/// its own with [`Parser::new_resumed`]: its link and footnote definitions,
/// and how its headings took their ids.
#[derive(Clone, Debug, Default)]
pub struct Resume {
    /// Link reference definitions, by label, in source order.
    refdefs: Vec<(String, LinkDef<'static>)>,
    /// Footnote definitions, by start and label.
    footnotes: Vec<(usize, String)>,
    /// Headings, by start, and how each took its id.
    headings: Vec<(usize, HeadingKey)>,
}

impl Resume {
    /// What the parse learned from the text before `at`.
    #[must_use]
    pub fn before(&self, at: usize) -> Self {
        Self {
            refdefs: self
                .refdefs
                .iter()
                .filter(|(_, definition)| definition.span.start < at)
                .cloned()
                .collect(),
            footnotes: self
                .footnotes
                .iter()
                .filter(|(start, _)| *start < at)
                .cloned()
                .collect(),
            headings: self
                .headings
                .iter()
                .filter(|(start, _)| *start < at)
                .cloned()
                .collect(),
        }
    }

    /// Join what a parse of the text before a resume learned with what the
    /// resumed parse learned after it.
    #[must_use]
    pub fn then(mut self, after: Self) -> Self {
        self.refdefs.extend(after.refdefs);
        self.footnotes.extend(after.footnotes);
        self.headings.extend(after.headings);
        self
    }
}

impl<'input, CB> core::fmt::Debug for Parser<'input, CB> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Only print the fields that have public types.
        f.debug_struct("Parser")
            .field("text", &self.inner.text)
            .field("options", &self.inner.options)
            .field("callbacks", &..)
            .finish()
    }
}

impl<'a> BrokenLink<'a> {
    /// Moves the link into version with a static lifetime.
    ///
    /// The `reference` member is cloned to a Boxed or Inline version.
    pub fn into_static(self) -> BrokenLink<'static> {
        BrokenLink {
            span: self.span,
            link_type: self.link_type,
            reference: self.reference.into_string().into(),
        }
    }
}

impl<'input> Parser<'input, DefaultParserCallbacks> {
    /// Creates a new event iterator for a markdown string without any options
    /// enabled.
    pub fn new(text: &'input str) -> Self {
        Self::new_ext(text, Options::empty())
    }

    /// Creates a new event iterator for a markdown string with given options.
    pub fn new_ext(text: &'input str, options: Options) -> Self {
        Self::new_with_callbacks(text, options, DefaultParserCallbacks)
    }
}

impl<'input, CB: ParserCallbacks<'input>> Parser<'input, CB> {
    /// Creates a new event iterator for markdown text with given options and
    /// callbacks.
    ///
    /// ```
    /// # use canopy_xmark::{BrokenLink, CowStr, Event, Options, Parser, ParserCallbacks, Tag};
    /// struct CustomCallbacks;
    /// impl<'input> ParserCallbacks<'input> for CustomCallbacks {
    ///     fn handle_broken_link(
    ///         &mut self,
    ///         link: BrokenLink<'input>,
    ///     ) -> Option<(CowStr<'input>, CowStr<'input>)> {
    ///         Some(("https://target".into(), link.reference))
    ///     }
    /// }
    ///
    /// let mut parser = Parser::new_with_callbacks("[broken]", Options::empty(), CustomCallbacks);
    ///
    /// assert!(matches!(
    ///     parser.nth(1),
    ///     Some(Event::Start(Tag::Link { .. }))
    /// ));
    /// ```
    ///
    /// See the [`ParserCallbacks`] trait for a list of callbacks that can be
    /// overridden.
    pub fn new_with_callbacks(text: &'input str, options: Options, callbacks: CB) -> Self {
        Self::new_with_limits(text, options, Limits::NONE, callbacks)
    }

    /// Creates a new event iterator for a markdown string with given options,
    /// callbacks, and bounds on its work. Past a bound, the parser stops:
    /// the block pass ends before it builds the whole document, and the
    /// events end. [`OffsetIter::limit_hit`] then says so.
    pub fn new_with_limits(
        text: &'input str,
        options: Options,
        limits: Limits,
        callbacks: CB,
    ) -> Self {
        Self::new_resumed(text, options, limits, 0, &Resume::default(), callbacks)
    }

    /// Creates an event iterator for the part of `text` from `from`, the
    /// start of a line that begins a top-level block, as if the document
    /// before it were already parsed. `resume` carries what that part
    /// learned: its link and footnote definitions, and its heading ids, so
    /// links resolve and ids number as in a parse of the whole. Source
    /// ranges index `text`.
    pub fn new_resumed(
        text: &'input str,
        options: Options,
        limits: Limits,
        from: usize,
        resume: &Resume,
        callbacks: CB,
    ) -> Self {
        let mut allocs = Allocations::new();
        for (label, definition) in &resume.refdefs {
            allocs
                .refdefs
                .0
                .entry(UniCase::new(label.clone().into()))
                .or_insert_with(|| definition.clone());
        }
        for (_, label) in &resume.footnotes {
            allocs.footdefs.0.insert(
                UniCase::new(label.clone().into()),
                FootnoteDef { use_count: 0 },
            );
        }
        let (mut tree, allocs, exceeded) = run_first_pass(text, options, limits, from, allocs);
        tree.reset();
        let inline_stack = Default::default();
        let link_stack = Default::default();
        let wikilink_stack = Default::default();
        let html_scan_guard = Default::default();
        Parser {
            callbacks,

            inner: ParserInner {
                text,
                options,
                tree,
                allocs,
                inline_stack,
                link_stack,
                wikilink_stack,
                html_scan_guard,
                // always allow 100KiB
                link_ref_expansion_limit: text.len().max(100_000),
                code_delims: CodeDelims::new(),
                math_delims: MathDelims::new(),
                heading_ids: HashMap::new(),
                limits,
                limit_hit: exceeded,
                events: 0,
                depth: 0,
                headings: Vec::new(),
                inline_footnotes: 0,
            },
        }
        .with_heading_ids(&resume.headings)
    }

    /// Number heading ids as if the `headings` before came first.
    fn with_heading_ids(mut self, headings: &[(usize, HeadingKey)]) -> Self {
        for (_, key) in headings {
            match key {
                HeadingKey::Base(base) => {
                    self.inner.unique_heading_id(base);
                }
                HeadingKey::Explicit(id) => {
                    self.inner.heading_ids.entry(id.clone()).or_default();
                }
            }
        }
        self
    }

    /// Returns a reference to the internal `RefDefs` object, which provides
    /// access to the internal map of reference definitions.
    pub fn reference_definitions(&self) -> &RefDefs<'input> {
        &self.inner.allocs.refdefs
    }

    /// Consumes the event iterator and produces an iterator that produces
    /// `(Event, Range)` pairs, where the `Range` value maps to the
    /// corresponding range in the markdown source.
    pub fn into_offset_iter(self) -> OffsetIter<'input, CB> {
        OffsetIter { parser: self }
    }
}

impl<'input, F> Parser<'input, BrokenLinkCallback<F>> {
    /// In case the parser encounters any potential links that have a broken
    /// reference (e.g `[foo]` when there is no `[foo]: ` entry at the bottom)
    /// the provided callback will be called with the reference name,
    /// and the returned pair will be used as the link URL and title if it is
    /// not `None`.
    ///
    /// This constructor is provided for backwards compatibility.
    /// This and other callbacks can also be customized with
    /// [`Parser::new_with_callbacks`].
    pub fn new_with_broken_link_callback(
        text: &'input str,
        options: Options,
        broken_link_callback: Option<F>,
    ) -> Self
    where
        F: FnMut(BrokenLink<'input>) -> Option<(CowStr<'input>, CowStr<'input>)>,
    {
        Self::new_with_callbacks(text, options, BrokenLinkCallback(broken_link_callback))
    }
}

impl<'input> ParserInner<'input> {
    /// Use a link label to fetch a type, url, and title.
    ///
    /// This function enforces the [`link_ref_expansion_limit`].
    /// If it returns Some, it also consumes some of the fuel.
    /// If we're out of fuel, it immediately returns None.
    ///
    /// The URL and title are found in the [`RefDefs`] map.
    /// If they're not there, and a callback was provided by the user,
    /// `handle_broken_link` will be invoked and given the opportunity
    /// to provide a fallback.
    ///
    /// The link type (that's "link" or "image") depends on the usage site, and
    /// is provided by the caller of this function.
    /// This function returns a new one because, if it has to invoke a callback
    /// to find the information, the link type is [mapped to an unknown type].
    ///
    /// [mapped to an unknown type]: crate::LinkType::to_unknown
    /// [`link_ref_expansion_limit`]: Self::link_ref_expansion_limit
    fn fetch_link_type_url_title(
        &mut self,
        link_label: CowStr<'input>,
        span: Range<usize>,
        link_type: LinkType,
        callbacks: &mut dyn ParserCallbacks<'input>,
    ) -> Option<(LinkType, CowStr<'input>, CowStr<'input>)> {
        if self.link_ref_expansion_limit == 0 {
            return None;
        }

        let (link_type, url, title) = self
            .allocs
            .refdefs
            .get(link_label.as_ref())
            .map(|matching_def| {
                // found a matching definition!
                let title = matching_def
                    .title
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "".into());
                let url = matching_def.dest.clone();
                (link_type, url, title)
            })
            .or_else(|| {
                // Construct a BrokenLink struct, which will be passed to the
                // callback
                let broken_link = BrokenLink {
                    span,
                    link_type,
                    reference: link_label,
                };

                callbacks
                    .handle_broken_link(broken_link)
                    .map(|(url, title)| (link_type.to_unknown(), url, title))
            })?;

        // Limit expansion from link references.
        // This isn't a problem for footnotes, because multiple references to
        // the same one reuse the same node, but links/images get their
        // HREF/SRC copied.
        self.link_ref_expansion_limit = self
            .link_ref_expansion_limit
            .saturating_sub(url.len() + title.len());

        Some((link_type, url, title))
    }

    /// Handle inline markup.
    ///
    /// When the parser encounters any item indicating potential inline markup,
    /// all inline markup passes are run on the remainder of the chain.
    ///
    /// Note: there's some potential for optimization here, but that's future
    /// work.
    fn handle_inline(&mut self, callbacks: &mut dyn ParserCallbacks<'input>) {
        self.handle_inline_pass1(callbacks);
        self.handle_emphasis_and_hard_break();
    }

    /// Handle inline HTML, code spans, and links.
    ///
    /// This function handles both inline HTML and code spans, because they have
    /// the same precedence. It also handles links, even though they have lower
    /// precedence, because the URL of links must not be processed.
    fn handle_inline_pass1(&mut self, callbacks: &mut dyn ParserCallbacks<'input>) {
        let mut cur = self.tree.cur();
        let mut prev = None;

        let block_end = self.tree[self.tree.peek_up().unwrap()].item.end;
        let block_text = &self.text[..block_end];

        while let Some(mut cur_ix) = cur {
            match self.tree[cur_ix].item.body {
                ItemBody::MaybeHtml => {
                    let next = self.tree[cur_ix].next;
                    let autolink = if let Some(next_ix) = next {
                        scan_autolink(block_text, self.tree[next_ix].item.start)
                    } else {
                        None
                    };

                    if let Some((ix, uri, link_type)) = autolink {
                        let node = scan_nodes_to_ix(&self.tree, next, ix);
                        let text_node = self.tree.create_node(Item {
                            start: self.tree[cur_ix].item.start + 1,
                            end: ix - 1,
                            body: ItemBody::Text {
                                backslash_escaped: false,
                            },
                        });
                        let link_ix =
                            self.allocs
                                .allocate_link(link_type, uri, "".into(), "".into());
                        self.tree[cur_ix].item.body = ItemBody::Link(link_ix);
                        self.tree[cur_ix].item.end = ix;
                        self.tree[cur_ix].next = node;
                        self.tree[cur_ix].child = Some(text_node);
                        prev = cur;
                        cur = node;
                        if let Some(node_ix) = cur {
                            self.tree[node_ix].item.start = max(self.tree[node_ix].item.start, ix);
                        }
                        continue;
                    } else {
                        let inline_html = next.and_then(|next_ix| {
                            self.scan_inline_html(block_text, self.tree[next_ix].item.start)
                        });
                        if let Some((html, ix)) = inline_html {
                            let node = scan_nodes_to_ix(&self.tree, next, ix);
                            self.tree[cur_ix].item.body = match html {
                                // unchanged: rendered straight from the input
                                CowStr::Borrowed(_) => ItemBody::InlineHtml,
                                owned => ItemBody::OwnedInlineHtml(
                                    self.allocs.allocate_cow(CowStr::from_replace_nuls(owned)),
                                ),
                            };
                            self.tree[cur_ix].item.end = ix;
                            self.tree[cur_ix].next = node;
                            prev = cur;
                            cur = node;
                            if let Some(node_ix) = cur {
                                self.tree[node_ix].item.start =
                                    max(self.tree[node_ix].item.start, ix);
                            }
                            continue;
                        }
                    }
                    self.tree[cur_ix].item.body = ItemBody::Text {
                        backslash_escaped: false,
                    };
                }
                ItemBody::MaybeMath(can_open, _can_close, brace_context) => {
                    if !can_open {
                        self.tree[cur_ix].item.body = ItemBody::Text {
                            backslash_escaped: false,
                        };
                        prev = cur;
                        cur = self.tree[cur_ix].next;
                        continue;
                    }
                    let is_display = self.tree[cur_ix].next.is_some_and(|next_ix| {
                        matches!(
                            self.tree[next_ix].item.body,
                            ItemBody::MaybeMath(_can_open, _can_close, _brace_context)
                        )
                    });
                    let result = if self.math_delims.is_populated() {
                        // we have previously scanned all math environment
                        // delimiters, so we can reuse
                        // that work
                        self.math_delims
                            .find(&self.tree, cur_ix, is_display, brace_context)
                    } else {
                        // we haven't previously scanned all math delimiters,
                        // so walk the AST
                        let mut scan = self.tree[cur_ix].next;
                        if is_display {
                            // a display delimiter, `$$`, is actually two
                            // delimiters
                            // skip the second one
                            scan = self.tree[scan.unwrap()].next;
                        }
                        let mut invalid = false;
                        while let Some(scan_ix) = scan {
                            if let ItemBody::MaybeMath(_can_open, can_close, delim_brace_context) =
                                self.tree[scan_ix].item.body
                            {
                                let delim_is_display =
                                    self.tree[scan_ix].next.is_some_and(|next_ix| {
                                        matches!(
                                            self.tree[next_ix].item.body,
                                            ItemBody::MaybeMath(
                                                _can_open,
                                                _can_close,
                                                _brace_context
                                            )
                                        )
                                    });
                                if !invalid && delim_brace_context == brace_context {
                                    if (!is_display && can_close)
                                        || (is_display && delim_is_display)
                                    {
                                        // This will skip ahead past everything
                                        // we
                                        // just inserted. Needed for correctness
                                        // to
                                        // ensure that a new scan is done after
                                        // this item.
                                        self.math_delims.clear();
                                        break;
                                    } else {
                                        // Math cannot contain $, so the current
                                        // item
                                        // is invalid. Keep scanning to fill
                                        // math_delims.
                                        invalid = true;
                                    }
                                }
                                self.math_delims.insert(
                                    delim_is_display,
                                    delim_brace_context,
                                    scan_ix,
                                    can_close,
                                );
                            }
                            scan = self.tree[scan_ix].next;
                        }
                        scan
                    };

                    if let Some(scan_ix) = result {
                        self.make_math_span(cur_ix, scan_ix);
                    } else {
                        self.tree[cur_ix].item.body = ItemBody::Text {
                            backslash_escaped: false,
                        };
                    }
                }
                ItemBody::MaybeCode(mut search_count, preceded_by_backslash) => {
                    if preceded_by_backslash {
                        search_count -= 1;
                        if search_count == 0 {
                            self.tree[cur_ix].item.body = ItemBody::Text {
                                backslash_escaped: false,
                            };
                            prev = cur;
                            cur = self.tree[cur_ix].next;
                            continue;
                        }
                    }

                    if self.code_delims.is_populated() {
                        // we have previously scanned all codeblock delimiters,
                        // so we can reuse that work
                        if let Some(scan_ix) = self.code_delims.find(cur_ix, search_count) {
                            self.make_code_span(cur_ix, scan_ix, preceded_by_backslash);
                        } else {
                            self.tree[cur_ix].item.body = ItemBody::Text {
                                backslash_escaped: false,
                            };
                        }
                    } else {
                        // we haven't previously scanned all codeblock
                        // delimiters, so walk the AST
                        let mut scan = if search_count > 0 {
                            self.tree[cur_ix].next
                        } else {
                            None
                        };
                        while let Some(scan_ix) = scan {
                            if let ItemBody::MaybeCode(delim_count, _) =
                                self.tree[scan_ix].item.body
                            {
                                if search_count == delim_count {
                                    self.make_code_span(cur_ix, scan_ix, preceded_by_backslash);
                                    self.code_delims.clear();
                                    break;
                                } else {
                                    self.code_delims.insert(delim_count, scan_ix);
                                }
                            }
                            scan = self.tree[scan_ix].next;
                        }
                        if scan.is_none() {
                            self.tree[cur_ix].item.body = ItemBody::Text {
                                backslash_escaped: false,
                            };
                        }
                    }
                }
                ItemBody::MaybeLinkOpen => {
                    self.tree[cur_ix].item.body = ItemBody::Text {
                        backslash_escaped: false,
                    };
                    let link_open_doubled = self.tree[cur_ix]
                        .next
                        .map(|ix| self.tree[ix].item.body == ItemBody::MaybeLinkOpen)
                        .unwrap_or(false);
                    if self.options.contains(Options::ENABLE_WIKILINKS) && link_open_doubled {
                        self.wikilink_stack.push(LinkStackEl {
                            node: cur_ix,
                            ty: LinkStackTy::Link,
                        });
                    }
                    self.link_stack.push(LinkStackEl {
                        node: cur_ix,
                        ty: LinkStackTy::Link,
                    });
                }
                ItemBody::MaybeImage => {
                    self.tree[cur_ix].item.body = ItemBody::Text {
                        backslash_escaped: false,
                    };
                    let link_open_doubled = self.tree[cur_ix]
                        .next
                        .map(|ix| self.tree[ix].item.body == ItemBody::MaybeLinkOpen)
                        .unwrap_or(false);
                    if self.options.contains(Options::ENABLE_WIKILINKS) && link_open_doubled {
                        self.wikilink_stack.push(LinkStackEl {
                            node: cur_ix,
                            ty: LinkStackTy::Image,
                        });
                    }
                    self.link_stack.push(LinkStackEl {
                        node: cur_ix,
                        ty: LinkStackTy::Image,
                    });
                }
                ItemBody::MaybeLinkClose(could_be_ref) => {
                    self.tree[cur_ix].item.body = ItemBody::Text {
                        backslash_escaped: false,
                    };
                    let tos_link = self.link_stack.pop();
                    if self.options.contains(Options::ENABLE_WIKILINKS)
                        && self.tree[cur_ix]
                            .next
                            .map(|ix| {
                                matches!(self.tree[ix].item.body, ItemBody::MaybeLinkClose(..))
                            })
                            .unwrap_or(false)
                        && let Some(node) = self.handle_wikilink(block_text, cur_ix, prev)
                    {
                        prev = Some(node);
                        cur = self.tree[node].next;
                        continue;
                    }
                    if let Some(tos) = tos_link {
                        // skip rendering if already in a link, unless its an
                        // image
                        if tos.ty != LinkStackTy::Image
                            && matches!(
                                self.tree[self.tree.peek_up().unwrap()].item.body,
                                ItemBody::Link(..)
                            )
                        {
                            continue;
                        }
                        if tos.ty == LinkStackTy::Disabled {
                            continue;
                        }
                        let next = self.tree[cur_ix].next;
                        if let Some((next_ix, url, title)) =
                            self.scan_inline_link(block_text, self.tree[cur_ix].item.end, next)
                        {
                            let next_node = scan_nodes_to_ix(&self.tree, next, next_ix);
                            if let Some(prev_ix) = prev {
                                self.tree[prev_ix].next = None;
                            }
                            cur = Some(tos.node);
                            cur_ix = tos.node;
                            let link_ix =
                                self.allocs
                                    .allocate_link(LinkType::Inline, url, title, "".into());
                            self.tree[cur_ix].item.body = if tos.ty == LinkStackTy::Image {
                                ItemBody::Image(link_ix)
                            } else {
                                ItemBody::Link(link_ix)
                            };
                            self.tree[cur_ix].child = self.tree[cur_ix].next;
                            self.tree[cur_ix].next = next_node;
                            self.tree[cur_ix].item.end = next_ix;
                            if let Some(next_node_ix) = next_node {
                                self.tree[next_node_ix].item.start =
                                    max(self.tree[next_node_ix].item.start, next_ix);
                            }

                            if tos.ty == LinkStackTy::Link {
                                self.disable_all_links();
                            }
                        } else {
                            // ok, so its not an inline link. maybe it is a
                            // reference
                            // to a defined link?
                            let scan_result =
                                scan_reference(&self.tree, block_text, cur_ix, self.options);
                            // A collapsed reference (`[label][]`) extends
                            // through the
                            // trailing `[]`, unlike a shortcut (`[label]`).
                            // Remember the
                            // end of that `[]` so the emitted span (and the
                            // span handed
                            // to the broken-link callback) covers the whole
                            // construct,
                            // matching how full references (`[label][ref]`) are
                            // handled.
                            let collapsed_end = if let RefScan::Collapsed(_, end) = &scan_result {
                                Some(*end)
                            } else {
                                None
                            };
                            let (node_after_link, link_type) = match scan_result {
                                // [label][reference]
                                RefScan::LinkLabel(_, end_ix) => {
                                    // Toggle reference viability of the last
                                    // closing bracket,
                                    // so that we can skip it on future
                                    // iterations in case
                                    // it fails in this one. In particular, we
                                    // won't call
                                    // the broken link callback twice on one
                                    // reference.
                                    let reference_close_node = if let Some(node) =
                                        scan_nodes_to_ix(&self.tree, next, end_ix - 1)
                                    {
                                        node
                                    } else {
                                        continue;
                                    };
                                    self.tree[reference_close_node].item.body =
                                        ItemBody::MaybeLinkClose(false);
                                    let next_node = self.tree[reference_close_node].next;

                                    (next_node, LinkType::Reference)
                                }
                                // [reference][]
                                RefScan::Collapsed(next_node, _) => {
                                    // This reference has already been tried,
                                    // and it's not
                                    // valid. Skip it.
                                    if !could_be_ref {
                                        continue;
                                    }
                                    (next_node, LinkType::Collapsed)
                                }
                                // [shortcut]
                                //
                                // [shortcut]: /blah
                                RefScan::Failed | RefScan::UnexpectedFootnote => {
                                    if !could_be_ref {
                                        continue;
                                    }
                                    (next, LinkType::Shortcut)
                                }
                            };

                            // FIXME: references and labels are mixed in the
                            // naming of variables
                            // below. Disambiguate!

                            // (label, source_ix end)
                            let label: Option<(ReferenceLabel<'input>, usize)> = match scan_result {
                                RefScan::LinkLabel(l, end_ix) => {
                                    Some((ReferenceLabel::Link(l), end_ix))
                                }
                                RefScan::Collapsed(..)
                                | RefScan::Failed
                                | RefScan::UnexpectedFootnote => {
                                    // No label? maybe it is a shortcut
                                    // reference
                                    let label_start = self.tree[tos.node].item.end - 1;
                                    let label_end = self.tree[cur_ix].item.end;
                                    scan_link_label(
                                        &self.tree,
                                        &self.text[label_start..label_end],
                                        self.options,
                                    )
                                    .map(|(ix, label)| (label, label_start + ix))
                                    .filter(|(_, end)| *end == label_end)
                                }
                            };

                            let id = match &label {
                                Some(
                                    (ReferenceLabel::Link(l), _) | (ReferenceLabel::Footnote(l), _),
                                ) => l.clone(),
                                None => "".into(),
                            };

                            // see if it's a footnote reference
                            if let Some((ReferenceLabel::Footnote(l), end)) = label {
                                let footref = self.allocs.allocate_cow(l);
                                if let Some(def) = self
                                    .allocs
                                    .footdefs
                                    .get_mut(self.allocs.cows[footref.0].to_owned())
                                {
                                    def.use_count += 1;
                                }
                                if !self.options.has_gfm_footnotes()
                                    || self.allocs.footdefs.contains(&self.allocs.cows[footref.0])
                                {
                                    // If this came from a MaybeImage, then the
                                    // `!` prefix
                                    // isn't part of the footnote reference.
                                    let footnote_ix = if tos.ty == LinkStackTy::Image {
                                        self.tree[tos.node].next = Some(cur_ix);
                                        self.tree[tos.node].child = None;
                                        self.tree[tos.node].item.body =
                                            ItemBody::SynthesizeChar('!');
                                        self.tree[cur_ix].item.start =
                                            self.tree[tos.node].item.start + 1;
                                        self.tree[tos.node].item.end =
                                            self.tree[tos.node].item.start + 1;
                                        cur_ix
                                    } else {
                                        tos.node
                                    };
                                    // use `next` instead of `node_after_link`
                                    // because
                                    // node_after_link is calculated for a
                                    // [collapsed][] link,
                                    // which footnotes don't support.
                                    self.tree[footnote_ix].next = next;
                                    self.tree[footnote_ix].child = None;
                                    self.tree[footnote_ix].item.body =
                                        ItemBody::FootnoteReference(footref);
                                    self.tree[footnote_ix].item.end = end;
                                    prev = Some(footnote_ix);
                                    cur = next;
                                    self.link_stack.clear();
                                    continue;
                                }
                            } else if let Some((ReferenceLabel::Link(link_label), end)) = label
                                && let Some((def_link_type, url, title)) = self
                                    .fetch_link_type_url_title(
                                        link_label,
                                        (self.tree[tos.node].item.start)
                                            ..collapsed_end.unwrap_or(end),
                                        link_type,
                                        callbacks,
                                    )
                            {
                                let link_ix =
                                    self.allocs.allocate_link(def_link_type, url, title, id);
                                self.tree[tos.node].item.body = if tos.ty == LinkStackTy::Image {
                                    ItemBody::Image(link_ix)
                                } else {
                                    ItemBody::Link(link_ix)
                                };
                                let label_node = self.tree[tos.node].next;

                                // lets do some tree surgery to add the link to
                                // the tree 1st:
                                // skip the label node and close node
                                self.tree[tos.node].next = node_after_link;

                                // then, if it exists, add the label node as a
                                // child to the link node
                                if label_node != cur {
                                    self.tree[tos.node].child = label_node;

                                    // finally: disconnect list of children
                                    if let Some(prev_ix) = prev {
                                        self.tree[prev_ix].next = None;
                                    }
                                }

                                self.tree[tos.node].item.end = collapsed_end.unwrap_or(end);

                                // set up cur so next node will be
                                // node_after_link
                                cur = Some(tos.node);
                                cur_ix = tos.node;

                                if tos.ty == LinkStackTy::Link {
                                    self.disable_all_links();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            prev = cur;
            cur = self.tree[cur_ix].next;
        }
        self.link_stack.clear();
        self.wikilink_stack.clear();
        self.code_delims.clear();
        self.math_delims.clear();
    }

    /// Handles a wikilink.
    ///
    /// This function may bail early in case the link is malformed, so this
    /// acts as a control flow guard. Returns the link node if a wikilink was
    /// found and created.
    fn handle_wikilink(
        &mut self,
        block_text: &'input str,
        cur_ix: TreeIndex,
        prev: Option<TreeIndex>,
    ) -> Option<TreeIndex> {
        let next_ix = self.tree[cur_ix].next.unwrap();
        // this is a wikilink closing delim, try popping from
        // the wikilink stack
        if let Some(tos) = self.wikilink_stack.pop() {
            if tos.ty == LinkStackTy::Disabled {
                return None;
            }
            // fetches the beginning of the wikilink body
            let Some(body_node) = self.tree[tos.node].next.and_then(|ix| self.tree[ix].next) else {
                // skip if no next node exists, like at end of input
                return None;
            };
            let start_ix = self.tree[body_node].item.start;
            let end_ix = self.tree[cur_ix].item.start;
            // bail early in case the link is malformed: on some inputs the
            // body can begin at or after the closing delimiter, which would
            // otherwise underflow the length below and invert the slice.
            if end_ix <= start_ix {
                return None;
            }
            let wikilink = match scan_wikilink_pipe(
                block_text,
                start_ix, // bounded by closing tag
                end_ix - start_ix,
            ) {
                Some((rest, wikitext)) => {
                    // bail early if the wikiname would be empty
                    if wikitext.is_empty() {
                        return None;
                    }
                    // [[WikiName|rest]]
                    if rest >= end_ix {
                        // Empty display text: the `|` is immediately followed
                        // by `]]`. Create a zero-length synthetic node so the
                        // anchor has no content, rather than accidentally
                        // capturing the closing `]` delimiter.
                        let body_node = self.tree.create_node(Item {
                            start: rest,
                            end: rest,
                            body: ItemBody::Text {
                                backslash_escaped: false,
                            },
                        });
                        Some((true, body_node, wikitext))
                    } else {
                        let body_node = scan_nodes_to_ix(&self.tree, Some(body_node), rest);
                        if let Some(body_node) = body_node {
                            // break node so passes can actually format
                            // the display text
                            self.tree[body_node].item.start = rest;

                            Some((true, body_node, wikitext))
                        } else {
                            None
                        }
                    }
                }
                None => {
                    let wikitext = &block_text[start_ix..end_ix];
                    // bail early if the wikiname would be empty
                    if wikitext.is_empty() {
                        return None;
                    }
                    let body_node = self.tree.create_node(Item {
                        start: start_ix,
                        end: end_ix,
                        body: ItemBody::Text {
                            backslash_escaped: false,
                        },
                    });
                    if wikitext.contains('\0') {
                        self.tree[body_node].item.body = ItemBody::SynthesizeText(
                            self.allocs
                                .allocate_cow(CowStr::from_replace_nuls(wikitext)),
                        );
                    }
                    Some((false, body_node, wikitext))
                }
            };

            if let Some((has_pothole, body_node, wikiname)) = wikilink {
                let link_ix = self.allocs.allocate_link(
                    LinkType::WikiLink { has_pothole },
                    CowStr::from_replace_nuls(wikiname),
                    "".into(),
                    "".into(),
                );
                if let Some(prev_ix) = prev {
                    self.tree[prev_ix].next = None;
                }
                if tos.ty == LinkStackTy::Image {
                    self.tree[tos.node].item.body = ItemBody::Image(link_ix);
                } else {
                    self.tree[tos.node].item.body = ItemBody::Link(link_ix);
                }
                self.tree[tos.node].child = Some(body_node);
                self.tree[tos.node].next = self.tree[next_ix].next;
                self.tree[tos.node].item.end = end_ix + 2;
                self.disable_all_links();
                return Some(tos.node);
            }
        }

        None
    }

    fn handle_emphasis_and_hard_break(&mut self) {
        let mut prev = None;
        let mut prev_ix: TreeIndex;
        let mut cur = self.tree.cur();

        let mut single_quote_open: Option<TreeIndex> = None;
        let mut double_quote_open: bool = false;

        while let Some(mut cur_ix) = cur {
            match self.tree[cur_ix].item.body {
                ItemBody::MaybeEmphasis(mut count, can_open, can_close) => {
                    let run_length = count;
                    let c = self.text.as_bytes()[self.tree[cur_ix].item.start];
                    let both = can_open && can_close;
                    if can_close {
                        'outer: while let Some(el) =
                            self.inline_stack
                                .find_match(&mut self.tree, c, run_length, both)
                        {
                            let match_count = min(count, el.count);
                            // start, end are tree node indices
                            let mut end = cur_ix - 1;
                            let mut start = el.start + el.count;

                            // work from the inside out
                            while start > el.start + el.count - match_count {
                                let inc = if start > el.start + el.count - match_count + 1 {
                                    2
                                } else {
                                    1
                                };
                                let ty = if c == b'~' {
                                    if inc == 2 {
                                        if self.options.contains(Options::ENABLE_STRIKETHROUGH) {
                                            Some(ItemBody::Strikethrough)
                                        } else {
                                            None
                                        }
                                    } else {
                                        if self.options.contains(Options::ENABLE_SUBSCRIPT) {
                                            Some(ItemBody::Subscript)
                                        } else if self
                                            .options
                                            .contains(Options::ENABLE_STRIKETHROUGH)
                                        {
                                            Some(ItemBody::Strikethrough)
                                        } else {
                                            None
                                        }
                                    }
                                } else if c == b'^' {
                                    if self.options.contains(Options::ENABLE_SUPERSCRIPT) {
                                        Some(ItemBody::Superscript)
                                    } else {
                                        None
                                    }
                                } else if c == b'=' {
                                    if inc == 2 && self.options.contains(Options::ENABLE_HIGHLIGHT)
                                    {
                                        Some(ItemBody::Highlight)
                                    } else {
                                        None
                                    }
                                } else if inc == 2 {
                                    Some(ItemBody::Strong)
                                } else {
                                    Some(ItemBody::Emphasis)
                                };

                                if let Some(ty) = ty {
                                    // have a match!
                                    let root = start - inc;
                                    end = end + inc;
                                    self.tree[root].item.body = ty;
                                    self.tree[root].item.end = self.tree[end].item.end;
                                    self.tree[root].child = Some(start);
                                    self.tree[root].next = None;
                                    start = root;
                                    if let Some(prev_ix) = prev {
                                        self.tree[prev_ix].next = None;
                                    }
                                } else {
                                    self.tree[el.start].item.body = ItemBody::Text {
                                        backslash_escaped: false,
                                    };
                                    continue 'outer;
                                }
                            }

                            // set next for top most emph level
                            prev_ix = el.start + el.count - match_count;
                            prev = Some(prev_ix);
                            cur = self.tree[cur_ix + match_count - 1].next;
                            self.tree[prev_ix].next = cur;

                            if el.count > match_count {
                                self.inline_stack.push(InlineEl {
                                    start: el.start,
                                    count: el.count - match_count,
                                    run_length: el.run_length,
                                    c: el.c,
                                    both: el.both,
                                })
                            }
                            count -= match_count;
                            if count > 0 {
                                cur_ix = cur.unwrap();
                            } else {
                                break;
                            }
                        }
                    }
                    if count > 0 {
                        if can_open {
                            self.inline_stack.push(InlineEl {
                                start: cur_ix,
                                run_length,
                                count,
                                c,
                                both,
                            });
                        } else {
                            for i in 0..count {
                                self.tree[cur_ix + i].item.body = ItemBody::Text {
                                    backslash_escaped: false,
                                };
                            }
                        }
                        prev_ix = cur_ix + count - 1;
                        prev = Some(prev_ix);
                        cur = self.tree[prev_ix].next;
                    }
                }
                ItemBody::MaybeSmartQuote(c, can_open, can_close) => {
                    self.tree[cur_ix].item.body = match c {
                        b'\'' => {
                            if let (Some(open_ix), true) = (single_quote_open, can_close) {
                                self.tree[open_ix].item.body = ItemBody::SynthesizeChar('‘');
                                single_quote_open = None;
                            } else if can_open {
                                single_quote_open = Some(cur_ix);
                            }
                            ItemBody::SynthesizeChar('’')
                        }
                        _ /* double quote */ => {
                            if can_close && double_quote_open {
                                double_quote_open = false;
                                ItemBody::SynthesizeChar('”')
                            } else {
                                if can_open && !double_quote_open {
                                    double_quote_open = true;
                                }
                                ItemBody::SynthesizeChar('“')
                            }
                        }
                    };
                    prev = cur;
                    cur = self.tree[cur_ix].next;
                }
                ItemBody::HardBreak(true) => {
                    if self.tree[cur_ix].next.is_none() {
                        self.tree[cur_ix].item.body = ItemBody::SynthesizeChar('\\');
                    }
                    prev = cur;
                    cur = self.tree[cur_ix].next;
                }
                _ => {
                    prev = cur;
                    cur = self.tree[cur_ix].next;
                }
            }
        }
        self.inline_stack.pop_all(&mut self.tree);
    }

    fn disable_all_links(&mut self) {
        self.link_stack.disable_all_links();
        self.wikilink_stack.disable_all_links();
    }

    /// Returns next byte index, url and title.
    fn scan_inline_link(
        &self,
        underlying: &'input str,
        mut ix: usize,
        node: Option<TreeIndex>,
    ) -> Option<(usize, CowStr<'input>, CowStr<'input>)> {
        if underlying.as_bytes().get(ix) != Some(&b'(') {
            return None;
        }
        ix += 1;

        let scan_separator = |ix: &mut usize| {
            let start = *ix;
            *ix += scan_while(&underlying.as_bytes()[*ix..], is_ascii_whitespace_no_nl);
            if let Some(bl) = scan_eol(&underlying.as_bytes()[*ix..]) {
                *ix += bl;
                *ix += skip_container_prefixes(
                    &self.tree,
                    &underlying.as_bytes()[*ix..],
                    self.options,
                );
            }
            *ix += scan_while(&underlying.as_bytes()[*ix..], is_ascii_whitespace_no_nl);
            *ix - start
        };

        scan_separator(&mut ix);

        let (dest_length, dest) = scan_link_dest(underlying, ix, LINK_MAX_NESTED_PARENS)?;
        let dest = unescape(dest, self.tree.is_in_table());
        ix += dest_length;

        let title_sep = scan_separator(&mut ix);

        let title = if let Some((bytes_scanned, t)) = self.scan_link_title(underlying, ix, node) {
            if title_sep == 0 {
                return None;
            }
            ix += bytes_scanned;
            scan_separator(&mut ix);
            t
        } else {
            "".into()
        };
        if underlying.as_bytes().get(ix) != Some(&b')') {
            return None;
        }
        ix += 1;

        Some((ix, dest, title))
    }

    // returns (bytes scanned, title cow)
    fn scan_link_title(
        &self,
        text: &'input str,
        start_ix: usize,
        node: Option<TreeIndex>,
    ) -> Option<(usize, CowStr<'input>)> {
        let bytes = text.as_bytes();
        let open = match bytes.get(start_ix) {
            Some(b @ b'\'') | Some(b @ b'\"') | Some(b @ b'(') => *b,
            _ => return None,
        };
        let close = if open == b'(' { b')' } else { open };

        let mut title = String::new();
        let mut mark = start_ix + 1;
        let mut i = start_ix + 1;

        while i < bytes.len() {
            let c = bytes[i];

            if c == close {
                let cow = if mark == 1 {
                    (i - start_ix + 1, CowStr::from_replace_nuls(&text[mark..i]))
                } else {
                    title.push_str(&text[mark..i]);
                    (i - start_ix + 1, CowStr::from_replace_nuls(title))
                };

                return Some(cow);
            }
            if c == open {
                return None;
            }

            if (c == b'\n' || c == b'\r')
                && let Some(node_ix) = scan_nodes_to_ix(&self.tree, node, i + 1)
                && self.tree[node_ix].item.start > i
            {
                title.push_str(&text[mark..i]);
                title.push('\n');
                i = self.tree[node_ix].item.start;
                mark = i;
                continue;
            }
            if c == b'&'
                && let (n, Some(value)) = scan_entity(&bytes[i..])
            {
                title.push_str(&text[mark..i]);
                title.push_str(&value);
                i += n;
                mark = i;
                continue;
            }
            if self.tree.is_in_table()
                && c == b'\\'
                && i + 2 < bytes.len()
                && bytes[i + 1] == b'\\'
                && bytes[i + 2] == b'|'
            {
                // this runs if there are an even number of pipes in a table
                // if it's odd, then it gets parsed as normal
                title.push_str(&text[mark..i]);
                i += 2;
                mark = i;
            }
            if c == b'\\' && i + 1 < bytes.len() && is_ascii_punctuation(bytes[i + 1]) {
                title.push_str(&text[mark..i]);
                i += 1;
                mark = i;
            }

            i += 1;
        }

        None
    }

    fn make_math_span(&mut self, open: TreeIndex, mut close: TreeIndex) {
        let start_is_display = self.tree[open].next.filter(|&next_ix| {
            next_ix != close
                && matches!(
                    self.tree[next_ix].item.body,
                    ItemBody::MaybeMath(_can_open, _can_close, _brace_context)
                )
        });
        let end_is_display = self.tree[close].next.filter(|&next_ix| {
            matches!(
                self.tree[next_ix].item.body,
                ItemBody::MaybeMath(_can_open, _can_close, _brace_context)
            )
        });
        let is_display = start_is_display.is_some() && end_is_display.is_some();
        if is_display {
            // This unwrap() can't panic, because if the next variable were
            // None, end_is_display would be None
            close = self.tree[close].next.unwrap();
            self.tree[open].next = Some(close);
            self.tree[open].item.end += 1;
            self.tree[close].item.start -= 1;
        } else {
            if self.tree[open].item.end == self.tree[close].item.start {
                // inline math spans cannot be empty
                self.tree[open].item.body = ItemBody::Text {
                    backslash_escaped: false,
                };
                return;
            }
            self.tree[open].next = Some(close);
        }
        let span_start = self.tree[open].item.end;
        let span_end = self.tree[close].item.start;

        let spanned_text = &self.text[span_start..span_end];
        let spanned_bytes = spanned_text.as_bytes();
        let mut buf: Option<String> = None;

        let mut start_ix = 0;
        let mut ix = 0;
        while ix < spanned_bytes.len() {
            let c = spanned_bytes[ix];
            if c == b'\r' || c == b'\n' {
                ix += 1;
                let buf = buf.get_or_insert_with(|| String::with_capacity(spanned_bytes.len()));
                buf.push_str(&spanned_text[start_ix..ix]);
                ix += skip_container_prefixes(&self.tree, &spanned_bytes[ix..], self.options);
                start_ix = ix;
            } else if c == b'\\'
                && spanned_bytes.get(ix + 1) == Some(&b'|')
                && self.tree.is_in_table()
            {
                let buf = buf.get_or_insert_with(|| String::with_capacity(spanned_bytes.len()));
                buf.push_str(&spanned_text[start_ix..ix]);
                buf.push('|');
                ix += 2;
                start_ix = ix;
            } else {
                ix += 1;
            }
        }

        let cow = if let Some(mut buf) = buf {
            buf.push_str(&spanned_text[start_ix..]);
            CowStr::from_replace_nuls(buf)
        } else {
            CowStr::from_replace_nuls(spanned_text)
        };

        self.tree[open].item.body = ItemBody::Math(self.allocs.allocate_cow(cow), is_display);
        self.tree[open].item.end = self.tree[close].item.end;
        self.tree[open].next = self.tree[close].next;
    }

    /// Make a code span.
    ///
    /// Both `open` and `close` are matching MaybeCode items.
    fn make_code_span(&mut self, open: TreeIndex, close: TreeIndex, preceding_backslash: bool) {
        let span_start = self.tree[open].item.end;
        let span_end = self.tree[close].item.start;
        let mut buf: Option<String> = None;

        let spanned_text = &self.text[span_start..span_end];
        let spanned_bytes = spanned_text.as_bytes();
        let mut start_ix = 0;
        let mut ix = 0;
        while ix < spanned_bytes.len() {
            let c = spanned_bytes[ix];
            if c == b'\r' || c == b'\n' {
                let buf = buf.get_or_insert_with(|| String::with_capacity(spanned_bytes.len()));
                buf.push_str(&spanned_text[start_ix..ix]);
                buf.push(' ');
                ix += scan_eol(&spanned_bytes[ix..]).unwrap_or(1);
                ix += skip_container_prefixes(&self.tree, &spanned_bytes[ix..], self.options);
                // A paragraph continuation line has its leading spaces and tabs
                // stripped.
                ix += scan_while(&spanned_bytes[ix..], |c| c == b' ' || c == b'\t');
                start_ix = ix;
            } else if c == b'\\'
                && spanned_bytes.get(ix + 1) == Some(&b'|')
                && self.tree.is_in_table()
            {
                let buf = buf.get_or_insert_with(|| String::with_capacity(spanned_bytes.len()));
                buf.push_str(&spanned_text[start_ix..ix]);
                buf.push('|');
                ix += 2;
                start_ix = ix;
            } else {
                ix += 1;
            }
        }

        let (opening, closing, all_spaces) = {
            let s = if let Some(buf) = &mut buf {
                buf.push_str(&spanned_text[start_ix..]);
                &buf[..]
            } else {
                spanned_text
            };
            (
                s.as_bytes().first() == Some(&b' '),
                s.as_bytes().last() == Some(&b' '),
                s.bytes().all(|b| b == b' '),
            )
        };

        let cow: CowStr<'input> = if !all_spaces && opening && closing {
            if let Some(mut buf) = buf {
                if !buf.is_empty() {
                    buf.remove(0);
                    buf.pop();
                }
                CowStr::from_replace_nuls(buf)
            } else {
                CowStr::from_replace_nuls(&spanned_text[1..(spanned_text.len() - 1).max(1)])
            }
        } else if let Some(buf) = buf {
            CowStr::from_replace_nuls(buf)
        } else {
            CowStr::from_replace_nuls(spanned_text)
        };

        if preceding_backslash {
            self.tree[open].item.body = ItemBody::Text {
                backslash_escaped: true,
            };
            self.tree[open].item.end = self.tree[open].item.start + 1;
            self.tree[open].next = Some(close);
            self.tree[close].item.body = ItemBody::Code(self.allocs.allocate_cow(cow));
            self.tree[close].item.start = self.tree[open].item.start + 1;
        } else {
            self.tree[open].item.body = ItemBody::Code(self.allocs.allocate_cow(cow));
            self.tree[open].item.end = self.tree[close].item.end;
            self.tree[open].next = self.tree[close].next;
        }
    }

    /// On success, returns the inline html and the byte offset after it. The
    /// html is borrowed from `text` unless container prefixes had to be
    /// removed from its continuation lines.
    fn scan_inline_html(
        &mut self,
        text: &'input str,
        ix: usize,
    ) -> Option<(CowStr<'input>, usize)> {
        let bytes = text.as_bytes();
        let c = *bytes.get(ix)?;
        if c == b'!' {
            let tree = &self.tree;
            let options = self.options;
            let end = scan_inline_html_comment(bytes, ix + 1, &mut self.html_scan_guard, &|b| {
                skip_container_prefixes(tree, b, options)
            })?;
            Some((self.strip_container_prefixes(&text[ix - 1..end]), end))
        } else if c == b'?' {
            let end = scan_inline_html_processing(bytes, ix + 1, &mut self.html_scan_guard)?;
            Some((self.strip_container_prefixes(&text[ix - 1..end]), end))
        } else {
            let (span, i) = scan_html_block_inner(
                // Subtract 1 to include the < character
                &bytes[(ix - 1)..],
                Some(&|bytes| skip_container_prefixes(&self.tree, bytes, self.options)),
            )?;
            let end = i + ix - 1;
            // `scan_html_block_inner` leaves its buffer empty when nothing was
            // skipped.
            let html = if span.is_empty() {
                CowStr::Borrowed(&text[ix - 1..end])
            } else {
                String::from_utf8(span).expect("invalid utf8").into()
            };
            Some((html, end))
        }
    }

    /// An inline HTML span that runs over several lines, without the container
    /// prefixes (`>`, list indentation) at the start of its continuation
    /// lines. Borrowed if there are none. Every cut is next to an ASCII
    /// byte (a line ending, or a prefix's space or `>`), so the pieces stay
    /// on char boundaries.
    fn strip_container_prefixes(&self, span: &'input str) -> CowStr<'input> {
        let bytes = span.as_bytes();
        let mut buffer = String::new();
        let mut last = 0;
        let mut i = 0;
        while let Some(n) = memchr::memchr2(b'\n', b'\r', &bytes[i..]) {
            i += n;
            let skipped = if let Some(eol) = scan_eol(&bytes[i..]) {
                i += eol;
                skip_container_prefixes(&self.tree, &bytes[i..], self.options)
            } else {
                // not a line break
                i += 1;
                0
            };
            if skipped > 0 {
                buffer.push_str(&span[last..i]);
                i += skipped;
                last = i;
            }
        }
        if last == 0 {
            CowStr::Borrowed(span)
        } else {
            buffer.push_str(&span[last..]);
            buffer.into()
        }
    }
}

/// Returns number of containers scanned.
pub fn scan_containers(
    tree: &Tree<Item>,
    line_start: &mut LineStart<'_>,
    options: Options,
    update_flags: bool,
) -> usize {
    let mut i = 0;
    let mut spine_iter = tree.walk_spine();
    for &node_ix in &mut spine_iter {
        match tree[node_ix].item.body {
            // This fix is lifted, almost verbatim,
            // from https://github.com/commonmark/cmark/pull/520/changes
            ItemBody::List(ref list_flags, _, _) | ItemBody::DefinitionList(ref list_flags)
                if update_flags =>
            {
                // Avoid quadratic behavior caused by deeply nested lists
                // for each blank line.
                if line_start.is_at_eol() {
                    if list_flags.get().contains(ListFlags::LAST_LINE_EMPTY)
                        && line_start.remaining_space() == 0
                    {
                        // Finish early if we encounter multiple blank lines.
                        // If the previous line was blank, then we know that
                        // this container has no
                        // children unless those children
                        // use indented lines to nest (in other words,
                        // blockquote
                        // and blockquote-like children aren't nested in here).
                        // Therefore, every child of this list will pass.
                        return tree.spine_len();
                    } else {
                        list_flags.set(list_flags.get() | ListFlags::LAST_LINE_EMPTY);
                    }
                } else {
                    list_flags.set(list_flags.get() & !ListFlags::LAST_LINE_EMPTY);
                }
            }
            ItemBody::BlockQuote(..) | ItemBody::Callout(..) => {
                let save = line_start.clone();
                let indent = line_start.scan_space_upto(4);
                if indent >= 4 || !line_start.scan_blockquote_marker() {
                    *line_start = save;
                    break;
                }
            }
            ItemBody::ListItem(indent) => {
                let save = line_start.clone();
                if !line_start.scan_space(indent) && !line_start.is_at_eol() {
                    *line_start = save;
                    break;
                }
            }
            ItemBody::DefinitionListDefinition(indent) => {
                let save = line_start.clone();
                if !line_start.scan_space(indent) && !line_start.is_at_eol() {
                    *line_start = save;
                    break;
                }
            }
            ItemBody::FootnoteDefinition(..) if options.has_gfm_footnotes() => {
                let save = line_start.clone();
                if !line_start.scan_space(4) && !line_start.is_at_eol() {
                    *line_start = save;
                    break;
                }
            }
            _ => (),
        }
        i += 1;
    }
    for &node_ix in &mut spine_iter {
        if let ItemBody::List(ref list_flags, _, _) | ItemBody::DefinitionList(ref list_flags) =
            tree[node_ix].item.body
        {
            list_flags.set(list_flags.get() & !ListFlags::LAST_LINE_EMPTY);
        }
    }
    i
}

pub fn skip_container_prefixes(tree: &Tree<Item>, bytes: &[u8], options: Options) -> usize {
    let mut line_start = LineStart::new(bytes);
    let _ = scan_containers(tree, &mut line_start, options, false);
    line_start.bytes_scanned()
}

impl Tree<Item> {
    pub(crate) fn append_text(&mut self, start: usize, end: usize, backslash_escaped: bool) {
        if end > start {
            if let Some(ix) = self.cur()
                && matches!(self[ix].item.body, ItemBody::Text { .. })
                && self[ix].item.end == start
            {
                self[ix].item.end = end;
                return;
            }
            self.append(Item {
                start,
                end,
                body: ItemBody::Text { backslash_escaped },
            });
        }
    }
    /// Returns true if the current node is inside a table.
    ///
    /// If `cur` is an ItemBody::Table, it would return false,
    /// but since the `TableRow` and `TableHead` and `TableCell`
    /// are children of the table, anything doing inline parsing
    /// doesn't need to care about that.
    pub(crate) fn is_in_table(&self) -> bool {
        fn might_be_in_table(item: &Item) -> bool {
            item.body.is_inline()
                || matches!(item.body, |ItemBody::TableHead| ItemBody::TableRow
                    | ItemBody::TableCell)
        }
        for &ix in self.walk_spine().rev() {
            if matches!(self[ix].item.body, ItemBody::Table(_)) {
                return true;
            }
            if !might_be_in_table(&self[ix].item) {
                return false;
            }
        }
        false
    }
}

#[derive(Copy, Clone, Debug)]
struct InlineEl {
    /// offset of tree node
    start: TreeIndex,
    /// number of delimiters available for matching
    count: usize,
    /// length of the run that these delimiters came from
    run_length: usize,
    /// b'*', b'_', or b'~'
    c: u8,
    /// can both open and close
    both: bool,
}

#[derive(Debug, Clone, Default)]
struct InlineStack {
    stack: Vec<InlineEl>,
    // Lower bounds for matching indices in the stack. For example
    // a strikethrough delimiter will never match with any element
    // in the stack with index smaller than
    // `lower_bounds[InlineStack::TILDES]`.
    lower_bounds: [usize; 11],
}

impl InlineStack {
    /// These are indices into the lower bounds array.
    /// Not both refers to the property that the delimiter can not both
    /// be opener as a closer.
    const UNDERSCORE_NOT_BOTH: usize = 0;
    const ASTERISK_NOT_BOTH: usize = 1;
    const ASTERISK_BASE: usize = 2;
    const TILDES: usize = 5;
    const UNDERSCORE_BASE: usize = 6;
    const CIRCUMFLEXES: usize = 9;
    const EQUALS: usize = 10;

    fn pop_all(&mut self, tree: &mut Tree<Item>) {
        for el in self.stack.drain(..) {
            for i in 0..el.count {
                tree[el.start + i].item.body = ItemBody::Text {
                    backslash_escaped: false,
                };
            }
        }
        self.lower_bounds = [0; 11];
    }

    fn get_lowerbound(&self, c: u8, count: usize, both: bool) -> usize {
        if c == b'_' {
            let mod3_lower = self.lower_bounds[Self::UNDERSCORE_BASE + count % 3];
            if both {
                mod3_lower
            } else {
                min(mod3_lower, self.lower_bounds[Self::UNDERSCORE_NOT_BOTH])
            }
        } else if c == b'*' {
            let mod3_lower = self.lower_bounds[Self::ASTERISK_BASE + count % 3];
            if both {
                mod3_lower
            } else {
                min(mod3_lower, self.lower_bounds[Self::ASTERISK_NOT_BOTH])
            }
        } else if c == b'^' {
            self.lower_bounds[Self::CIRCUMFLEXES]
        } else if c == b'=' {
            self.lower_bounds[Self::EQUALS]
        } else {
            self.lower_bounds[Self::TILDES]
        }
    }

    fn set_lowerbound(&mut self, c: u8, count: usize, both: bool, new_bound: usize) {
        if c == b'_' {
            self.lower_bounds[Self::UNDERSCORE_BASE + count % 3] = new_bound;
            if !both {
                self.lower_bounds[Self::UNDERSCORE_NOT_BOTH] = new_bound;
            }
        } else if c == b'*' {
            self.lower_bounds[Self::ASTERISK_BASE + count % 3] = new_bound;
            if !both {
                self.lower_bounds[Self::ASTERISK_NOT_BOTH] = new_bound;
            }
        } else if c == b'^' {
            self.lower_bounds[Self::CIRCUMFLEXES] = new_bound;
        } else if c == b'=' {
            self.lower_bounds[Self::EQUALS] = new_bound;
        } else {
            self.lower_bounds[Self::TILDES] = new_bound;
        }
    }

    fn truncate(&mut self, new_bound: usize) {
        self.stack.truncate(new_bound);
        for lower_bound in &mut self.lower_bounds {
            if *lower_bound > new_bound {
                *lower_bound = new_bound;
            }
        }
    }

    fn find_match(
        &mut self,
        tree: &mut Tree<Item>,
        c: u8,
        run_length: usize,
        both: bool,
    ) -> Option<InlineEl> {
        let lowerbound = min(self.stack.len(), self.get_lowerbound(c, run_length, both));
        let res = self.stack[lowerbound..]
            .iter()
            .cloned()
            .enumerate()
            .rfind(|(_, el)| {
                if (c == b'~' || c == b'^' || c == b'=') && run_length != el.run_length {
                    return false;
                }
                el.c == c
                    && (!both && !el.both
                        || !(run_length + el.run_length).is_multiple_of(3)
                        || run_length.is_multiple_of(3))
            });

        if let Some((matching_ix, matching_el)) = res {
            let matching_ix = matching_ix + lowerbound;
            for el in &self.stack[(matching_ix + 1)..] {
                for i in 0..el.count {
                    tree[el.start + i].item.body = ItemBody::Text {
                        backslash_escaped: false,
                    };
                }
            }
            self.truncate(matching_ix);
            Some(matching_el)
        } else {
            self.set_lowerbound(c, run_length, both, self.stack.len());
            None
        }
    }

    fn trim_lower_bound(&mut self, ix: usize) {
        self.lower_bounds[ix] = self.lower_bounds[ix].min(self.stack.len());
    }

    fn push(&mut self, el: InlineEl) {
        if el.c == b'~' {
            self.trim_lower_bound(Self::TILDES);
        } else if el.c == b'^' {
            self.trim_lower_bound(Self::CIRCUMFLEXES);
        } else if el.c == b'=' {
            self.trim_lower_bound(Self::EQUALS);
        }
        self.stack.push(el)
    }
}

#[derive(Debug, Clone)]
enum RefScan<'a> {
    // label, source ix of label end
    LinkLabel(CowStr<'a>, usize),
    // the next node index, and the source ix of the end of the `[]`
    Collapsed(Option<TreeIndex>, usize),
    UnexpectedFootnote,
    Failed,
}

/// Skips forward within a block to a node which spans (ends inclusive) the
/// given index into the source.
fn scan_nodes_to_ix(
    tree: &Tree<Item>,
    mut node: Option<TreeIndex>,
    ix: usize,
) -> Option<TreeIndex> {
    while let Some(node_ix) = node {
        if tree[node_ix].item.end <= ix {
            node = tree[node_ix].next;
        } else {
            break;
        }
    }
    node
}

/// Scans an inline link label, which cannot be interrupted.
/// Returns number of bytes (including brackets) and label on success.
fn scan_link_label<'text>(
    tree: &Tree<Item>,
    text: &'text str,
    options: Options,
) -> Option<(usize, ReferenceLabel<'text>)> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'[' {
        return None;
    }
    let linebreak_handler = |bytes: &[u8]| Some(skip_container_prefixes(tree, bytes, options));
    if options.contains(Options::ENABLE_FOOTNOTES)
        && b'^' == bytes[1]
        && bytes.get(2) != Some(&b']')
    {
        let linebreak_handler: &dyn Fn(&[u8]) -> Option<usize> = if options.has_gfm_footnotes() {
            &|_| None
        } else {
            &linebreak_handler
        };
        if let Some((byte_index, cow)) =
            scan_link_label_rest(&text[2..], linebreak_handler, tree.is_in_table())
        {
            return Some((byte_index + 2, ReferenceLabel::Footnote(cow)));
        }
    }
    let (byte_index, cow) =
        scan_link_label_rest(&text[1..], &linebreak_handler, tree.is_in_table())?;
    Some((byte_index + 1, ReferenceLabel::Link(cow)))
}

fn scan_reference<'b>(
    tree: &Tree<Item>,
    text: &'b str,
    cur_ix: TreeIndex,
    options: Options,
) -> RefScan<'b> {
    let start = tree[cur_ix].item.end;
    let tail = &text.as_bytes()[start..];

    if tail.starts_with(b"[]") {
        let next_ix = match tree[cur_ix].next {
            Some(next_ix) if tree[next_ix].item.start == tree[cur_ix].item.end => next_ix,
            _ => return RefScan::Failed,
        };
        // TODO: this unwrap is sus and should be looked at closer
        let closing_node = tree[next_ix].next.unwrap();
        RefScan::Collapsed(tree[closing_node].next, tree[closing_node].item.end)
    } else {
        let label = scan_link_label(tree, &text[start..], options);
        match label {
            Some((ix, ReferenceLabel::Link(label))) => RefScan::LinkLabel(label, start + ix),
            Some((_ix, ReferenceLabel::Footnote(_label))) => RefScan::UnexpectedFootnote,
            None => RefScan::Failed,
        }
    }
}

#[derive(Clone, Default)]
struct LinkStack {
    inner: Vec<LinkStackEl>,
    disabled_ix: usize,
}

impl LinkStack {
    fn push(&mut self, el: LinkStackEl) {
        self.inner.push(el);
    }

    fn pop(&mut self) -> Option<LinkStackEl> {
        let el = self.inner.pop();
        self.disabled_ix = core::cmp::min(self.disabled_ix, self.inner.len());
        el
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.disabled_ix = 0;
    }

    fn disable_all_links(&mut self) {
        for el in &mut self.inner[self.disabled_ix..] {
            if el.ty == LinkStackTy::Link {
                el.ty = LinkStackTy::Disabled;
            }
        }
        self.disabled_ix = self.inner.len();
    }
}

#[derive(Clone, Debug)]
struct LinkStackEl {
    node: TreeIndex,
    ty: LinkStackTy,
}

#[derive(PartialEq, Clone, Debug)]
enum LinkStackTy {
    Link,
    Image,
    Disabled,
}

/// Contains the destination URL, title and source span of a reference
/// definition.
#[derive(Clone, Debug)]
pub struct LinkDef<'a> {
    pub dest: CowStr<'a>,
    pub title: Option<CowStr<'a>>,
    pub span: Range<usize>,
}

impl<'a> LinkDef<'a> {
    pub fn into_static(self) -> LinkDef<'static> {
        LinkDef {
            dest: self.dest.into_static(),
            title: self.title.map(|s| s.into_static()),
            span: self.span,
        }
    }
}

/// Contains the destination URL, title and source span of a reference
/// definition.
#[derive(Clone, Debug)]
pub struct FootnoteDef {
    pub use_count: usize,
}

/// Tracks tree indices of code span delimiters of each length. It should
/// prevent quadratic scanning behaviours by providing (amortized) constant time
/// lookups.
struct CodeDelims {
    inner: HashMap<usize, VecDeque<TreeIndex>>,
    seen_first: bool,
}

impl CodeDelims {
    fn new() -> Self {
        Self {
            inner: Default::default(),
            seen_first: false,
        }
    }

    fn insert(&mut self, count: usize, ix: TreeIndex) {
        if self.seen_first {
            self.inner.entry(count).or_default().push_back(ix);
        } else {
            // Skip the first insert, since that delimiter will always
            // be an opener and not a closer.
            self.seen_first = true;
        }
    }

    fn is_populated(&self) -> bool {
        !self.inner.is_empty()
    }

    fn find(&mut self, open_ix: TreeIndex, count: usize) -> Option<TreeIndex> {
        while let Some(ix) = self.inner.get_mut(&count)?.pop_front() {
            if ix > open_ix {
                return Some(ix);
            }
        }
        None
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.seen_first = false;
    }
}

/// Tracks brace contexts and delimiter length for math delimiters.
/// Provides amortized constant-time lookups.
struct MathDelims {
    inner: HashMap<u8, VecDeque<(TreeIndex, bool, bool)>>,
}

impl MathDelims {
    fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }

    fn insert(
        &mut self,
        delim_is_display: bool,
        brace_context: u8,
        ix: TreeIndex,
        can_close: bool,
    ) {
        self.inner
            .entry(brace_context)
            .or_default()
            .push_back((ix, can_close, delim_is_display));
    }

    fn is_populated(&self) -> bool {
        !self.inner.is_empty()
    }

    fn find(
        &mut self,
        tree: &Tree<Item>,
        open_ix: TreeIndex,
        is_display: bool,
        brace_context: u8,
    ) -> Option<TreeIndex> {
        while let Some((ix, can_close, delim_is_display)) =
            self.inner.get_mut(&brace_context)?.pop_front()
        {
            if ix <= open_ix || (is_display && tree[open_ix].next == Some(ix)) {
                continue;
            }
            let can_close = can_close && tree[open_ix].item.end != tree[ix].item.start;
            if (!is_display && can_close) || (is_display && delim_is_display) {
                return Some(ix);
            }
            // if we can't use it, leave it in the queue as a tombstone for the
            // next thing that tries to match it
            self.inner
                .get_mut(&brace_context)?
                .push_front((ix, can_close, delim_is_display));
            break;
        }
        None
    }

    fn clear(&mut self) {
        self.inner.clear();
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct LinkIndex(usize);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CowIndex(usize);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AlignmentIndex(usize);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct HeadingIndex(NonZeroUsize);

/// Where an embed's data sits in the allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedIndex(usize);

/// Where a callout's data sits in the allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalloutIndex(usize);

#[derive(Clone)]
pub struct Allocations<'a> {
    pub refdefs: RefDefs<'a>,
    pub footdefs: FootnoteDefs<'a>,
    links: Vec<(LinkType, CowStr<'a>, CowStr<'a>, CowStr<'a>)>,
    cows: Vec<CowStr<'a>>,
    alignments: Vec<Vec<Alignment>>,
    headings: Vec<HeadingAttributes<'a>>,
    callouts: Vec<Callout<'a>>,
    embeds: Vec<Embed<'a>>,
    /// Each footnote definition's label and start, in source order.
    pub footnote_starts: Vec<(usize, CowStr<'a>)>,
}

/// Used by the heading attributes extension.
#[derive(Clone)]
pub struct HeadingAttributes<'a> {
    pub id: Option<CowStr<'a>>,
    pub classes: Vec<CowStr<'a>>,
    pub attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
}

/// Keeps track of the reference definitions defined in the document.
#[derive(Clone, Default, Debug)]
pub struct RefDefs<'input>(
    pub(crate) HashMap<LinkLabel<'input>, LinkDef<'input>>,
    /// Every definition's source span, in source order, duplicates included.
    pub(crate) Vec<Range<usize>>,
);

/// Keeps track of the footnote definitions defined in the document.
#[derive(Clone, Default, Debug)]
pub struct FootnoteDefs<'input>(pub(crate) HashMap<FootnoteLabel<'input>, FootnoteDef>);

impl<'input, 'b, 's> RefDefs<'input>
where
    's: 'b,
{
    /// Performs a lookup on reference label using unicode case folding.
    pub fn get(&'s self, key: &'b str) -> Option<&'b LinkDef<'input>> {
        self.0.get(&UniCase::new(CowStr::from_replace_nuls(key)))
    }

    /// The source span of every reference definition, in source order. A
    /// label defined twice has both spans, though only the first takes
    /// effect and [`Self::iter`] gives only it.
    pub fn spans(&self) -> &[Range<usize>] {
        &self.1
    }

    /// Provides an iterator over all the document's reference definitions.
    pub fn iter(
        &'s self,
    ) -> impl Iterator<Item = (&'s str, &'s LinkDef<'input>)> + use<'s, 'input> {
        self.0.iter().map(|(k, v)| (k.as_ref(), v))
    }
}

impl<'input, 'b, 's> FootnoteDefs<'input>
where
    's: 'b,
{
    /// Performs a lookup on reference label using unicode case folding.
    pub fn contains(&'s self, key: &'b str) -> bool {
        self.0
            .contains_key(&UniCase::new(CowStr::from_replace_nuls(key)))
    }
    /// Performs a lookup on reference label using unicode case folding.
    pub fn get_mut(&'s mut self, key: CowStr<'input>) -> Option<&'s mut FootnoteDef> {
        self.0.get_mut(&UniCase::new(key))
    }
}

impl<'a> Allocations<'a> {
    pub fn new() -> Self {
        Self {
            refdefs: RefDefs::default(),
            footdefs: FootnoteDefs::default(),
            links: Vec::with_capacity(128),
            cows: Vec::new(),
            alignments: Vec::new(),
            headings: Vec::new(),
            callouts: Vec::new(),
            embeds: Vec::new(),
            footnote_starts: Vec::new(),
        }
    }

    pub fn allocate_cow(&mut self, cow: CowStr<'a>) -> CowIndex {
        let ix = self.cows.len();
        self.cows.push(cow);
        CowIndex(ix)
    }

    pub fn allocate_link(
        &mut self,
        ty: LinkType,
        url: CowStr<'a>,
        title: CowStr<'a>,
        id: CowStr<'a>,
    ) -> LinkIndex {
        let ix = self.links.len();
        self.links.push((ty, url, title, id));
        LinkIndex(ix)
    }

    pub fn allocate_alignment(&mut self, alignment: Vec<Alignment>) -> AlignmentIndex {
        let ix = self.alignments.len();
        self.alignments.push(alignment);
        AlignmentIndex(ix)
    }

    pub fn allocate_embed(&mut self, embed: Embed<'a>) -> EmbedIndex {
        self.embeds.push(embed);
        EmbedIndex(self.embeds.len() - 1)
    }

    pub fn take_embed(&mut self, ix: EmbedIndex) -> Embed<'a> {
        let empty = Embed {
            target: "".into(),
            kind: EmbedKind::Note,
            width: None,
            height: None,
        };
        core::mem::replace(&mut self.embeds[ix.0], empty)
    }

    pub fn allocate_callout(&mut self, callout: Callout<'a>) -> CalloutIndex {
        self.callouts.push(callout);
        CalloutIndex(self.callouts.len() - 1)
    }

    pub fn take_callout(&mut self, ix: CalloutIndex) -> Callout<'a> {
        let empty = Callout {
            kind: BlockQuoteKind::Note,
            label: "".into(),
            title: None,
            fold: None,
        };
        core::mem::replace(&mut self.callouts[ix.0], empty)
    }

    pub fn heading_mut(&mut self, ix: HeadingIndex) -> &mut HeadingAttributes<'a> {
        &mut self.headings[ix.0.get() - 1]
    }

    pub fn allocate_heading(&mut self, attrs: HeadingAttributes<'a>) -> HeadingIndex {
        let ix = self.headings.len();
        self.headings.push(attrs);
        // This won't panic. `self.headings.len()` can't be `usize::MAX` since
        // such a long Vec cannot fit in memory.
        let ix_nonzero = NonZeroUsize::new(ix.wrapping_add(1)).expect("too many headings");
        HeadingIndex(ix_nonzero)
    }

    pub fn take_cow(&mut self, ix: CowIndex) -> CowStr<'a> {
        core::mem::replace(&mut self.cows[ix.0], "".into())
    }

    pub fn take_link(&mut self, ix: LinkIndex) -> (LinkType, CowStr<'a>, CowStr<'a>, CowStr<'a>) {
        let default_link = (LinkType::ShortcutUnknown, "".into(), "".into(), "".into());
        core::mem::replace(&mut self.links[ix.0], default_link)
    }

    pub fn take_alignment(&mut self, ix: AlignmentIndex) -> Vec<Alignment> {
        core::mem::take(&mut self.alignments[ix.0])
    }
}

impl<'a> Index<CowIndex> for Allocations<'a> {
    type Output = CowStr<'a>;

    fn index(&self, ix: CowIndex) -> &Self::Output {
        self.cows.index(ix.0)
    }
}

impl<'a> Index<LinkIndex> for Allocations<'a> {
    type Output = (LinkType, CowStr<'a>, CowStr<'a>, CowStr<'a>);

    fn index(&self, ix: LinkIndex) -> &Self::Output {
        self.links.index(ix.0)
    }
}

impl<'a> Index<AlignmentIndex> for Allocations<'a> {
    type Output = Vec<Alignment>;

    fn index(&self, ix: AlignmentIndex) -> &Self::Output {
        self.alignments.index(ix.0)
    }
}

impl<'a> Index<HeadingIndex> for Allocations<'a> {
    type Output = HeadingAttributes<'a>;

    fn index(&self, ix: HeadingIndex) -> &Self::Output {
        self.headings.index(ix.0.get() - 1)
    }
}

/// A struct containing information on the reachability of certain inline HTML
/// elements. In particular, for cdata elements (`<![CDATA[`), processing
/// elements (`<?`) and declarations (`<!DECLARATION`). The respectives usizes
/// represent the indices before which a scan will always fail and can hence
/// be skipped.
#[derive(Clone, Default)]
pub struct HtmlScanGuard {
    pub cdata: usize,
    pub processing: usize,
    pub declaration: usize,
    pub comment: usize,
}

/// Trait to customize [`Parser`] behavior with callbacks. See
/// [`Parser::new_with_callbacks`].
///
/// All methods have a default implementation, so you can choose which ones to
/// override.
pub trait ParserCallbacks<'input> {
    /// Potentially provide a custom definition for a broken link.
    ///
    /// In case the parser encounters any potential links that have a broken
    /// reference (e.g `[foo]` when there is no `[foo]: ` entry at the bottom)
    /// this callback will be called with information about the reference,
    /// and the returned pair will be used as the link URL and title if it is
    /// not `None`.
    fn handle_broken_link(
        &mut self,
        #[allow(unused_variables)] link: BrokenLink<'input>,
    ) -> Option<(CowStr<'input>, CowStr<'input>)> {
        None
    }
}

/// Wrapper to implement [`ParserCallbacks::handle_broken_link`] with a closure.
///
/// Used internally by [`Parser::new_with_broken_link_callback`].
#[allow(missing_debug_implementations)]
pub struct BrokenLinkCallback<F>(Option<F>);

impl<'input, F> ParserCallbacks<'input> for BrokenLinkCallback<F>
where
    F: FnMut(BrokenLink<'input>) -> Option<(CowStr<'input>, CowStr<'input>)>,
{
    fn handle_broken_link(
        &mut self,
        link: BrokenLink<'input>,
    ) -> Option<(CowStr<'input>, CowStr<'input>)> {
        self.0.as_mut().and_then(|cb| cb(link))
    }
}

impl<'input> ParserCallbacks<'input> for Box<dyn ParserCallbacks<'input>> {
    fn handle_broken_link(
        &mut self,
        link: BrokenLink<'input>,
    ) -> Option<(CowStr<'input>, CowStr<'input>)> {
        (**self).handle_broken_link(link)
    }
}

/// [Parser] callbacks that do nothing.
///
/// Used when no custom callbacks are provided.
#[allow(missing_debug_implementations)]
pub struct DefaultParserCallbacks;

impl<'input> ParserCallbacks<'input> for DefaultParserCallbacks {}

/// Markdown event and source range iterator.
///
/// Generates tuples where the first element is the markdown event and the
/// second is a the corresponding range in the source string.
///
/// Constructed from a `Parser` using its
/// [`into_offset_iter`](struct.Parser.html#method.into_offset_iter) method.
#[derive(Debug)]
pub struct OffsetIter<'a, CB> {
    parser: Parser<'a, CB>,
}

impl<'a, CB: ParserCallbacks<'a>> OffsetIter<'a, CB> {
    /// Returns a reference to the internal reference definition tracker.
    pub fn reference_definitions(&self) -> &RefDefs<'a> {
        self.parser.reference_definitions()
    }
}

impl<'a, CB> OffsetIter<'a, CB> {
    /// Whether the parse passed one of its limits, and stopped early.
    pub fn limit_hit(&self) -> bool {
        self.parser.inner.limit_hit
    }

    /// What the parse learned, for resuming a later parse with
    /// [`Parser::new_resumed`]. Heading ids are known once the events are
    /// read, so take this after reading them all.
    pub fn resume(&self) -> Resume {
        let allocs = &self.parser.inner.allocs;
        let mut refdefs: Vec<(String, LinkDef<'static>)> = allocs
            .refdefs
            .0
            .iter()
            .map(|(label, definition)| (label.to_string(), definition.clone().into_static()))
            .collect();
        refdefs.sort_by_key(|(_, definition)| definition.span.start);
        Resume {
            refdefs,
            footnotes: allocs
                .footnote_starts
                .iter()
                .map(|(start, label)| (*start, label.to_string()))
                .collect(),
            headings: self.parser.inner.headings.clone(),
        }
    }
}

impl<'a, CB: ParserCallbacks<'a>> Iterator for OffsetIter<'a, CB> {
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        self.parser
            .inner
            .next_event_range(&mut self.parser.callbacks)
    }
}

impl<'a, CB: ParserCallbacks<'a>> Iterator for Parser<'a, CB> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Event<'a>> {
        self.inner
            .next_event_range(&mut self.callbacks)
            .map(|(event, _range)| event)
    }
}

impl<'a, CB: ParserCallbacks<'a>> FusedIterator for Parser<'a, CB> {}

impl<'input> ParserInner<'input> {
    /// Parse the inline content of the block at `ix`, which is the current
    /// node, ahead of its events: all of it, so a pass over it sees it whole.
    fn parse_inline_children(
        &mut self,
        ix: TreeIndex,
        callbacks: &mut dyn ParserCallbacks<'input>,
    ) {
        debug_assert_eq!(self.tree.cur(), Some(ix));
        let mut child = self.tree[ix].child;
        let mut raw = false;
        while let Some(at) = child {
            raw |= self.tree[at].item.body.is_maybe_inline();
            child = self.tree[at].next;
        }
        if raw {
            self.tree.push();
            self.handle_inline(callbacks);
            self.tree.pop();
        }
    }

    /// Prepare the inline content of the block at `ix` for this fork's
    /// passes, which read it whole: parse it, link its bare URLs, and take
    /// its block id.
    fn prepare_inline(&mut self, ix: TreeIndex, callbacks: &mut dyn ParserCallbacks<'input>) {
        let body = &self.tree[ix].item.body;
        let holds_inline = matches!(
            body,
            ItemBody::Paragraph
                | ItemBody::TightParagraph
                | ItemBody::Heading(..)
                | ItemBody::TableCell
                | ItemBody::DefinitionListTitle
        );
        let block = matches!(
            body,
            ItemBody::Paragraph | ItemBody::TightParagraph | ItemBody::Heading(..)
        );
        if !holds_inline {
            return;
        }
        self.parse_inline_children(ix, callbacks);
        if self.options.contains(Options::ENABLE_INLINE_FOOTNOTES) {
            self.take_inline_footnotes(ix);
        }
        if self.options.contains(Options::ENABLE_OBSIDIAN_WIKILINKS) {
            self.read_wikilinks(ix);
        }
        if self.options.contains(Options::ENABLE_AUTOLINKS) {
            self.link_bare_urls(ix);
        }
        if block && self.options.contains(Options::ENABLE_BLOCK_IDS) {
            self.take_block_id(ix, callbacks);
        }
    }

    /// Turn the inline footnotes among the children of `parent`, `^[note]`,
    /// into references to footnotes of their own, which go at the end of
    /// the document.
    fn take_inline_footnotes(&mut self, parent: TreeIndex) {
        let mut cur = self.tree[parent].child;
        while let Some(ix) = cur {
            if let Some((open, close)) = self.inline_footnote_at(ix) {
                cur = Some(self.inline_footnote(ix, open, close));
            }
            cur = cur.and_then(|at| self.tree[at].next);
        }
    }

    /// The unresolved brackets of an inline footnote whose `^` ends the
    /// text at `ix`, if one does and holds anything.
    fn inline_footnote_at(&self, ix: TreeIndex) -> Option<(TreeIndex, TreeIndex)> {
        let is_text = |at: TreeIndex, text: Option<&str>| {
            let item = &self.tree[at].item;
            matches!(item.body, ItemBody::Text { .. })
                && text.is_none_or(|text| &self.text[item.start..item.end] == text)
        };
        let item = &self.tree[ix].item;
        if !is_text(ix, None) || !self.text[item.start..item.end].ends_with('^') {
            return None;
        }
        let open = self.tree[ix].next?;
        if !is_text(open, Some("[")) || self.tree[open].item.start != item.end {
            return None;
        }
        let first = self.tree[open].next?;
        let mut depth = 0usize;
        let mut at = first;
        loop {
            if is_text(at, Some("[")) {
                depth += 1;
            } else if is_text(at, Some("]")) {
                if depth == 0 {
                    return (at != first).then_some((open, at));
                }
                depth -= 1;
            }
            at = self.tree[at].next?;
        }
    }

    /// Replace the inline footnote whose `^` ends the text at `ix`, with
    /// brackets `open` and `close`, by a reference, and move its content to
    /// a footnote at the end of the document. Return the reference's node.
    fn inline_footnote(&mut self, ix: TreeIndex, open: TreeIndex, close: TreeIndex) -> TreeIndex {
        self.inline_footnotes += 1;
        let label: CowStr<'input> = format!("inline-{}", self.inline_footnotes).into();
        let caret = self.tree[ix].item.end - 1;
        let end = self.tree[close].item.end;
        let first = self.tree[open]
            .next
            .expect("an inline footnote holds content");
        let mut last = first;
        while self.tree[last].next != Some(close) {
            last = self.tree[last]
                .next
                .expect("the content ends at its bracket");
        }
        let content = self.tree[first].item.start..self.tree[last].item.end;
        let after = self.tree[close].next;
        self.tree[last].next = None;
        // The reference takes the caret's place.
        let reference = Item {
            start: caret,
            end,
            body: ItemBody::FootnoteReference(self.allocs.allocate_cow(label.clone())),
        };
        let reference = if self.tree[ix].item.start == caret {
            self.tree[ix].item = reference;
            ix
        } else {
            self.tree[ix].item.end = caret;
            let node = self.tree.create_node(reference);
            self.tree[ix].next = Some(node);
            node
        };
        self.tree[reference].next = after;
        // The footnote, holding the content as a paragraph, ends the document.
        let paragraph = self.tree.create_node(Item {
            start: content.start,
            end: content.end,
            body: ItemBody::Paragraph,
        });
        self.tree[paragraph].child = Some(first);
        let footnote = self.tree.create_node(Item {
            start: caret,
            end,
            body: ItemBody::FootnoteDefinition(self.allocs.allocate_cow(label.clone())),
        });
        self.tree[footnote].child = Some(paragraph);
        self.allocs
            .footdefs
            .0
            .insert(UniCase::new(label), FootnoteDef { use_count: 1 });
        if let Some(root) = self.tree.last_root() {
            self.tree[root].next = Some(footnote);
        }
        reference
    }

    /// Read the wikilinks below `parent` as Obsidian does: an unlabelled
    /// link shows Obsidian's label, and a wikilink image is an embed.
    fn read_wikilinks(&mut self, parent: TreeIndex) {
        let mut cur = self.tree[parent].child;
        while let Some(ix) = cur {
            match self.tree[ix].item.body {
                ItemBody::Link(link) => {
                    let (link_type, ref target, ..) = self.allocs[link];
                    if link_type == (LinkType::WikiLink { has_pothole: false }) {
                        let label = wikilink_label(target);
                        self.relabel(ix, label);
                    }
                }
                ItemBody::Image(link) => {
                    let (link_type, ref target, ..) = self.allocs[link];
                    if let LinkType::WikiLink { has_pothole } = link_type {
                        let target = target.clone();
                        let mut label = String::new();
                        self.inline_text(self.tree[ix].child, &mut label);
                        let size = has_pothole.then(|| embed_size(&label)).flatten();
                        let embed = self.allocs.allocate_embed(Embed {
                            kind: EmbedKind::of(&target),
                            width: size.map(|(width, _)| width),
                            height: size.and_then(|(_, height)| height),
                            target: target.clone(),
                        });
                        self.tree[ix].item.body = ItemBody::Embed(embed);
                        // A size, or no label, shows the target's label.
                        if !has_pothole || size.is_some() {
                            self.relabel(ix, wikilink_label(&target));
                        }
                    }
                }
                _ if self.tree[ix].child.is_some() => self.read_wikilinks(ix),
                _ => {}
            }
            cur = self.tree[ix].next;
        }
    }

    /// Replace the content of the link or embed at `ix` with `label`.
    fn relabel(&mut self, ix: TreeIndex, label: String) {
        let (start, end) = (self.tree[ix].item.start, self.tree[ix].item.end);
        let cow = self.allocs.allocate_cow(label.into());
        let node = self.tree.create_node(Item {
            start,
            end,
            body: ItemBody::SynthesizeText(cow),
        });
        self.tree[ix].child = Some(node);
    }

    /// Link the bare URLs in the text below `parent`, outside links and
    /// images.
    fn link_bare_urls(&mut self, parent: TreeIndex) {
        let mut cur = self.tree[parent].child;
        while let Some(ix) = cur {
            match self.tree[ix].item.body {
                ItemBody::Text { backslash_escaped } => {
                    // The run of text items that join in the source.
                    let start = self.tree[ix].item.start;
                    let mut last = ix;
                    while let Some(next) = self.tree[last].next {
                        let item = &self.tree[next].item;
                        if !matches!(item.body, ItemBody::Text { .. })
                            || item.start != self.tree[last].item.end
                        {
                            break;
                        }
                        last = next;
                    }
                    let end = self.tree[last].item.end;
                    let after = self.tree[last].next;
                    let before = self.text[..start].chars().next_back();
                    let links = autolink::find(&self.text[start..end], before);
                    if !links.is_empty() {
                        self.splice_links(ix, start..end, &links, backslash_escaped, after);
                    }
                    cur = after;
                }
                ItemBody::Link(..) | ItemBody::Image(..) | ItemBody::Embed(..) => {
                    cur = self.tree[ix].next;
                }
                _ => {
                    if self.tree[ix].child.is_some() {
                        self.link_bare_urls(ix);
                    }
                    cur = self.tree[ix].next;
                }
            }
        }
    }

    /// Replace the run of text at `first`, over `run`, with its text and the
    /// bare `links` in it, keeping `first` as the first node, and linking
    /// the last to `after`.
    fn splice_links(
        &mut self,
        first: TreeIndex,
        run: Range<usize>,
        links: &[(Range<usize>, autolink::Bare)],
        backslash_escaped: bool,
        after: Option<TreeIndex>,
    ) {
        let text = |start, end, escaped| Item {
            start,
            end,
            body: ItemBody::Text {
                backslash_escaped: escaped,
            },
        };
        let mut pieces: Vec<(Item, Option<Item>)> = Vec::new();
        let mut at = run.start;
        for (range, kind) in links {
            let (start, end) = (run.start + range.start, run.start + range.end);
            if start > at {
                pieces.push((
                    text(at, start, pieces.is_empty() && backslash_escaped),
                    None,
                ));
            }
            let shown = &self.text[start..end];
            let (link_type, dest) = match kind {
                autolink::Bare::Url => (LinkType::Autolink, CowStr::Borrowed(shown)),
                autolink::Bare::Www => (LinkType::Autolink, format!("http://{shown}").into()),
                autolink::Bare::Email => (LinkType::Email, CowStr::Borrowed(shown)),
            };
            let link = self
                .allocs
                .allocate_link(link_type, dest, "".into(), "".into());
            pieces.push((
                Item {
                    start,
                    end,
                    body: ItemBody::Link(link),
                },
                Some(text(start, end, false)),
            ));
            at = end;
        }
        if at < run.end {
            pieces.push((text(at, run.end, false), None));
        }
        let mut node = first;
        for (index, (item, child)) in pieces.into_iter().enumerate() {
            if index > 0 {
                let next = self.tree.create_node(item);
                self.tree[node].next = Some(next);
                node = next;
            } else {
                self.tree[node].item = item;
            }
            self.tree[node].child = child.map(|child| self.tree.create_node(child));
        }
        self.tree[node].next = after;
    }

    /// Take an Obsidian block id from the block at `ix`: a trailing ` ^id`
    /// becomes a block id after its text, and a paragraph of only `^id`
    /// becomes the id itself.
    fn take_block_id(&mut self, ix: TreeIndex, callbacks: &mut dyn ParserCallbacks<'input>) {
        self.parse_inline_children(ix, callbacks);
        let Some(first) = self.tree[ix].child else {
            return;
        };
        let mut last = first;
        while let Some(next) = self.tree[last].next {
            last = next;
        }
        let item = self.tree[last].item.clone();
        if !matches!(item.body, ItemBody::Text { .. }) {
            return;
        }
        let text = &self.text[item.start..item.end];
        // A paragraph of only an id names the block before it.
        if first == last && matches!(self.tree[ix].item.body, ItemBody::Paragraph) {
            let trimmed = text.trim();
            if trimmed.strip_prefix('^').is_some_and(is_block_id) {
                let start = item.start + (text.len() - text.trim_start().len());
                self.tree[ix].item = Item {
                    start,
                    end: start + trimmed.len(),
                    body: ItemBody::BlockId,
                };
                self.tree[ix].child = None;
                return;
            }
        }
        let trimmed = text.trim_end();
        let Some(caret) = trimmed.rfind('^') else {
            return;
        };
        if caret == 0
            || !trimmed[..caret].ends_with(char::is_whitespace)
            || !is_block_id(&trimmed[caret + 1..])
        {
            return;
        }
        let id = Item {
            start: item.start + caret,
            end: item.start + trimmed.len(),
            body: ItemBody::BlockId,
        };
        let kept = item.start + trimmed[..caret].trim_end().len();
        if kept == item.start {
            // The id was all the text left.
            self.tree[last].item = id;
        } else {
            self.tree[last].item.end = kept;
            let node = self.tree.create_node(id);
            self.tree[last].next = Some(node);
        }
    }

    /// Give the heading at `ix` an id from its text, unless it has one.
    fn give_heading_id(&mut self, ix: TreeIndex, callbacks: &mut dyn ParserCallbacks<'input>) {
        let ItemBody::Heading(level, attrs) = self.tree[ix].item.body else {
            return;
        };
        let start = self.tree[ix].item.start;
        if let Some(id) = attrs.and_then(|attrs| self.allocs[attrs].id.as_deref()) {
            self.heading_ids.entry(id.to_owned()).or_default();
            self.headings
                .push((start, HeadingKey::Explicit(id.to_owned())));
            return;
        }
        // The heading's inline content parses first, so the id reads its
        // text rather than its syntax.
        self.parse_inline_children(ix, callbacks);
        let mut text = String::new();
        self.inline_text(self.tree[ix].child, &mut text);
        // An empty id counts, as on GitHub: a heading of punctuation alone
        // takes none, and its repeats take `-1`, `-2`.
        let base = crate::heading_slug(&text);
        self.headings.push((start, HeadingKey::Base(base.clone())));
        let id = self.unique_heading_id(&base);
        if id.is_empty() {
            return;
        }
        match attrs {
            Some(attrs) => self.allocs.heading_mut(attrs).id = Some(id.into()),
            None => {
                let attrs = self.allocs.allocate_heading(HeadingAttributes {
                    id: Some(id.into()),
                    classes: Vec::new(),
                    attrs: Vec::new(),
                });
                self.tree[ix].item.body = ItemBody::Heading(level, Some(attrs));
            }
        }
    }

    /// Return `base`, or with a `-1`, `-2` suffix when the document already
    /// has it, as GitHub numbers repeated headings.
    fn unique_heading_id(&mut self, base: &str) -> String {
        let mut id = base.to_owned();
        while self.heading_ids.contains_key(&id) {
            let count = self.heading_ids.entry(base.to_owned()).or_default();
            *count += 1;
            id = format!("{base}-{count}");
        }
        self.heading_ids.insert(id.clone(), 0);
        id
    }

    /// Append the text of the inline items from `child` on to `out`.
    fn inline_text(&self, mut child: Option<TreeIndex>, out: &mut String) {
        while let Some(ix) = child {
            let item = &self.tree[ix].item;
            match item.body {
                ItemBody::Text { .. } => out.push_str(&self.text[item.start..item.end]),
                ItemBody::Code(cow) | ItemBody::SynthesizeText(cow) => {
                    out.push_str(&self.allocs[cow])
                }
                ItemBody::SynthesizeChar(ch) => out.push(ch),
                ItemBody::Hashtag => out.push_str(&self.text[item.start..item.end]),
                ItemBody::SoftBreak | ItemBody::HardBreak(_) => out.push(' '),
                ItemBody::Html
                | ItemBody::InlineHtml
                | ItemBody::OwnedInlineHtml(_)
                | ItemBody::BlockId => {}
                _ => self.inline_text(self.tree[ix].child, out),
            }
            child = self.tree[ix].next;
        }
    }

    /// Whether the block at `ix` holds only text that comments hid.
    fn holds_only_comments(&self, ix: TreeIndex) -> bool {
        let mut child = self.tree[ix].child;
        while let Some(at) = child {
            let item = &self.tree[at].item;
            let hidden = match item.body {
                ItemBody::Text { .. } => {
                    comments::only_hidden(&self.text.as_bytes()[item.start..item.end])
                }
                ItemBody::SoftBreak | ItemBody::HardBreak(_) => true,
                _ => false,
            };
            if !hidden {
                return false;
            }
            child = self.tree[at].next;
        }
        true
    }

    /// Give the next event, unless the parse passed a limit.
    fn next_event_range(
        &mut self,
        callbacks: &mut dyn ParserCallbacks<'input>,
    ) -> Option<(Event<'input>, Range<usize>)> {
        if self.limit_hit {
            return None;
        }
        let next = self.next_unbounded(callbacks)?;
        self.events += 1;
        match next.0 {
            Event::Start(_) => self.depth += 1,
            Event::End(_) => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        if self.events > self.limits.max_items || self.depth > self.limits.max_depth {
            self.limit_hit = true;
            return None;
        }
        Some(next)
    }

    fn next_unbounded(
        &mut self,
        callbacks: &mut dyn ParserCallbacks<'input>,
    ) -> Option<(Event<'input>, Range<usize>)> {
        match self.tree.cur() {
            None => {
                let ix = self.tree.pop()?;
                let ix = if matches!(self.tree[ix].item.body, ItemBody::TightParagraph) {
                    // tight paragraphs emit nothing
                    self.tree.next_sibling(ix);
                    return self.next_unbounded(callbacks);
                } else {
                    ix
                };
                let tag_end = body_to_tag_end(&self.tree[ix].item.body);
                self.tree.next_sibling(ix);
                let span = self.tree[ix].item.start..self.tree[ix].item.end;
                debug_assert!(span.start <= span.end);
                Some((Event::End(tag_end), span))
            }
            Some(cur_ix) => {
                if self.options.intersects(
                    Options::ENABLE_BLOCK_IDS
                        | Options::ENABLE_HEADING_IDS
                        | Options::ENABLE_AUTOLINKS,
                ) {
                    self.prepare_inline(cur_ix, callbacks);
                }
                let cur_ix = if matches!(self.tree[cur_ix].item.body, ItemBody::TightParagraph) {
                    // tight paragraphs emit nothing
                    self.tree.push();
                    self.tree.cur().unwrap()
                } else {
                    cur_ix
                };
                if self.tree[cur_ix].item.body.is_maybe_inline() {
                    self.handle_inline(callbacks);
                }
                if self.options.contains(Options::ENABLE_HEADING_IDS) {
                    self.give_heading_id(cur_ix, callbacks);
                }
                let comments = self.options.contains(Options::ENABLE_COMMENTS);
                // A paragraph that held only comments shows nothing.
                if comments
                    && matches!(self.tree[cur_ix].item.body, ItemBody::Paragraph)
                    && self.holds_only_comments(cur_ix)
                {
                    self.tree.next_sibling(cur_ix);
                    return self.next_unbounded(callbacks);
                }

                let item = self.tree[cur_ix].item.clone();
                let mut event = item_to_event(&item, self.text, &mut self.allocs);
                if let Event::Start(..) = event {
                    self.tree.push();
                } else {
                    self.tree.next_sibling(cur_ix);
                }
                if comments {
                    match &mut event {
                        Event::Text(text) | Event::Html(text) | Event::InlineHtml(text)
                            if text.as_bytes().contains(&comments::HIDDEN) =>
                        {
                            let shown = comments::strip_hidden(text);
                            if shown.is_empty() {
                                return self.next_unbounded(callbacks);
                            }
                            *text = shown.into();
                        }
                        _ => {}
                    }
                }
                debug_assert!(item.start <= item.end);
                Some((event, item.start..item.end))
            }
        }
    }
}

/// Whether `id` can be an Obsidian block id: ASCII letters, digits, and
/// `-`, at least one.
fn is_block_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn body_to_tag_end(body: &ItemBody) -> TagEnd {
    match *body {
        ItemBody::Paragraph => TagEnd::Paragraph,
        ItemBody::Emphasis => TagEnd::Emphasis,
        ItemBody::Superscript => TagEnd::Superscript,
        ItemBody::Subscript => TagEnd::Subscript,
        ItemBody::Strong => TagEnd::Strong,
        ItemBody::Strikethrough => TagEnd::Strikethrough,
        ItemBody::Highlight => TagEnd::Highlight,
        ItemBody::Link(..) => TagEnd::Link,
        ItemBody::Image(..) => TagEnd::Image,
        ItemBody::Embed(..) => TagEnd::Embed,
        ItemBody::Heading(level, _) => TagEnd::Heading(level),
        ItemBody::IndentCodeBlock | ItemBody::FencedCodeBlock(..) => TagEnd::CodeBlock,
        ItemBody::Container(_, kind, _) => TagEnd::ContainerBlock(kind),
        ItemBody::BlockQuote(kind) => TagEnd::BlockQuote(kind),
        ItemBody::Callout(kind, _) => TagEnd::Callout(kind),
        ItemBody::HtmlBlock => TagEnd::HtmlBlock,
        ItemBody::List(_, c, _) => {
            let is_ordered = c == b'.' || c == b')';
            TagEnd::List(is_ordered)
        }
        ItemBody::ListItem(_) => TagEnd::Item,
        ItemBody::TableHead => TagEnd::TableHead,
        ItemBody::TableCell => TagEnd::TableCell,
        ItemBody::TableRow => TagEnd::TableRow,
        ItemBody::Table(..) => TagEnd::Table,
        ItemBody::FootnoteDefinition(..) => TagEnd::FootnoteDefinition,
        ItemBody::MetadataBlock(kind) => TagEnd::MetadataBlock(kind),
        ItemBody::DefinitionList(_) => TagEnd::DefinitionList,
        ItemBody::DefinitionListTitle => TagEnd::DefinitionListTitle,
        ItemBody::DefinitionListDefinition(_) => TagEnd::DefinitionListDefinition,
        _ => panic!("unexpected item body {:?}", body),
    }
}

fn item_to_event<'a>(item: &Item, text: &'a str, allocs: &mut Allocations<'a>) -> Event<'a> {
    let tag = match item.body {
        ItemBody::Text { .. } => return Event::Text(text[item.start..item.end].into()),
        ItemBody::Code(cow_ix) => return Event::Code(allocs.take_cow(cow_ix)),
        ItemBody::SynthesizeText(cow_ix) => {
            return Event::Text(allocs.take_cow(cow_ix));
        }
        ItemBody::SynthesizeChar(c) => return Event::Text(c.into()),
        ItemBody::HtmlBlock => Tag::HtmlBlock,
        ItemBody::Html => {
            return Event::Html(CowStr::from_replace_nuls(&text[item.start..item.end]));
        }
        ItemBody::InlineHtml => {
            return Event::InlineHtml(CowStr::from_replace_nuls(&text[item.start..item.end]));
        }
        ItemBody::OwnedInlineHtml(cow_ix) => return Event::InlineHtml(allocs.take_cow(cow_ix)),
        ItemBody::SoftBreak => return Event::SoftBreak,
        ItemBody::HardBreak(_) => return Event::HardBreak,
        ItemBody::FootnoteReference(cow_ix) => {
            return Event::FootnoteReference(allocs.take_cow(cow_ix));
        }
        ItemBody::TaskListMarker(mark) => {
            return match mark {
                b' ' => Event::TaskListMarker(false),
                b'x' | b'X' => Event::TaskListMarker(true),
                state => Event::TaskState(char::from(state)),
            };
        }
        ItemBody::BlockId => return Event::BlockId(text[item.start + 1..item.end].into()),
        ItemBody::Hashtag => return Event::Hashtag(text[item.start + 1..item.end].into()),
        ItemBody::IgnoredCells(count) => return Event::IgnoredCells(count),
        ItemBody::Rule => return Event::Rule,
        ItemBody::Paragraph => Tag::Paragraph,
        ItemBody::Emphasis => Tag::Emphasis,
        ItemBody::Superscript => Tag::Superscript,
        ItemBody::Subscript => Tag::Subscript,
        ItemBody::Strong => Tag::Strong,
        ItemBody::Strikethrough => Tag::Strikethrough,
        ItemBody::Highlight => Tag::Highlight,
        ItemBody::Link(link_ix) => {
            let (link_type, dest_url, title, id) = allocs.take_link(link_ix);
            Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }
        }
        ItemBody::Image(link_ix) => {
            let (link_type, dest_url, title, id) = allocs.take_link(link_ix);
            Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }
        }
        ItemBody::Embed(embed_ix) => Tag::Embed(allocs.take_embed(embed_ix)),
        ItemBody::Heading(level, Some(heading_ix)) => {
            let HeadingAttributes { id, classes, attrs } = allocs.index(heading_ix);
            Tag::Heading {
                level,
                id: id.clone(),
                classes: classes.clone(),
                attrs: attrs.clone(),
            }
        }
        ItemBody::Heading(level, None) => Tag::Heading {
            level,
            id: None,
            classes: Vec::new(),
            attrs: Vec::new(),
        },
        ItemBody::FencedCodeBlock(cow_ix) => {
            Tag::CodeBlock(CodeBlockKind::Fenced(allocs.take_cow(cow_ix)))
        }
        ItemBody::IndentCodeBlock => Tag::CodeBlock(CodeBlockKind::Indented),
        ItemBody::Container(_, kind, cow_ix) => Tag::ContainerBlock(kind, allocs.take_cow(cow_ix)),
        ItemBody::BlockQuote(kind) => Tag::BlockQuote(kind),
        ItemBody::Callout(_, ix) => Tag::Callout(allocs.take_callout(ix)),
        ItemBody::List(_, c, listitem_start) => {
            if c == b'.' || c == b')' {
                Tag::List(Some(listitem_start))
            } else {
                Tag::List(None)
            }
        }
        ItemBody::ListItem(_) => Tag::Item,
        ItemBody::TableHead => Tag::TableHead,
        ItemBody::TableCell => Tag::TableCell,
        ItemBody::TableRow => Tag::TableRow,
        ItemBody::Table(alignment_ix) => Tag::Table(allocs.take_alignment(alignment_ix)),
        ItemBody::FootnoteDefinition(cow_ix) => Tag::FootnoteDefinition(allocs.take_cow(cow_ix)),
        ItemBody::MetadataBlock(kind) => Tag::MetadataBlock(kind),
        ItemBody::Math(cow_ix, is_display) => {
            return if is_display {
                Event::DisplayMath(allocs.take_cow(cow_ix))
            } else {
                Event::InlineMath(allocs.take_cow(cow_ix))
            };
        }
        ItemBody::DefinitionList(_) => Tag::DefinitionList,
        ItemBody::DefinitionListTitle => Tag::DefinitionListTitle,
        ItemBody::DefinitionListDefinition(_) => Tag::DefinitionListDefinition,
        _ => panic!("unexpected item body {:?}", item.body),
    };

    Event::Start(tag)
}

#[cfg(test)]
mod test {
    use alloc::{borrow::ToOwned, string::ToString, vec::Vec};

    use super::*;
    use crate::tree::Node;

    // TODO: move these tests to tests/html.rs?

    fn parser_with_extensions(text: &str) -> Parser<'_> {
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_FOOTNOTES);
        opts.insert(Options::ENABLE_STRIKETHROUGH);
        opts.insert(Options::ENABLE_SUPERSCRIPT);
        opts.insert(Options::ENABLE_SUBSCRIPT);
        opts.insert(Options::ENABLE_TASKLISTS);

        Parser::new_ext(text, opts)
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn node_size() {
        let node_size = core::mem::size_of::<Node<Item>>();
        assert_eq!(48, node_size);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn body_size() {
        let body_size = core::mem::size_of::<ItemBody>();
        assert_eq!(16, body_size);
    }

    #[test]
    fn single_open_fish_bracket() {
        // dont crash
        assert_eq!(3, Parser::new("<").count());
    }

    #[test]
    fn lone_hashtag() {
        // dont crash
        assert_eq!(2, Parser::new("#").count());
    }

    #[test]
    fn lots_of_backslashes() {
        // dont crash
        Parser::new("\\\\\r\r").count();
        Parser::new("\\\r\r\\.\\\\\r\r\\.\\").count();
    }

    #[test]
    fn issue_1030() {
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_WIKILINKS);

        let parser = Parser::new_ext("For a new ferrari, [[Wikientry|click here]]!", opts);

        let offsets = parser
            .into_offset_iter()
            .map(|(_ev, range)| range)
            .collect::<Vec<_>>();
        let expected_offsets = vec![
            (0..44),  // Paragraph START
            (0..19),  // `For a new ferrari, `
            (19..43), // Wikilink START
            (31..41), // `click here`
            (19..43), // Wikilink END
            (43..44), // `!`
            (0..44),  // Paragraph END
        ];
        assert_eq!(offsets, expected_offsets);
    }

    #[test]
    fn issue_320() {
        // dont crash
        parser_with_extensions(":\r\t> |\r:\r\t> |\r").count();
    }

    #[test]
    fn issue_319() {
        // dont crash
        parser_with_extensions("|\r-]([^|\r-]([^").count();
        parser_with_extensions("|\r\r=][^|\r\r=][^car").count();
    }

    #[test]
    fn issue_303() {
        // dont crash
        parser_with_extensions("[^\r\ra]").count();
        parser_with_extensions("\r\r]Z[^\x00\r\r]Z[^\x00").count();
    }

    #[test]
    fn issue_313() {
        // dont crash
        parser_with_extensions("*]0[^\r\r*]0[^").count();
        parser_with_extensions("[^\r> `][^\r> `][^\r> `][").count();
    }

    #[test]
    fn issue_311() {
        // dont crash
        parser_with_extensions("\\\u{0d}-\u{09}\\\u{0d}-\u{09}").count();
    }

    #[test]
    fn issue_283() {
        let input = core::str::from_utf8(b"\xf0\x9b\xb2\x9f<td:^\xf0\x9b\xb2\x9f").unwrap();
        // dont crash
        parser_with_extensions(input).count();
    }

    #[test]
    fn issue_289() {
        // dont crash
        parser_with_extensions("> - \\\n> - ").count();
        parser_with_extensions("- \n\n").count();
    }

    #[test]
    fn issue_306() {
        // dont crash
        parser_with_extensions("*\r_<__*\r_<__*\r_<__*\r_<__").count();
    }

    #[test]
    fn issue_305() {
        // dont crash
        parser_with_extensions("_6**6*_*").count();
    }

    #[test]
    fn another_emphasis_panic() {
        parser_with_extensions("*__#_#__*").count();
    }

    #[test]
    fn offset_iter() {
        let event_offsets: Vec<_> = Parser::new("*hello* world")
            .into_offset_iter()
            .map(|(_ev, range)| range)
            .collect();
        let expected_offsets = vec![(0..13), (0..7), (1..6), (0..7), (7..13), (0..13)];
        assert_eq!(expected_offsets, event_offsets);
    }

    #[test]
    fn issue_1142_deflist_offset_is_char_boundary() {
        // Title `%`, definition whose text is combining acute U+0301, then a
        // TAB-indented second definition. The second definition's start used
        // to land inside the combining character.
        let doc = "%\n:\u{301}\n\t:";
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_DEFINITION_LIST);
        for (_event, range) in Parser::new_ext(doc, opts).into_offset_iter() {
            let _ = &doc[range];
        }
    }

    #[test]
    fn issue_983_tab_indented_list_offsets() {
        // A nested list indented with a TAB could start on the previous line:
        // inside the code block in the issue's input, or inside `漢` in the
        // definition list below. With spaces it starts on its own line.
        for (doc, expected_starts) in [
            (
                "*     indented code block\n\t+ nested list\n",
                [0, 0, 27, 27],
            ),
            (
                "*     indented code block\n  + nested list\n",
                [0, 0, 28, 28],
            ),
            ("- a\r\n\t1. b\r\n", [0, 0, 6, 6]),
            ("> - a\n>\t- b\n", [2, 2, 8, 8]),
        ] {
            let starts: Vec<_> = Parser::new(doc)
                .into_offset_iter()
                .filter_map(|(event, range)| match event {
                    Event::Start(Tag::List(_) | Tag::Item) => Some(range.start),
                    _ => None,
                })
                .collect();
            assert_eq!(starts, expected_starts, "{doc:?}");
        }

        {
            let (doc, expected_starts) = ("a\n:\n\t漢\n\t- o\n", [0, 0, 2, 10, 10]);
            let starts: Vec<_> = Parser::new_ext(doc, Options::ENABLE_DEFINITION_LIST)
                .into_offset_iter()
                .filter_map(|(event, range)| match event {
                    Event::Start(
                        Tag::List(_)
                        | Tag::Item
                        | Tag::DefinitionList
                        | Tag::DefinitionListTitle
                        | Tag::DefinitionListDefinition,
                    ) => Some(range.start),
                    _ => None,
                })
                .collect();
            assert_eq!(starts, expected_starts, "{doc:?}");
        }
    }

    #[test]
    fn reference_link_offsets() {
        let range =
            Parser::new("# H1\n[testing][Some reference]\n\n[Some reference]: https://github.com")
                .into_offset_iter()
                .filter_map(|(ev, range)| match ev {
                    Event::Start(
                        Tag::Link {
                            link_type: LinkType::Reference,
                            ..
                        },
                        ..,
                    ) => Some(range),
                    _ => None,
                })
                .next()
                .unwrap();
        assert_eq!(5..30, range);
    }

    #[test]
    fn collapsed_link_offsets() {
        // A collapsed reference link (`[foo][]`) must span the trailing `[]`,
        // like a full reference link, rather than stopping at the label.
        let range = Parser::new("[foo][]\n\n[foo]: /url")
            .into_offset_iter()
            .filter_map(|(ev, range)| match ev {
                Event::Start(
                    Tag::Link {
                        link_type: LinkType::Collapsed,
                        ..
                    },
                    ..,
                ) => Some(range),
                _ => None,
            })
            .next()
            .unwrap();
        assert_eq!(0..7, range);
    }

    #[test]
    fn issue_1131_code_span_strips_continuation_indent() {
        // The leading spaces and tabs of a paragraph continuation line are not
        // part of the code span content, and CRLF is a single line ending. The
        // span's range still covers the source from backtick to backtick.
        for (doc, code, range) in [
            ("`` a\n  `b`\n  ``", "a `b`", 0..15),
            ("`a\n\t b`", "a b", 0..7),
            ("`a\r\n   b`", "a b", 0..9),
            ("> `a\r\n>   b`", "a b", 2..12),
        ] {
            let spans: Vec<_> = Parser::new(doc)
                .into_offset_iter()
                .filter_map(|(ev, range)| match ev {
                    Event::Code(code) => Some((code.to_string(), range)),
                    _ => None,
                })
                .collect();
            assert_eq!(vec![(code.to_string(), range)], spans, "{doc:?}");
        }
    }

    #[test]
    fn footnote_offsets() {
        let range = parser_with_extensions("Testing this[^1] out.\n\n[^1]: Footnote.")
            .into_offset_iter()
            .filter_map(|(ev, range)| match ev {
                Event::FootnoteReference(..) => Some(range),
                _ => None,
            })
            .next()
            .unwrap();
        assert_eq!(12..16, range);
    }

    #[test]
    fn footnote_offsets_exclamation() {
        let mut immediately_before_footnote = None;
        let range = parser_with_extensions("Testing this![^1] out.\n\n[^1]: Footnote.")
            .into_offset_iter()
            .filter_map(|(ev, range)| match ev {
                Event::FootnoteReference(..) => Some(range),
                _ => {
                    immediately_before_footnote = Some((ev, range));
                    None
                }
            })
            .next()
            .unwrap();
        assert_eq!(13..17, range);
        if let (Event::Text(exclamation), range_exclamation) =
            immediately_before_footnote.as_ref().unwrap()
        {
            assert_eq!("!", &exclamation[..]);
            assert_eq!(&(12..13), range_exclamation);
        } else {
            panic!("what came first, then? {immediately_before_footnote:?}");
        }
    }

    #[test]
    fn table_offset() {
        let markdown = "a\n\nTesting|This|Outtt\n--|:--:|--:\nSome Data|Other data|asdf";
        let event_offset = parser_with_extensions(markdown)
            .into_offset_iter()
            .map(|(_ev, range)| range)
            .nth(3)
            .unwrap();
        let expected_offset = 3..59;
        assert_eq!(expected_offset, event_offset);
    }

    #[test]
    fn table_cell_span() {
        let markdown = "a|b|c\n--|--|--\na|  |c";
        let event_offset = parser_with_extensions(markdown)
            .into_offset_iter()
            .filter_map(|(ev, span)| match ev {
                Event::Start(Tag::TableCell) => Some(span),
                _ => None,
            })
            .nth(4)
            .unwrap();
        let expected_offset_start = "a|b|c\n--|--|--\na|".len();
        assert_eq!(
            expected_offset_start..(expected_offset_start + 2),
            event_offset
        );
    }

    #[test]
    fn offset_iter_issue_378() {
        let event_offsets: Vec<_> = Parser::new("a [b](c) d")
            .into_offset_iter()
            .map(|(_ev, range)| range)
            .collect();
        let expected_offsets = vec![(0..10), (0..2), (2..8), (3..4), (2..8), (8..10), (0..10)];
        assert_eq!(expected_offsets, event_offsets);
    }

    #[test]
    fn offset_iter_issue_404() {
        let event_offsets: Vec<_> = Parser::new("###\n")
            .into_offset_iter()
            .map(|(_ev, range)| range)
            .collect();
        let expected_offsets = vec![(0..4), (0..4)];
        assert_eq!(expected_offsets, event_offsets);
    }

    // FIXME: add this one regression suite
    #[cfg(feature = "html")]
    #[test]
    fn link_def_at_eof() {
        let test_str = "[My site][world]\n\n[world]: https://vincentprouillet.com";
        let expected = "<p><a href=\"https://vincentprouillet.com\">My site</a></p>\n";

        let mut buf = String::new();
        crate::html::push_html(&mut buf, Parser::new(test_str));
        assert_eq!(expected, buf);
    }

    #[cfg(feature = "html")]
    #[test]
    fn no_footnote_refs_without_option() {
        let test_str = "a [^a]\n\n[^a]: yolo";
        let expected = "<p>a <a href=\"yolo\">^a</a></p>\n";

        let mut buf = String::new();
        crate::html::push_html(&mut buf, Parser::new(test_str));
        assert_eq!(expected, buf);
    }

    #[cfg(feature = "html")]
    #[test]
    fn ref_def_at_eof() {
        let test_str = "[test]:\\";
        let expected = "";

        let mut buf = String::new();
        crate::html::push_html(&mut buf, Parser::new(test_str));
        assert_eq!(expected, buf);
    }

    #[cfg(feature = "html")]
    #[test]
    fn ref_def_cr_lf() {
        let test_str = "[a]: /u\r\n\n[a]";
        let expected = "<p><a href=\"/u\">a</a></p>\n";

        let mut buf = String::new();
        crate::html::push_html(&mut buf, Parser::new(test_str));
        assert_eq!(expected, buf);
    }

    #[cfg(feature = "html")]
    #[test]
    fn no_dest_refdef() {
        let test_str = "[a]:";
        let expected = "<p>[a]:</p>\n";

        let mut buf = String::new();
        crate::html::push_html(&mut buf, Parser::new(test_str));
        assert_eq!(expected, buf);
    }

    #[test]
    fn broken_links_called_only_once() {
        for &(markdown, expected) in &[
            ("See also [`g()`][crate::g].", 1),
            ("See also [`g()`][crate::g][].", 1),
            ("[brokenlink1] some other node [brokenlink2]", 2),
        ] {
            let mut times_called = 0;
            let callback = &mut |_broken_link: BrokenLink| {
                times_called += 1;
                None
            };
            let parser =
                Parser::new_with_broken_link_callback(markdown, Options::empty(), Some(callback));
            for _ in parser {}
            assert_eq!(times_called, expected);
        }
    }

    #[test]
    fn simple_broken_link_callback() {
        let test_str = "This is a link w/o def: [hello][world]";
        let mut callback = |broken_link: BrokenLink| {
            assert_eq!("world", broken_link.reference.as_ref());
            assert_eq!(&test_str[broken_link.span], "[hello][world]");
            let url = "YOLO".into();
            let title = "SWAG".to_owned().into();
            Some((url, title))
        };
        let parser =
            Parser::new_with_broken_link_callback(test_str, Options::empty(), Some(&mut callback));
        let mut link_tag_count = 0;
        for (typ, url, title, id) in parser.filter_map(|event| match event {
            Event::Start(tag) => match tag {
                Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => Some((link_type, dest_url, title, id)),
                _ => None,
            },
            _ => None,
        }) {
            link_tag_count += 1;
            assert_eq!(typ, LinkType::ReferenceUnknown);
            assert_eq!(url.as_ref(), "YOLO");
            assert_eq!(title.as_ref(), "SWAG");
            assert_eq!(id.as_ref(), "world");
        }
        assert!(link_tag_count > 0);
    }

    #[test]
    fn code_block_kind_check_fenced() {
        let parser = Parser::new("hello\n```test\ntadam\n```");
        let mut found = 0;
        for (ev, _range) in parser.into_offset_iter() {
            if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(syntax))) = ev {
                assert_eq!(syntax.as_ref(), "test");
                found += 1;
            }
        }
        assert_eq!(found, 1);
    }

    #[test]
    fn code_block_kind_check_indented() {
        let parser = Parser::new("hello\n\n    ```test\n    tadam\nhello");
        let mut found = 0;
        for (ev, _range) in parser.into_offset_iter() {
            if let Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) = ev {
                found += 1;
            }
        }
        assert_eq!(found, 1);
    }

    #[test]
    fn ref_defs() {
        let input = r###"[a B c]: http://example.com
[another]: https://google.com

text

[final ONE]: http://wikipedia.org
"###;
        let mut parser = Parser::new(input);

        assert!(parser.reference_definitions().get("a b c").is_some());
        assert!(parser.reference_definitions().get("nope").is_none());

        if let Some(_event) = parser.next() {
            // testing keys with shorter lifetimes than parser and its input
            let s = "final one".to_owned();
            let link_def = parser.reference_definitions().get(&s).unwrap();
            let span = &input[link_def.span.clone()];
            assert_eq!(span, "[final ONE]: http://wikipedia.org");
        }
    }

    #[test]
    fn common_lifetime_patterns_allowed<'b>() {
        let temporary_str = String::from("xyz");

        // NOTE: this is a limitation of Rust, it doesn't allow putting lifetime
        // parameters on the closure itself. Hack it by attaching the
        // lifetime to the test function instead. TODO: why is the `'b`
        // lifetime required at all? Changing it to `'_` breaks things :(
        let mut closure = |link: BrokenLink<'b>| Some(("#".into(), link.reference));

        fn function(link: BrokenLink<'_>) -> Option<(CowStr<'_>, CowStr<'_>)> {
            Some(("#".into(), link.reference))
        }

        for _ in Parser::new_with_broken_link_callback(
            "static lifetime",
            Options::empty(),
            Some(&mut closure),
        ) {}
        /* This fails to compile. Because the closure can't say `for <'a>
         * fn(BrokenLink<'a>) -> CowStr<'a>` and has to use the
         * enclosing `'b` lifetime parameter, `temporary_str` lives
         * shorter than `'b`. I think this is unlikely to occur in real life,
         * and if it does, the fix is simple: move it out to a
         * function that allows annotating the lifetimes.
         */
        //for _ in Parser::new_with_broken_link_callback(&temporary_str,
        // Options::empty(), Some(&mut callback)) {
        //}

        for _ in Parser::new_with_broken_link_callback(
            "static lifetime",
            Options::empty(),
            Some(&mut function),
        ) {}
        for _ in Parser::new_with_broken_link_callback(
            &temporary_str,
            Options::empty(),
            Some(&mut function),
        ) {}
    }

    #[test]
    fn inline_html_inside_blockquote() {
        // Regression for #960
        let input = "> <foo\n> bar>";
        let events: Vec<_> = Parser::new(input).collect();
        let expected = [
            Event::Start(Tag::BlockQuote(None)),
            Event::Start(Tag::Paragraph),
            Event::InlineHtml(CowStr::Boxed("<foo\nbar>".to_string().into())),
            Event::End(TagEnd::Paragraph),
            Event::End(TagEnd::BlockQuote(None)),
        ];
        assert_eq!(&events, &expected);
    }

    #[test]
    fn wikilink_has_pothole() {
        let input = "[[foo]] [[bar|baz]]";
        let events: Vec<_> = Parser::new_ext(input, Options::ENABLE_WIKILINKS).collect();
        let expected = [
            Event::Start(Tag::Paragraph),
            Event::Start(Tag::Link {
                link_type: LinkType::WikiLink { has_pothole: false },
                dest_url: CowStr::Borrowed("foo"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("foo")),
            Event::End(TagEnd::Link),
            Event::Text(CowStr::Borrowed(" ")),
            Event::Start(Tag::Link {
                link_type: LinkType::WikiLink { has_pothole: true },
                dest_url: CowStr::Borrowed("bar"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("baz")),
            Event::End(TagEnd::Link),
            Event::End(TagEnd::Paragraph),
        ];
        assert_eq!(&events, &expected);
    }

    #[test]
    fn wikilink_no_event_blowup() {
        // Regression: handle_wikilink reused an existing tree node as
        // the wikilink's child without terminating that node's `next`
        // chain.  The `next` chain threaded through the closing `]]`
        // into the content after the wikilink, so the EventIter
        // visited that tail content both as children (via child ptr)
        // and as siblings (via next ptr). Adversarial repeated
        // `[[|]]` patterns compounded exponentially.
        let one = "[[[[^(\n|]]]]=]]]]]]]]\n".to_string();
        let eight = "[[[[^(\n|]]]]=]]]]]]]]\n".repeat(8);

        let n1 = Parser::new_ext(&one, Options::ENABLE_WIKILINKS).count();
        let n8 = Parser::new_ext(&eight, Options::ENABLE_WIKILINKS).count();

        assert!(
            n8 <= n1 * 20,
            "Event count should scale linearly with input length; \
             got {n1} events for 1× and {n8} events for 8× ({}× ratio, expected ≤20×)",
            n8 / n1.max(1)
        );
    }

    #[test]
    fn issue_1154() {
        // If a `_` that can only close finds no opener, a `_` of the same
        // length that can also open won't find one either, so its bound
        // can go up too.
        let mut tree = Tree::with_capacity(4);
        let mut stack = InlineStack::default();
        for _ in 0..3 {
            let start = tree.append(Item {
                start: 0,
                end: 1,
                body: ItemBody::Text {
                    backslash_escaped: false,
                },
            });
            stack.push(InlineEl {
                start,
                count: 1,
                run_length: 1,
                c: b'*',
                both: false,
            });
        }
        assert!(stack.find_match(&mut tree, b'_', 1, false).is_none());
        assert_eq!(3, stack.get_lowerbound(b'_', 1, false));
        assert_eq!(3, stack.get_lowerbound(b'_', 1, true));
    }
}
