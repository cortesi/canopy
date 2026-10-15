# Interactive Markdown

## Purpose and status

Build two sibling terminal presentations of one Markdown document: **Rendered** for reading
and interaction, and **Source** for reading the original Markdown text. A visible control
switches between them without losing the reader's place. In Rendered, a table becomes a
compact grid with a spreadsheet inspector, and code becomes a highlighted card with a
scrollable, selectable viewer. Links have readable labels, inspectable destinations, and
explicit browser actions. Source keeps delimiters, indentation, and definitions visible.

Streaming is a core use case. A reader can inspect earlier content while more text arrives.
Accepted source stays available, uncertain syntax stays visible, and updates preserve the
reader's position, focus, and selection.

The design was grounded in Canopy at `65fc709d` and fh at `57cafd1` on
2026-10-15. Implementation now exists in both workspaces. The API sketches below
remain conceptual; current declarations live in the source and generated API captures.

Current acceptance evidence:

- Canopy's full suite passes 1,729 tests with one existing skip (`r2f006`). Tidy and
  production feature checks pass (`r4cbd3`); the isolated Markdown build without default
  features passes 119 tests. The unchanged TableView minimum-feature suite passes 23 tests.
- fh's full suite passes 484 tests with three existing skips (`r8465b`). Its tidy passes
  (`rce630`), and all 25 tracked smoke scripts pass both natively and through the external
  runner. The browser server remains in fh.
- Canopy's five smoke suites pass all 11 scenarios. The gallery contains 36 pages across
  nine sections, with the new readers and TableView also captured at 80×24 and 160×50.
  Inspected output includes Source, code/table/cell inspectors, narrow layouts, light
  syntax colors, and unavailable actions. The capture font lacks some CJK/emoji glyphs;
  byte mapping and copying have separate Unicode tests.
- Canopy API refresh and freshness checks pass (`r001f2`, `rdb8f7`), as do fh's
  (`rd682d`, `r341bf`). fh's gallery passes all seven sections: 80 pages and 240 images
  across three sizes, including visible host search status and partial-file metadata.
- Both syntax-enabled and plain release benchmarks pass all 28 measured p95 targets on
  an Apple M2 Max: each input/append workload uses 1,000 events and cold operations use ten
  repeats. Worst input p95 is 3.06 ms against 16 ms; worst stream-publication p95 is
  71.78 ms against 100 ms.
- The ten-minute lifetime soak completed 718 cycles. Drained source/model/layout estimates
  and node counts remained constant; inspectors and executor jobs returned to zero after
  removal. Allocation-trace finalization is still pending, so actual allocator peak and
  retained-byte attribution remain unqualified. RSS is recorded separately.
- PTY keyboard/mouse switching, inspector open/close, and terminal cleanup pass. Desktop
  Terminal inspection is unqualified because the computer-use tool rejected access.

Detailed local receipts and limitations are retained under `tmp/markdown-acceptance` and
`tmp/markdown-qualification`. The [fixture manifest](../crates/canopy-markdown/tests/README.md)
distinguishes independent expected outputs from static/streamed equivalence checks.

Inactive surface layouts share a configurable 32 MiB cache budget. Accepted source,
semantic identity, active Editor buffers, in-flight work, inspectors, and action payloads
are accounted separately. Eviction retains positions and selections and never discards
accepted source.

## Scope

The initial release includes:

- A reading presentation for headings, lists, quotes, tables, code, and links.
- A reusable terminal source viewer extracted from fh's text-view integration, with
  highlighting, wrapping, line numbers, search, selection, and exact source copying.
- A **Rendered / Source** control, independent of Rendered's **Scroll / Interact** modes.
- A standalone, reusable **TableView** with cell navigation, two-axis scrolling, rectangle
  selection, full cell inspection, and copying.
- A read-only code inspector with highlighting, scrolling, text selection, and copying.
- Link selection and inspection, URL copying, and host-mediated browser opening.
- Static and streamed documents, with explicit follow and interaction preserved through
  appends, resize, and background publication.
- Document text selection, literal search, an outline, and a source inspector.

Stage 2 of the delivery plan is the first usable slice. Stages 3 and 4 complete and qualify
the release.

Extensions that must not block the release: section folding, wrapped table rows, draggable
column resizing, a frozen first data column, CSV export, a live inspector for the trailing
code block, persistent jump history, and spreadsheet behavior in TableView such as sorting,
filtering, width refitting, and live updates. Outside the release: editing, formulas, remote
cell providers, image fetching, math rendering, diagram execution, and transport control.

### Crates and dependencies

Add **`canopy-markdown`**, depending on `canopy` and `canopy-widgets` with default features
disabled. Its default `syntax` feature forwards to `canopy-widgets/syntax`. Markdown does not
require graphics or a terminal emulator. Put both Markdown presentations and their shared
source/position adapters in this crate. Export **MarkdownSource** as a reusable source widget
that also works alone; **Markdown** owns the document, rendered surface, and optional source
surface with the standard toggle. Do not add a third public controller or a viewer registry.
Keep generic text navigation and highlighting in Editor. Put **TableView in
`canopy-widgets`**, available without optional features. Neither the stock widgets nor the
runtime depend on Markdown.

Use `pulldown-cmark` 0.13, as `fh-markdown` does. Support CommonMark plus tables, task lists,
strikethrough, footnotes, GitHub alert quotes, and YAML front matter through documented parser
options. Bare HTTP/HTTPS links are an adapter pass, as they are in fh, not a parser option.
Do not advertise full GFM equivalence. Bare-link rules and terminal presentation have their
own tests.

Parse the whole document on a worker, coalesced. Do not introduce a general document
framework, plugin registry, HTML pipeline, incremental parser, or asynchronous table-provider
abstraction.

### Runtime constraints

The existing interfaces provide the needed mechanisms, with these limits:

- [Widget](../crates/canopy/src/widget.rs) provides measurement, canvas, render, polling,
  focus, pointer admission, intents, key prediction, semantics, and reveal hooks. `measure` and
  `canvas` take shared access, and `render` receives a read-only context. None may parse or
  fully wrap a document.
- [Context](../crates/canopy/src/core/context.rs) provides typed widget access, mouse capture,
  modal scopes, posted commands, deferred removal, and logical reveals. A reveal whose hook
  returns `None` is consumed, so keep an unresolved anchor locally and reissue it when its
  geometry is ready.
- [Scroll ordering](../crates/canopy/src/core/world/scroll/mod.rs) lets a later scroll or
  reveal supersede an earlier one. `ChangeOutcome::Changed` can mean a reveal was canceled
  while the offset stayed put. Follow policy must compare actual offsets.
- [Navigation intents](../crates/canopy/src/core/inputmap/intent.rs) move a widget's selection
  when accepted and otherwise fall back to scrolling. Admission and handling must agree, also
  at selection boundaries.
- [Editor](../crates/canopy-widgets/src/editor/widget.rs) has read-only navigation, selection,
  search, and wrapping. Inspectors call `set_text` once, when they open. Rendering prepares
  highlighting for the whole buffer, so a payload above the syntax cap opens as plain text.
  `set_text` replaces the buffer and resets its cursor, layout, and search. A live Source
  surface needs a preserving publication path; calling `set_text` on each append is not one.
- [Highlighting](../crates/canopy-widgets/src/highlight.rs) has `Grammar::for_fence`, optional
  Syntect, and foreground-oriented spans. `Highlighter` has no `Send` bound, so only the
  built-in grammars may run on a worker. Host highlighters run on the UI thread.
- [Scrollbar](../crates/canopy-widgets/src/scrollbar.rs) accepts explicit target nodes and
  tracks, with a single owner per surface. [Dialog](../crates/canopy-widgets/src/dialog.rs)
  provides appearance, not modal admission or root-overlay ownership.
- [Modal scopes](../crates/canopy/src/core/world/modal.rs) retire when their owner disappears,
  but the root overlay is a separate node that still needs removal.
  [Lifecycle cleanup](../crates/canopy/src/core/world/completion.rs) rejects new deferred work,
  so `on_unmount` cannot post that removal.
- [NodeWakeHandle](../crates/canopy/src/core/wake.rs) coalesces notifications and expires with
  the owning incarnation. The producer owns bounded result storage. Node-lifetime polling
  continues while hidden or detached.
- [WidgetSemantics](../crates/canopy/src/core/snapshot.rs) describes one node, not thousands of
  virtual cells. Detailed queries are explicit, bounded, and revision-addressed.

### What moves from fh

