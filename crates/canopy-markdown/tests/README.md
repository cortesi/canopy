# Markdown semantic fixtures

The focused fixtures live beside the implementation in Rust unit-test modules.
Their expected strings, byte ranges, identities, and structural assertions were authored
for this crate. No imported CommonMark or GFM example corpus is included.
CommonMark and GFM syntax inform the cases, but these tests do not claim upstream-suite
conformance. The enabled parser extensions are explicit in source.

| Source | Fixtures and contract |
| --- | --- |
| [document.rs](../src/document.rs) | Independent readable-text and exact-source goldens cover entities, emphasis, CRLF, headings, images with destinations, and partial readable copy. Table fixtures cover escaped pipes, inline code, missing cells, excess cells, and literal Receiving tails. Code fixtures check closure, normalized payload, and atomic copy. |
| [document.rs](../src/document.rs) | Reference fixtures cover delayed admission, titles, duplicate definitions, and stable table keys after admission. Identity fixtures cover growth, reclassification, and IDs consumed by rejected results. Limit fixtures check fallback and adjacent boundary values without losing accepted source. URL, heading, HTML, quote, list, and rule fixtures check explicit semantic and source-span contracts. |
| [source_store.rs](../src/source_store.rs) | Exact immutable snapshots, all-or-nothing append limits, epoch replacement, and iterative flatten/reclamation of 50,000 chunks. |
| [search.rs](../src/search.rs) | Shared literal/regex smart case, Unicode columns, skipped zero-width matches, invalid/multiline patterns, scan/result bounds, cell/code addressing, and exact original SourceCard byte ranges. |

The streaming test
`short_fixtures_all_utf8_splits_and_codepoint_appends_finish_like_static` uses eight short
fixtures: empty input, Unicode with CRLF and a tab, a Setext heading, a reference link,
a table, a Luau fence, a list, and combining text with an entity and inline code.
It tries every two-part UTF-8 boundary split, inserts an empty append, and also appends
one code point at a time. Each intermediate snapshot must have valid source spans.
Completed and Aborted final projections must match a static parse and retain exact source.
Those final equivalence checks supplement the independently authored goldens; they do not
provide independent expected output for every intermediate prefix.

Other layers keep their tests with their source: [source_view.rs](../src/source_view.rs)
checks exact Source mappings and mounted editor behavior;
[presentation/tests.rs](../src/presentation/tests.rs) checks layout, projection, and paint;
[widget/tests.rs](../src/widget/tests.rs) checks controller behavior; and
[executor.rs](../src/executor.rs) checks publication, stale results, queue bounds, and removal.
The [qualification harness](../benches/README.md) describes generated performance workloads.
Its output is measurement evidence, rather than semantic fixture provenance.
