# Markdown and source readers

`canopy-markdown` provides a terminal Markdown reader with Rendered and Source
views. Its table inspector, `canopy_widgets::TableView`, also works on its own,
and `canopy_widgets::TableReader` shows a standalone table in the layout the
reader gives an embedded one, with the same search.
Run the examples with `cargo run -p gyms -- markdown` and
`cargo run -p gyms -- table`.

## Embed a reader

Register `Markdown` during setup, construct it with `Markdown::new(text, options)`,
and mount it as a child. Preparation starts after the reader receives its viewport
width. `ready()` reports whether the active view has caught up with accepted input,
geometry, and search. The outgoing view remains usable during preparation.

Create one `MarkdownExecutor` per host and clone it into the `MarkdownOptions` of
all readers. The default options create a service for a single reader. Inspectors
share their reader's service. Hosts own shutdown; dropping a document cancels its
work without blocking the UI thread. Tests can inject `MarkdownExecutor::manual`
and explicitly complete, reject, or reorder held work.

For a stream, construct `Markdown::streaming(options)`, then call `append` with
complete UTF-8 chunks and `finish` with `FinishOutcome::Completed` or
`FinishOutcome::Aborted(reason)`. Append rejection leaves accepted input unchanged.
Finish interprets the final bytes as EOF; it does not invent a closing fence.
`replace` starts a new epoch and explicitly chooses Complete or Receiving input.

Rendered is the default. `set_view_type` and `toggle_view_type` request a switch;
`view_type` reports the displayed view and `requested_view_type` reports its target.
Inspectors and active pointer drags temporarily disable switching. A paused round
trip restores the original position unless navigation supersedes it. Following
reveals the new view's tail. A parse failure still permits an explicit Source view.

A status row below the document holds the Rendered / Source control, the
navigation mode, the lifecycle, and the outcome of the last action. A host that
sets `MarkdownOptions::status_row` to false hides it, and shows what matters of
the same state from the reader's `Widget::status` items: a partial render, a
failure, an aborted or receiving stream, new bytes past a paused view, an
action in progress or one that failed, and where a selected link leads. The
host reports an action it performed itself. A whole, current document reports
nothing.

## Paint documents yourself

A host that lays out many documents in one canvas, such as a transcript, uses
the pipeline under the reader. `SourceStore` accepts the text, complete or as a
stream. `document::parse_document` parses an accepted revision, and
`PresentationSnapshot::prepare` lays it out for a `GeometryKey`. Each step takes
the result before it, and reuses what did not change.

`PresentationSnapshot::paint_at` paints a document rectangle with its top-left
cell at a render point, so each document can sit below the one before it.
`end_caret` gives the cell after the last grapheme, where a stream's marker
goes. The host registers the styles with `canopy_markdown::register_styles`,
and wraps a paint in `Render::with_effect` to tint one document, such as
muted reasoning. Such a host shows the document only: objects, links, search,
and selection are the reader's.

## Reading and actions

The rendered view has two modes. Scroll moves the document. Interact is the
block cursor. A bar in the left margin, in the style `markdown/block/bar`,
marks each content row of one edit unit. At a width of three columns or less,
blocks have no margin. The unit's rows then take the cursor's ground,
`markdown/block`, instead.

`i` enters the block cursor. It marks the unit it marked before, if that unit
still shows, or else the first unit with a row in view. `j` and `k` move to the
next or previous unit, and `g` and `G` to the first or last. A unit that fits
in the view scrolls into view by the smallest move. A taller unit entered going
down shows its top at the top of the view. Entered going up, it shows its
bottom at the bottom of the view.

While the marked unit continues past the view, `j` or `k` scrolls and the
cursor stays. The step is a page less two rows, and at least one row. Page
keys, `Space`, and the wheel scroll freely. When the marked unit leaves the
view, the cursor marks the first unit with a row in view. A resize or a new
layout keeps the marked unit on its screen row where it can, and in view. A
click on a unit, and a search match that becomes current, move the cursor to
that unit.

`MarkdownOptions::resting_mode` sets where the view rests. With Scroll, the
default, `Esc` leaves the block cursor. With Interact, a new document opens in
the block cursor, and `Esc` keeps it.

`select_block_at` marks the unit that `units::unit_at` finds for a source
byte. A host uses it to restore a place after a reload. Before the document
shows, the reader marks the unit once it does. `selected_block` returns the
marked unit.

