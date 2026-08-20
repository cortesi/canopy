# PLAN: Clear Wins Sweep

## Description

This survey records every change in the canopy workspace that passes the
clear-win test: the problem is proven by live code or a counted cost, the
change improves at least one quality and degrades none, it fits the settled
no-backward-compatibility stance (api-surface/README.md), and a named gate can
prove the result. Nine read-only agents swept all hand-written code: the
canopy, canopy-widgets, canopy-geom, canopy-derive, canopy-mcp, and canopyctl
crates, both example crates, xtask, all manifests and CI, the Luau sources,
and the docs. Excluded: vendor/, target/, tmp/, generated api-surface
skeletons, binary assets, and the sibling repositories (ruau, tmcp, itty).
Tree state at sweep time: clean except the pre-existing deletions of
plans/help.md and plans/wins.md. Items are ranked by leverage: correctness,
then counted performance, then API surface, then duplication and line count.

## Checklist

Tick each item when its change and its proof land. Note rejections and
modifications beside the item. The item text below the checklist carries the
full evidence, change, and proof.

**Correctness**

- [x] W1: Layout cache drops line changes after three or more edits between syncs
- [x] W2: Key spec cannot name the bare `+` and `-` keys

**Performance**

- [x] W5: Every `add_child` captures a redundant full-arena snapshot
- [x] W6: `TermBuf::screen_text` allocates per cell on the script poll path

**API surface**

- [x] W3: Inspector advertises a phantom Stats tab
- [x] W4: Script-error schema omits `invalid`
- [x] W7: Drop the unused `Send` supertrait on `Widget`
- [x] W8: `Modal` is a byte-for-byte duplicate of `Center`
- [x] W9: `TerminalConfig` carries seven frozen fields
- [x] W10: `commands.rs` carries conversion impls the type system cannot express
- [x] W11: Dissolve `Slot<K>`
- [x] W12: Make the `help` module crate-private
- [x] W13: Drop the canopy-widgets error module
- [x] W14: Font-renderer configuration can only restate defaults
- [x] W15: Dissolve `RemovePolicy`
- [x] W16: Dead and hand-recomposed `Render` style methods
- [x] W17: Delete unused `StyleRules::attrs`
- [x] W18: Make `PartialStyle::{resolve, join, is_complete}` private
- [x] W19: Delete unused Selector and Dropdown getters
- [x] W20: Collapse two node-type mismatch error variants

**Duplication and line count**

- [ ] W21: Delete forty-one empty `render` overrides
- [ ] W22: Remove no-op command machinery on command-less widgets
- [ ] W23: Delete dead `CommandNode` impls in integration tests
- [ ] W24: Deduplicate `handle_prompt_event` arms
- [ ] W25: Share the two click-count state machines
- [ ] W26: Parameterize the shared scroll-binding table
- [ ] W27: Collapse `focus_dir` direction dispatch
- [ ] W28: Delete unused `PathFilter::check` depth payload and `match_end`
- [ ] W29: Drop the `Option` wrapper on `Node.effects`
- [ ] W30: Share panes column-node bookkeeping
- [ ] W31: Inline Frame's invariant `ScrollGlyphs` field
- [ ] W32: Drop the duplicate UTF-8 check in `compile_source`
- [ ] W33: Route `outer_clip_to_local` through `RectI32::to_local_point`
- [ ] W34: Stop threading unused `NodeId` through `post_render`
- [ ] W35: Read MCP structured content directly in the test
- [ ] W36: Correct two wrong comments

**Open decisions (not implemented unless decided during the pass)**

- [ ] OD1: canopy-mcp embedding re-exports
- [ ] OD2: `root.dump_diagnostics` half-wired command
- [ ] OD3: Terminal Ctrl+Shift+C copy binding
- [ ] OD4: Collapse `Input` onto single-line `Editor`

## Implementation notes

Record rejections, modifications, and proof commands here as items land.

- W1: Implemented as specified. `PendingChange::{None, One, Dirty}` in
  `buffer.rs`. `take_change` drains every state and returns `Some` only for
  `One`. `apply_edit` marks `Dirty`. `LayoutCache::sync` drains on rebuild.
  Proof: `cargo nextest run -p canopy-widgets -E 'test(sync_rebuilds_after_three_edits_between_syncs) or test(visual_indent_of_three_wrapped_lines_keeps_layout)'`.
- W2: Implemented as specified (single-character spec before split). The
  evidence that `ctrl-+` already worked is false: `+` and `-` are
  separators, so a trailing `+`/`-` key with modifiers still fails. Left
  as a follow-up; this win only names the bare keys. Proof:
  `cargo nextest run -p canopy -E 'test(parse_specs)'`.
- W3: Implemented, with one adjacent deletion: `tab_active_fg` and the
  parent `/tab` rule left the palette because they had no remaining
  consumers. Proof: `cargo nextest run -p canopy-widgets -E 'test(inspector_pane_draws_its_frame)'`,
  `cargo xtask api`.
- W4: Added `invalid` to the `ScriptErrorInfo.error_type` doc. Proof:
  `cargo xtask api`.
- W5: `add_child_to_boxed` and `add_child_to_keyed_boxed` call `attach_inner`
  so they no longer nest a second arena snapshot. Proof: the four named
  rollback tests in `cargo nextest run -p canopy`.
- W6: `screen_text` writes into one String via `Cell::push_text`. Proof:
  `cargo nextest run -p canopy -E 'test(termbuf)'`.
- W7: Dropped `Send` from `Widget` and all four propagated bounds. All four
  compiled. Proof: `cargo check --workspace --all-targets --all-features`,
  `cargo xtask api`.
- W8: Deleted `Modal` and pointed help, todo, and stylegym at `Center`.
  Slot names stay `ModalSlot` because they name the overlay role.
- W9: Removed the seven frozen fields and the dead branches. `TerminalColors`
  is now a private defaults type. `SharedClipboard` is a `Mutex<String>`.
  Dropped `DriverPortal` and its `unsafe impl Send` because W7 made the
  Widget `Send` requirement go away. Proof: terminal tests;
  `cargo xtask api`.
- W10: Deleted tuple/`()` ToArgValue and FromArgValue impls, the unused
  CommandArgs From impls, and the self-test. Proof: `cargo check --workspace
  --all-targets --all-features`, `cargo xtask api`.
- W11: Moved get-or-create onto `impl dyn Context`. Button no longer
  stores zero-sized Slot fields. Context method budget unchanged.
- W12: `pub mod help` is now crate-private. BindingList stays reachable
  inside the crate.
- W13: `Font::from_bytes` returns `canopy::error::Result`. Deleted
  error.rs and the thiserror dependency.
