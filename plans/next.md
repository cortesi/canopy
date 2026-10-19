# Next: Canopy follow-ups

## Description

These are the open points from the Markdown reader work and from a code
survey of the whole workspace. Each was checked against the code on
2026-10-19, and each decision was settled the same day. An external review
of the plan corrected C1, C2, C5, C6, and C8. fh's plan added C9 and C10,
which its own C19 and C8 need. The parser fork's own work, upstream
offers, and remaining syntax are in `xmark.md`. fh's follow-ups are in
`../fh/plans/next.md`.

Runtime work lands in `crates/canopy`, widget and table work in
`crates/canopy-widgets`, reader work in `crates/canopy-markdown`, and gym
work in `examples/gyms`. API changes migrate fh (`../fh`) in the same
change, and refresh `api/`.

## Changes

### Runtime

#### C1: `NodeArena::require`

28 sites in `crates/canopy/src/core` repeat
`nodes.get(id).ok_or(Error::NodeNotFound(id))`, or its `get_mut` twin. 12
more check `contains_key` and then return `NodeNotFound`. Most are in
`world/layout_driver` and `tree.rs`.

`NodeArena` (`core/id.rs`) gets `require` and `require_mut`, which return
`Error::NodeNotFound`, and each of those sites calls one of them.
`ViewContext::type_id_of` (`core/context.rs`) is out of scope, because it is
not an arena lookup. So are membership checks that do not return
`NodeNotFound`, such as the liveness checks in `routing.rs` and
`rendering.rs`. `NodeArena` is
crate-private, so the public API does not change.

#### C2: `key_for` follows a focus override

`NodeCtx::key_for` (`core/context.rs`) calls `Core::key_for`
(`core/help.rs`), which calls `explain_key(None, key)` and so explains each
key from the real focus. A context with a `focus_override` answers
`is_focused` and `focused_node` for the override. For that context,
`key_for` can therefore name a key that its own route does not reach.

Decision: `Core::key_for` takes the route start, and `NodeCtx` passes
`focused_node()`. The Luau `canopy.key_for` (`core/script/base_api.rs`)
passes `None`, so it keeps the live focus. A focus override exists only
while `select.rs` predicts another node's key outcome or intent. The test
calls `key_for` from that path, and checks that the key resolves for the
predicted focus.

#### C9: Tell an owner that a descendant scrolled

fh's flat listing polls every 50 ms to see whether its listing scrolled
(`../fh` C19). No event can replace the poll today:

- A wheel scroll that a child's default action handles returns before it
  bubbles (`core/canopy/routing.rs`).
- A direct `scroll_node` changes the child alone.
- Canopy polls only nodes that are scheduled or woken
  (`core/canopy/turn.rs`).

Decision: a node can observe the scroll of another node. When the observed
node's scroll position changes, by any path, Canopy wakes the observer, so
its next poll reads the new view. The observer registers through its
context, and the registration ends when either node is removed. Tests cover
the wheel default, keyboard scrolling, `scroll_node`, a scroll that changes
nothing, and removal of either node.

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
  `ScriptCheckResult::into_diagnostics` (`core/script/mod.rs`). The
  borrowing `diagnostics()` stays.
- Remove `ScrollbarGlyphs::track_horizontal` (`scrollbar.rs`). Only a test
  sets it, and no widget draws a horizontal track glyph.
- Keep `Border::with_border_style`, `FontBanner::set_renderer`, and
  `ColumnChart::set_lower_rows`. Each is one half of a `with_` and `set_`
  pair, and each pair is one configuration in two forms.

#### C5: Public table geometry methods

`PreparedTable::{fit, header_lines, row_at, row_lines, wraps}`
(`table/mod.rs`) are public, but only `TableView` calls them. `paint` and
`hit_test` are public and handle wrapped tables. The docs of
`TableLayoutOptions::wrap` tell an outside caller to refit through `fit`.

Decision: keep `fit`, `header_lines`, and `wraps` public, for outside
painters. Narrow `row_at` and `row_lines` to `pub(crate)`.

#### C6: Table header height while fitting

`TableView`'s shell (`table_view/mod.rs`) reads `header_lines` from the
current geometry when it computes its layout padding. The body's canvas
refits the table only later. When a fit narrows a column so that its label
wraps, the header and the first body line overlap for one frame.

