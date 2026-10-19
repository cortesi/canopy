# canopy-xmark

A CommonMark pull parser with GitHub's and Obsidian's syntax, forked from
[pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark).

Canopy's Markdown reader and fh's browser preview both parse with this
crate, so the terminal and the browser read one grammar. The crate holds
everything that reads Markdown syntax. The consumers only present what it
reports.

## The fork

The fork starts from pulldown-cmark's `main` at commit
[`ad1622b`](https://github.com/pulldown-cmark/pulldown-cmark/commit/ad1622be42b65cdf24ce41e070e516d8844436f0)
(2026-10-16), ahead of its 0.13.4 release. It keeps pulldown-cmark's
`Parser`, `Event`, and `Tag` API, its HTML writer behind the `html`
feature, and its spec suites.

The fork leaves out these parts of pulldown-cmark:

- The benchmarks, the fuzzers, the guide, the command-line binary, and the
  examples.
- The `simd` feature. The crate forbids `unsafe` code.
- The `serde`, `hashbrown`, and `no_std` support.
- `pulldown-cmark-escape`. It is the `escape` module here, behind `html`.

`cargo xtask xmark-specs` in the Canopy workspace generates the spec tests
in `tests/suite` from `specs/` and `third_party/`. No build script does. An
example's fence suffix names the extensions it enables, such as
`example_wikilinks`. The suffix `example_xmark` enables this fork's syntax.

pulldown-cmark has the MIT license, and so does this fork. See `LICENSE`.

## Changes

Each change is off until its option is set. `Options::obsidian()` sets the
options that Canopy and fh read.

### `Options::obsidian()`

These options are GitHub's tables, footnotes, strikethrough, task lists,
and alerts, YAML front matter, wikilinks, math, and upstream's highlights,
`==text==`. They also include all of this fork's Obsidian syntax.

### Callouts: `ENABLE_CALLOUTS`

A block quote whose first line is `[!label]` is a `Tag::Callout`. A fold
sign, `+` or `-`, and a title can follow the label. The tag carries the
label, the title, the fold sign, and the kind.

`BlockQuoteKind::from_label` gives each label of the common vocabularies
the closest of GitHub's five kinds. These vocabularies are Obsidian's,
MkDocs', and Docusaurus'. An unknown label opens no callout. Callouts take
precedence over GitHub alerts, so `[!NOTE]` is a callout too. The HTML
writer gives a callout the alert class of its kind, and writes its title.

### Comments: `ENABLE_COMMENTS` and `hide_comments`

Obsidian's `%%…%%` hides the text it holds, within a line or across
blocks, outside code and math. A comment can open in one paragraph and
close blocks later. So `hide_comments` blanks comments before the parse,
and keeps every byte offset:

- A line that a comment hides entirely becomes blank.
- On a line that keeps other text, a hidden byte becomes U+0001.

A parser with `ENABLE_COMMENTS` removes these bytes from text. Where a
comment was between two words, one space stays. The parser drops a
paragraph that held only comments. A `%%` with no partner shows as written.

### Heading ids: `ENABLE_HEADING_IDS`

A heading without its own id gets the id that GitHub gives it. That id is
`heading_slug` of its text: lowercase, with letters, digits, `-`, and `_`,
and with spaces as `-`. A repeat in the document gets `-1`, `-2`, and so
on. The parser reads the inline content of the heading first, so the id
comes from its text, not its syntax.

### Block ids: `ENABLE_BLOCK_IDS`

A trailing ` ^id` on a paragraph, list item, or heading is an
`Event::BlockId` inside that block. The parser cuts it from the text. It
must be outside code and links. A paragraph of only `^id` is an
`Event::BlockId` between blocks, and names the block before it.

### Tags: `ENABLE_HASHTAGS`

`#name` at the start of a word is an `Event::Hashtag`, without its `#`. A
name has letters, digits, `_`, `-`, and `/`. A name of only digits, such as
`#1`, is not a tag. The inline pass reads tags, so `\#x` and code stay text.

### Autolinks: `ENABLE_AUTOLINKS`

This is GFM's autolink extension. Bare `http://`, `https://`, and `ftp://`
URLs, `www.` hosts, and email addresses become links, outside links and
images. Their type is `LinkType::Autolink` or `LinkType::Email`.
`specs/gfm_autolinks.txt` holds the examples of the GFM specification.

### Wikilinks and embeds: `ENABLE_OBSIDIAN_WIKILINKS`

An unlabelled wikilink shows `wikilink_label` of its target. The label has
no `.md`, and shows the anchor after `›`. A wikilink image, `![[target]]`,
is a `Tag::Embed` of an image or a note, as `EmbedKind::of` decides. Its
label can give its size: `|width` or `|widthxheight`. `wikilink_href`
gives the relative URL that a wikilink names in an HTML page.

### Dropped table cells: `ENABLE_IGNORED_CELLS`

GFM drops the cells of a table row past the count of the header. With this
option, an `Event::IgnoredCells` ends such a row, with the number of cells.

### Task states: `ENABLE_TASK_STATES`

A task list marker can hold any visible ASCII character, such as `[/]` or
`[-]`, in addition to `[ ]` and `[x]`. Such a marker is an
`Event::TaskState` with its character. The HTML writers show it as a done
task, with a `data-task` attribute.

### Inline footnotes: `ENABLE_INLINE_FOOTNOTES`

`^[note]` is a `FootnoteReference` in its place. The note's content becomes
a footnote at the end of the document. The labels are `inline-1`,
`inline-2`, in order.

### Inline content parses first

Heading ids, block ids, autolinks, inline footnotes, and Obsidian
wikilinks each need the whole inline content of a block. With any of them
on, the parser reads all of it when it enters the block. These passes then see all of it. Upstream starts
at the first inline construct instead.

### Growing documents

- `RefDefs::spans` gives the span of every reference definition. This
  includes duplicates, which pulldown-cmark drops.
- `TailWatch` reads the events of a parse. It tells where the part of a
  growing document starts that more input can still change. It also tells
  how many quotes hold that part.
- `Parser::new_with_limits` bounds the events and the nesting of a parse.
  Past a bound, the block pass stops and the events end.
  `OffsetIter::limit_hit` then says so.
- `Parser::new_resumed` parses a document from the start of a top-level
  block. It takes the `Resume` from a parse of the text before that block:
  its definitions and how its headings got their ids. Its events are the
  same as those of a parse of the whole document, from that block on.

### GitHub's HTML: `github::push_html`

This writer is behind `html`. It writes GitHub's markup for footnotes,
which it collects and numbers at the end. It also writes heading anchors,
alert and callout boxes, task lists, and block id anchors. The boxes use
Primer Octicons, which have the MIT license. See `LICENSE-octicons`. A hook
writes fenced code.

### Helpers

- `section`: the source range of the section that an anchor names in a
  note, for a heading or a block.
- `split_wikilink`, `split_destination`, and `percent_decode`: the path and
  anchor of a destination.
- `fenced_code` and `fence_language`: fenced code blocks and their
  languages, for tools that read Markdown source line by line.
- `is_markdown` and `MARKDOWN_EXTENSIONS`: the files that hold Markdown.