- W14: Deleted `with_ramp` and `with_fallback`. `GlyphRamp` is private.
  `FontCell`/`FontLayout` stay crate-internal for FontBanner. Ruskel
  still names `FontLayout` as the `layout` return type.
- W15: Deleted `RemovePolicy` and the Hide branch. Reconcile always
  removes the subtree. Deleted `keyed_reconcile_prunes_removed_hidden_nodes`.
- W16-W18: `resolve_style` is pub. Deleted `resolve_style_at`,
  `resolve_style_name_raw`, and unused `StyleRules::attrs`. PartialStyle
  resolve/join/is_complete are private. Updated `themes.golden` (missed
  in W3 when tab rules left).
- W19: Deleted the unused Selector and Dropdown getters. Tests assert
  through fields.
- W20: `with_node` uses `checked_typed_id`. Deleted `Error::TypeMismatch`.

## Items

### W1: The editor layout cache silently drops line changes after three or more edits between syncs

- Outcome: Incremental layout sync never applies a `LineChange` that omits earlier un-synced edits. Wrap segments and line counts stay correct after multi-edit transactions.
- Evidence: `TextBuffer::bump_revision` (crates/canopy-widgets/src/editor/buffer.rs:419-423) toggles `pending_change`: Some→None, then None→Some on the next edit. The sole consumer `LayoutCache::sync` (crates/canopy-widgets/src/editor/layout.rs:116-120) applies only the returned change. Three edits yield Some(c1)→None→Some(c3), and sync applies only c3. Live repro: `indent_selection` (crates/canopy-widgets/src/editor/vi.rs:1070-1088) issues one `replace_range` per selected line in one transaction. An odd count ≥ 3 (`V j j >`) leaves earlier lines with stale `WrapSegment` data, so `render_line` (editor/widget.rs:746-749) truncates them.
- Change: Replace `pending_change: Option<LineChange>` with a three-state value (None | One(LineChange) | Dirty). `bump_revision`: None→One, One→Dirty, Dirty→Dirty. `apply_edit` sets Dirty. `take_change` returns Some only for One. Drain the pending state on the `needs_rebuild` path in `sync`. Net about ±5 lines.
- Constraints: Keep the public `take_change() -> Option<LineChange>` signature. Preserve single-edit incremental behavior. `input.rs` holds a `TextBuffer` but never syncs a layout cache.
- Proof: New regression test: three `replace_range` calls between syncs, then assert the cache equals a fresh rebuild. New harness test for `V j j >` under soft wrap. Both fail at HEAD. Gate: `cargo xtask test`, then `cargo xtask ci`.

### W2: The key spec language cannot name the bare `+` and `-` keys

- Outcome: `Key::parse_spec("+")` and `("-")` produce bindable keys, like every other printable character.
- Evidence: `Key::parse_spec` (crates/canopy/src/core/event/key.rs:292-295) splits on `['-','+']`; `parse_spec_parts` (key.rs:301-311) filters empty parts, so a bare `+` or `-` becomes "input specification cannot be empty". No `plus`/`minus` alias exists (only `space`, key.rs:362). The path is live public surface: framework bindings and the Luau bind path feed user strings through it. Modifier forms such as `ctrl-+` already work.
- Change: In `Key::parse_spec`, return the normalized single-character key when the trimmed spec is exactly one character, before splitting. About 3 lines.
- Constraints: Strictly widens the accepted language. `Mouse::parse_spec` has no single-character form and stays unchanged.
- Proof: New asserts in `tests::parse_specs` fail at HEAD and pass after. `cargo nextest run -p canopy -E 'test(parse_specs)'`.

### W3: The inspector advertises a phantom "Stats" tab

- Outcome: The inspector stops showing a Stats view that has no content. The internal `Tabs` widget and its orphaned palette entries go with it.
- Evidence: crates/canopy-widgets/src/inspector/view.rs:34 installs `Tabs::new(vec!["Stats", "Logs"])`, but only a Logs panel exists (view.rs:35-39). The Tab binding (inspector/mod.rs:16-18) moves a highlight while the content never changes. `Tabs` (tabs.rs, 66 lines) has no other consumer and is not exported. Core palette entries `/tab/active` and `/tab/inactive` (crates/canopy/src/core/style/palette.rs:71,75) have tabs.rs as their only consumer.
- Change: Install Logs directly. Delete tabs.rs, the `mod tabs` declaration, the Tab binding, `add_commands::<tabs::Tabs>()` (inspector/mod.rs:102), and the two `/tab/*` palette entries. About 75 lines.
- Constraints: No test asserts tab text or the "tabs" node name. `inspector_pane_draws_its_frame` checks frame corners only.
- Proof: `cargo xtask ci` (nextest, luau check).

### W4: The published script-error schema omits an emitted value

- Outcome: The `ScriptErrorInfo.error_type` doc, which schemars serves to MCP clients, lists every value the code emits.
- Evidence: crates/canopy-mcp/src/script.rs:82 documents `build`, `typecheck`, `timeout`, `runtime`. `evaluate_live` emits `"invalid"` (script.rs:345), and the test at script.rs:837 asserts it. The struct derives `JsonSchema` (script.rs:80).
- Change: Add `invalid` to the doc comment. Regenerate the api-surface skeleton.
- Constraints: None.
- Proof: `cargo xtask api --check` inside `cargo xtask ci`.

### W5: Every `add_child` captures a redundant full-arena snapshot

- Outcome: `Core::add_child_to_boxed` and `add_child_to_keyed_boxed` capture one `TreeStateSnapshot` per call instead of two, with identical rollback semantics. Building N children stops costing N extra full-arena clones.
- Evidence: crates/canopy/src/core/world/tree.rs:570-581 opens a tree edit, then calls `core.attach(...)`, which re-enters `with_tree_edit` (tree.rs:609-614). The nested branch (tree.rs:198-207) runs `TreeStateSnapshot::capture` — a full clone of the node arena including each node's children Vec and keyed HashMap (crates/canopy/src/core/node.rs:12-55). The savepoint is waste on success and redundant on failure: the outer journal restores the identical pre-edit state. Same pattern in `add_child_to_keyed_boxed` (tree.rs:584-599). Callers: `Context::add_child`/`add_child_keyed` (context.rs:703,753) — every widget that builds children.
- Change: In `add_child_to_boxed`, call `core.attach_inner(parent, child, None)?` instead of `core.attach(...)`. In `add_child_to_keyed_boxed`, call `core.attach_inner(parent, child, Some(key))?`. `attach_inner` (tree.rs:630) performs all validation. `attach`/`attach_keyed` keep their own `with_tree_edit` for direct callers.
- Constraints: A widget hook that calls `ctx.add_child` mid-edit still gets exactly one savepoint from `add_child_to_boxed`'s own `with_tree_edit`.
- Proof: `cargo xtask test` — the rollback tests (`add_child_rolls_back_on_mount_failure`, `keyed_child_add_rolls_back_key_and_node_on_mount_failure`, `handled_nested_failure_restores_its_savepoint`, `nested_mount_failure_joins_outer_tree_edit`) and the world proptests pin the exact behavior.

