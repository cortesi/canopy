# Markdown and source readers

`canopy-markdown` provides a terminal Markdown reader with Rendered and Source
views. The shared `canopy-widgets::TableView` also works independently. Run the
examples with `cargo run -p gyms -- markdowngym` and
`cargo run -p gyms -- tablegym`.

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
text selection. Links open an inspector first; a separate Open action requests a
browser launch. Code cards show a short preview and open a pinned full-text
inspector. Completed tables open a keyed TableView; a receiving trailing table
remains literal source until it closes.

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

The built-in search uses literal matching. Hosts such as fh can retain their own
search field and call `set_search` with regex and smart-case matching. Rendered
searches readable text, including omitted code and table cells; Source searches
raw Markdown. A new query or view does not jump until the reader activates a
match. Outline and local fragment jumps keep a return position.

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