Decision: fitting never narrows a column below the width of its label's
widest line, so a fit never wraps a header. A label with explicit line
breaks keeps one header line for each, at every width, so its height does
not change with a fit. A table narrower than its labels scrolls sideways.
This needs no new widget hook.

`PreparedTable` records each column's natural width, but not its label's
width. The fit needs the label floors in every branch that narrows a
column. That includes the branch where the base widths already fit. If the
floors are stored, `derived_bytes` counts them. A `TableView` test covers a
mount and a resize. A `fit` test alone cannot see the shell read the header
height before the body refits.

### Markdown reader

#### C7: Embed naming

`BlockKind::Embed` (`document.rs`) carries an `ImageBlock`, whose size hint
means nothing for a note.

Decision: rename `ImageBlock` to `EmbedBlock`. fh does not name it.
`PaintOverlays` and `ImagePixels` (`presentation/mod.rs`) stay `!Send` and
`!Sync`. Their mosaic grid cache is an `Rc<RefCell<…>>`, and nothing sends
them across threads.

#### C10: Recheck named resources

`Markdown::recheck_resources` (`widget/mod.rs`) asks again for every
resource of the given kinds. fh's image refresh (`../fh` C8) must ask again
only for the images in one changed directory.

Decision: `Markdown` gets a recheck for named destinations of one kind,
next to `recheck_resources`. It treats each named resource as
`recheck_resources` does. An answered one is forgotten and asked again. A
requested one is asked again when its answer arrives. The bound counts it
again. A destination the document does not request is ignored.

### Gyms

#### C8: Leftover gym names

Gym subcommands and files dropped the `gym` suffix. Script owner names
(`"chargym"`, `"markdowngym"`, and others), style paths (`termgym/*`,
`fontgym/*`, `cursorgym/*`, and the motion gym's), and the `"tablegym"`
dataset id keep it. Widget types and node paths keep the suffix by design,
such as the `fontgym-block` and `fontgym-label` nodes.

Decision: drop the suffix from the script owners, the style paths, and the
dataset id. One name can be all three at once: `termgym` is a script
owner, a style layer, and a node path that `tests/help.rs` asserts. The
change keeps a mapping from each old name to its new one, by category. A
scan finds old names, and lists the node names it keeps. Only
`examples/gyms/src` and its tests use these names.

The smoke suite launches only the list gym, and the gallery leaves out the
cursor, motion, and term gyms. Tests that resolve the renamed styles cover
every renamed gym, the cursor, font, motion, and term gyms in particular.

## Decisions

- The reader keeps ignoring callout fold signs. canopy-xmark parses them
  into `Callout::fold`, but every callout shows open. A fold needs open
  state for each callout, in a reader whose layout is fixed for each
  preparation.

## Execution Plan

### Stage 1: Runtime

- [x] Add `NodeArena::require` and `require_mut`, and use them at each
      lookup site that C1 names (C1).
- [x] Pass the route start through `Core::key_for`, with a test from the
      prediction path (C2).
- [x] Let a node observe another node's scroll, with its tests (C9).
- [x] Run `ncode test -p canopy`.

### Stage 2: Widget and reader surface

- [ ] Paint the frame title through `roles::TITLE` (C3).
- [ ] Remove the four unused items (C4).
- [ ] Narrow `row_at` and `row_lines` (C5).
- [ ] Rename `ImageBlock` to `EmbedBlock` (C7).
- [ ] Add the recheck of named resources, with tests (C10).
- [ ] Run `ncode test -p canopy-widgets` and `ncode test -p canopy-markdown`.

### Stage 3: Table fitting

- [ ] Floor each fitted column at its label's widest line, in every
      narrowing branch, with `fit` tests and a `TableView` mount and resize
      test (C6).
- [ ] Run `ncode test -p canopy-widgets` and `ncode test -p canopy-markdown`.

### Stage 4: Gym names

- [ ] Rename the script owners, style paths, and dataset id, with the
      mapping, the scan, and the style tests (C8).
- [ ] Run `ncode test -p gyms` and `cargo xtask smoke`.

### Stage 5: Acceptance

- [ ] Refresh `api/` with `ncode api`, review its diff, then run
      `ncode api --check`.
- [ ] Run `ncode tidy` and `ncode test` in Canopy and fh.
- [ ] Run `cargo xtask smoke`, and the canopyctl smoke suite in fh.