### W6: `TermBuf::screen_text` allocates per cell on the script poll path

- Outcome: `screen_text` allocates one output String instead of W×H cell Strings, H row Vecs, H row Strings, and a join.
- Evidence: crates/canopy/src/core/termbuf/mod.rs:491-498 builds the text via `rows()` (mod.rs:476-489), which allocates a String per cell and a Vec per row; all intermediates are discarded. `canopy.wait_for_screen_text` polls `screen_text` in a loop (crates/canopy/src/core/script/base_api.rs:439-447), so a default-size buffer costs thousands of transient allocations per poll tick. `Cell::push_text` (mod.rs:137-147) already appends without allocation.
- Change: Iterate rows directly and `push_text` into one String with `'\n'` between rows. Keep `rows()` for its live callers (script/records.rs:234, canopy-widgets/src/terminal.rs:1228).
- Constraints: Output stays byte-identical: empty cells render one space, continuation cells contribute nothing. Existing assertions pin this (termbuf/tests.rs:127,129,943,953,1041).
- Proof: `cargo xtask test` (nextest, including the termbuf property model).

### W7: The `Send` supertrait on `Widget` buys nothing and forces four downstream bounds

- Outcome: `pub trait Widget: Any`. Widget authors stop proving `Send` for types that can never cross a thread.
- Evidence: crates/canopy/src/widget/mod.rs:31 declares `Widget: Any + Send`, but widgets are stored in `Rc<RefCell<Option<Box<dyn Widget>>>>` (core/node.rs:15, core/world/mod.rs:90,108), so the containers are already `!Send`. No `dyn Widget + Send` coercion exists; downcasts use `&mut dyn Any`. Thread sites move channels, paths, and PTY state only. `run_automated` enforces `Send` on its closure captures independently. Forced bounds with no other purpose: canopy-widgets list.rs:641 (`Selectable + Send`), dropdown.rs:170 and selector.rs:193 (`Label + Send`), editor/highlight.rs:21 (`Highlighter: Send`). The bound arrived wholesale in the arena migration (7ff1779b) with no recorded rationale.
- Change: Drop `+ Send` from the trait and the four propagated bounds. Regenerate api-surface.
- Constraints: Keep terminal.rs's `Arc<dyn Fn .. + Send + Sync>` callbacks — its PTY reader thread is real. The compiler adjudicates each removal; any bound that fails to compile is load-bearing and stays.
- Proof: `cargo check --workspace --all-targets --all-features`, `cargo xtask api`, then `cargo xtask ci`.

### W8: `Modal` is a byte-for-byte duplicate of `Center`

- Outcome: One centering stack container and one fewer exported type.
- Evidence: crates/canopy-widgets/src/modal.rs:35-50 and center.rs:28-43 have identical `layout()` and empty `render()`; only the node name differs. Consumers: help/mod.rs:51 (which overrides the layout anyway), examples/todo/src/lib.rs:211, crates/examples/src/stylegym.rs:316. No path filter, query, style path, or test matches the node name "modal". Cost: 50 duplicated lines and one pub type.
- Change: Delete modal.rs and its lib.rs export. Switch the three `Modal::new()` sites and `key!` declarations to `Center`. Move the dimming-pattern doc onto Center or the help module.
- Constraints: The diagnostic node name changes from "modal" to "center"; nothing depends on it.
- Proof: `cargo xtask ci` (nextest including todo and stylegym tests, api --check).

### W9: `TerminalConfig` carries seven frozen fields whose setters were deleted

- Outcome: `TerminalConfig` holds only state that can vary (`command`, `cwd`, `on_exit`). The dead branches guarded by frozen fields disappear. `TerminalColors` leaves the pub surface.
- Evidence: Commit 30b63f86 deleted `with_env`, `with_scrollback_lines`, `with_mouse_reporting`, `with_bracketed_paste`, `with_kitty_keyboard`, `with_colors`, `with_clipboard_store`, and `with_clipboard_load` but left the private fields (crates/canopy-widgets/src/terminal.rs:285-300). Every construction goes through `new()`/`default()` plus the three surviving setters, so each field is permanently default. Dead code proven by that: the env loop (terminal.rs:927-929), the bracketed-paste fallback (659), the `mouse_reporting &&` conjunct (813), the scrollback/kitty folds (913-914), the colors field (754,915), and the `SharedClipboard` store/load plumbing (57-87). `TerminalColors` (169-235, exported at lib.rs:74) is constructible but cannot be applied anywhere. About 55 source lines plus 45 skeleton lines.
- Change: Delete the seven fields and their dead branches. Fold the defaults at the `terminal_config` call site. Keep `TerminalColors` as a private defaults type. Reduce `SharedClipboard` to the `Mutex<String>` shim itty requires. Keep the Ctrl+Shift+C swallow behavior unchanged (see the open decision on the inert copy binding).
- Constraints: Behavior is bit-identical for every reachable configuration.
- Proof: `cargo xtask api --check` and `cargo xtask test` (terminal tests at terminal.rs:1184-1355 use defaults), then full `cargo xtask ci`.

### W10: `commands.rs` carries conversion impls the command type system cannot express

- Outcome: About 122 lines of unreachable or consumer-free conversion surface leave `commands.rs`.
- Evidence: Every command parameter and return type must implement `CommandType` (canopy-derive/src/codegen.rs:43-44,173-174). No `CommandType` impl exists for tuples or `()` (commands.rs:530-623), and unit returns short-circuit to `ReturnKind::Unit` in the derive (parse.rs:129-152). Dead: `ToArgValue`/`FromArgValue` for `(A,)`, the two tuple macros with three invocations each, and the `()` impls (commands.rs:283-314, 456-516); their only reference is the self-test `tuple_lengths_validate` (1615-1622). Unused pub surface: `From<[T; N]>`, `From<Vec<T>>`, `From<BTreeMap<String, T>>` for `CommandArgs` (804-834) — every workspace `call_with` caller passes `()`.
- Change: Delete the listed impls, macros, and the self-test. Regenerate api-surface.
- Constraints: Keep `From<()> for CommandArgs` (backs `CommandSpec::call()`), the `ArgValue` identity impls, and the `&str` impls (live: canopy-mcp/src/server.rs:298). `call_with(CommandArgs::Positional(...))` retains the capability.
- Proof: `cargo xtask ci` (clippy, nextest, api --check).

### W11: `Slot<K>` is a stateless shell since its cache was removed

