# Markdown and source readers

`canopy-markdown` provides a terminal Markdown reader with Rendered and Source
views. Its table inspector, `canopy_widgets::TableView`, also works on its own.
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

## Reading and actions

Scroll mode moves the document. Interact mode adds an object cursor and readable
text selection. The cursor gives its object one accent-tinted ground, which spans
the full width of a card or table, and a bar in the left margin marks each of its
rows. Links open an inspector first; a separate Open action requests a
browser launch. Code cards show a short preview and open a pinned full-text
inspector. A table preview shows every row and fits the reading width. Long
values wrap within their columns, while short and single-word columns keep their
widths. Accent labels and a heavy rule mark the header, and light rules separate
rows when any of them wraps. When prose would need columns too narrow to hold
it, the preview keeps the leading columns that fit, marks each line at its
broken right margin, and labels the table with the columns it shows. Completed
tables open a keyed TableView; a receiving trailing table remains literal source
until it closes. The table inspector shows every value in full: values wrap
within their columns, the columns fit the inspector's width, and the grid
scrolls sideways only when even narrowed columns do not fit.

An inspector opens over the reader itself, so it fills the reader's area and
leaves the rest of the host visible. It takes focus and the keys until it
closes, and the document's scrollbars hide while it covers the document.

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
omitted code and table cells; Source searches raw Markdown. A new query or view
does not jump until the reader activates a match. Outline and local fragment
jumps keep a return position.

Hosts with a separate search or status bar can set `publication_wake` to their
node's wake handle. Accepted publications and view changes then refresh host
state without a polling delay. Read-only queries do not generate wakes.

## Bounds and automation

Options bound accepted source, semantic nodes/depth/table cells, automatic syntax
work, search results, host actions, copied payloads, and inactive layout caches.
An oversized inactive layout is evicted while semantic identity and view positions
are retained. The cache budget is separate from active source, semantic models,
Editor buffers, in-flight jobs, inspectors, and host payloads.

`state`, `object_window`, `outline_window`, and `search_window` expose bounded,
revision-addressed automation data. Collection windows cap their item count and
text length. State includes publication, follow, inspector readiness, search
coverage, and conservative resource categories. It never returns the full source.

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

The tracked gym smoke suites live in `examples/gyms/markdown-smoke` and
`examples/gyms/table-smoke`. Gallery scripts cover both representations, inspectors,
streaming transitions, narrow sizes, and TableView. See the
[qualification harness](../crates/canopy-markdown/benches/README.md) for reproducible
latency and memory workloads. Correctness tests use controlled work completion;
performance measurements run separately in release mode.
