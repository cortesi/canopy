# Next: Markdown reader follow-ups

## Description

fh reads Obsidian vaults through the Markdown reader: wikilinks, block
references, callouts, and embedded images and notes. A review of that work
left these open points, and each waits on a decision.

Reader work lands in `crates/canopy-markdown`, table work in
`crates/canopy-widgets`, and host work in fh (`../fh`), which migrates in
the same change. The browser's own follow-ups are in `../fh/plans/next.md`.

## Changes

### C3: `%%` comments that end mid-block or span line breaks

Moved to `xmark.md`, C4: the parser fork blanks comments before parsing.

### C4: Marks that span formatting, and escaped tags

Moved to `xmark.md`, C3 and C5: highlights and tags become parser syntax.

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

Options:

- Refit in the shell before layout. The shell does not learn its width
  before layout today, so this needs a new widget hook.
- Give each column a fit floor of its label's width, so fitting never
  wraps a header.

### C7: Public table geometry methods

`PreparedTable::{fit, header_lines, row_at, row_lines, wraps}` are public,
but only `TableView` calls them. `paint` and `hit_test` are public and
handle wrapped tables, so an outside painter needs `header_lines` and
`wraps`. Decide whether to keep the set public or narrow it to
`pub(crate)`.

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
rename them too. Widget types and node paths keep the suffix by design.