- Outcome: One fewer exported type on the app-author surface. The get-or-create ergonomics survive as `impl dyn Context` helpers, which the budget README excludes from the Context method count.
- Evidence: Commit 1f602468 removed Slot's cached id, leaving a `PhantomData`-only struct whose methods take `&mut self` but touch no state (crates/canopy/src/core/context.rs:53-97). The sole consumer is button.rs:35-39,51-53,105-112 (three zero-sized marker fields). Slot is exported three times (lib.rs:39, prelude.rs:5, core/mod.rs:68). All other keyed-child users call `get_child`/`add_keyed` directly.
- Change: Add `get_or_create<K: ChildKey>` and `get_or_create_in` to `impl dyn Context + '_` with Slot's exact bodies. Rewrite the three button.rs calls. Delete Slot, its Default impl, the Button fields, and the exports. About −35 lines.
- Constraints: `derive(Debug)` on Button keeps working. Regenerate api-surface.
- Proof: `cargo xtask ci` (button behavior covered by widget tests and example snapshots; api --check proves the surface change).

### W12: The `help` module is public but every consumer is internal

- Outcome: The help modal becomes an implementation detail of `Root`. About 108 skeleton lines leave the audited surface.
- Evidence: crates/canopy-widgets/src/lib.rs:25 exports `Help`, `BindingList`, and seven scroll commands (api-surface/canopy-widgets.rs:381-488). Zero references to `canopy_widgets::help` or `BindingList` exist outside crates/canopy-widgets/src/. Wiring is internal (root.rs:19), and command dispatch is by registered name, independent of Rust visibility.
- Change: `pub mod help` → `mod help`. Downgrade `help/mod.rs:18`'s `pub(crate) use` to plain `use`. Adjust `pub(crate)` markers if `clippy::redundant_pub_crate` fires. Regenerate api-surface.
- Constraints: root.rs unit tests use `crate::help::BindingList` — crate-internal, unaffected.
- Proof: `cargo xtask api`, then `cargo xtask ci`.

### W13: The canopy-widgets error module has one variant, one construction site, and no consumers

