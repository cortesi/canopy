# Next: Canopy follow-ups

## Description

These are the open points from the Markdown reader work and from a code
survey of the whole workspace. Each was checked against the code on
2026-10-19. The parser fork's own work, upstream offers, and remaining
syntax are in `xmark.md`. fh's follow-ups are in `../fh/plans/next.md`.

C1 needs no decision. Each other change ends with a recommendation and the
decision it needs. Runtime work lands in `crates/canopy`, widget and table
work in `crates/canopy-widgets`, reader work in `crates/canopy-markdown`,
and gym work in `examples/gyms`. API changes migrate fh (`../fh`) in the
same change, and refresh `api/`.

## Changes

### Runtime

#### C1: `NodeArena::require`

29 sites in `crates/canopy/src/core` repeat
`nodes.get(id).ok_or(Error::NodeNotFound(id))`, or its `get_mut` twin. About
12 more check `contains_key` before they return `NodeNotFound`. Most are in
`world/layout_driver` and `tree.rs`.

Proposal: `NodeArena` (`core/id.rs`) gets `require` and `require_mut`, which
return `Error::NodeNotFound`, and each site calls one of them. `NodeArena` is
crate-private, so the public API does not change. A method on the arena,
not on `Core`, keeps the borrows of other `Core` fields free.

#### C2: `key_for` under a focus override

`NodeCtx::key_for` (`core/context.rs`) calls `Core::key_for`
(`core/help.rs`), which calls `explain_key(None, key)` and so explains each
key from the real focus. A context with a `focus_override` answers
`is_focused` and `focused_node` for the override. For that context,
`key_for` can therefore name a key that its own route does not reach.

Recommendation: `Core::key_for` takes the route start, and `NodeCtx` passes
`focused_node()`. `explain_key` already takes one. Decide whether a key hint
under an override must name the key for the override's route.

### Widgets

#### C3: The frame title role

`roles::TITLE` (`core/style/mod.rs`) has no users. `Frame` paints its title
with the literal `"frame/title"` (`canopy-widgets/src/frame.rs`), where
other widgets paint the shared role under their own layer.

Recommendation: `Frame` paints `roles::TITLE` under its layer, so the
`frame/title` path stays the same. Decide whether to do this, or to remove
the constant.

#### C4: Unused public items

Nothing in the workspace or fh reads these:

- `ScrollbarGlyphs::track_horizontal` (`scrollbar.rs`). Only a test sets
  it, and `Frame` draws no horizontal track glyph.
- `Spinner::contains` and `ColumnChart::clear_hover`.
- `ScriptCheckResult::into_diagnostics` (`core/script/mod.rs`).
  `diagnostics()` covers the same use.
- `Border::with_border_style`, `FontBanner::set_renderer`, and
  `ColumnChart::set_lower_rows`. Each is one half of a `with_` and `set_`
  pair.

Recommendation: remove `Spinner::contains`, `ColumnChart::clear_hover`, and
`ScriptCheckResult::into_diagnostics`. Keep the pair halves, because each
pair is one configuration in two forms. Decide whether `Frame` draws the
horizontal track glyph, or whether the field goes.

#### C5: Public table geometry methods

`PreparedTable::{fit, header_lines, row_at, row_lines, wraps}`
(`table/mod.rs`) are public, but only `TableView` calls them. `paint` and
`hit_test` are public and handle wrapped tables, so an outside painter
needs `header_lines` and `wraps`.

Recommendation: keep `header_lines` and `wraps` public, and narrow `fit`,
`row_at`, and `row_lines` to `pub(crate)`. Decide whether an outside
painter must also fit a table to its width.

#### C6: Table header height while fitting

`TableView`'s shell (`table_view/mod.rs`) reads `header_lines` from the
current geometry when it computes its layout padding. The body's canvas
refits the table only later. When a fit narrows a column so that its label
wraps, the header and the first body line overlap for one frame.

Options:

- Refit in the shell before layout. The shell does not learn its width
  before layout today, so this needs a new widget hook.
- Give each column a fit floor of its label's width, so that fitting never
  wraps a header.

Recommendation: the fit floor, because it needs no new hook. Decide whether
a table that is narrower than its labels can scroll sideways instead of
wrapping its header.

### Markdown reader

#### C7: Embed naming and thread safety

`BlockKind::Embed` (`document.rs`) carries an `ImageBlock`, whose size hint
means nothing for a note. `PaintOverlays` and `ImagePixels`
(`presentation/mod.rs`) are `!Send` and `!Sync`, because the mosaic grid
cache is an `Rc<RefCell<…>>`.

Decide whether to rename `ImageBlock`, for example to `EmbedBlock`, and
whether the overlays must be `Send`. Nothing sends them across threads
today.

#### C8: Fold callouts

canopy-xmark parses a callout's fold sign (`+`, `-`) into `Callout::fold`,
but the reader ignores it. Obsidian folds a `-` callout until it is opened.

Proposal: a folded callout shows its title row, and Enter opens it. Decide
whether folding belongs in a reader whose layout is fixed for each
preparation, or whether fold signs stay ignored.

### Gyms

#### C9: Leftover gym names

Gym subcommands and files dropped the `gym` suffix. Script owner names
(`"chargym"`, `"markdowngym"`, and others), style paths (`termgym/*`,
`fontgym/*`, `cursorgym/*`), and the `"tablegym"` dataset id keep it.
Widget types and node paths keep the suffix by design.

Recommendation: drop the suffix from script owners, style paths, and the
dataset id. Only `examples/gyms/src` and its tests use these names, and no
smoke script does. Decide whether to rename them.

## Execution Plan

### Stage 1: Changes with no decision

- [ ] Add `NodeArena::require` and `require_mut`, and use them at each
      `NodeNotFound` site in `crates/canopy/src/core` (C1).
- [ ] Run `ncode test -p canopy`, and `ncode check` in fh.

### Stage 2: Public surface

- [ ] Settle C2 to C5 with the user.
- [ ] Pass the route start through `Core::key_for`, with a test of a key
      hint under a focus override (C2).
- [ ] Paint the frame title through `roles::TITLE`, or remove the constant
      (C3).
- [ ] Remove the agreed unused items, and draw or remove
      `track_horizontal` (C4).
- [ ] Narrow the agreed `PreparedTable` methods (C5).
- [ ] Refresh `api/` and review its diff. Run `ncode check` in fh.

### Stage 3: Table and reader behaviour

- [ ] Settle C6, C7, and C8 with the user.
- [ ] Fit columns no narrower than their labels, or add the shell hook, with
      a test that a fitted header never overlaps the body (C6).
- [ ] Rename `ImageBlock`, and make the overlays `Send`, if agreed (C7).
- [ ] Fold callouts, if agreed, with a gym case and a smoke check (C8).
- [ ] Run `ncode test -p canopy-widgets`, `ncode test -p canopy-markdown`,
      and `cargo xtask smoke`.

### Stage 4: Gym names

- [ ] Settle C9 with the user.
- [ ] Rename the script owners, style paths, and dataset id (C9).
- [ ] Run `ncode test -p gyms` and `cargo xtask smoke`.

### Stage 5: Acceptance

- [ ] Run `ncode tidy` and `ncode test` in Canopy and fh.
- [ ] Run `cargo xtask smoke`, and the canopyctl smoke suite in fh.