The object cursor works inside the block cursor. `Tab` and `shift-Tab` move
it through the links, cards, and tables of the document, in source order, and
enter the block cursor from Scroll. The block cursor follows to the object's
unit, and a move to another unit drops the object. The active object takes an accent-tinted ground, which spans the
full width of a card or table.

`Enter` opens the active object. With none, it opens the marked unit's own
card, such as its code or table, or else the first object in the unit. `y`
and `Markdown::copy_item` copy the text selection, else the active object,
else the marked unit's exact source. `v` starts a text selection at the active
object, or else at the start of the marked unit. `Esc` clears the text
selection first, and then the active object. It then leaves the block cursor
if the view rests in Scroll.

Links open an inspector first; a separate Open action requests a
browser launch. The rendered view never cuts content vertically. Code and
source cards show every line; a line wider than its card clips at the card's
edge, and the pinned code inspector shows it whole. A table preview shows
every row and fits the reading width. Long values wrap within their columns
over as many lines as they need, while short and single-word columns keep their
widths. Accent labels and a heavy rule mark the header, and light rules separate
rows when any of them wraps. When prose would need columns too narrow to hold
it, the preview keeps the leading columns that fit, marks each line at its
broken right margin, and labels the table with the columns it shows. Completed
tables open a keyed TableView; a receiving trailing table remains literal source
until it closes. The table inspector shows every value in full: values wrap
within their columns, the columns fit the inspector's width, and the grid
scrolls sideways only when even narrowed columns do not fit.

A quote's rail runs unbroken down its whole length, through the blank rows
between its blocks and beside the code cards, tables, and rules it holds.
Adjacent quotes keep a gap between their rails.

Alerts take the colour of their tone on their rail and label. GitHub's five
kinds set the tones, and the common labels of other tools, such as Obsidian's
`info`, `success`, `question`, and `danger`, take the tone of the closest
kind. `AlertTone::of` lists them. A marker must stand alone on the quote's
first line, as GitHub requires; an unknown label stays quote text.

An inspector takes the reader's place: it covers the whole reader, its status
row included, with no frame or buttons of its own, and leaves the rest of the
host visible. A status row shows a prompt or the outcome of an action, and
hides otherwise. The inspector takes focus until it closes, and the document's
scrollbars hide while it covers the document. Its modal admits the host's
bindings scoped to the reader, and the keys of a menu they open, so a host's
menus still reach it; bindings with no path stay out, so a table keeps its own
keys. Its `y`, which copies its selection, is an application binding scoped
to it, so a host can take the key; its other keys belong to its framework
group. A menu the host opens over it shows its keys as it does over the
reader. A table numbers its rows while source shows line numbers, so one
preference toggles both. `Markdown::has_item` reports whether the reader has
an item to copy: its text selection, the active object, the unit the block
cursor marks, or what an open inspector selects, such as a table's cells.
`Markdown::copy_item` copies it.

Source preserves Markdown syntax, tabs, CRLF, and final-newline bytes. Its display
substitutes unsafe terminal controls, but exact copy uses accepted source ranges.
`MarkdownSource` is also usable on its own: prepare a `PreparedSource` off the UI
thread and publish it without resetting the current selection or paused viewport.
A standalone host must prepare new geometry when `requested_width()` changes.

Copy and Open require explicit host capabilities. Drain `take_requests(limit)`,
perform each immutable action once, and call `complete_request` with success,
rejection, or failure. Keep the request ID through asynchronous host work. The
reader ignores duplicate replies and replies for replaced documents. Parsing,
painting, switching, and link inspection never perform these external actions.

The built-in search field matches case-sensitive literal text. A host with its
own search field, such as fh, sets `external_search` and calls
`set_search(query, regex)`: `true` matches a smart-case regular expression, and
`false` a case-sensitive literal. Rendered searches readable text, including
omitted code and table cells; Source searches raw Markdown. Rendered search
follows the conventions of `canopy_widgets::view_search`, as the editor and the
diff view do: when a new query's matches publish, the first match at or below
the top of the view becomes current, and the reader centres it, unless the
reader has moved or follows its tail since the query. `keep_search_position`
cancels that reveal, for a host that restores its own position after a
reload. It takes the `MatchPlace` that `search_place` gave before the reload,
shifted through the edit as a `ReaderPosition` is, and makes that match, or
the next one, current. The editor and `TableReader` keep their current match
through `restore_matches`.
The current match paints apart from the others, and the scrollbar
marks every row that holds a match. A match in a code line or a table cell
marks only the text it found, a match in a column the preview cuts marks the
margin that stands for it, and a table inspector opened on a match shows
the table's matches too. Source does not jump until the reader
activates a match. Local fragment and footnote jumps keep a return position.

