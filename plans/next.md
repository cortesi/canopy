# Next: Markdown reader follow-ups

## Description

fh reads Obsidian vaults: it follows wikilinks and relative links, walks a
trajectory with `b` and `g f`, reveals block references, and draws embedded
images and notes through the reader's resource protocol. A review of that
work left these open points. Each needs a decision before it is built.

Reader work lands in `crates/canopy-markdown`, table and highlighting work
in `crates/canopy-widgets`, and host work in fh (`../fh`), which migrates in
the same change. The browser's own follow-ups are in `../fh/plans/next.md`.

## Changes

### C1: A host's own reveal records no jump

`Markdown::reveal_anchor` records a jump, as the reader's own jumps do. fh
calls it for a pending anchor and then calls `take_jumps` to discard that
jump (`canopy-fileselect/src/trajectory.rs`), which also discards any reader
jump that fh has not yet drained.

Proposal: the public `reveal_anchor` restores without a jump. Link
activation inside the reader keeps the jump through its own path. fh drops
the `take_jumps` workaround. Decide whether a host reveal still sets the
one-step `return_from_jump` position.

### C2: A callout with only a title

`> [!tip] Remember water` with no body shows nothing: the marker line is
the callout's only text, and the reader removes it. Obsidian shows the
title.

Proposal: when the marker paragraph is all the callout holds, the title
becomes the callout's one line, under its label. Decide what a bare
`> [!tip]` shows: its label alone, or nothing.

### C3: `%%` comments that end mid-block or span line breaks

A comment over several blocks ends only at a block whose text ends with
`%%`. So `end of draft %% Back to text.` hides every block after it.
Within one paragraph, a comment across soft breaks is not hidden, because
marks are read one text run at a time.

Proposal: end a block comment at any odd count of `%%` in a block, and
show the text after the last one. Read marks across the soft breaks of a
paragraph. Alternatively, keep the limits and record them in tests and in
`docs/markdown.md`.

### C4: Marks that span formatting, and escaped tags

Highlights, comments, and tags are read within one text run, so a mark
that spans emphasis or a link stays as written. `\#x` still marks a tag,
because the escape is gone before the run is scanned.

Proposal: read marks over the inline events of a paragraph, using source
offsets to respect escapes. Decide whether the gain is worth a second
inline pass.

### C5: Fold callouts

Callout fold signs (`+`, `-`) are dropped. Obsidian folds a `-` callout
until it is opened.

Proposal: a folded callout shows its title row, and Enter opens it.
Decide whether folding belongs in a reader whose layout is immutable per
preparation, or whether fold signs stay dropped.

### C6: Table header height while fitting

`TableView`'s shell reads `header_lines` from the current geometry during
layout, before the body's canvas refits it. When a fit narrows a column so
that its label wraps, the header and the first body line overlap for one
frame.

Options: refit in the shell, which knows its width, before layout; or give
each column a fit floor of at least its label's width, so a header never
wraps by fitting.

### C7: Public table geometry methods

`PreparedTable::{fit, header_lines, row_at, row_lines, wraps}` are public,
but only `TableView` calls them. `paint` and `hit_test` are public and
handle wrapped tables, so an outside painter needs `header_lines` and
`wraps`. Decide whether to keep the set public or narrow it to
`pub(crate)`.

### C8: One presentation entry point

`PresentationSnapshot::prepare` is now used only by tests and benches;
the executor calls `prepare_with`. Proposal: one `prepare` that takes
`&Supplied`, with tests passing `&Supplied::default()`.

### C9: Embed naming and thread safety

`BlockKind::Embed` carries an `ImageBlock`, whose size hint means nothing
for a note. `PaintOverlays` and `ImagePixels` are now `!Send` and `!Sync`,
because the mosaic grid cache is an `Rc<RefCell<…>>`. Decide whether to
rename `ImageBlock` (for example to `EmbedBlock`) and whether the overlays
must stay `Send`.

### C10: Leftover gym names

Gym subcommands and files dropped the `gym` suffix. Script names
(`"chargym"`, `"markdowngym"`, …), style paths (`termgym/*`, `fontgym/*`,
`cursorgym/*`), and the `"tablegym"` dataset id keep it. Decide whether to
rename them too; widget types and node paths keep the suffix by design.

### C13: Bound syntax highlighting

Selecting a large minified JavaScript file in fh's Diff mode blocks the UI
for about 20 seconds, and returning to its cached diff blocks again. The
case found was a 1.3 MB bundle of 28 lines, the longest 550 KB, in a
private repository's `app/assets/viewer/viewer.js`. The same bytes named
`viewer.txt` show in 0.7 s.

`DiffView::render` and `draw_code` (`crates/canopy-widgets/src/diff_view.rs`)
ask the highlighter for spans of the visible lines while rendering.
`Source::spans_for` and `Engine::spans` (`highlight.rs`) call Syntect on
every complete line before them. `MAX_STATEFUL_LINES` bounds how many lines
are kept, not line length or parser work, so one long line parses in full
even in a narrow pane. fh's `Changes::serve` calls `DiffContent::show` on a
cached diff, which builds new highlighters, so every visit parses again.

Proposal: before Syntect runs, bound the source's bytes and its longest
line, and show a source past either bound as plain text. A bound on lines
skipped one at a time would break parser state, so the fallback covers the
whole source. File views share the highlighter and get the same bound. fh
keeps the prepared highlighters with its cached diff, so a revisit does not
parse again. Highlighting off the render path is the alternative if full
highlighting of such files matters.

Test with long JavaScript and JSON lines: a first visit, a cached revisit,
and fast selection changes. Input and frames must continue while the diff
loads.
