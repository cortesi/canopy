# Next: Canopy follow-ups

## Description

These are the open points from the Markdown reader work and from a code
survey of the whole workspace. Each was checked against the code on
2026-10-19, and each decision was settled the same day. The parser fork's
own work, upstream offers, and remaining syntax are in `xmark.md`. fh's
follow-ups are in `../fh/plans/next.md`.

Runtime work lands in `crates/canopy`, widget and table work in
`crates/canopy-widgets`, reader work in `crates/canopy-markdown`, and gym
work in `examples/gyms`. API changes migrate fh (`../fh`) in the same
change, and refresh `api/`.

## Changes

### Runtime

#### C1: `NodeArena::require`

29 sites in `crates/canopy/src/core` repeat
`nodes.get(id).ok_or(Error::NodeNotFound(id))`, or its `get_mut` twin. About
12 more check `contains_key` before they return `NodeNotFound`. Most are in
`world/layout_driver` and `tree.rs`.

`NodeArena` (`core/id.rs`) gets `require` and `require_mut`, which return
`Error::NodeNotFound`, and each site calls one of them. `NodeArena` is
crate-private, so the public API does not change. A method on the arena,
not on `Core`, keeps the borrows of other `Core` fields free.

#### C2: `key_for` follows a focus override

`NodeCtx::key_for` (`core/context.rs`) calls `Core::key_for`
(`core/help.rs`), which calls `explain_key(None, key)` and so explains each
key from the real focus. A context with a `focus_override` answers
`is_focused` and `focused_node` for the override. For that context,
`key_for` can therefore name a key that its own route does not reach.

Decision: `Core::key_for` takes the route start, and `NodeCtx` passes
`focused_node()`. `explain_key` already takes one.

### Widgets

#### C3: The frame title role

`roles::TITLE` (`core/style/mod.rs`) has no users. `Frame` paints its title
with the literal `"frame/title"` (`canopy-widgets/src/frame.rs`), where
other widgets paint the shared role under their own layer.

Decision: `Frame` paints `roles::TITLE` under its layer, so the
`frame/title` path stays the same.

#### C4: Unused public items

Nothing in the workspace or fh reads these. Decisions:

- Remove `Spinner::contains`, `ColumnChart::clear_hover`, and
  `ScriptCheckResult::into_diagnostics` (`core/script/mod.rs`).
  `diagnostics()` covers the last one's use.
- Remove `ScrollbarGlyphs::track_horizontal` (`scrollbar.rs`). Only a test
  sets it, and `Frame` draws no horizontal track glyph.
- Keep `Border::with_border_style`, `FontBanner::set_renderer`, and
  `ColumnChart::set_lower_rows`. Each is one half of a `with_` and `set_`
  pair, and each pair is one configuration in two forms.

#### C5: Public table geometry methods

`PreparedTable::{fit, header_lines, row_at, row_lines, wraps}`
(`table/mod.rs`) are public, but only `TableView` calls them. `paint` and
`hit_test` are public and handle wrapped tables, so an outside painter
needs `header_lines` and `wraps`.

Decision: keep `header_lines` and `wraps` public, and narrow `fit`,
`row_at`, and `row_lines` to `pub(crate)`.

#### C6: Table header height while fitting

`TableView`'s shell (`table_view/mod.rs`) reads `header_lines` from the
current geometry when it computes its layout padding. The body's canvas
refits the table only later. When a fit narrows a column so that its label
wraps, the header and the first body line overlap for one frame.

Decision: fitting never narrows a column below the width of its label, so a
fit never wraps a header. A table narrower than its labels scrolls
sideways. This needs no new widget hook.

### Markdown reader

#### C7: Embed naming

`BlockKind::Embed` (`document.rs`) carries an `ImageBlock`, whose size hint
means nothing for a note.

Decision: rename `ImageBlock` to `EmbedBlock`. fh does not name it.
`PaintOverlays` and `ImagePixels` (`presentation/mod.rs`) stay `!Send` and
`!Sync`. Their mosaic grid cache is an `Rc<RefCell<…>>`, and nothing sends
them across threads.

### Gyms

#### C8: Leftover gym names

Gym subcommands and files dropped the `gym` suffix. Script owner names
(`"chargym"`, `"markdowngym"`, and others), style paths (`termgym/*`,
`fontgym/*`, `cursorgym/*`), and the `"tablegym"` dataset id keep it.
Widget types and node paths keep the suffix by design.

Decision: drop the suffix from the script owners, the style paths, and the
dataset id. Only `examples/gyms/src` and its tests use these names, and no
smoke script does.

## Decisions

- The reader keeps ignoring callout fold signs. canopy-xmark parses them
  into `Callout::fold`, but every callout shows open. A fold needs open
  state for each callout, in a reader whose layout is fixed for each
  preparation.

## Execution Plan

### Stage 1: Runtime

- [ ] Add `NodeArena::require` and `require_mut`, and use them at each
      `NodeNotFound` site in `crates/canopy/src/core` (C1).
- [ ] Pass the route start through `Core::key_for`, with a test of a key
      hint under a focus override (C2).
- [ ] Run `ncode test -p canopy`.

### Stage 2: Widget and reader surface

- [ ] Paint the frame title through `roles::TITLE` (C3).
- [ ] Remove the four unused items (C4).
- [ ] Narrow `fit`, `row_at`, and `row_lines` (C5).
- [ ] Rename `ImageBlock` to `EmbedBlock` (C7).
- [ ] Refresh `api/` and review its diff. Run `ncode check` in fh.

### Stage 3: Table fitting

- [ ] Give each column a fit floor of its label's width, with a test that a
      fitted header never wraps or overlaps the body (C6).
- [ ] Run `ncode test -p canopy-widgets` and `ncode test -p canopy-markdown`.

### Stage 4: Gym names

- [ ] Rename the script owners, style paths, and dataset id (C8).
- [ ] Run `ncode test -p gyms` and `cargo xtask smoke`.

### Stage 5: Acceptance

- [ ] Run `ncode tidy` and `ncode test` in Canopy and fh.
- [ ] Run `cargo xtask smoke`, and the canopyctl smoke suite in fh.