Hosts with a separate search or status bar can set `publication_wake` to their
node's wake handle. Accepted publications and view changes then refresh host
state without a polling delay. Read-only queries do not generate wakes.

## Edit units

`canopy_markdown::units` divides a document into edit units for editing a
block at a time. An edit unit is the whole source lines of one block of a
`DocumentSnapshot`. So it keeps the markers of the block's quotes and lists.
Blocks whose lines overlap make one unit of kind `Merged`. Text with no block,
such as a link reference definition or a lone block id, has no unit.

Units come from the document alone, not from its layout. `edit_units` reads
them from a document. `parse_units` parses raw text into them, as for an
editor's text.

A unit's range starts at the start of its first line. It ends at the end of
the text of its last line, before that line's terminator. So a splice of a unit
never splits a CRLF pair, and the terminators inside the unit are part of it.
`unit_at` returns the unit that holds a byte, or else the next unit, or else
the one before. The end of a unit's last line counts as inside it. `unit_at`
returns none only for text with no units.

Each unit has a kind and its own text: the bytes that a spoken replace
changes. `replace_own_text` builds the splice that replaces them, and keeps the
structure of the block:

| Kind | Own text | A replacement |
| --- | --- | --- |
| Paragraph, list item, footnote | The text after the markers of the first line | New lines take the prefix of continuation lines |
| ATX heading | The text between the `#` marks and any closing marks | Line breaks become spaces |
| Setext heading | The text lines, without the underline | Line breaks become spaces |
| Callout of only its title | The title | Line breaks become spaces |
| Fenced code | The lines between the fences | New lines take the prefix of code lines |
| Indented code | The code lines | New lines take the indent |

The prefix of continuation lines keeps quote markers. It turns a list marker,
a task box, or a footnote label into blanks, so each column stays where it
was. Line breaks take the unit's terminator, and line breaks at the end of a
replacement go, except in code. Tables, rules, HTML blocks, front matter,
images and embeds alone in their paragraphs, source cards, and merged units
have no own text. An edit of the whole unit still works on them.

Markdown is not local, so an edit can change the kinds of the units before
it. For example, a `===` line under a paragraph makes it a heading. The bytes before the edited
unit do not change.

## Bounds and automation

Options bound accepted source, semantic nodes/depth/table cells, automatic syntax
work, search results, host actions, copied payloads, and inactive layout caches.
An oversized inactive layout is evicted while semantic identity and view positions
are retained. The cache budget is separate from active source, semantic models,
Editor buffers, in-flight jobs, inspectors, and host payloads.

`state`, `object_window`, and `search_window` expose bounded,
revision-addressed automation data. Collection windows cap their item count and
text length. State includes publication, follow, the marked unit, inspector
readiness, search coverage, and conservative resource categories. The marked
unit gives its range, own text, kind, and rows. It never returns the full
source. The `select_block_at` command marks a unit from a script.

The default crate feature enables syntax highlighting. With default features
removed, both views retain reading, search, selection, and copy. The crate does
not require the graphics or terminal-emulator widget features.

Built-in syntax colors follow the current palette in both views and code
inspectors. Theme changes reuse prepared text and geometry. Fence grammar
selection uses the shared resolver, including Luau, grammar names, and fence
attributes such as `rust,ignore`; unknown languages remain plain text.

## Consumer boundary and checks

fh owns file loading, raw input limits, its held regex query and focus policy, and
clipboard/browser services. It retains the pane's preferred Markdown view during
the session. Partial files use Source with truncation metadata outside the exact
source payload. The `fh-markdown` browser server and fh's `F m` action remain in fh.

The `testing` feature shares Proptest strategies with other tests: a grammar
of documents, hostile edits, and the parser's spec examples. Its `check_units`
and `check_edit` check the invariants of edit units against a slow reference,
as a fuzz target can. Routine runs use a fixed seed. `cargo xtask soak` runs
the property tests with many cases and random seeds, and keeps each failure in
the test's `proptest-regressions` file.

The tracked gym smoke suites live in `examples/gyms/markdown-smoke` and
`examples/gyms/table-smoke`. Gallery scripts cover both representations, inspectors,
streaming transitions, narrow sizes, and TableView. See the
[qualification harness](../crates/canopy-markdown/benches/README.md) for reproducible
latency and memory workloads. Correctness tests use controlled work completion;
performance measurements run separately in release mode.