[fh's terminal view](../../fh/crates/canopy-fileselect/src/view_pane.rs) currently configures
Editor with `Interaction::Display`, soft wrapping, line numbers, and a path-selected
highlighter. Its loading, pane selection, and focus control live in `canopy-fileselect`.
[fh-markdown](../../fh/crates/fh-markdown/src/lib.rs) is the separate HTTP/browser preview
server. It stays in fh, including its watcher, HTML/CSS, and browser-opening command.

Extract the terminal Markdown presentation and source-position adapter into MarkdownSource;
reuse Editor rather than moving or copying it. fh retains file detection, bounded reads,
watching, filesystem errors, pane layout, and its view menu. Ordinary text, directories,
images, and diff views continue through their existing paths. The extracted source widget
does no filesystem reads and can display an in-memory document in a gym without fh.

The [current file loader](../../fh/crates/canopy-fileselect/src/entry.rs) retains at most
256 KiB, decodes lossily, sanitizes lines, and inserts display messages. The view then joins
the lines with LF. That path cannot supply exact Markdown source. Add a bounded raw UTF-8
input path for Markdown, preserving CRLF, tabs, final newlines, and empty input. Keep file
status and truncation labels outside the document. Preserve fh's read limit; Canopy's larger
source limit does not authorize an unbounded file read. Invalid UTF-8 uses the existing
plain-text fallback with an encoding notice. An oversized file uses the bounded source
preview, labeled as partial, with Rendered unavailable; exclude an incomplete trailing UTF-8
sequence and report the retained byte range. Do not present a file prefix as a complete parse.

[fh's retained search](../../fh/crates/canopy-fileselect/src/search.rs) uses ripgrep-style
regular expressions with smart case, not the reader's default literal matching. Keep this
behavior in both fh presentations through the shared search options described below. Reuse
fh-markdown's useful parser and grammar fixtures without making terminal Markdown depend on
the browser server or treating its HTML output as the terminal oracle.

## Architecture

### Four models

1. **SourceStore:** immutable append chunks, epoch, revision, and lifecycle. It preserves
   exact accepted UTF-8 bytes. The host decodes split transport sequences before appending.
2. **DocumentSnapshot:** semantic blocks, inline content, objects, outline, the provisional
   region, and reading units, each with a source range. Parsing and mapping need no Canopy
   nodes.
3. **PresentationSnapshot:** one DocumentSnapshot, a geometry key, a block-height index,
   visible text layout, and a hit map. Interpretation and geometry are never swapped
   independently. Source prepares a separate line layout over the same SourceStore, without
   depending on a successful Markdown parse. Each surface reports its displayed revision.
4. **ReaderState:** active view type, rendered interaction mode, shared follow and search
   query, per-view anchors and selections, object cursor, and inspector state. Only the UI
   thread changes it. Workers never return reader state.

The [parser offset iterator](https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.OffsetIter.html)
gives event ranges, not a character map. Parent and child ranges overlap, and visible text
differs from source. Reading units therefore carry a source range, not a byte-for-byte map.

### Reading units

A document is an ordered sequence of reading units in source order: prose text, explicit
logical separators, and atomic table, code, and source-card units. Each unit has an identity,
a source range, and readable text. Generated decoration is not readable text.

Footnote definitions render as labeled blocks at their source positions, with reference
jumps, not in a synthetic appendix. Outline and search are indexes over this one order.

Cards are atomic for document selection. A selection that crosses a card includes its whole
payload and highlights the whole card. A code card contributes the parser's code payload, a
table card its tab-separated cells with the header row, and a source card its raw source. An
open table (see [Markdown tables](#markdown-tables)) is literal prose, not a card.

Copy joins the readable text of the selected units: LF between logical lines, one blank line
between top-level units, a single separator where list structure requires it, and no rules,
borders, status text, or ellipses. Golden fixtures define this projection. Exact source bytes
come from Source or the source inspector, whose selections map back to SourceStore byte
ranges. Never reconstruct exact copy from rendered text, sanitized lines, or screen cells.

### Identity and anchors

Block and object IDs are opaque within a document epoch. Match an item across revisions by
source start, structural role, and enclosing source identity. Equal text is never identity.
An ID survives while its construct grows. If the construct changes kind, retire the ID and
keep its source position. Never recycle a retired ID within an epoch.

A reference link becomes an object only when its definition is admitted. Definitions arrive
in source order and the first definition of a label wins, so a resolved destination never
changes within an epoch. Missing table cells have keys and empty values, not invented source
spans. Reference definitions and other unpainted syntax stay reachable through the source
inspector.

A reading anchor holds an epoch, a reading-unit identity with its source start, an offset into
the unit's readable text, and the desired viewport row. Capture it from the first useful
visible content, ignoring borders and padding. Restore it against the newly displayed
snapshot: the same unit first, then the unit whose source range contains the start, then the
nearest following unit, then the nearest preceding one, clamped to the document.

Epoch replacement resets document reader state while retaining the selected view type and
presentation preferences. A host that wants continuity appends instead, after verifying the
old bytes are a prefix of the new ones. A persisted position needs a
host-checked document fingerprint and a source location. Internal IDs are not a persistence
format.

### Selection

A document selection is a pair of reading positions, each a reading-unit identity and an
offset into its readable text. Copy materializes the projection at that moment. A
publication remaps both endpoints by unit identity. If either endpoint's unit disappears, or
the readable text between the endpoints changes, the publication clears the selection and
shows a notice. A selection never silently copies text other than what is on screen.

During a drag, keep the anchor position and map each pointer event through the current
painted hit map. A user event may revise the selection. A publication may only remap or clear
it. If the drag's anchor becomes unrepresentable, cancel the gesture and release mouse capture.

## Reading and interaction

### Rendered and Source

Rendered is the initial view for Markdown. Show a labeled **Rendered / Source** control in
the fixed chrome, with the current choice visible at narrow widths too. Expose `set_view_type`
and `toggle_view_type` commands for host menus and key bindings. Use one admission predicate
for commands, control state, and help. fh adds this control to its terminal view menu; its
existing `F m` browser preview remains a separate action. Choosing a view never opens a browser.

Source is a first-class reading surface, not the frozen source inspector. It displays the
original Markdown syntax, supports soft wrap or horizontal scrolling, optional absolute line
numbers, read-only selection, copy, search, and line jump. Line numbers, status labels, and
terminal-safe representations of control characters are decoration and never enter a copy.
Use a source-byte-to-display map for those representations and for CRLF. A selection copies
its exact accepted byte interval; Copy Source copies the whole displayed source revision,
including its final newline, subject to the common output limit.

MarkdownSource composes Editor's view interaction and shared highlighting. It needs a
state-preserving source publication adapter with byte-addressed anchors and selections.
Append preserves the existing prefix, selection, and paused position; replace starts a new
epoch. Prepare line indexing and expensive wrapping off the input path, and enforce the
syntax cap before Editor can prepare a whole-buffer highlighter during render. Qualify this
path in Stage 1. If Editor requires a generic preserving-update operation, add that narrow
operation in `canopy-widgets`; do not create another text editor inside Markdown.

The two surfaces share accepted source, lifecycle, action dispatch, and follow state. They
retain independent geometry, selection, wrap settings, and search results; Rendered also
retains Scroll/Interact and its object cursor. Only the active surface takes input or owns a
scrollbar. A hidden source surface can prepare lazily and holds no second mutable document.
It may display accepted source even when Markdown parsing has failed.

Switch behavior is part of the public contract:

| State | Required result |
| --- | --- |
| Paused, no navigation between toggles | Return to the original view's exact anchor, including horizontal offset where meaningful |
| Paused, reader navigated in the outgoing view | Map its current source location to the incoming view at the same viewport row where possible |
| Following | Remain Following and reveal the incoming view's tail; switching alone neither pauses nor resumes |
| Text selected | Retain each view's selection independently; never reinterpret a rendered selection as exact source or copy the hidden selection |
| Search prompt active | Keep query, matching options, and prompt focus; recompute results for the incoming representation without jumping |
| Inspector or drag active | Disable switching until dismissal or gesture completion; direct commands return an unavailable result without a queued switch |
| Incoming presentation pending | Keep the outgoing surface usable with a switching indication; commit only when the incoming geometry is ready |
| Document replaced | Cancel any pending switch, reset document positions and selections, and retain the host's view preference |

Use an epoch and source-byte location as the bridge. Rendered maps an exact location when its
reading unit provides one, otherwise the unit's source start. Source maps that location to a
line and character position. In the opposite direction choose the containing rendered unit,
then the nearest following one, then the preceding one. Hidden definitions and discarded
table cells therefore have a predictable fallback. Do not promise equal rows or exact
character placement through entities, collapsed whitespace, or omitted card content. Retain
the original anchor for a toggle round trip until navigation supersedes it; resize and
publication reconcile it by the normal rules.

A switch never displays an older source revision than the outgoing surface, even though
Source may be ahead of Rendered while parsing is pending. Track a switch generation; capture
the outgoing anchor at the actual switch, after any intervening input. A second toggle
cancels a pending switch. Replacement, later view requests, and newer navigation invalidate
stale restoration work. Follow the displayed revision for selection, copy, and search, and
expose both accepted and displayed revisions to automation. A failed target preparation keeps
the current surface and reports the failure; an explicit Source request remains available
after a rendered parse failure.

While a switch is pending, pin the outgoing displayed revision at the request's revision,
keeping navigation and selection live. Accepted input continues with an Updating indication.
This gives the target a fixed minimum revision to reach: a continuously updating Source
surface must not make Rendered chase a moving target forever. Discard and acknowledge
superseded outgoing work without blocking its lane. After completion or cancellation,
schedule the latest accepted source for the active surface.

Changing view type preserves the focused region: a focused document transfers input to its
new surface; an unfocused fh preview does not take focus from the file listing. Show search
scope as Source or Rendered because a link destination or delimiter can exist in only one
projection. Results, counts, and current match identities never cross projections unchanged.

### Presentation

Use a quiet document ground, a small left inset, and an 88-cell default prose width cap.
Keep prose left aligned. Tables and code cards may use the full body width. Reserve the
scrollbar gutter from the start so its appearance cannot change wrapping. The status strip
and search prompt sit outside the document canvas.

Headings are ordinary terminal text. H1 and H2 get strong roles, spacing, and short rules.
H3 through H6 keep visible level distinctions through labels or attributes. Lists use hanging
indentation, `•`, `◦`, and `▪` for nested bullets, and the parser's ordered numbering. Task
boxes are read-only content. Quote rails, restrained block spacing, and a distinct inline code
ground establish hierarchy without consuming width.

Honor hard breaks and wrap soft breaks as prose. Images show alt text and destination without
fetching. Raw HTML and metadata become labeled source cards. Inline HTML stays literal.
Unknown extensions stay text or code. No source control character or escape sequence reaches
the terminal as an executable sequence.

Provide Unicode and ASCII glyph presets. Measure with Canopy's shared grapheme width and tab
rules. A glyph wider than the available region must still make layout progress. Test width
zero, width one, combining sequences, CJK, emoji, tabs, and clipped wide graphemes.

Register palette roles under `markdown` for hierarchy, links, selection, search, provisional
content, cards, and status. Table roles belong to the shared table renderer. Resolve colors
while painting, without reparsing or reflowing. Glyph or spacing changes need new geometry.
Fill a code card's whole body ground, including empty cells at line ends.

```text
  Deployment notes
  ━━━━━━━━━━━━━━━━

  • Review the migration guide ↗
  • Run the checks before release.

  ┌ Table · 243 rows × 6 columns ────────────────────────────┐
  │ Service       Region        Status          Latency     │
  │ api           auckland      healthy             24 ms   │
  │ jobs          sydney        healthy             39 ms   │
  │ … 241 more rows · 2 more columns                        │
  └ Inspect table                                          ┘

  ┌ Rust · 34 lines ────────────────────────────────────────┐
  │ fn main() {                                            │
  │     run_service();                                     │
  │ }                                                      │
  └ … 31 more lines · Inspect code                         ┘

  Next, **verify the                      [Receiving]

  Scroll · Receiving · Paused · New content       i Interact
```

The sketch fixes hierarchy and abbreviation. Exact decoration needs gallery review. Counts
describe real data rows, columns, or code lines. Omitted columns and clipped cell values are
different conditions with different indicators.

### Modes and the object cursor

**Scroll** is the default. Arrows, `hjkl`, page keys, wheel, and the scrollbar move the
document. Cards have no independent scroll positions. **Interact** adds a logical object
cursor over links, table cards, code cards, source cards, and footnote references. Headings
live in the outline rather than adding a stop per heading.

The surface stays the Canopy focus owner while the object cursor moves. Objects are model
entries, not nodes, and exist only outside the provisional region, so a code block or table
is inspectable only once it is complete. Inspectors are real modal subtrees with their own
focus. Mode, follow, object cursor, text selection, and open inspector are independent state.

**An inspector's content is fixed while it is open.** It shows its object as of the
publication at which it opened. If a later publication removes that object or changes its
kind, the inspector closes with a notice. The one exception is a table whose cell text
changes under a resolved reference, which republishes through TableView's keyed `publish`.

Bindings are semantic intents the host can override:

| Context | Input | Behavior |
| --- | --- | --- |
| Scroll | `i` | Enter Interact at the remembered visible object, otherwise the first visible one |
| Interact | Up/Down, `k/j` | Move to the previous/next object and reveal it |
| Interact | Tab/Shift+Tab | Move between objects, leaving through normal focus traversal at the boundary |
| Interact | Enter | Inspect the selected object |
| Interact | `v` | Start or extend text selection |
| Interact | Escape | End selection first, otherwise return to Scroll |
| Either | Page keys, wheel | Scroll without moving the object cursor |
| Either | `/`, `n`, `N` | Search, then visit retained results outside the prompt |
| Either | Outline, Inspect Source | Open the corresponding inspector |
| Either | Toggle View | Switch between Rendered and Source |
| Either | Follow | Go to the tail and resume follow |
| Interact | Copy | Copy the selection, otherwise the object's default payload |

If no object is visible, choose the nearest following object, then the preceding one. A
document with no objects still permits text selection and source inspection in Interact.

Text selection starts at the current reading anchor or the selected object's boundary. While
it is active, Left/Right move the endpoint by grapheme and Up/Down by displayed row with a
preferred column. Escape ends selection and restores the object cursor. The source and code
inspectors use Editor's own selection.

Directional object movement is consumed at the first and last objects. Tab leaves the surface
at a boundary through an explicit focus operation or a declined binding.

A single click selects an object and enters Interact. A second activation or double-click
opens its inspector. No click launches a browser. A text drag starts selection. Scrollbar
dragging only scrolls. Pointer mapping uses the presentation actually painted.

Prompts own text input and expose accurate key hints. Escape closes the innermost prompt or
cell detail before the inspector. Tab inside a table inspector visits cells and stays inside.
A modal's wheel boundary never scrolls the document behind it. Drag capture ends on release,
Escape, dismissal, loss of admission, or removal, and never transfers to another object.

### Follow

Follow has two states, **Following** and **Paused**. Static documents start Paused. Streams
start Following unless configured otherwise.

Pausing: selection, object navigation, search navigation, inspector entry, or a scroll that
moves the offset away from the tail. Entering Interact alone, a blocked key, resize, and theme
changes do not pause. Reaching the bottom by ordinary scrolling does not resume. Only the
Follow action does. Closing an inspector never resumes.

While Following, each publication reveals the last readable row. While Paused, publications
preserve the reading anchor. Handle wheel and owned
scrollbar events through this one policy, comparing actual offsets before and after.

The new-content indicator counts accepted bytes since the last acknowledged tail, not rendered
rows. Acknowledge only bytes actually displayed at the active surface's tail; switching to an
Updating surface cannot acknowledge undisplayed bytes. Clearing the indicator discards nothing.

## Streaming

### Lifecycle

A document is **Receiving** or **Complete**. Every accepted append and the finish transition
advance the document revision. An empty append is a no-op. Appends accept all supplied bytes
or return an error without mutation. The accepted revision is distinct from the displayed one.

| Current state | Operation | Result |
| --- | --- | --- |
| Receiving | Append | Accept bytes, advance revision, schedule publication |
| Receiving | Finish | Complete, advance revision, parse with ordinary EOF rules |
| Complete | Append | Reject without mutation |
| Complete | Finish | No-op |
| Any | Replace | New epoch, explicitly static or receiving |

Finish carries an outcome: completed, or aborted with a reason. Both apply ordinary EOF rules,
so an unclosed fence becomes EOF-terminated code and unmatched inline syntax stays literal.
Finish inserts no missing delimiter. The status strip shows the outcome and reason.

A static constructor starts Complete. An empty stream starts Receiving. Show `Finishing`
while the producer has finished but the final revision has not published. A hidden pane or
dismissed inspector does not cancel the producer. Transport lifetime and message provenance
belong to the host.

### The provisional region

CommonMark lets EOF terminate fences, and lets later definitions change earlier content.
Those rules describe a complete document, not certainty at an arbitrary token boundary. See
the [CommonMark specification](https://spec.commonmark.org/0.31.2/).

Parse the whole accepted source normally to discover structure. While Receiving, the
**provisional region** is the trailing leaf block whose closure still depends on EOF, plus
its open container ancestors, such as the list item and quote that contain it. An explicit
closing boundary can leave no provisional leaf. A reference definition takes effect only
once its block is outside the provisional region.

Determine openness from source and block structure, not just `event.range.end == source.len()`:
blank lines and reference definitions need not produce visible events, and a fence's closing
line can terminate the block before Finish. Keep a compact closure/admission record alongside
parser events. Stage 1 must demonstrate it with actual parser ranges, including trailing
blank lines and invisible definitions. Mask inadmissible definitions with offset-preserving
whitespace for semantic resolution, or use an equivalent bounded pass; merely hiding their
own presentation does not prevent the parser from resolving earlier links. Both passes use
the same immutable revision, and retain the unmodified source for inspection and copy.

| Provisional block | Presentation |
| --- | --- |
| Paragraph or heading | Parsed text with a mutable-tail indication |
| Fenced code with a complete opener line | Code card with receiving status |
| Fenced code with an incomplete opener line | Literal source |
| Table, HTML, metadata, reference definition | Literal source |
| Open list item or quote | Its container decoration, with the leaf rendered by this table |

The provisional region contains no objects. Links inside it are text, bare URLs have no Open
action, and tables have no card. Everything before the provisional region is interpreted
normally and may still change on a later publication, for example when a definition
resolves, a setext underline arrives, or list tightness flips. Those changes republish like
any other and go through the anchor and selection rules. Inline flicker, such as `**bold`
later closing, is accepted.

Every byte of the displayed revision is in an interpreted block, unpainted syntax, or the
provisional region. Bytes accepted since that revision are not displayed until the next
publication. One publication explains each transition.

A trailing CR may gain its LF on the next append. Keep bytes exact and let the parser see the
whole source each time.

## Tables

### Data, preparation, view

`canopy_widgets::TableView` is a read-only spreadsheet viewer for application data. Its
`table` module has three concepts:

1. **TableData:** an immutable validated snapshot with dataset identity, revision, ordered
   columns, and ordered rows. Keys are unique per axis. Each row has one cell per column. A
   builder accepts column labels and keyed plain rows.
2. **PreparedTable:** TableData plus geometry, prepared for explicit column widths, glyphs, and
   presentation options. It owns or shares exactly the data it measured. Preparation, visible
   painting, and hit testing work without TableView.
3. **TableView:** cursor, selection, viewport, input, and events around a PreparedTable. It
   can be interactive or display-only, in a pane or a modal.

Row and column keys exist so that cursor and selection survive a republication and so that
later spreadsheet behavior, such as sorting, filtering, and live updates, has stable identity
to build on. The first version uses them only for survival across `publish`.

Cells hold full text and optional style spans on validated UTF-8 boundaries. Labels may
repeat or be empty. Keys may not. A cell may contain tabs and newlines. The grid paints a
one-line representation. Detail and copy expose the full value. Style spans change no text
and encode no commands, URLs, source spans, or widgets.

The shared renderer measures and paints headers, cell text, alignment, separators, clipping,
selection, and hit regions. Hit testing derives a cell from column prefix sums and row
geometry. There is no per-cell rectangle. Paint only visible cells. Body rows are
fixed-height and single-line.

Markdown builds TableData and paints inline previews with PreparedTable directly. Its
inspector mounts TableView on the same engine. Preview and inspector may differ in widths and
row windows, but equal data and options produce equal glyphs and hit boundaries. There is no
second grid renderer.

TableView emits a **cell activation** carrying dataset identity, revision, row key, and column
key. A standalone host opens a plain value panel. Markdown shows formatted text, links, and
the cell's source. Markdown owns table source ranges, completion policy, card labels, link
interpretation, and cell-detail content. Defer inline action-region tokens until a
non-Markdown consumer needs them.

### Geometry and navigation

A TableView is a small composite: fixed header, row-number gutter, two scrollbars, and one
scrollable body node. Header and gutter paint from the body's offsets and sit outside its
canvas. The body owns the only scroll position. The shell owns its scrollbar tracks.

Arrows or `hjkl` move the cell cursor. Page keys move it by the visible body height. Home/End
select the first/last cell of the row. First/last-row actions and a coordinate prompt provide
larger jumps. Tab traverses row-major order. Movement reveals the cursor. Wheel and scrollbar
move the viewport without relocating it.

Column widths come from deterministic bounded sampling of the label and an initial row
prefix, with sample and byte limits in options, computed once per PreparedTable. Never scan
every cell on a navigation event. Apply minimum widths, declared alignment, and a
preferred-width cap.

At small sizes, drop decoration before the current cell. If no usable grid fits, show the
selected cell's coordinates, label, and clipped value with the same navigation. With no rows
or no columns the cursor is `None` and movement is a consumed no-op. A header-only table can
still copy its labels.

### Selection and copying

Shift movement and dragging form a rectangle. Select-row, select-column, and select-all
capture the current keys. A completed selection is the Cartesian product of explicit row-key
and column-key membership, represented as shared immutable key slices or sets. Select-all
allocates no per-cell records.

Copy Cell returns the full value. Copy Selection and Copy Rows emit tab-separated fields:
fields containing tabs, newlines, or quotes are quoted with quotes doubled, rows end in LF
with no trailing separator, and empty fields stay empty. Copy All includes labels once. Copy
Selection includes body cells only. Header inclusion is an explicit option. Glyphs, clipping
markers, padding, and row numbers never enter a payload.

TableView does not open a clipboard. It serializes from the displayed data and selection and
emits one complete payload. Large copies run on the host's worker. An action over its
configured output limit fails visibly. Nothing is truncated silently.

### Table publication

Publish one PreparedTable, never separate data and layout mutations. It carries dataset
identity, data revision, and geometry key, so new values cannot land under old hit regions. A
publication may advance data or change geometry at the same revision. It never rolls back a
displayed revision.

On `publish` with the same dataset identity, the cursor keeps its cell if both keys survive,
and otherwise clamps by index per axis. Selection membership intersects with surviving keys
and clears if either axis empties. The top row keeps its key if it survives. A different
dataset identity resets cursor, selection, and viewport. Nothing else is reconciled in the
first version. Live-updating datasets, drags across updates, and value-change marking are
specified with the spreadsheet extensions.

Small tables prepare synchronously under a documented bound. Hosts prepare larger snapshots
off the input path and publish through Context. Markdown uses its worker. TableView has no
worker of its own.

### Markdown tables

Use parser table structure and inline grammar. Never split on pipes. Per the
[GFM table specification](https://github.github.com/gfm/#tables-extension-), rows may omit
cells and excess cells are ignored. Keep ignored source for inspection with a card warning.
Header and delimiter counts must match.

The inline card shows the header and up to eight body rows, indicating omitted rows, omitted
columns, and clipped values. At narrow widths it shows a summary and labeled values. The
preview has no wheel behavior of its own.

**A table renders only once it leaves the provisional region.** While it is the trailing
block, its whole source is literal Markdown at its position: pipes, delimiters, line breaks,
and inline syntax preserved. It is selectable, searchable, and in the source inspector. It has
no object, no inspector, and no live links. Never combine a grid prefix with an unfinished
row.

Once a following block exists or Finish arrives, adapt the whole table to TableData, prepare
the preview, and publish grid and inspector action together through the normal publication
path. The reading anchor, focus, and follow state are preserved by the publication rules. A
selection over the literal source clears with a notice, as any reinterpreted selection does.
Rendering the table never selects it, opens it, or resumes follow.

Body-row keys come from source position and column keys from header position. A completed
table never gains rows, so Markdown needs no growing-table reconciliation. A later reference
definition can change a cell's value under its existing key. That republishes the table at
the same dataset identity, and cursor and selection survive by key.

## Code

### Code cards

A code card has a distinct body ground, a language label, a line count, and a state. Show at
most six unwrapped lines by default, marking clipped columns and omitted lines. Unknown
languages are plain text. Resolve grammars with `Grammar::for_fence`, including Luau and
aliases, and keep the full info string for inspection.

Count content lines, not Editor's synthetic caret row: an empty payload has zero lines, and a
final newline terminates the last line without adding one. Copy Code returns the parser's
payload with its whitespace and final newline. Container indentation removal and line-ending
normalization mean the payload can differ from the source bytes, which the source inspector
shows.

Editor `TextPosition` columns are character indices, source ranges are UTF-8 bytes, and
screen columns are cells. Convert at the adapter.

### The code inspector

The inspector opens only on a complete code block, so its payload is fixed. It composes
MarkdownSource over `Editor` in `Interaction::View`, unwrapped, with absolute line numbers,
and publishes an immutable prepared view. It supports two-axis scrolling, visual selection,
Copy Selection, Copy Block, search,
and line jump. No execution. Wrap toggle arrives once its cost is qualified. Reopening starts
at the top.

Prepare inspector indexing, wrapping, and built-in highlighting on the shared worker before
publication. Above the syntax cap the inspector opens as plain text with a label. Card
previews highlight only the lines they show, up to the same cap. Prepared results retain
code identity, payload revision, language, and geometry. Syntax spans carry palette roles
and font attributes, so changing the theme recolors the current view without parsing or
wrapping it again. A stale result never recolors a different payload.

The source inspector follows the same rule: it pins the source revision that the active
surface displays when it opens and does not update while open. Reuse MarkdownSource's byte
mapping and exact-copy adapter in this fixed snapshot. Label the pinned revision, and keep
**Inspect Source** distinct from **View Source**, which switches the live document surface.
After a parse failure, View Source can expose newer accepted bytes that the rendered snapshot
has not yet displayed.

## Links and host actions

Link labels use an underline or palette role. Selection shows the destination in the status
strip. Enter opens an inspector with the full label, original and resolved destinations, and
explicit Copy URL and Open actions. Opening never follows from selection, focus, reflow,
reference resolution, or completion of streamed syntax.

Recognize parser links and autolinks. Detect bare HTTP/HTTPS URLs only in ordinary text, with
tests for balanced parentheses and trailing punctuation. Exclude code, HTML, metadata,
existing links, images, and the provisional region.

Fragment-only links navigate within the document using one documented heading-slug algorithm
with source-order duplicate disambiguation. Heading keys stay independent of slugs. Relative
links need a host-supplied base URL or resolver. Never infer one from the working directory.
By default only resolved HTTP/HTTPS may open. Other schemes and local paths need an explicit
host handler.

Represent URLs structurally, reject control characters for opening, and show escaped forms in
inspection. Never interpolate a URL into a shell command.

The widget emits `OpenUrl` and `CopyText` requests with unique IDs and immutable payloads,
tagged with epoch, displayed revision, and object identity for diagnostics only. The host
drains a bounded queue, performs each action once, and replies with success, rejection, or
failure. Duplicate replies are ignored. A late reply for a replaced document shows nothing on
the replacement.

Browser and clipboard capabilities are configured explicitly. Admission, contextual help, and
handling share one predicate. Queue pressure produces a visible failed action, never a claimed
copy. A request-ready posted command can notify a host controller after callbacks return.
External actions never run during parsing, layout, or paint.

## Background work and publication

### Work ownership

A small executor is shared by the application's Markdown instances. The host creates its
handle once and passes clones through MarkdownOptions, which bounds aggregate workers and
makes shutdown explicit. It is a concrete Markdown work service with a controlled test
scheduler, not a general task runtime. Removal never joins a worker synchronously.

Each document has at most one running job, one latest desired request, and one completed
result awaiting disposition. The executor takes ready documents fairly. A rejected enqueue
leaves a dirty request with its owner for a later wake. The latest desired state covers
replaceable parse, per-view geometry, search, and preview-highlight work. A Source request
can prepare directly from SourceStore without a successful parse; prioritize the requested
surface and coalesce hidden-view work. Copy jobs occupy separately
bounded FIFO slots and are never coalesced away by a newer source request.

A job holds an immutable source snapshot, parser options, the previously published semantic
base, and the requested geometry. It returns owned data, not UI commands. Source chunks are
shared. An admitted parse snapshot is flattened only when the parser requires contiguous
input. Whole parsing and flattening are O(source bytes).

Results keep structural sharing where possible. Every job handshake completes, including
rejected results, so a stale width cannot leave a lane waiting. The next job builds on the
latest actually published base. IDs come from a monotonic allocator per epoch, so rejected
work never collides with published work.

Coalesce small updates at a tunable 33 ms interval. Finish schedules the final revision
without another producer event. New input supersedes queued work, not the running job.
Cooperative cancellation happens between passes. The parser itself is not interruptible, so
bound its input.

### Presentation publication

Each presentation carries `(epoch, source_revision, view_type, geometry_key)`. Geometry keys
include allocated body width, glyph preset, and layout options, not colors. Source options
also include wrap and line-number settings. Search work additionally carries the view type
and query generation. A job's revision captures lifecycle, even when no byte changed. The
following steps describe Rendered; Source applies the same revision, geometry, and current
reader-state checks to its line layout. A view switch commits only a prepared target surface.

1. Accept source on the UI thread, advance the revision, update the new-content count, and
   schedule or coalesce work. Until a result arrives, show the last published presentation
   with an Updating indication. Before the first publication, show only the status strip.
2. Reject a result from another epoch, with obsolete parser options, with a revision older
   than the displayed one, or with a geometry key that differs from the current allocation.
   After a rejection, schedule the latest source and geometry. A newer source revision may
   publish even if more bytes have arrived since.
3. Capture the reader's current anchor, selection, and object cursor immediately before
   publication and reconcile them against the result. Never restore state from job submission
   time.
4. Swap semantic model, geometry, reading map, and hit map atomically. Close an inspector
   whose object disappeared or changed kind. Republish a table inspector whose data changed.
5. Apply follow or anchor preservation against the painted geometry. If no mapped target or
   usable viewport exists, keep the anchor pending locally with a navigation generation. New
   user navigation supersedes it. A later publication captures the newer position.
6. Acknowledge the result, release superseded work, and schedule the latest dirty source or
   geometry.

During a resize, the display keeps its old geometry and hit map until a result at the new
width publishes. Resize coalesces on the same tick as appends, so a drag delays publication
by at most one interval after it pauses. A completed final parse must eventually replace
Receiving and Finishing presentation even when bytes stopped changing.

Reader-state changes never invalidate parsing. Object cursor, selection, and viewport are
paint overlays and publication-time inputs, which is why a job never carries them.

### Layout and bounds

A block-height index gives logarithmic viewport lookup. Paint visible blocks plus small
overscan. Reuse prepared layouts for unchanged blocks at an unchanged width. There is no node
per paragraph, link, row, or cell. `measure` and `canvas` read prepared sizes. The reader
fills its allocation rather than measuring intrinsically.

Observe actual body width after allocation and gutter reservation. Render may record a new
width request and wake `poll`, where mutable work runs. Suppress duplicate width wakes.

Initial configurable bounds: 16 MiB accepted source, 100,000 semantic nodes, 128 container
levels, 100,000 cells per rich table, 256 KiB automatic syntax work per document, and 32 MiB
derived caches per document. Oversized rich regions become labeled source cards. Enforce
adapter limits during construction. The parser allocates before events are consumed. Source
limits and worker isolation bound that but do not sandbox it. Account separately for source,
flattened input, models, jobs, inspector, requests, and executor queues. The cache limit is
not a limit on the total.

Bound action count and payload bytes, source-query size, and search result storage.
Distinguish an exact count from a limited window. Input limits reject before acceptance;
derived-work limits degrade presentation or report incomplete coverage explicitly.
Accepted source is never dropped. A producer channel, if offered, applies backpressure before
acceptance and preserves order.

A worker failure keeps the last good snapshot, shows the failure in the status strip, and
offers retry. Accepted source remains available through the live Source surface, independent
of parse success. Malformed Markdown is ordinary input, not a failure. Epoch replacement
invalidates the old document's jobs and requests. A delivered host action may already be
running. Replacement does not undo it.

## Inspectors and lifecycle

`Markdown` owns sibling rendered and source surfaces, a fixed status strip with the view
control, one optional search row, and inspector state. Mount Source lazily and keep bounded
per-view state. MarkdownSource can also mount independently with its own host-provided input.
Create an inspector overlay lazily and remove it after dismissal, keeping bounded plain state.
Use `add_modal` and `open_modal` with the Markdown instance as owner and explicit inspector
binding groups. Overlays bound their own routes. Nothing bubbles from a root sibling into the
owner. Callbacks use exact posted commands or an owned mailbox.

Dismissal uses normal deferred removal outside lifecycle hooks. Owner removal or replacement
retires the scope automatically, but the overlay still needs reclamation. A small private
mechanism wakes the overlay from the owner's unmount. On its next poll the overlay checks an
incarnation-aware owner wake handle and, if the owner expired, queues its own removal. If a
structural rollback kept the owner alive, the overlay stays. Prove this path before adding
inspector types.

Owner unmount signals cancellation without joining workers or clearing retained source and
reader state. If rollback keeps the owner alive, its next wake reschedules canceled work.
Node-lifetime wakes distinguish hide and detach from removal. Stale results cannot target a
replacement widget at a reused NodeId. Hidden readers may publish less often but keep their
source. Avoid polling timers when no work is pending.

Closing an inspector, by the user or by the fixed-content rule, restores document focus and
the current reconciled anchor, with the object cursor on the object or at its source position.
Append never calls `set_focus`, opens a modal, or replaces the active surface node. Only an
explicit view switch transfers focus between already prepared surfaces.

Register commands, intents, descriptions, and framework groups during setup, before script
API finalization. `accepts_intent`, `on_intent`, `key_outcome`, and contextual help describe
the same state. In Scroll, handle document navigation explicitly through Context scrolling,
declining at a boundary so a parent may scroll. In Interact, consume object movement,
including the no-op at either end. Inspector scroll admission stops at the modal.

## Search, outline, and automation

By default, search covers the readable text of every reading unit, including omitted card
content, as literal text with a case option. It indexes units independently, so a match never
spans cells or unrelated blocks. Code and source searches match line content and report
line-addressed results. The first version rejects multi-line queries with a clear message.

Source searches raw source lines, including delimiters, definitions, and link destinations.
Rendered searches its readable projection. Both accept a shared query value and explicit
matching mode. Literal is the standalone default. fh selects regex with smart case using
[the existing matcher](../crates/canopy-widgets/src/regex_search.rs) in both views, preserving
content-find query semantics. Compile once per query generation and run bounded matching over
the active projection; do not run the current unbounded collecting helper on the input path.
Invalid regex retains the query and reports its error without moving the reader. Ignore
zero-width matches consistently with fh. Switching reindexes and clears the old match
cursor; only explicit next/previous or activation reveals a result.

Search runs over the displayed revision and reruns on each publication. The current result
survives if its unit and offset do. Search work carries epoch, source revision, and query
generation. An old result cannot overwrite a newer query. Editing the query does not move the
reader. Next, previous, and activation do.

A table in the provisional region is searched as literal Markdown, and a result there
navigates to the text. A completed table has cell-addressed results. A code or table match
selects the containing card, and activation opens its inspector at that location. A
source-card match opens the source inspector. The outline lists level, text, and heading
identity, keeping its filter and keyed selection as headings arrive. Fragment jumps and
footnotes offer one return anchor.

Node semantics are concise: role, label, active key, selection presence, and action status.
Bounded explicit queries expose active and requested view type, rendered mode, accepted and
per-view displayed revision, lifecycle, follow,
anchor, objects, outline, search coverage, and inspector state. Collection queries return a
revision with their window so callers can detect a changed snapshot between pages. The
semantic value never carries the whole source or a node per cell.

Tests use deterministic producer injection, fake scheduling, and controlled worker
completion. No correctness test needs a network, a clipboard, a browser, or a wall-clock
sleep.

## Public API

Export Markdown, MarkdownSource, and TableView at their crate roots, with domain types in
public modules. AST events,
source storage, worker channels, and reconciliation caches stay private. These sketches
describe operations, not compile-checked signatures.

```rust
// canopy_markdown::Markdown
pub fn new(source: String, options: MarkdownOptions) -> Result<Self>;
pub fn streaming(options: MarkdownOptions) -> Result<Self>;
pub fn append(&mut self, ctx: &mut dyn Context, text: &str) -> Result<Revision>;
pub fn finish(&mut self, ctx: &mut dyn Context, outcome: FinishOutcome) -> Result<Revision>;
pub fn replace(&mut self, ctx: &mut dyn Context, source: DocumentInput) -> Result<DocumentEpoch>;
pub fn view_type(&self) -> ViewType;
pub fn set_view_type(&mut self, ctx: &mut dyn Context, view: ViewType) -> Result<()>;
pub fn toggle_view_type(&mut self, ctx: &mut dyn Context) -> Result<()>;
pub fn set_mode(&mut self, ctx: &mut dyn Context, mode: Mode) -> Result<()>;
pub fn follow(&mut self, ctx: &mut dyn Context) -> Result<()>;
pub fn position(&self) -> ReadingPosition;
pub fn restore_position(&mut self, ctx: &mut dyn Context, position: ReadingPosition) -> Result<()>;
pub fn take_requests(&mut self, limit: usize) -> Vec<HostRequest>;
pub fn complete_request(&mut self, ctx: &mut dyn Context, reply: HostReply) -> Result<()>;

// canopy_widgets::table: pure preparation; plain cells are the easy path.
pub fn prepare(data: Arc<TableData>, options: TableLayoutOptions) -> Result<PreparedTable>;

// canopy_widgets::TableView: one data/geometry publication operation.
pub fn new(table: Arc<PreparedTable>, options: TableViewOptions) -> Self;
pub fn publish(&mut self, ctx: &mut dyn Context, table: Arc<PreparedTable>) -> Result<()>;
pub fn cursor(&self) -> Option<CellKey>;
pub fn set_cursor(&mut self, ctx: &mut dyn Context, cell: CellKey) -> Result<()>;
pub fn selection(&self) -> &TableSelection;
pub fn take_events(&mut self, limit: usize) -> Vec<TableEvent>;
```

`FinishOutcome` is completed or aborted with a reason. `DocumentInput` makes Complete versus
Receiving replacement explicit. Read-only accessors and bounded queries supplement these.
`ViewType::{Rendered, Source}` is independent of `Mode::{Scroll, Interact}`; mode applies only
to Rendered. View commands may initiate preparation; query the active and requested view to
distinguish completion. ReadingPosition identifies its originating view and carries the
common source-location fallback; a rendered unit ID alone cannot restore Source.
MarkdownSource needs standalone construction, bounded source
publication, source-position access, and the same copy/search contract, without exposing
SourceStore internals. Let its standalone gym and fh adapter determine those signatures.
Copy may complete after background serialization, so there is no synchronous unbounded
`selection_text() -> String` on the input path. Expose the selection's positions or a bounded
helper instead.

Table events use general cell and copy concepts, independent of HostRequest. PreparedTable
paints a clipped grid and answers hits without a mounted widget. Let the tablegym and the
Markdown preview together drive its final signatures.

## Delivery

### Stage 1: the contracts

Build the provisional-region rule, reading units, source ranges, lifecycle transitions, and
the controlled scheduler harness, with plain test renderings. Establish reference resolution
timing, whole-table publication, code-payload normalization, and selection remapping.
Prototype one modal open, dismiss, and owner-removal cycle against real lifecycle semantics.
Prototype MarkdownSource's preserving append, exact-byte copy, and two-way source anchors
against Editor. Establish independent expected outputs before using static/streamed
comparisons, and exercise literal as well as fh-style regex search.

Gate: every accepted byte has a defined owner. Chunked completion matches static parsing.
The provisional region contains no objects. Removed owners leak no overlays. Source selection
and paused position survive append without a full `set_text` reset; CRLF and trailing-newline
copy are exact. A source/rendered round trip has a tested position rule.

### Stage 2: first usable slice

Build TableData, PreparedTable, TableView, and a tablegym: fixed headers, single-line cells,
two-axis navigation, rectangle selection, activation, and TSV copy. Build the Markdown surface
with streamed appends, anchors, Scroll and Interact, completed-table previews, and link,
code, and table inspectors. Add MarkdownSource as a usable standalone widget and as the
sibling Source surface, the visible toggle, the pinned source inspector, immutable host
requests, and a markdowngym with append, finish, abort, replace, and resize controls. Add
focused harness tests and tracked gym scenarios with this slice rather than at release time.

Gate: a user reads a streamed document, pauses, selects a table rectangle, inspects and copies
code, and inspects and opens a URL through a recording host, with position and focus preserved
through appends. Trailing tables show readable Markdown and no grid. Completing one publishes
the grid without losing the anchor or firing an action. The same document switches to source
and back while receiving, paused, or following, without rereading input, changing epochs, or
stealing host focus. Source still works when a rendered parse fails.

### Stage 3: the complete reading contract

Add document text selection, search over hidden card content, outline, footnotes, and
relative links. Finish narrow-size behavior, the ASCII preset, accurate action availability,
and failure recovery. Complete the toggle, exact-copy, and search projection cases. Qualify
input, source updates, view switching, and publication costs before elaborating caches.

Gate: every library feature and invariant in this document passes. Limited content stays
inspectable. Copy is deterministic. fh integration is qualified in Stage 4.

### Stage 4: consumer and release

Integrate the shared Markdown reader into fh with Rendered as its initial Markdown view and
the Source/Rendered control in the view menu. Retain the choice for the pane during the
session, including across non-Markdown entries. Migrate the Markdown-specific Editor glue
to MarkdownSource while keeping file loading and browser policy in fh. Add the bounded raw
input path and preserve held regex search, focus, wrap/line-number controls, and full-screen
navigation. Show the effective Source fallback separately from the retained preference when
Rendered is unavailable. The browser server and `F m` remain in fh. A file reload is a new
epoch unless the host verifies the old bytes are a prefix of a receiving document; Complete
documents cannot acquire an append by implication.

Migrate affected Canopy consumers in the same change, without compatibility shims. Refresh
API captures in every changed workspace. Extend fh's tracked Luau smokes and gallery
coverage. Record performance and memory evidence on a named machine. Incremental parsing is
justified only if that evidence requires it, verified against the whole-parse oracle.

Gate: focused and repository-required checks pass. fh demonstrates the complete reader.
Performance and gallery receipts support the release.

### Later extensions

Wrapped rows, column resizing, a frozen data column, CSV export, a live inspector for the
trailing code block, section folding, and richer navigation each arrive as a separate bounded
change. Folding needs a policy for selection, search visibility, anchors, and new content in
hidden sections. A live code inspector may reuse Source's preserving text-update mechanism,
but still needs code-payload reconciliation and a separate inspector update contract.

Spreadsheet behavior in TableView builds on the keyed model: sorting and filtering need
explicit membership and copy order, width refitting needs a preparation request with
generations, live updates need rules for cursor survival, in-flight drags, and changed
selected values, and editing needs a host action contract. Specify each
with its first consumer rather than in advance. External renderers and editable task items
likewise need concrete host action contracts.

## Testing strategy and release evidence

Tests accompany each delivery stage. Put assertions at the cheapest layer that can prove
the behavior, then add a few complete user journeys through the real widget and host. Keep
independent expected outputs: a streamed parser agreeing with the static version can still
mean both are wrong. Likewise, a gallery image cannot establish which widget received a key.

### Ownership and test layers

| Layer | Owner and mechanism | Evidence it must provide |
| --- | --- | --- |
| Source, parser adapter, mappings | `canopy-markdown` pure fixtures and property tests | Exact bytes, expected structure, closure, copy, and source-position rules |
| Source widget and shared text changes | MarkdownSource tests plus focused Editor regressions | Preserving updates, wrap, selection, line mapping, and syntax limits |
| Grid model and renderer | `canopy-widgets` table tests without optional features | Validation, keys, geometry, hit testing, navigation, and TSV |
| Publication and concurrency | Markdown's controlled scheduler and recording host | Stale-result rejection, queue bounds, fairness, liveness, and immutable actions |
| Mounted interaction | Canopy Harness with real bindings and modal scopes | Focus, admission, pointer capture, scroll ownership, and lifecycle cleanup |
| Application journeys | markdowngym, tablegym, and fh Luau scripts | Public commands, default menus, view switching, and consumer compatibility |
| Appearance | Scripted Canopy/fh galleries and visual inspection | Legibility, clipping, state labels, contrast, and narrow layouts |
| Cost | Release benchmarks, counters, and bounded soak | Input latency, publication lag, work scaling, and retained memory |

Use [Harness](../crates/canopy/src/core/testing/harness.rs) and
[ManualClock](../crates/canopy/src/core/testing/clock.rs) for mounted deterministic tests.
Advance the clock and deliver runtime wakes explicitly. Add a private scheduler seam that
can start, hold, finish, fail, and reject jobs without replacing the production publication
logic. New widget correctness tests use no real network, clipboard, browser, watcher timing,
or sleeps.
Keep a small real-worker integration test for wake delivery, executor shutdown, and removal
during work; synchronize with barriers/channels and use a timeout only as a failure bound.

### Fixture corpus and independent oracles

Store small Markdown inputs with reviewed expected semantic blocks, source ranges, readable
copy, and exact source copy under the new crate's tests. Reuse these inputs in gyms and
gallery scenes where useful. Add a short fixture manifest naming the contract each case
proves. Large documents and tables are deterministic generators with recorded seeds, not
multi-megabyte checked-in golden files.

Pin parser options and fixture provenance. Select relevant examples from the linked
CommonMark and GFM specifications, plus fh's existing parser and highlighting tests. Upstream
HTML expectations establish syntax only; terminal block structure, readable text, omitted
content, and copy need separate expected values. Document intentional differences such as
footnote placement and bare-link handling. Do not regenerate expected outputs from the code
under test and accept them without inspection.

For short fixtures, test every two-part UTF-8-boundary split, one-code-point-at-a-time input,
and empty appends. This covers each split point, not every possible partition. Use seeded
multi-part partitions for larger inputs and promote failures to minimal fixed fixtures.
At each prefix, compare the presentation with that prefix's expected provisional behavior;
after Finish compare semantic content and copy with both the independent expectations and
a static parse. Compare opaque identities only within an epoch, not between independent runs.
The host decoder has separate tests for transport chunks split inside UTF-8 code points;
`append(&str)` itself never receives an invalid UTF-8 fragment.

The corpus covers:

- Empty input, no final newline, LF, CRLF split between chunks, bare CR, tabs, entities,
  escapes, combining sequences split across appends, CJK, emoji, and embedded terminal controls.
- All heading levels, setext reclassification, duplicate heading slugs, hard/soft breaks,
  nested lists and quotes, list tightness changes, task lists, alerts, and depth limits.
- Inline syntax across lines; indented and fenced code; empty payloads; incomplete opener
  lines; closed fences at receiving EOF; unknown languages; aliases including Luau; and
  highlighting just below, at, and above its cap.
- Reference definitions, including multiline titles, duplicates, missing labels, invisible
  trailing definitions, and blank lines after them. Assert that an unadmitted definition
  cannot change an earlier link's label, action, or destination.
- Raw HTML, inline HTML, metadata, image alt text, footnotes, fragments, relative links,
  balanced URL parentheses, trailing punctuation, and unsupported schemes.
- Tables with escaped pipes, inline code and links, unequal header/delimiter counts, empty
  bodies, short/excess rows, duplicate labels, and references resolved after table closure.

At each split inside an open table, assert literal syntax, no PreparedTable, and no table
action. Include a header plus many complete rows with no closing boundary, trailing blank
lines, a following block, and Finish with no new bytes. Check the exact publication where
literal text becomes a grid, including its selection-clear notice. Closure fixtures must
have explicit expected boundaries rather than deriving them from the implementation's own
classifier. If source ranges cannot support the proposed admission rule, revise that rule
in Stage 1 before building the interaction surface.

Mapping goldens cover many-to-one entities, removed indentation, synthetic missing cells,
ignored excess cells, normalized code payloads, and control-character display substitutions.
Assert copied bytes, not just screen text. Source ranges must be valid UTF-8 boundaries in
the same revision. Decoration has no source payload. Every accepted byte remains recoverable
even when parser event ranges overlap or omit unpainted syntax.

### Source/Rendered switching and fh migration

Exercise both views independently before exercising their shared controller. A standalone
MarkdownSource test must work without fh, a filesystem path, or a successful Markdown parse.
Use the following focused cases in the mounted harness, then a representative fh smoke:

| Scenario | Required assertions |
| --- | --- |
| Scroll mid-document, toggle twice | Same source neighborhood on first switch; original anchor restored on return without intervening navigation |
| Navigate in Source before returning | Rendered follows the new source location, using the documented hidden-syntax fallback |
| Resize or toggle wrap between switches | Logical position survives; screen row is clamped only when necessary |
| Follow a stream while toggling | Shared follow state and epoch stay unchanged; no unpainted bytes are acknowledged |
| Append while Paused in either view | Anchor and unchanged source selection survive; no focus transfer or full-buffer state reset |
| Select and copy in each view | Independent selections; source preserves CRLF/tabs/final newline; rendered copy uses readable projection |
| Search destinations or Markdown delimiters | Source finds them; Rendered counts only its projection; query and prompt focus survive the switch |
| fh regex query and invalid regex | Matching syntax and smart case survive; errors remain visible; no silent conversion to a literal query |
| Toggle while target preparation is held | Outgoing revision stays pinned and navigation stays usable; current position wins; a second toggle cancels |
| Replace while a switch or search is pending | Old result cannot activate a view, restore a position, or highlight the new file |
| Parse fails, then select Source | Accepted bytes remain readable and copyable; retry can later restore Rendered |
| Inspector or text/scrollbar drag active | Control, help, and direct-command admission agree; closing/releasing enables the toggle |
| Toggle an unfocused fh preview | Listing selection and focus stay put; a focused or full-screen view retains its region |

Add fh loader fixtures for empty Markdown, CRLF, final newline, invalid UTF-8, NUL/binary
input, unreadable/disappearing files, and sizes immediately below, at, and above 256 KiB.
Test the cap cutting a multibyte sequence. A partial view shows its retained range and
cannot activate Rendered; labels never enter the source or copy payload. File replacement
resets document state while holding the query and pane preference according to fh's policy.
Verify a Markdown → ordinary text → image/directory → Markdown sequence, held searches,
line numbers, wrap, full-screen exit, and the unchanged `F m` browser-preview command.

### Deterministic state-machine and resource tests

Drive explicit operations through a small reference state model: append, finish/abort,
replace, resize, navigate, select, toggle, query, inspect, dismiss, hide/detach, remove, and
deliver worker/action results. Use the existing workspace property-test approach for bounded
generated traces, with replayable seeds and operation logs. Assert invariants after each
operation, not just after the final frame:

- Accepted source is append-only within an epoch. Empty append and repeated Finish are
  no-ops. Append after Complete or beyond the source cap rejects without changing bytes,
  revision, or lifecycle. Completed and aborted Finish both receive final EOF interpretation.
- Each displayed surface has one coherent epoch/revision/geometry/hit map. Its revision
  never decreases. Switching never moves back to an older source revision. Stale parse,
  layout, highlight, search, and host replies cannot mutate the current document.
- Reader input after job submission wins at publication. A changed selection clears with
  exactly one notice; an unchanged one copies the displayed payload. Provisional content
  has no actionable objects, and reference completion cannot trigger an external action.
- Every job, including a rejected result, releases its lane. Per-document job slots,
  executor workers, results, copy requests, and payload bytes stay within configured bounds.
- With input and geometry held steady and scheduled work allowed to finish, the newest
  revision eventually displays, Finish leaves Finishing, and a requested switch completes.
  A continuously active document does not starve another ready document. Bound this by
  scheduler turns in the deterministic test, not by elapsed milliseconds.

Force specific races: navigation between submission and publication; resize plus append;
Finish overtaking a Receiving result; source ahead of rendered; query/view change with old
search results queued; replacement during copy; and removal followed by NodeId reuse. Test
render failure and retry, enqueue rejection followed by a wake, and worker completion while
the owner is hidden or detached. After resize stops, eventual publication uses the final
width; do not require one result to contain bytes accepted after that job's snapshot.
Keep appending while a Source-to-Rendered switch waits: the pinned outgoing revision lets
the first suitable target commit without waiting for the producer to stop.

Exercise every configured limit at N−1, N, and N+1 using reduced test limits, plus one real
default-bound case per costly path. Cover source bytes, depth, semantic nodes, rich-table
cells, syntax work, cache storage, search windows, query size, and action count/payload bytes.
Distinguish rejection of new input from degradation of already accepted content. Inspect
retained-allocation and queue counters after work drains and owners are removed; caches may
plateau, but old epochs, overlays, and worker results must not remain reachable indefinitely.

A recording host verifies exact request IDs, payloads, displayed revisions, and reply
handling. One activation produces one request. Focus, resize, reference completion, view
switching, and duplicate replies produce none. A copy finishing after the source advances
still returns its captured displayed snapshot. Queue pressure, absent capabilities, invalid
schemes, missing bases, payload limits, and late failures produce truthful status without
effects on a replacement document.

### Mounted interaction and TableView

Install the real default bindings in Harness, not just type registration. Exercise commands,
keyboard, wheel, scrollbar, and pointer paths. Assert focused node, active modal owner,
scroll offsets, selection, capture owner, notices, and bounded semantics alongside rendered
cells. Test Scroll boundary propagation, Interact's consumed boundaries, Tab exit, nested
prompt Escape order, and modal wheel containment. Directional admission, `key_outcome`,
contextual help, and actual handling must agree at boundaries and zero-sized layouts.

Open each inspector, then append, resize, hide/detach, replace, and remove its owner. Verify
fixed content, the documented keyed table exception, return focus, and reclamation of both
modal scope and overlay. Test structural rollback separately: the surviving owner must still
work. End drags through release, Escape, lost admission, and removal, including while geometry
changes. Repeated open/close cycles leave node and subscription counts at their baseline.

Run TableView without Markdown or optional features. Cover plain/styled cells, zero rows or
columns, header-only data, duplicate keys, malformed style spans, dataset replacement,
surviving/missing keys, narrow fallback, and stale prepared results. Navigate past all four
edges; verify header/gutter synchronization with two-axis body scroll. Select rectangles in
both directions, rows, columns, and all cells. Publish changed values under existing keys
and verify the copied data belongs to the displayed revision.

Use exact TSV goldens for empty fields, embedded tabs/newlines/quotes, header inclusion, row
ordering, and final LF. Compare preview paint/hits with mounted TableView for identical
PreparedTable input. Include hit points on borders, gutters, clipped/wide graphemes, and
scrolled cells. Counters must show no per-cell nodes, no per-cell select-all allocation, and
no full dataset rescan on navigation. Hand-checked geometry fixtures complement comparisons
that use the shared renderer on both sides.

### Luau journeys and visual review

Add markdowngym and tablegym scripts that drive public controls, inject deterministic chunks,
and inspect revision-addressed state. Include read → pause → inspect table → copy → dismiss,
stream → toggle source → search → toggle back, and replace → deliver stale result → continue
reading. The recording host exposes requests without invoking a real browser or clipboard.

Wire each script to its actual gym in
[the gym test runner](../examples/gyms/src/tests/smoke.rs) and
[xtask](../xtask/src/main.rs). The current default gym smoke launch selects listgym; dropping
a Markdown script into that directory alone will not exercise the new app. Add an explicit
runner or suite configuration and verify script counts. Extend fh's existing view-focus,
view-modes, view-scroll, full-screen, and Markdown-preview coverage with the new toggle
journey. Commands and bounded state queries must remain usable headlessly.

Use the existing `canopyctl gallery` infrastructure. Add the new gyms to the explicit
gallery catalog and fh scenes to its gallery scripts. Review light/dark themes and both glyph
presets at 160×50, 110×36, 80×24, and a narrow 40×12 viewport; use targeted harness snapshots
for zero, one-cell, and minimal layouts. Cover each state in a representative matrix rather
than generating every state × size × theme combination.

Capture Source and Rendered at the same logical anchor, both rendered modes, literal-to-grid
transition, Updating/switching, aborted output, selected links, long code, cell detail,
unavailable actions, syntax-limit fallback, and a partial file. Check status/toggle
legibility, ground continuity, selection/search contrast, and clipped wide glyphs. Record
which images were inspected; successful capture alone is not visual acceptance. Inspect a
real terminal once for focus, mouse, and perceived scrolling, since headless timings and
screenshots do not establish terminal smoothness.

### Performance and memory qualification

Measure release builds at 120×40 cells on a named machine. Record both repository revisions,
Rust version, enabled features, fixture seed/size, run count, and whether caches are cold or
warm. Separate prepared-view navigation, cold construction, first switch, cached switch,
stream publication, and inspector opening. Report p50/p95/max, not only a mean. Use at least
1,000 events for input/append percentile runs and report repeats for cold operations.

Initial release targets:

| Workload | Target or proof |
| --- | --- |
| Prepared 1 MiB prose/mixed document, either view | p95 input-to-prepared-frame below 16 ms |
| 50 chunks/second, deterministic 256-byte appends, document up to 1 MiB | p95 accept-to-first-frame-containing-that-revision below 100 ms |
| Cached view switch at current revision and width | p95 below 16 ms; first preparation keeps outgoing input responsive |
| 16 MiB source, one long line or one large code block, parsing/resizing in flight | p95 input-to-frame below 16 ms; eventual publication after input settles, with measured completion time |
| TableView at 500, 5,000, and 50,000 rows with fixed visible dimensions | Navigation/paint counters scale with visible cells; no full scan per event |
| Two readers competing for work, repeated switches and inspector use | Both make publication progress; queues stay bounded |
| Ten-minute soak with appends, replacements, and open/close cycles | Retained allocations return to the declared active-source/cache envelope after drain |

For the 50 Hz run, replace the document before the source cap so the workload cannot silently
turn into rejected appends. Include bursty delivery and an adversarial table/code fixture as
separate runs. Track each accepted revision until the first published frame containing it;
coalesced revisions still contribute a latency sample. Measure a stalled producer's Finish
and the last outstanding byte too. A source revision cannot count as rendered publication.

Record flattening, parsing, source indexing, mapping, reconciliation, layout, highlighting,
paint, publication lag, and peak retained bytes separately. Count source chunks, flattened
input, models, both surface layouts, Editor buffers, inspector snapshots, running/queued
jobs, and action payloads; the 32 MiB cache budget is not a total-memory claim. Prove
unchanged-prefix layout reuse and absence of parse/wrap work in render with counters.

Keep absolute timing qualification out of noisy correctness CI. Use deterministic work and
queue bounds there, with dedicated benchmark runs for the latency gates. If whole parsing or
Source's Editor adapter misses the envelope, record the failed workload and fix the measured
bottleneck before claiming completion. Consider incremental parsing only when parsing is
responsible, and retain the whole-parse oracle for its validation.

### Validation gates and receipts

Stage 1 closes the parser/admission, preserving-source, and modal-lifecycle risks with focused
tests. Stage 2 requires a working gym journey and inspected screenshots for both views and
TableView. Stage 3 completes the fixture, trace, boundary, and performance suites. Stage 4
requires fh migration evidence as well as the library tests; no earlier stage's pass stands
in for consumer acceptance.

Run focused crate tests as changes settle, then repository-required Ncode checks once per
settled change. Extend `cargo xtask feature-check` with independent `canopy-markdown`
default/no-default-feature checks. Run the standalone TableView tests with
`canopy-widgets --no-default-features` separately, so Cargo feature unification cannot hide
an accidental graphics, terminal, or syntax dependency. Both views must retain navigation,
search, and copy without highlighting. Keep production builds separate from test-feature
builds, following the existing feature-check workflow.

Run `cargo xtask smoke` in Canopy and fh's tracked smoke runner, including the new gym
registration. Refresh and review `ncode api` in each changed workspace, then verify with
`ncode api --check`. Generate Canopy's `cargo xtask gallery` and fh's
`cargo xtask gallery update`, and inspect the relevant outputs. Run mosaic scoring only if
matcher code or symbols change. Sequence sibling Cargo work to avoid contention in their
shared target directory.

Keep implementation receipts under ignored `tmp/`: revisions, commands, feature matrix,
fixture seeds, test/scenario counts, gallery IDs and reviewed sizes, and benchmark/memory
results. Distinguish passing, failing, and unrun gates. Missing sockets or launch failures are
environment blockers to rerun, not a passed scenario or evidence of a widget defect.

## Design risks

Whole-document parsing may miss the latency targets on large documents. The plan supplies
bounded queues and transitional rendering, not proof those costs are acceptable. Keep
performance qualification attached to the real inspector path.

Source reuse is not a crate rename: fh currently owns presentation glue around an Editor
that resets state on `set_text`, and its file loader changes the source bytes. Preserving
updates, bounded preparation, and exact copying need their Stage 1 proof. Keep any required
Editor changes generic and narrow; if they grow into a second document framework, revisit
the adapter design before proceeding.

The provisional-region rule accepts visible flicker in exchange for simplicity. If a common
producer pattern makes that flicker unreadable, add a targeted rule for that pattern rather
than a general classifier.

Shared grid rendering must stay smaller than a spreadsheet framework. Fixed-height rows,
atomic prepared snapshots, keyed identity, and cell-level activation bound the first API while
leaving room for the spreadsheet extensions.

Overlay reclamation must survive Canopy's lifecycle and rollback rules without a runtime
change. If the wake-based cleanup cannot pass those tests, resolve that ownership gap before
shipping inspectors rather than leaving it to host convention.