- Outcome: canopy-widgets exposes one error vocabulary (canopy's) and drops its thiserror dependency.
- Evidence: crates/canopy-widgets/src/error.rs defines `Error::FontLoad`, constructed only at font.rs:135-136. No target names `canopy_widgets::Error` or `Result`. Both external `from_bytes` callers treat the error opaquely (fontgym.rs:155 expects; widget/font.rs:88-90 reformats into `canopy::Error::Invalid`). image_view.rs:336-338 already uses `canopy::error::Error::Invalid` for the identical job. thiserror (canopy-widgets/Cargo.toml:39) has no other use in the crate.
- Change: `Font::from_bytes` returns `canopy::error::Result<Self>`, mapping to `Error::Invalid(format!("font loading failed: {e}"))`. Delete error.rs, the lib.rs re-export, and the thiserror dependency.
- Constraints: `widget/font.rs`'s `map_err` keeps compiling.
- Proof: `cargo xtask ci`.

### W14: The font-renderer configuration surface can only restate its defaults

- Outcome: `FontRenderer::new(font)` is the whole construction story.
- Evidence: `GlyphRamp`'s only constructor is `blocks()` (font.rs:83-105), which is the default (font.rs:218), and its field is private — `with_ramp` (225-228) is provably a no-op. `with_fallback` (231-235) defaults to `'?'`, and both workspace calls pass exactly `'?'` (fontgym.rs:158, widget/font.rs:98). The `FontCell`/`FontLayout` re-exports (lib.rs:62) have zero external consumers.
- Change: Delete `with_ramp` and `with_fallback`. Remove `GlyphRamp` (and `FontCell`/`FontLayout`, subject to ruskel rendering) from lib.rs:62. Delete the two no-op call chains in the examples.
- Constraints: `GlyphRamp` stays as internal machinery. `Font::name` stays (fontgym.rs:886).
- Proof: `cargo xtask api`, then `cargo xtask ci`.

### W15: `RemovePolicy` has one live variant; dissolve it

- Outcome: `KeyedChildren::reconcile` loses its policy parameter and hide-and-revive branch. The enum leaves the public API.
- Evidence: The only production consumer is `List`, which always passes `RemoveSubtree` (list.rs:194,232,240) and threads the parameter through two private helpers (593,625) for nothing. `Hide` is constructed only by its own unit test (world/tests.rs:1555-1605). With Hide gone, the enum (children.rs:14-21), the parameter, the `retained_hidden` logic (123-125), the Hide arm (195-197), the unhide pass (208-210), the exports, and the skeleton entries all go — about 80 net lines and one public type. Precedent: `RemovePolicy::Detach` was deleted at f739468f the moment its last construction site vanished.
- Change: Delete the variant, enum, parameter, branches, and the Hide test. Simplify List's helpers. Regenerate api-surface.
- Constraints: This deletes a designed hide-for-reuse capability with zero live consumers. General `set_hidden_of` visibility control is untouched.
- Proof: `cargo check --workspace --all-targets --all-features`, `cargo nextest run -p canopy -E 'test(keyed_reconcile)'`, `cargo xtask ci`.

### W16: The `Render` style cluster has a dead method and a hand-recomposed one

- Outcome: Style resolution shrinks from four pub methods to two (`apply_effects`, `resolve_style`), plus `resolve_style_name_at` for its one live caller.
- Evidence: `Render::resolve_style_at` (render/mod.rs:169) has zero callers anywhere. `resolve_style_name_raw` (164) has exactly one caller — editor/widget.rs:118, which recomposes the private `resolve_style` (158) by hand: `r.apply_effects(r.resolve_style_name_raw(name))`. About 14 lines plus 4 skeleton entries.
- Change: Delete `resolve_style_at`. Make `resolve_style` pub. Delete `resolve_style_name_raw`. Change editor/widget.rs:118 to `r.resolve_style(name)`.
- Constraints: Keep `apply_effects` (editor/widget.rs:798) and `resolve_style_name_at` (font_banner.rs:147).
- Proof: `cargo xtask ci` (clippy, api --check, nextest).

### W17: `StyleRules::attrs` has zero callers

- Outcome: One dead pub builder method gone.
- Evidence: The two-argument `StyleRules::attrs(path, AttrSet)` (style/mod.rs:553-557) has no call anywhere; every `.attrs(` in the workspace is the one-argument `StyleBuilder::attrs`. The sibling `StyleRules::attr` is live (fontgym.rs:911-913). 9 lines plus 2 skeleton entries.
- Change: Delete the method.
- Constraints: None.
- Proof: `cargo xtask ci` (api --check, nextest).

### W18: `PartialStyle::{resolve, join, is_complete}` are internals on the pub surface

- Outcome: Style-resolution internals — including a method that panics on incomplete input — become private.
- Evidence: All callers live in style/mod.rs (`join` at 588,729; `resolve` at 731,736; `is_complete` at 730). `resolve()` (406-412) panics via `expect` when a component is unset. External consumers of `PartialStyle` use only the constructors and public fields.
- Change: Drop `pub` from the three methods (plain private, avoiding `redundant_pub_crate`). Removes 6 skeleton entries.
- Constraints: `PartialStyle` itself and its constructors and fields stay pub (BufTest matching depends on them).
- Proof: `cargo xtask ci` (api --check, clippy, nextest).

### W19: Selector and Dropdown expose eight getters with no consumers

- Outcome: Each widget keeps exactly the surface its consumers use.
- Evidence: Zero consumers outside the defining files for `Selector::selected_indices` (selector.rs:46-48), `is_selected` (58-61), `focused_index` (63-66), `len` (150-153), `is_empty` (155-158), `selected_count` (160-163), `Dropdown::set_selected` (dropdown.rs:61-68), and `is_expanded` (70-73). External uses are stylegym's `selected_items`/`selected_index`/`selected` and the luau scripts' commands. Precedent: commit 30b63f86 removed `Dropdown::len`/`is_empty` for the same reason. About 45 lines, 8 methods.
- Change: Delete the methods. In-file tests assert via fields, which they already write directly (selector.rs:290,295).
- Constraints: Keep `selected_items`, `selected_index`, and `selected`. Do not touch the interaction commands (see Watch List).
- Proof: `cargo xtask api`, then `cargo xtask ci`.

### W20: Two error variants express the same node-type mismatch

- Outcome: One variant (`NodeTypeMismatch`), one shared check path, and no String allocations on the failure path.
- Evidence: `Error::TypeMismatch` (error.rs:250-257) has one construction site: context.rs:668 in `with_node`. `Error::NodeTypeMismatch` (error.rs:258-265) also has one: context.rs:301 in `checked_typed_id`. Both map to the same `ScriptErrorKind` (script/errors.rs:92). No test asserts the TypeMismatch message; world/tests.rs:300 matches NodeTypeMismatch.
- Change: `with_node` calls `checked_typed_id::<W, _>(&*self, node)?` before `with_widget_mut` and drops the inline check (the `downcast_mut` + `Error::Internal` safety net stays). Delete the variant. Simplify errors.rs:92. About −15 lines and one fewer public variant.
- Constraints: The `with_node` mismatch message changes to the NodeTypeMismatch form, dropping the actual type name but gaining the node id. Scripts see the same `ScriptErrorKind::TypeMismatch`. Regenerate api-surface.
- Proof: `cargo xtask ci`; existing NodeTypeMismatch assertions and it/script.rs:923.

### W21: Forty-one empty `render` overrides duplicate the trait default

- Outcome: Widgets that render nothing omit `render`, matching the default (widget/mod.rs:50-52) and the many widgets that already omit it.
- Evidence: 25 in-scope sites: crates/examples/src (chargym.rs:205, focusgym.rs:243, framegym.rs:203, listgym.rs:324, pager.rs:33, stylegym.rs:413,423, termgym.rs:146, textgym.rs:99, widget/font.rs:170, widget/mod.rs:135, widget/term.rs:41,65), examples/todo/src/lib.rs:141,401, crates/canopy/tests/it (commands.rs:47,65; focus.rs:61; node_render.rs:45,64; on_mount.rs:39,80; script.rs:88,856; tree.rs:29). 16 more: canopy-derive/tests/derive.rs:98,127; canopy-widgets benches editor.rs:31, rendering.rs:31; canopy-widgets/src root.rs:393,497,519,545, editor/tests.rs:60, pad.rs:48, center.rs:36, modal.rs:43; canopy core canopy/tests.rs:228,994,1013, world/mod.rs:348. Each body is byte-identical `Ok(())`. About 90-160 lines.
- Change: Delete each override and any `Render` import that becomes unused. `impl Widget for X {}` as an empty impl is legal.
- Constraints: None — bodies equal the default. W8 removes modal.rs first.
- Proof: `cargo xtask ci` (fmt, clippy unused-import, full tests) passes unchanged.

### W22: No-op command machinery on command-less widgets

- Outcome: `#[derive_commands]` and `add_commands` appear only where a widget exposes commands.
- Evidence: `derive_commands` on an impl with no `#[command]` methods generates only an empty `commands()` (canopy-derive/src/codegen.rs:496-538). `add_commands` of an empty set adds nothing (canopy/mod.rs:902-906) and produces no Luau global (base_api.rs:1481-1490). Dead sites, each verified to have zero `#[command]` methods: empty blocks and paired registrations in stylegym (DemoContent, ModalContent, Container; registrations at stylegym.rs:495-497), intervals (StatusBar), todo lib.rs (StatusBar, MainContent), textgym.rs:123, fontgym.rs:219, framegym.rs:210 (Self only), widget_editor.rs:66 (Self only), editorgym.rs:323 (EditorColumn), it/node_render.rs (three registrations); dead attributes on command-less impls in chargym, textgym, pager, fontgym, framegym, widget_editor, editorgym, listgym, termgym, intervals, todo, it/node_render.rs, it/tree.rs. About 35-40 lines plus misleading registrations.
- Change: Remove the listed attributes, empty impl blocks, and no-op `add_commands` calls. Where a `Loader::load` body becomes empty, use the default.
- Constraints: Keep every widget that has `#[command]` methods and every registration of one. Verify per site during implementation; all are compile-checked.
- Proof: `cargo xtask ci` — binding-driven help, termgym, and stylegym tests prove the registered command set is unchanged.

### W23: Dead `CommandNode` impls and a redundant `Loader::load` in the integration tests

- Outcome: Test widgets stop implementing a trait nothing requires.
- Evidence: Tree insertion requires `Widget + 'static` only (context.rs:687-698, canopy/mod.rs:366-376); `Harness::builder` requires `Widget + Loader`; `Loader::load` defaults to `Ok(())` (canopy/mod.rs:1391-1397). No `add_commands` or `::commands()` call exists in these files. Dead impls (5 lines each): it/layout.rs:24,48,76; it/on_mount.rs:32,73; it/focus.rs:50. it/layout.rs:92-96's `Loader` impl equals the default. About 35 lines.
- Change: Delete the six impls, their `commands::` imports, and collapse the Loader impl to `impl Loader for Root {}`.
- Constraints: None; on_mount.rs already demonstrates the empty-Loader form.
- Proof: `cargo xtask test` — the same tests compile and pass.

### W24: `handle_prompt_event` copies its text-editing arms three times and clones per keystroke

- Outcome: One implementation of prompt text input; no per-keystroke clone of the prompt state.
- Evidence: crates/canopy-widgets/src/editor/search.rs:249-479 (231 lines, 14 arms). The Backspace arm appears 3 times (274-287, 325-335, 374-387), the Char-append arm 3 times (288-301, 336-346, 388-401), the Esc arm 4 times (302-311, 347-356, 402-411, 467-476) — identical bodies differing only in the re-wrapped state. Line 254 clones the whole `PromptState` per keystroke, including the `matches` Vec in `ReplaceConfirm`.
- Change: Handle Esc once up front. Operate in place via `&mut self.prompt`, taking the state only for Enter transitions, with a helper that edits the active String field for Search/ReplaceQuery/ReplaceWith. Removes about 110-130 lines and the clone. The ReplaceConfirm y/n/a/q logic is untouched.
- Constraints: Behavior stays byte-identical: Enter transitions, ctrl/alt guards, and prompt render text (`prompt_text`, widget.rs:1048). Use `Option::take` around the Enter borrows.
- Proof: Existing `search_replace_all` (editor/tests.rs:278) and search unit tests; `cargo xtask test` and clippy.

### W25: Two identical click-count state machines in one crate

- Outcome: The multi-click tracker (same location + within threshold → count saturating at 3, else reset) exists once.
- Evidence: `ClickState` + `Terminal::selection_type_for_click` (terminal.rs:43-51,494-518) and `ClickState` + `MouseState::click_type` (editor/widget.rs:78-86,1004-1045) are structurally identical — about 70 lines for two copies. They silently disagree on the threshold (400ms at terminal.rs:41 vs 500ms at widget.rs:28).
- Change: Add a crate-private `ClickTracker { threshold, last }` with a count method; both widgets delegate. Keep the threshold a parameter so behavior is unchanged.
- Constraints: Preserve each widget's threshold and the saturate-at-3 behavior. No pub surface.
- Proof: `double_click_selects_word` (terminal.rs:1240), `mouse_double_click_selects_word`, `mouse_triple_click_selects_line` (editor/tests.rs:291,335); `cargo xtask test`.

### W26: framegym duplicates the shared scroll-binding table

- Outcome: One scroll and page binding table with receiver and path slots serves pager, chargym, and framegym.
- Evidence: `TEXT_SCROLL_BINDINGS` (crates/examples/src/lib.rs:75-126, receiver hardcoded to `text`) and framegym.rs:13-58 duplicate the same 13 bindings with receiver `test_pattern`. framegym differs only by `root.default_bindings()`, a Tab binding, and the two mouse-scroll rows. About 35 net duplicated lines.
- Change: Parameterize the receiver (`{receiver}.`). pager and chargym pass `("text", path)`; framegym passes `("test_pattern", "frame_gym")` and keeps its prefix. Also call `framegym::setup_bindings` in the framegym harness tests (crates/examples/src/tests/framegym.rs installs no bindings today) and drive one key, so the generated script runs under test.
- Constraints: framegym gains mouse ScrollUp and ScrollDown scrolling — strictly additive and consistent with every other demo. Name the rider in the commit message.
- Proof: `cargo xtask test` with the new setup_bindings-driven assertion; `cargo xtask ci`.

### W27: `focus_dir` duplicates its direction dispatch and sorts where a minimum suffices

- Outcome: One pass computes each candidate's key and takes the minimum. The unreachable fallback branch and the second per-direction match disappear.
- Evidence: world/focus.rs:140-156 (retain predicate on `dir`) and 162-183 (sort key on `dir`) duplicate the four-direction dispatch. The empty-candidates early return (158-160) makes the `else` at 185-189 dead. About 49 lines collapse to roughly half; `sort_by_key` becomes `min_by_key`.
- Change: One helper returns `Option<u64>` per candidate (None = filtered), then `filter_map(...).min_by_key(...)`. The stable sort's first minimal element and min_by_key's first-minimum tie-break select the same node.
- Constraints: Preserve the key formula (`edge_dist * 10000 + center_dist`) and first-in-pre-order tie-breaking.
- Proof: `cargo xtask test` — focus_dir tests (canopy/tests.rs:875-895) and it/focus.rs.

### W28: `path.rs` carries a depth-returning API nobody reads and a pure forwarder

- Outcome: `PathFilter` exposes one matching entry point; the matcher loses a pass-through.
- Evidence: `PathFilter::check` (path.rs:159-163) returns `Option<usize>`, but its single non-test caller (context.rs:278) and all 24 test asserts call `.is_some()` — the depth payload has zero consumers. `match_end` (204-210) duplicates `walk_match_end`'s base case and forwards with one caller (175).
- Change: Delete both. context.rs:278 uses `check_match(..).is_some()`. `check_match` calls `walk_match_end` directly. About −12 lines.
- Constraints: Match semantics untouched — the path proptests (path.rs:303-322) pin them.
- Proof: `cargo nextest run -p canopy -E 'test(path)'`, `cargo xtask ci`.

### W29: `Node.effects` is `Option<Vec<Effect>>` with a false rationale

- Outcome: One less wrapper at all three use sites and a comment that stops claiming a nonexistent optimization.
- Evidence: node.rs:52-54 says None "avoids per-node Vec allocation", but `Vec::new()` does not allocate and the Option is the same size (niche). Consumers pay for the wrapper: context.rs:1072 (`get_or_insert_with`), context.rs:1082 (`= None`), rendering.rs:150-152 (`if let Some` around an extend). The prior sweep's watch list recorded this contradiction.
- Change: Field becomes `Vec<Effect>`. `push_effect` pushes directly. `clear_effects` assigns `Vec::new()`. Rendering extends unconditionally. Rewrite the comment.
- Constraints: Byte-identical rendering; an empty-vec extend is a no-op.
- Proof: `cargo nextest run -p canopy -E 'test(effect)'`, then `cargo xtask ci`.

### W30: Panes repeats its column-node bookkeeping verbatim

- Outcome: One definition each for "active column nodes" and "ensure one column node per column".
- Evidence: panes.rs:144-149 repeats the `column_nodes()` helper (52-58) verbatim; the ensure-loop appears verbatim at 118-121 and 139-142.
- Change: `sync_layout` calls `self.column_nodes()`. Hoist the ensure-loop into `ensure_column_nodes`, called from `insert_col` and `sync_layout`. Net about −6 lines.
- Constraints: Preserve insert_col's ordering (ensure before inserting at x+1).
- Proof: `cargo xtask ci` (pane tests in crates/examples/src/tests/listgym.rs).

### W31: Frame carries an invariant `ScrollGlyphs` field

- Outcome: Frame holds only state that can vary.
- Evidence: frame.rs:16-28 defines the struct and const `SCROLL`; the field (73-74,87) is always `SCROLL` — no builder or setter exists. The two read sites (153,157) are the only uses. About 13 lines of indirection.
- Change: Delete the struct, const, and field. Use two char consts at the fill sites.
- Constraints: None.
- Proof: `cargo xtask ci` (framegym and termgym render tests).

### W32: `compile_source` validates UTF-8 twice with the identical error

- Outcome: One validation path.
- Evidence: script/mod.rs:871-876 runs the `as_str().ok_or_else(...)` check and discards the result; the next line calls `strict_named_source` (877), whose first statement (480-487) performs the identical check with the byte-identical message. 6 redundant lines and a duplicated string.
- Change: Delete the leading check. (`compile_startup_source` at 914-921 is not redundant — its binding is used and its message differs.)
- Constraints: Error text and category are unchanged by construction.
- Proof: `cargo xtask test`.

### W33: `outer_clip_to_local` reimplements `RectI32::to_local_point`

- Outcome: One clamped screen-to-local conversion algorithm.
- Evidence: rendering.rs:279-286 duplicates rect_i32.rs:31-38 operation for operation (same i64 subtraction, clamp, try_from). Single caller at rendering.rs:112. Routing already uses the geom helper (routing.rs:67).
- Change: In `render_node`, compute `let local = view.outer.to_local_point(screen_clip.tl);` and build the rect from it. Delete the helper. About −7 lines.
- Constraints: Bit-identical behavior (`left()`/`top()` are `i64::from`).
- Proof: `cargo xtask ci` (trender, tresize, it/node_render.rs, example snapshots).

### W34: `post_render` threads a `NodeId` it never reads

- Outcome: The cursor placement state carries only what it uses.
- Evidence: rendering.rs:208 builds `Option<(NodeId, View, cursor::Cursor)>`; the consumer at 222 destructures `(_nid, view, c)`.
- Change: Make it `Option<(View, cursor::Cursor)>`.
- Constraints: None; private function.
- Proof: `cargo xtask ci` (termbuf and render tests cover the cursor overlay).

### W35: A canopy-mcp test round-trips JSON through a string for nothing

- Outcome: The test reads `structured_content` directly, matching its four sibling tests. 9 lines become 1.
- Evidence: server.rs:382-390 serializes the parsed Value to a string and re-parses it. Siblings at server.rs:341,407,420,441 use `.expect("structured content")` directly.
- Change: `let payload = result.structured_content.expect("structured content");`.
- Constraints: Keep the two assertions unchanged.
- Proof: `cargo xtask test`.

### W36: Two comments are provably wrong

- Outcome: Comments match the code they sit on.
- Evidence: (a) layout_driver/tests.rs:28-29 carries doc lines for `attach_root_child` and `simple_widget`, which moved to test_support.rs:82-89; the lines now sit on `clamp_outer_no_bounds`. (b) canopy-geom/src/rect.rs:28-30: the `carve_hend` doc says it returns an array; it returns a tuple.
- Change: Delete the two stale lines; correct the carve_hend doc.
- Constraints: None.
- Proof: `cargo xtask ci` unaffected; review of the diff.

## Open Decisions

- canopy-mcp re-exports `AppEvaluator`, `evaluate_live`, `UdsServerHandle`, `serve_stdio`, `serve_uds` (lib.rs:18-23) have zero workspace consumers (canopyctl's `.serve_stdio()` at main.rs:458 is a `tmcp::Server` method). But commit f85a0e83 deliberately narrowed this list one day before the sweep and kept these five, so they may be an intended embedding seam for apps that bypass `launch()`. Decide: drop the re-exports (about 45 skeleton lines) or record the seam as intended.
- `root.dump_diagnostics` (root.rs:164-170) has no binding, caller, doc, or test; the documented path is `canopy.diagnostic_dump`. Its backing `Context::request_diagnostic_dump` (context.rs:626) would keep only core-test callers. The half-wired state is the defect; the remedy — delete the command and mechanism, or bind it — is a product call.
- Terminal Ctrl+Shift+C copy is inert today: `copy_selection` (terminal.rs:639-646) calls a store callback that is always None, and itty's `Session::copy_selection` is a pure read. Decide: delete the binding (the key then forwards to the PTY) or route the text into `SharedClipboard`. W9 keeps the current swallow behavior either way.
- `Input` (input.rs, 331 lines) is a second single-line text widget over `TextBuffer`, while `EditorConfig::with_multiline(false)` already produces one (editorgym.rs:187). Whether Input collapses onto Editor is a design decision.

## Watch List

Core:
- world/layout_driver/mod.rs:15-31 — `validate_invariants` runs a full O(N·depth) walk on every release-mode render frame; removal trades a safety net for unmeasured cost.
- world/layout_driver/mod.rs:42-45 — `locate_node` mis-clips when the root's `view.outer` is offset; no live caller passes one.
- world/focus.rs:211-215 — the recompute fallback is unreachable from live callers.
- world/tree.rs:198-224 — every nested `with_tree_edit` snapshots the full arena even on success; beyond W5 this is inherent to the pinned savepoint semantics.
- children.rs:137-148 — reconcile computes `removed` with a nested find per child, O(children × map).
- inputmap/mod.rs:604-619 — `set_mode`/`push_mode` return Result but never fail; `push_mode("")` silently no-ops while validation elsewhere rejects empty names.
- inputmap/mod.rs:543-558 — `push_exclusive_bindings` accepts a detached or missing owner until the next tree edit prunes it.
- inputmap/mod.rs:511-513 — the "not eligible in the active scope" diagnostic arm is unreachable.
- inputmap/mod.rs:128-148 — `BindingRecord.owner` is fully determined by `scope`; collapsing changes the Luau `owner` field.
- event/key.rs parse_spec — modifier forms with a trailing `+`/`-` key (`ctrl-+`) still fail because those characters are separators. W2 only names the bare keys.
- event/mouse.rs:118-129 — `Mouse`'s Display output does not round-trip `parse_spec`.
- widget/mod.rs:99-103 — default `Widget::name` on `Foo<Bar>` yields "bar"; every live generic widget overrides.
- commands.rs:688-691 vs script/bridge.rs:183-188 — the NodeId token record shape is duplicated in two encodings.
- script/records.rs:47-50,71 — `tree_node_to_arg` builds a children array it immediately overwrites per node.
- script/base_api.rs:1116-1119 — `host_send_scroll` accepts case-insensitive directions beyond its declared literal type.
- script/mod.rs:810-811 — a needless rebinding (`let pending = ...; let mut pending = pending;`).
- backend/crossterm.rs:731 vs 759-767 — the initial render's flush error bypasses `handle_render_error`.
- backend/crossterm.rs:754 with canopy/routing.rs:219 — a runtime error in a user binding terminates the runloop; fail-soft is a design decision.
- backend/crossterm.rs:534 — `shift_chars` truncates coordinates that `text()` rejects via try_from.
- canopy/routing.rs:309-311 — Resize sets `render_pending` twice.
- termbuf/mod.rs:311-334 — `fill_with` re-validates the constant fill character per cell.
- termbuf/mod.rs:502-504 — `diff` re-validates `prev` every frame although it passed validation as `self`.
- termbuf/mod.rs:902-904 — the empty-text branch appears unreachable but is not provably safe to delete.
- testing/buf.rs:100-134 — `contains_text_style` requires only one matched cell to carry the style.
- render/mod.rs:94-101,255-268 — after W33, the clamped point-translation pattern still exists in `untranslate` and `translate_point`; semantic equivalence with `to_local_point` is unverified.
- ViewContext::locate (context.rs:225) — sole consumer is its own integration test (it/tree.rs:97); surface-review lead.

Widgets:
- editor/highlight.rs:59-63 — `SyntectHighlighter` ignores cross-line parse state, so multi-line strings and comments highlight wrongly.
- editor/vi.rs:38-41,601-683 — `PendingKey` docs claim "or motion" but only d/c/y/gg/gj/gk are accepted.
- editor/vi.rs:827,861,153-160 — visual delete/change record the wrong repeatable edit; `o`/`O` repeat loses the typed insert.
- editor/vi.rs:1258-1273 — repeated `cc`/`C` opens the transaction after the delete, ungrouping it for undo.
- editor/widget.rs:858-869 — the `undo`/`redo` commands skip `ensure_cursor_visible` while vi `u`/Ctrl-R call it.
- panes.rs:92-111 — `delete_focus` detaches removed panes but never removes the subtrees; confirm arena semantics.
- selector.rs:102-148, dropdown.rs:130-139 — `select_first/select_last/clear/select_all/cancel` have zero bindings or callers; removal deletes real capability, a product call.
- boxed.rs:57-65 — the `DOUBLE` glyph set has no consumer; the catalogue looks intentional.
- label.rs:15-19 — `impl Label for &str` is never instantiated.
- inspector/mod.rs:56, inspector/logs.rs:27 — `Inspector` and `LogEntry` need not be pub; only `Logs` is pinned by tests/logs_subscriber.rs:16.
- list.rs:189 — `was_empty` actually means "had no selection".

Tooling and examples:
- canopyctl main.rs:265-271 — setup failures after spawn leave the child attached to the terminal.
- canopyctl main.rs:273-276 — a signal-terminated child yields exit status 0.
- xtask main.rs:252-276 vs 522-531 — the luau gate duplicates the nextest pin check and `xtask ci` runs the tracked_luau tests twice.
- .github/workflows/ci.yml:43-45 — nextest and ruskel compile from source every run; the workflow is documented red until the sibling crates publish.
- examples/todo/src/main.rs:78-84 — a missing path prints to stdout and exits 0.
- examples/todo/.canopyctl.toml — the run target fails on a fresh clone until tmp/ exists.
- termgym.rs:131-150 vs widget/term.rs:50-70 — two near-identical private stack containers; consolidation is an API decision.
- framegym.rs:93-121 vs editorgym.rs:148-176 — identical scroll command trios; sharing needs macro machinery.
- stylegym.rs:333-352 — `apply_theme` does two tree searches where one closure suffices.

## Rejected

- `Canopy::set_render_limits` and `register_script_module` removal — zero non-test consumers, but both survived a dedicated surface purge, one is documented (docs/scripting.md:21), and one is the oxau integration seam; settled intent to keep.
- value.rs `scoped_*`/`marshaled_*` consolidation — the parallel families mirror two distinct ruau input types; unification adds indirection.
- `ScriptCheckDiagnostic.severity` as an enum — a stable serialized shape consumed by canopy-mcp.
- `invocation_limits` unlimited-override concern — verified safe: ruau `Limits::overlay` keeps builder ceilings for None fields.
- `with_widget_read`/`with_widget_render` merge behind a generic guard — generics cost more than the shared lines.
- Direction-axis dispatch consolidation in the layout driver — the same complexity relocated.
- Demoting internal-only pub methods on `Core` — `mod core` is private (lib.rs:22); no external surface exists.
- Render-path micro-performance (TermBuf reuse across frames, `pre_render` lookup reduction, `StyleManager` per-lookup Vec, per-glyph backend batching, `resolve_match` mode clone, `eligible_keys` sort, `matches_for_line` per-row Vec, `BindingList::display_lines` caching) — all unmeasured; no counted hot-path proof.
- `PendingNode` manual Ord → derives + `cmp::Reverse` — wrapper noise at every heap operation; ordering already pinned by test.
- `MediaKeyCode`/`ModifierKeyCode` trim — the crossterm backend constructs them; removal drops delivered events.
- `PartialEq<char> for Key`, derive tweaks, `AttrSet` manual Default, `NopBackend` ceremony, `Harness.root` pub field, image_view test helpers — trivia; churn exceeds gain.
- Hand-rolling `NodeName::convert` to drop convert_case — canopy-derive shares the dependency for identical naming; divergence risk.
- `#[cfg(test)] mod tests` wrappers (it/ modules, todo tests, render_tests.rs, derive.rs) — required by `clippy::tests_outside_test_module` or pure reindentation churn.
- `tests/it.rs` `#[path]` scheme → directory layout — layout preference; the current form documents its rationale.
- Dropdown/Selector `content_size`/`handle_click` sharing — the behaviors differ (highlight-confirm vs focus-toggle).
- `List::measure` and `Logs` magic Unbounded widths — no demonstrated defect; layout heuristics risk silent regressions.
- fontgym's hand-rolled input editor — a deliberate demo of raw event handling.
- editor `handle_paste` vs `handle_insert_text` 6-line overlap and the visual-operator prologue dedup — no complexity reduction.
- `TextBuffer::new` cursor-at-end semantics — pub API churn for a trivial saving.
- syn `extra-traits` and model.rs Debug derives — enabled transitively anyway; Debug is exercised.
- Path-dep version-spec normalization and CI tool caching — publishing is blocked on unpublished siblings; no gate can prove the change.
- `FORMAT_TOOLCHAIN`/`MIRI_TOOLCHAIN` merge — same value, separate intents.
- canopyctl `smoke` vs `canopy_mcp::run_suite` unification — different transports and reporting.
- Derive codegen's dead-looking `ArityMismatch` arm → `unreachable!()` — the arm typechecks the generated match; the panic-free form is safer.
- Docs staleness — none found: the scripting type list, canopyctl commands, fixture and smoke tables, budget mechanics, and all README links were verified against live code.
