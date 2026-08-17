# Canopy clear-win plan

This plan lists changes that clearly improve clarity, structure, API, testability, or line count.
Every item was verified against the current tree: each "unused" claim was checked with a
workspace-wide grep of `crates/`, `examples/`, and `xtask/`, and each "duplicate" claim was
checked by reading both copies. Line numbers refer to the tree at commit `bd2b1f29`.

Canopy has zero backwards-compatibility requirements, and this covers public APIs, MCP payload
shapes, and example or CLI invocations. Remove obsolete API outright and update every in-repo
consumer in the same commit. Add no deprecated aliases or shims.

Path convention: a path that does not start with `crates/`, `examples/`, `xtask/`, `docs/`,
`api-surface/`, `assets/`, `tests/`, or `.github/` is relative to `crates/canopy/src/` in
Stages 1 to 3, to `crates/canopy-widgets/src/` in Stage 4, and to `crates/canopy/src/core/`
for `testing/...` paths in Stage 6. `world/tests.rs` means `crates/canopy/src/core/world/tests.rs`.

## How to use this plan

- Work through the stages in order. Items inside a stage are independent unless the text says
  otherwise. Land each item as one coherent commit. Bundle items (1.6, 2.12, 4.2, 6.7) list
  independent small changes; land each sentence of a bundle as its own commit or one commit per
  file. Each file move, deletion, and test migration has exactly one owning item.
- Tick an item immediately after its change and proof land. Add discovered work as new items.
- Documentation travels with the change: an item that alters a contract named in
  `docs/architecture.md`, `docs/scripting.md`, `docs/agent-loop.md`, or `api-surface/README.md`
  updates that text in the same commit, and regenerates the affected `api-surface/*.rs`
  skeleton (item 0.10 automates this). Stage 8 is only a final consistency check.
- Proof for every item: the focused tests of each touched module pass
  (`cargo nextest run -p <crate> -E 'test(<name>)'` during the hot loop), the item's own
  proof clause when it names one, and the stage gate `cargo xtask tidy` then `cargo xtask ci`
  before the stage closes. Zero-caller deletions prove themselves by compiling with
  `cargo check --workspace --all-targets --all-features`.
- Estimates: roughly 4,800 lines removed, eleven unused dependency declarations removed
  (item 0.5), `comfy-table`, `loom` (two crates), `color-backtrace`, and the derive crate's
  `proc-macro-error` (with `syn 1`) and `thiserror` removed, and every duplicated source of
  truth found by the review collapsed to one. Line counts marked "about" are estimates.

## Stage 0: Repository hygiene

Independent, low-risk, mostly deletions. Do these first.

- [x] **0.1 Delete stale artifacts.** `GEMINI.md` (superseded by `AGENTS.md` and the nano
  skills; zero references), `TODO.md` (last touched July 2025; lists shipped features as todo),
  `demos/*.rhai` (Rhai recorder scripts; the recorder now uses Luau; keep `demos/*.gif`),
  `crates/canopy/Cargo.lock` (nested lockfile inside a workspace member; Cargo ignores it),
  `.scripts/docs` and `.github/workflows/main.yml` (both run `mdbook build ./docs`; the book
  was removed in commit 327ea473). Remove the "Guide" and "API" links in `README.md:57-58`.

- [x] **0.2 Make CI honest.** (Prerequisite: item 0.10 for the `ruskel` pin.)
  `.github/workflows/ci.yml` runs `cargo xtask test` on the `beta`
  toolchain (overriding `rust-toolchain.toml`) on Ubuntu, Windows, and macOS, without installing
  the pinned `cargo-nextest 0.9.99`, so it fails before it builds. The Windows leg cannot pass
  regardless: `canopyctl/src/main.rs:15-27` uses `tokio::net::UnixStream` and
  `canopy-mcp/src/server.rs:1-12` uses `tokio::net::UnixListener` without target gates.
  Replace the workflow with one job on `ubuntu-latest` and `macos-latest` that installs the
  `rust-toolchain.toml` channel, the format nightly (`xtask/src/main.rs:53`), and the pinned
  nextest, and the pinned `ruskel` (item 0.10), then runs `cargo xtask ci`. Blocker to note in
  the workflow file: the sibling path dependencies (`ruau`, `tmcp`, `itty-core`, and the
  `itty-script` dev-dependency) keep hosted CI red until they are published, vendored, or
  checked out beside the workspace in the job. Do not hide that behind a passing no-op job.
  Completion proof for the workflow itself is one clean hosted run once the sibling crates are
  reachable. Windows support is a product decision (see Considered and rejected).

- [x] **0.3 Keep one copy of the font assets.** `assets/fonts/*` and
  `crates/canopy-widgets/assets/fonts/*` are byte-identical (six files, ~365 KB). Both
  `include_bytes!` users already point at the widgets copy (`crates/canopy-widgets/src/font.rs:929`,
  `crates/examples/src/fontgym.rs:946-963`). Point `DEFAULT_FONT_DIR` in
  `crates/examples/examples/widget.rs:26` at `crates/canopy-widgets/assets/fonts` and delete
  `assets/fonts/`. Keep `assets/tiger.jpg`.

- [x] **0.4 Remove the workspace-root `tests/snapshots/` directory.** Only
  `crates/canopy-widgets/src/snapshots.rs:65-107` reads it, through a three-branch cwd fallback.
  Replace the four snapshot files (14 lines total) with inline
  `harness.tbuf().assert_matches(buf![..])` assertions, the idiom used 12 times elsewhere
  (`termbuf.rs`, `render.rs`, `test_node_render.rs`, `editor/tests.rs`). The snapshot files
  encode spaces as `.` (`visible_snapshot`), while `BufTest` compares literal spaces and marks
  null cells `X`; convert each line back to spaces of the same width when writing the `buf!`
  expectations. Delete `snapshot_dir`, `snapshot_path`, `visible_snapshot`, `render_snapshot`,
  `assert_snapshot`, and the directory.

- [x] **0.5 Delete unused dependency declarations.** Verified by identifier grep in each
  crate's own targets: `canopy-widgets`: `canopy-derive` (macros come through `canopy::`),
  `tracing`; `canopy-mcp`: `async-trait`; `canopyctl`: `async-trait`, `schemars`;
  `canopy-examples`: `anyhow`, `textwrap`, `tracing`, `tracing-subscriber`; `todo`: `tracing`
  and dev-dep `tmcp` (eleven declarations). Later items remove `comfy-table` (1.4),
  `color-backtrace` (2.6), `proc-macro-error` and `thiserror` in `canopy-derive` (3.3), and
  `loom` in two crates (6.1).

- [x] **0.6 Drop the AV1 encoder from `image`.** `canopy-widgets` enables `image` default
  features. In `image 0.25` the `avif` feature is encoder-only (`dep:ravif`; decoding needs the
  separate `avif-native` feature, which is not enabled), so every app build compiles `rav1e`
  for a capability `ImageView` cannot use (`image_view.rs:340` only calls `image::open`).
  `cargo tree -e features -i image` shows canopy-widgets is the only enabler of `default`. Set
  `default-features = false` and list `rayon` plus every `default-formats` entry except `avif`:
  `bmp, dds, exr, ff, gif, hdr, ico, jpeg, png, pnm, qoi, tga, tiff, webp`. This preserves every
  decoder the viewer has today. Proof: `cargo tree -p canopy-widgets -e normal -i rav1e` reports
  no path, and the imgview demo still opens `assets/tiger.jpg`. Trimming the decoder list further
  is a product choice (see Considered and rejected).

- [x] **0.7 Fix the `testing` features.** (a) `canopy-widgets` declares `testing = []` that
  gates nothing (`grep 'feature = "testing"' crates/canopy-widgets/src` is empty). Delete the
  feature, the two `required-features = ["testing"]` lines on its benches (they silently skip
  under `cargo bench -p canopy-widgets`), and `features = ["testing"]` on the `canopy-widgets`
  dev-deps in `crates/examples/Cargo.toml:20` and `examples/todo/Cargo.toml:18`. (b)
  `cargo check -p canopy --tests` fails today because 11 of 15 integration tests import
  `canopy::testing`, which only exists under `--all-features`. Add
  `canopy = { path = ".", features = ["testing"] }` to canopy's `[dev-dependencies]` (a self
  path dev-dependency unifies the feature into the library build for test targets; verified:
  `cargo check -p canopy --tests` passes with that line and fails without it). (c) Move
  `NopBackend` from `crates/canopy/src/core/testing/render.rs` next to `RenderBackend` in
  `render.rs` (ungated, new path `canopy::render::NopBackend`), delete the private
  `SnapshotBackend` (identical trait impl) in `core/canopy/rendering.rs:25-52` and use
  `NopBackend` in `refresh_snapshot` (`:66`), move every importer in the same commit
  (`crates/canopy-mcp/src/script.rs:11`, `core/testing/harness.rs:3`,
  `crates/canopy-widgets/src/root.rs:368`), delete `core/testing/render.rs`, and drop
  `features = ["testing"]` from `canopy-mcp`'s normal dependency on canopy
  (`crates/canopy-mcp/Cargo.toml:10`), so production app binaries stop compiling the
  ~1,900-line testing module.

- [x] **0.8 Centralize shared dependencies.** Add `[workspace.dependencies]` for everything
  declared in three or more manifests (`thiserror`, `tokio`, `clap`, `serde`, `serde_json`,
  `schemars`, `anyhow`, `textwrap`, `unicode-width`, `unicode-segmentation`, `proptest`,
  `criterion`, `tmcp`, and the three sibling path crates) and use `.workspace = true`. This
  normalizes requirement spelling (`criterion = "0.8"` vs `"0.8.2"`; the lockfile already
  resolves one version) and declares each sibling path once. `examples/todo/Cargo.toml` should
  inherit `edition` and `license` like every other member. Proof: `Cargo.lock` is unchanged
  after `cargo metadata`.

- [x] **0.9 Stop running doctests.** The workspace has four doctests
  (`crates/canopy/src/core/context.rs:35-43,137-149`, `core/style/mod.rs:330-343,524-533`), and
  `xtask test`/`xtask ci` build seven doctest harnesses to run them. Per the adopted nano-rust
  standard: convert the four to unit tests, set `doctest = false` on all seven `[lib]` targets,
  and delete `run_doctests` from xtask.

- [x] **0.10 Automate `api-surface/`.** The skeletons are current, but the hand-maintained
  "Current" counts in `api-surface/README.md:31-36` have drifted (`impl Canopy` in the
  checked-in `canopy.rs` skeleton has 69 `pub fn` vs the documented 66, under the README's own
  deep-path metric), and nothing checks either. Add `cargo xtask api` that
  runs the seven `ruskel` commands and prints the current method counts for the four
  intent-level surfaces, and make `xtask ci` fail when the checked-in skeletons differ. Keep the
  budgets, ceilings, and review rules; replace the stale "Current" columns with the computed
  values (or drop the columns and let `xtask api` print them). Pin the tool like nextest: add
  `const RUSKEL_VERSION: &str = "0.0.11"` (the version installed today) to xtask, have
  `xtask api`/`xtask ci` fail with the exact `cargo install ruskel --version 0.0.11` message when
  the installed `ruskel --version` differs, and install that version in the CI job (item 0.2).
  Proof: `cargo xtask api` leaves no diff on a clean tree.

- [x] **0.11 xtask cleanups.** `run_fmt`/`run_fmt_check` duplicate the argument list and both
  branch on the existence of `rustfmt-nightly.toml`, a tracked file; write one `fmt_args(check)`.
  Tidy clippy passes deprecated `--all` plus `--tests --examples` (subsumed by `--all-targets`).
  (Item 6.1 owns the loom step in `run_dynamic`.)

## Stage 1: canopy core (arena, contexts, facade)

- [x] **1.1 One parent-chain walk and one home for focus queries.** Five copies of "walk
  parents looking for X": `tree.rs` `is_ancestor` (344-354) and `is_attached_to_root` (356-366),
  `focus.rs` `is_on_focus_path` (22-32) and `is_descendant` (437-446), `context.rs`
  `is_descendant` (1193-1203). Keep one `Core::is_ancestor_or_self`; land that part first in
  Stage 1 (it is item 1.1 for that reason), before 1.2 and 1.3 touch `context.rs` and
  `focus.rs`. Then: `Core::focus_path`
  becomes `self.focus.map_or(Path::empty(), |f| self.node_path(root, f))` (same loop today);
  move `focusable_leaves_for`/`focused_leaf_for` from `context.rs` into focus code as `Core`
  methods and have `focus_dir` call `focusable_leaves` (its loop body is that filter); make
  `Core::subtree_pre_order` `pub(crate)` and stop building a `CoreViewContext` in the four
  places in `focus.rs` (`:135, :354, :380, :421`) that do so only to iterate; drop
  `ViewContext::focus_path` from the trait (never called through a `dyn ViewContext`); move
  `core/focus.rs` to `core/world/focus.rs` beside the other `Core`
  concern splits and remove its stray `#[allow(clippy::multiple_inherent_impl)]`. About −80 lines.

- [x] **1.2 Collapse the duplicate `ViewContext` implementation.** `impl ViewContext for
  CoreContext` (`core/context.rs:1253-1361`) and `for CoreViewContext` (`:1598-1706`) are
  byte-identical apart from `&mut Core` vs `&Core`. Replace both structs with
  `pub struct NodeCtx<C> { core: C, node_id: NodeId }`, one
  `impl<C: Deref<Target = Core>> ViewContext for NodeCtx<C>`, one
  `impl Context for NodeCtx<&mut Core>`, and type aliases `CoreContext<'a> = NodeCtx<&'a mut Core>`,
  `CoreViewContext<'a> = NodeCtx<&'a Core>` so the 45 `::new` call sites (33 outside test
  modules) do not change. While
  there: make `is_focused`, `is_on_focus_path`, `child_keyed`, and `layout` default methods over
  their `node_*`/`*_in` forms (as `children()` already is), and return `View` by value from
  `view()` (it is `Copy`; all 56 callers use `.field`/`.method()`), which deletes the
  `DEFAULT_VIEW` literal at `:549-563`. About −120 lines.

- [x] **1.3 Delete dead `Context`/`ViewContext` helpers.** Zero callers anywhere:
  `Context::{hide, hide_node, show, show_node, set_clear_inherited_effects}`,
  `ViewContext::canvas`, and on `impl dyn Context`: `add_child_to_with_layout`, `add_children`,
  `add_children_to`, `add_children_boxed`, `add_children_to_boxed`, `try_with_node`,
  `with_node_at`, `try_with_node_at`, `add_keyed_to_with_layout`,
  `with_focused_or_first_descendant`, `try_with_first_descendant`; on `impl dyn ViewContext`:
  `first_child`, `try_find_one`, `try_find_one_matching`. Inline the internal-only aliases
  `add_child_keyed`/`add_child_to_keyed` (only called by `add_keyed`/`add_keyed_to`),
  `first_from`/`all_from`, `find_one_matching`, `try_with_keyed`. Remove the
  `clear_inherited_effects` feature end-to-end: the `Node` field (`node.rs:57-58`) is only ever
  set to `false` and read once in `render_recursive` (`canopy/rendering.rs:161-191`), so delete
  the field, the setter, the `DummyContext` stub, and the render branch. About −275 lines;
  `Context` 48→43 trait methods, `ViewContext` 32→31 (`canvas`; the `dyn` helpers are not
  trait methods; item 1.1 removes `focus_path` for 30).

- [x] **1.4 Prune the `Canopy` facade.** Delete zero-caller methods: `print_command_table`
  (sole user of `comfy-table`; also delete `CommandSpec::signature`, used only there and in one
  assert), `run_default_script` (pure alias of `eval_script`; rename its 20 callers),
  `unbind_mouse_input` (zero callers), `discover_project_script_root_from`,
  `clear_script_journal`, `input_mode_stack`, `bindings_for_mode`, `script_module_roots`,
  `invalidate_script_modules`/`invalidate_user_script_modules` (keep one
  `invalidate_script_modules(root: Option<&str>)` because `invalidate_project_script_modules` has
  a test caller and hot reload is a real capability). Merge `unbind_key_input` (one caller:
  `host_unbind_key`, `script/mod.rs:3122`) into `unbind_input(InputSpec, mode, path)`
  (`InputMap::unbind_input` already takes an `InputSpec`) and update `host_unbind_key` to pass
  `InputSpec::Key(..)`. Keep `register_script_module` (documented seam). In
  `script/modules.rs`, keep `ScriptModuleRoots::new` (used by `Canopy::new`,
  `canopy/mod.rs:347`) and delete `with_user_root`, `with_project_root`, `clear_user_root`,
  `clear_project_root`: `Canopy` exposes only `&ScriptModuleRoots`, so no app can reach them;
  rewrite the two `modules.rs` tests that use `with_*` to call `set_user_root`/`set_project_root`.
  Replace the three inline "sealed after finalize" checks at
  `canopy/mod.rs:750-754, 785-789, 1073-1077` with `ensure_api_unfinalized(..)`. `ScriptJournalBaseline` and `DefaultBindingsRun` need no visibility change: they live in a
  private module and are not re-exported, so they are already crate-internal, and
  `clippy::redundant_pub_crate` rejects the `pub(crate)` spelling there. Fix the stray doc line at
  `canopy/mod.rs:1682` ("Validate a child view position..." on `Loader`). About −150 lines.

- [x] **1.5 One journaled script-eval path.** `eval_script`, `eval_script_value`, and
  `eval_script_value_with_timeout` (`canopy/mod.rs:464-510`) copy the same
  begin-journal / ensure-finalized / compile / run / record block; `run_config` has the same
  shape. Write one private `eval_journaled(origin, source, run)` and a private
  `ensure_finalized()` (the `if !is_finalized() { finalize_api()? }` idiom appears seven times).
  On `LuauHost`, merge `execute`/`execute_value`/`execute_value_with_timeout` into
  `execute(canopy, node, sid, timeout: Option<Duration>)` and delete `compile_named` (its only
  external caller passes the default name). About −60 lines.

- [x] **1.6 Small core dedupes.** `context.rs:565-579` `clamp_scroll_offset` duplicates
  `layout_driver.rs:839-853` `clamp_scroll` (make the latter `pub(crate)`).
  `WidgetOperationKind` (`world/mod.rs:145-189, 359-378`) mirrors `NodeOperationKind` 1:1; store
  `NodeOperationKind` in `WidgetOperation` and delete the mapping. `Core::new` (`world/mod.rs:195-218`)
  and `add_boxed` (`tree.rs:82-106`) build the same 17-field `Node` literal; add `Node::new(widget)`.
  `create_detached` has four spellings (`add_boxed`, `create_detached`, `create_detached_boxed`,
  `try_create_detached_boxed`); keep `create_detached` + `create_detached_boxed`, fold the
  rollback guard into the latter. The `with_widget_mut` + `CoreContext::new(core, id)` pair
  appears 11 times; add `Core::with_widget_ctx(id, |widget, ctx| ..)`. `dump.rs` `dump` equals
  `dump_with_focus(_, _, None)`, and its one conditional caller (`backend/crossterm.rs:732-736`)
  branches on that; keep one `dump(core, root, focus)`. `node.rs:61-91` has six `pub(crate)`
  getters over `pub(crate)` fields (used only in `script/mod.rs:1265-1286`); use the fields.
  `layout_driver.rs:228-236` `resolve_outer_size` and `tree.rs:411-419` `validate_widget_slot` are
  single-caller pass-throughs. `KeyedChildren::reconcile` has zero callers and `try_reconcile`
  nine: rename `try_reconcile`→`reconcile`, inline `key_at`. `Error::NoResult` is never
  constructed by library code (a test uses it as a sentinel;
  use `Error::Internal` there). `preorder()` can return `impl Iterator<Item = NodeId> + '_` and stop
  exporting `Preorder`. About −150 lines.

- [x] **1.7 Routing twins.** `canopy/routing.rs`: `route_input` wraps
  `route_input_with_scope(.., None)`; `mouse`/`mouse_in_script_scope` and `key`/`key_in_script_scope`
  are pairs; `dispatch_focus_event` and both repeat the "focus_first if none, then focus-or-root"
  prologue. Use `key(scope: Option<&Scope>, ..)`, `mouse(scope, ..)`, one `route_input(.., scope)`,
  and a `focus_or_root()` helper. `diagnostic_dump` (`canopy/mod.rs:1562-1589`) and
  `help_snapshot_for_focus` (`:1500-1531`) contain the same loop (`active_modes` ×
  `bindings_matching_path`, the `anchored_end && depth > 0` kind test, `binding_label`) but
  over different paths: help matches the focus path, diagnostics match the dump target's path,
  and diagnostics also print `mb.info.id`. Extract one
  `fn matched_bindings(&self, path: &Path) -> Vec<HelpBinding<'_>>` that takes the path
  explicitly, add `id: BindingId` to `HelpBinding`, and call it from both; keep target-path
  matching for diagnostics. Proof: the diagnostic-dump and help-snapshot tests pass unchanged.
  About −60 lines.

- [x] **1.8 Layout driver copy sites.** Six hand-copied overflow-inheritance blocks
  (`layout_driver.rs:137-142, 377-382, 410-415, 470-475, 547-552, 576-581`); add
  `Layout::inherit_overflow(x, y)` and call it. `update_canvas:672-675` re-clamps a canvas that
  `compute_canvas:663-666` already clamped; `clamp_axis:757-762` handles `min > max` that
  `Layout::validate` (run on every refresh) already rejects. About −40 lines.

- [x] **1.9 Widget slots do not need a lock.** `Node.widget` is `Rc<RwLock<Option<Box<dyn Widget>>>>`
  (`node.rs:16`); `Rc` makes the type single-threaded, so `parking_lot::RwLock` only adds
  atomics and a misleading signal. Use `Rc<RefCell<..>>` with `try_borrow`/`try_borrow_mut` in
  `widget_access.rs`; the guards map 1:1 (`Ref`/`RefMut`). Keep `parking_lot` for the backend
  `Mutex`. Re-run `cargo xtask dynamic` (the `widget_slot_restores*` Miri filters cover this).

## Stage 2: rendering, style, backend, geometry, events

- [x] **2.1 Prune `canopy-geom` to what canopy uses.** Zero external callers (verified per
  method): `Rect::{area, at, carve_hstart, carve_vstart, carve_vend, clamp_within, inner,
  rebase_point, rebase_rect, shift, shift_within, split_vertical, split_panes, search_up,
  search_down, search_left, search_right, search, sub}` (`expanse` is used only by
  `world/tests.rs`; keep or inline), `LineSegment::{saturating_enclose, abuts, intersects}`,
  `Point::{clamp, scroll_within}`, every method and impl on `PointI32` except the struct itself,
  `From<Rect> for RectI32`, `Size::{area, contains}`, and the error variants that become
  unreachable (`ClampTargetTooSmall`, `PointOutsideRect`, `RectOutsideRect`,
  `PaneColumnCountOverflow`). Delete them with their unit tests. `Rect::search*` is a leftover of
  an older focus-navigation design (focus now uses `RectI32::center`/`overlaps_*`). Also make the
  `Rect` methods that take `&Copy` (`hslice`, `intersect`, `contains_rect`) take by value like
  their siblings. About −650 lines of the crate's 2,170.

- [x] **2.2 `Render` has one construction path.** `RenderTarget`, `Render::new`,
  `Render::new_with_limits`, and `Render::buffer()` (`render.rs:57-131, 318-331`) exist only for
  tests; production uses `new_shared` once (`canopy/rendering.rs:139`). Make the shared-buffer
  constructor the only one (rename to `new`, field `buf: &mut TermBuf`) and update ~12 test
  sites. `Render::resolve_style_name` (`:159`) is an unused pub alias; delete it. Delete
  `Render::solid_frame` (`render.rs:212`) and `TermBuf::solid_frame` (`termbuf.rs:416`): no
  widget calls them; rewrite their tests (`tests/test_render.rs:370-425`, `render.rs` inline
  tests near `:771`) as four `fill` calls on the frame parts and drop the `solid_frame` case from
  `benches/core.rs:348`. About −80 lines.

- [x] **2.3 One grapheme-clipping loop.** `Render::text` (`render.rs:226-283`) calls
  `TermBuf::text`, which already pads `col..max` with spaces (`termbuf.rs:452-465`), and then
  fills the same `pad_rect` again (`:241-248`); the non-solid path re-implements the same
  grapheme walk with a per-cell style. Give `TermBuf` one `text_with(line, txt, style_at)` and
  make both `Render::text` paths use it; delete the redundant pad and the duplicated loop. Same
  shape for `Render::fill` vs `TermBuf::fill`. Existing termbuf/render tests cover behavior.

- [x] **2.4 `TermBuf` constructors and diff.** `empty_with_style_and_limits` is a verbatim copy
  of `new_with_limits` with `ch = '\0'`; collapse the five constructors (`termbuf.rs:203-259`) to
  `new` + `new_with_limits`. Delete `fill_empty` (test-only). In `diff`, the whole-buffer row-shift
  block (`:559-583`, `detect_row_shift`, `buffer_matches_shift`) is the `rect = self.rect()`
  special case of the `_in_rect` variants (`:585-612, 838-940`); keep the rect variants and one
  "shift region + repaint exposed rows" helper. The proptest replay harness covers the
  "diff equals full repaint" contract. About −120 lines.

- [x] **2.5 `RenderBackend` defaults.** `supports_char_shift`, `shift_chars`, and `reset`
  (`render.rs:12-33`) have no default bodies, so eight of eleven impls write the trivial ones;
  `supports_line_shift`/`shift_lines` already default. Add defaults. About −50 lines.

- [x] **2.6 One runloop entry point.** `runloop()` has zero callers; every caller (14 example
  bins and `canopy-mcp/launch.rs`) uses `runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())`,
  which is `ctrl_c: DumpTreeAndExit`, `install_panic_hook: false`,
  `enable_keyboard_enhancements: true`. No caller sets any other value. Delete `RunloopOptions`,
  `CtrlCBehavior`, the never-enabled panic-hook branch (`crossterm.rs:800-812`), and the
  `color-backtrace` dependency; rename `runloop_with_options` to `runloop(canopy)` and hard-code
  the live behavior: Ctrl-C dumps the tree and exits 130, keyboard enhancements on. This changes
  no observed behavior (`TerminalSession::drop` already restores the terminal on unwind,
  `backend/mod.rs:79-83`). Installing a panic hook by default is a separate product choice (see
  Considered and rejected). Public migration: remove the `runloop: RunloopOptions` field from
  `canopy_mcp::LaunchMode::Run` (`canopy-mcp/src/launch.rs:14-18`) and the parameter of
  `run_interactive` (`:71-82`), update `LaunchMode::run`/`run_with_mcp` (`:33, :41`), the 14
  example launchers, `crate::terminal` re-exports in `lib.rs:47-51`, and regenerate
  `api-surface/canopy.rs` and `api-surface/canopy-mcp.rs`. `handle_render_error` should take
  `&Core` and read root/focus itself. Proof: every example binary and `canopy-mcp` compile
  against `runloop`; `cargo tree -i color-backtrace` reports no path. About −80 lines.

- [x] **2.7 Delete the unused mouse-spec operator DSL.** `event/mouse.rs`: nine `Add` impls,
  `From<Button>`/`From<Action>` for `Mouse`, and five cross-type `PartialEq` impls
  (`:31-56, 98-128, 140-205, 289-306`) have no production callers; production builds `Mouse` via
  `parse_spec` and `From<MouseEvent>`, and widgets compare `.action`/`.button` fields. Keep the
  key DSL (`key.rs`), which is used. Rewrite `tmouse` with struct literals. About −130 lines.

- [x] **2.8 One partial-style builder.** `StyleBuilder` (`style/mod.rs:345-389`) and
  `PartialStyle::{with_fg, with_bg, with_attr, with_attrs}` (`:391-455`) are two builders for the
  same value; keep `StyleBuilder` (used by themes and examples) and delete the `with_*` set.
  The `PartialStyle::{fg, bg, attrs}` static constructors stay: `StyleRules` builds every rule
  through them. Delete `StyleMap::add_attr`, `StyleRules::{fg_all, bg_all,
  attr_all, attrs_all}` (`:543-556, 638-693`; zero callers, `style_all` stays), `AttrSet::is_empty`,
  and fold test-only `GradientSpec::new` into `with_stops`. About −120 lines.

- [x] **2.9 Palette-driven themes.** `solarized_dark`, `solarized_light` (`solarized.rs`),
  `dracula` (`dracula.rs`), and `gruvbox_dark` (`gruvbox.rs`) build the same rule structure from
  ~15 role colours each, but the rule sets have drifted: both solarized variants define seven
  `/help/*` rules (`solarized.rs:101-113, 179-191`), while dracula and gruvbox stop after
  `/editor/prompt` (`dracula.rs:93-115`, `gruvbox.rs:101-117`), so the help overlay renders with
  root style in those two themes. Two ordered steps, each its own commit. Step 1 (drift fix,
  acknowledged visual change in dracula and gruvbox help only): add the seven `/help/*` rules to
  both themes, taking each colour from the palette role solarized uses for that path (the
  theme's `frame` colour for `help/frame`, its accent for `help/key`, its `fg`/`bg` for
  `help/content`, and so on). Step 2 (behavior-preserving consolidation): define
  `struct Palette { fg, bg, frame, frame_focused, frame_active, frame_title, accent,
  selection_bg, search_bg, help_bg, .. }` and one `fn theme(&Palette) -> StyleMap`; each theme
  becomes a palette value. Callers: `canopy/mod.rs:360` and `examples/src/stylegym.rs:112-120`.
  Proof: a golden test captures each theme's full ordered rule map (path → partial style) after
  step 1 and asserts it is unchanged after step 2. If the help colours for dracula/gruvbox should
  be chosen by hand instead of mapped, do step 1 differently; step 2 does not depend on which
  colours step 1 picks. About −110 lines.

- [x] **2.10 `Color::rgb()` returns a tuple.** `Color::to_rgb` (`style/color.rs:108-165`)
  returns `Color`, so eight call sites (six in `color.rs:169-239`,
  `canopy-widgets/src/terminal.rs:1007`, `script/mod.rs:1544`) write
  `let Self::Rgb{..} = self.to_rgb() else { unreachable!() }`, and its
  16 named arms duplicate `ansi_to_rgb(0..=15)`. Replace with `fn rgb(self) -> (u8, u8, u8)` and
  one `const ANSI16: [(u8, u8, u8); 16]` table indexed by the named variants. All 44 non-test
  callers of the runtime hex parser `Color::rgb(&str)` (`color.rs:83-104`, which panics on
  malformed input) pass string literals; convert them to the existing compile-time `rgb!` macro
  (`color.rs:53`) and delete the parser (its only variable-input caller is its own unit test).
  Rename the new tuple accessor `rgb()` only after the parser is gone so the two do not collide.
  About −75 lines.

- [x] **2.11 Effects catalogue.** `dim` and `brighten` (`style/effects.rs:70-78`) are the same
  `ScaleBrightness(f)`; keep one `brightness(f)` and update the callers. `swap_fg_bg`, `tint`,
  `underline`, `attr_dim`, `set_attrs`, `clear_attrs`, and the structs behind them have zero
  callers; delete them (re-add on demand). About −60 lines.

- [x] **2.12 Small render/event cleanups.** `Cursor.blink` is written by four widgets and read
  by nothing except an unused `impl Add<Point> for Cursor` (`cursor.rs:24-37`); delete the field,
  its four initializers, and the `Add` impl (blinking is unimplemented; wiring it is a feature,
  not a cleanup). `Key::normalize` (`key.rs:285-313`) has two identical
  shift-clearing arms. `StyleManager::new()` builds a non-reset state that production immediately
  `reset()`s (`style/mod.rs:793-806`, `rendering.rs:221-222`); make `new()` produce the reset
  state. `text.rs:22-27`: the second condition is implied by the first. `BufTest::lines()`
  (`testing/buf.rs:278-296`) hard-codes `'X'` instead of `self.null_char`, and `matches`, `lines`,
  and `line_text` each re-implement the row-to-string loop; add one `row_string(y)`.

## Stage 3: scripting, commands, input, derive

- [x] **3.1 Bindings are Luau closures.** `BindingTarget` (`inputmap.rs:93-106`) has five
  variants; production constructs only `LuauFunction` (`script/mod.rs:2531`). `Command` and
  `CommandSequence` have no constructor anywhere; `Script` is built only in tests;
  `SetInputMode` only via `bind_input_mode`, whose callers are two tests. Delete the four
  variants, `InputMap::bind_input_mode`, `Canopy::bind_input_mode`, `validate_binding_target`,
  `help::extract_command_id`, most of `help::binding_label`, `script::invocation_target`,
  `script::binding_target_summary`, and collapse the `match`es in `routing.rs:385-437`,
  the commit/rollback paths in `canopy/mod.rs:719-746`, and `release_binding_target`. Store the
  `LuauFunctionId` directly (or keep a one-variant newtype). The mode-switch capability stays
  available to apps through Luau: `canopy.bind_with(key, opts, function() canopy.set_mode(m) end)`
  (`base_api.rs:240`). Test migration, exactly: rewrite `tbindings` (`canopy/tests.rs:462-481`)
  and `input_mode_binding_target_switches_modes` (`:517-519`) to install their bindings with
  `canopy.eval_script("canopy.bind_with(...)")` and observe the same outcomes; for the
  `inputmap.rs` unit tests, add a `#[cfg(test)]` constructor `LuauFunctionId::for_test(u64)` and
  a `#[cfg(test)]` helper `InputMap::bind_test(mode, input, path, id)` that calls
  `replace_binding` with a `LuauFunction` target, so no test needs a script host. Update
  `docs/scripting.md:100-103`, which describes Rust command/mode bindings. Proof: the rewritten
  routing and inputmap tests pass; `grep -rn "BindingTarget::" crates` shows only the closure
  form. About −200 lines.

- [x] **3.2 One declaration path for the `.d.luau` surface.** Today the API text is rendered by
  a hand-driven `declaration::Builder` (`script/defs.rs:17-35, 116-239, 403-462`,
  `base_api.rs:470-508`, `DeclRegistryTarget` in `commands.rs:706-780`) while the VM audits the
  same surface through the built `NativeModule`s. `NativeModule::declaration().render()` (ruau
  `ruau-vm/src/api.rs:1055`, `ruau-declaration` `DeclarationSource::render`) already yields each
  module's declaration text. Render `script_api()` in the same order that `prepare_finalize`
  installs modules (`script/mod.rs:3735-3744`): the `preamble.d.luau` header comment, the base
  module declaration, each module registered through `register_script_module`, each owner
  module declaration, then the fixture comment lines. Delete `defs::preamble`,
  `register_owner_declaration`, `FrameworkDeclarationSink`, `base_api::register_declarations`
  and the two `*_function_field` renderers, `DeclRegistryTarget`, and `DeclRegistry::new`. This
  also fixes a gap: modules added via `register_script_module` are audited but never appear in
  `canopy.api()`. Update the golden tail in `canopy-mcp/src/script.rs:786-813`. Proof: a new
  test registers a custom native module and asserts that `script_api()` contains its
  declaration exactly once; the existing declaration-conformance and `luau_check` tests pass.
  About −150 lines.

- [x] **3.3 Replace `proc-macro-error` with `syn::Error`.** `canopy-derive` uses
  `proc-macro-error` (unmaintained; the only reason `syn 1.0.109` is in the lockfile) plus a
  local `thiserror` enum that reports every error at `Span::call_site()`. This item owns every
  call site: return `syn::Result` from `parse.rs` with real spans; replace `abort!`,
  `abort_call_site!`, `ResultExt`, and the `#[proc_macro_error]` attributes in `lib.rs:14-22,
  49-54, 75-78, 119-135, 174-195` and `codegen.rs:1, 533-560` with `syn::Error` values that the
  three macro entry points turn into `to_compile_error()` output; delete `src/error.rs`; then drop
  `proc-macro-error`, `proc-macro-error-attr`, and `thiserror` from the derive crate. Land item
  3.10's `parse_impl_item` change (emit a second `impl` block instead of re-parsing generated
  tokens) inside this item, because those abort paths disappear with it. `canopy-derive` also
  needs `syn`'s `extra-traits` feature, which `thiserror` used to pull in for the model's
  `Debug` derives. Adjust the `parse.rs`
  tests that match on `Error::Unsupported` to message matching. Proof: `tests/derive.rs` passes;
  `cargo tree -i syn@1` reports no path.

- [x] **3.4 Host-function boilerplate.** `script/mod.rs` has 53 `.map_err(|e| canopy_to_host(&e))`
  and 48 `with_current_canopy(scope, ..)` calls, and the "normal context or reentrant bridge"
  branch is written four times (`:572-598, 831-872`). Add `impl From<error::Error> for
  ruau RuntimeError` so host fns use `?`; write one `with_canopy(scope, f)` that owns the branch;
  delete `current_script_anchor` (it is `with_current_canopy(scope, |_, id| Ok(id))`); add a
  `host_value(scope, f)` helper for the ~20 pure getters. `host_cmd_on` (`:2560-2586`) duplicates
  `dispatch_command_by_name` (`:1804-1820`) and downgrades structured errors to
  `Error::Script(..)`; give `dispatch_command_by_name` an `Option<NodeId>`. About −135 lines.

- [x] **3.5 Dead script-host state.** `LuauState.definitions` is written and never read
  (`Canopy.script_api_text` is the real copy); `Script.chunk`/`ScriptCache::chunk` serve only an
  existence check at `:3989`; `compile_chunk` → `compile_chunk_with_runtime_capabilities` →
  `canopy_runtime_capabilities()` is three functions for
  `RuntimeCapabilities::default().compile_source(..)`; `ScriptDiagnostics` (`:401-446`) is two
  `Vec`s whose eight methods `LuauHost` re-forwards verbatim (`:3655-3720`);
  `ScriptCheckResult::ok()` and `ScriptCheckDiagnostic::error()` have zero callers;
  `format_typecheck_diagnostics` (`:745-752`) duplicates `format_script_diagnostics`
  (`canopy/mod.rs:1672-1680`). Exact changes: delete `LuauState.definitions` and its writes,
  `Script.chunk`, `ScriptCache::chunk`, `ScriptCheckResult::ok`, and
  `ScriptCheckDiagnostic::error`; replace the three-function compile chain with one
  `compile_chunk(source, name)` that calls `RuntimeCapabilities::default().compile_source(..)`;
  move the two `Vec`s of `ScriptDiagnostics` onto `LuauState` and delete the eight forwarders
  (callers use the fields through `LuauHost` accessors that remain); keep one formatter as
  `ScriptCheckResult::format_diagnostics(&self) -> String` and delete both free functions.
  About −80 lines.

- [x] **3.6 One base-API table.** `base_api.rs` keeps `BaseFunction` and `AsyncBaseFunction`
  as two structs, two consts, two field renderers, and two registration loops (`:24-46, 49-468,
  490-531`); `wait_for_host_fn` and friends (`script/mod.rs:2144-2157`) exist only to fit the
  async slot type. Use one struct with `handler: Sync(..) | Async(..)`, one table, one loop.
  About −50 lines.

- [x] **3.7 `help.rs` unused API.** `HelpCommand::is_available`, `pre_event_bindings`,
  `fallback_bindings`, `available_commands`, `unavailable_commands`, `OwnedHelpCommand`,
  `OwnedHelpSnapshot.commands`, `OwnedHelpBinding.{path_match, mode, path_filter}`, and
  `HelpCommand.owner` (duplicates `spec.dispatch`) have zero callers; the only consumer of the
  owned snapshot (`canopy-widgets/src/help/mod.rs:247-282`) reads `bindings[].{input, label,
  kind}`. Delete them and the work `to_owned` does to compute them. About −90 lines.

- [ ] **3.8 `commands.rs` dead API and duplicated impls.** Delete `named_args!` (zero users),
  the fallible-args layer `TryToArgValue`, `TryIntoCommandArgs`, `CommandArgs::try_from_args`,
  `CommandSpec::try_call_with` (test-only callers; keep `SerdeArg`, the only way to encode a
  `#[derive(CommandArg)]` struct from Rust), the `Injected<T>` and `Arg<T>` newtypes together
  with only the two `extract_single_generic(inner, "Arg" | "Injected")` branches in
  `canopy-derive/src/parse.rs:328-330` (zero users; keep `ParamKind::Injected`,
  `is_builtin_injected`, and the `Inject` impls for `Event`, `MouseEvent`, and `ListRowContext`,
  which are the live injection path), and `CommandError::DuplicateCommand`
  with `ScriptErrorKind::DuplicateCommand` (never constructed; `CommandSet::add` yields
  `ConflictingCommand`). `InjectError::Failed` is never constructed, so `Inject::inject` can return
  `Option<Self>` and the 13-line `map_err` in `codegen.rs:99-113` becomes `.ok_or(..)`.
  `impl_int_from_arg_value!`, `impl_uint_from_arg_value!`, and the standalone `isize`/`usize`
  impls (`:378-446`) share one body; use one macro over the ten integer types. About −200 lines.

- [ ] **3.9 Command docs have one field.** `CommandDocSpec.short` is always the first sentence
  of `long` because `#[command(desc = ..)]` has zero users, and `defs::command_doc`
  (`:465-488`) de-dups only whole lines, so multi-sentence first lines are emitted twice in the
  generated API (see `root.quit` in `focusgym --api`). Remove `desc`, `DocMeta.short`,
  `CommandDocSpec.short`; emit `long` + `@param` lines. Also delete `#[command(hidden)]` and
  every `.doc.hidden` filter (`help.rs:131,137`, `script/mod.rs:1675`; no command sets it) and
  `#[canopy(type_name = ..)]` (`derive/lib.rs:120-137`; no users).

- [ ] **3.10 Derive codegen tidy.** The `Option<String>` → `Some("..")`/`None` token helper is
  written four times (`codegen.rs:13-19, 58-64, 198-204, 257-268`): write one
  `opt_str_tokens`. `#[doc]` extraction is written twice (`lib.rs:150-166` vs
  `parse.rs:18-46`): keep the `parse.rs` version. `LUAU_VALUES` (`lib.rs:236-239`) is a public
  const used only inside the same generated block: inline it. The three `Foo {..}` literals in
  `tests/derive.rs:201-248` can be `Foo::default()`. (Item 3.3 owns the `parse_impl_item`
  rewrite.) About −40 lines.

- [ ] **3.11 One precedence rule.** `BindingPriority` (`inputmap.rs:56-79`, derived `Ord`) and
  `PathMatch::score()` (`path.rs:165-170`) both define which match wins; compare
  `(m.score(), idx)` tuples and delete the struct. `NodeHandle` (`script/mod.rs:874-902`) wraps
  `NodeId` for no gain (ruau requires only `T: Send + 'static`); register `NodeId` directly.

- [ ] **3.12 Documentation drift.** `docs/scripting.md`: the preamble is a 20-line comment
  header, not the declaration list the doc describes; owner-name normalization is "suffix Luau
  keywords with `_cmd`", not "replace non-identifier characters"; `check_script` takes
  `(source_name, source)`. Fix after 3.1, 3.2, and 3.9 land.

## Stage 4: canopy-widgets

- [ ] **4.1 Empty shells carry their own layout.** `MainPane` (`root.rs:309-323`) and
  inspector `View` (`inspector/view.rs:8-18`) return the default `Layout::column()` and rely on
  a later `set_layout_of` (`root.rs:79, 283`; `inspector/view.rs:34`) for their real layout;
  `PaneColumn` returns `Layout::fill()` and `panes.rs:177` sets it again. `invalidate_layout()`
  marks a node dirty and `refresh_layouts` (`layout_driver.rs:51-67`) then overwrites
  `node.layout` with `widget.layout()`, so an invalidation would silently revert those shells
  to `column()`. Make `MainPane::layout()` return `Layout::fill().direction(Direction::Row)`
  and `View::layout()` return `Layout::fill()`, and delete the now-redundant `set_layout_of`
  calls for the three shells. Public types are unchanged. Proof: root and inspector tests pass;
  `grep -n set_layout_of` in `root.rs`, `panes.rs`, `inspector/view.rs` shows no shell node.
  About −5 lines; removes a latent revert.

- [ ] **4.2 Widget dead state and paths.** `Input::text` and its support (`InputBuffer::text`,
  `visible_range`, `byte_index_for_char`; `input.rs:61-67, 150-168, 211-214, 302-314`) are an
  unreachable second "visible slice" implementation (rendering uses `render_text`; zero
  callers); delete them and remove the `Input::text`/`value` note from
  `api-surface/README.md:80-83` in the same commit.
  `Help.snapshot` and `Help::{set_snapshot, clear_snapshot, snapshot, set_content_snapshot}` plus
  `HelpContent::set_snapshot` are write-only (`help/mod.rs:57-91, 174-178`). `Terminal.app_focused`
  is written twice and never read (`terminal.rs:442,467,961,966`). `ViState.insert_start` has four
  writes and no reads. `font.rs`: `OverflowPolicy`/`LayoutOptions.overflow` are never read;
  `Font::from_ascii_art` always errors and `FromStr` delegates to it (then
  `Error::UnsupportedFormat` is dead); `Glyph` stays a crate-private cache type (`font.rs:62-79`)
  but is exported from `lib.rs:60-63` although no public fn returns it — remove only the export;
  `align_offset` is duplicated in `font_banner.rs:210-219`; `Glyph.bearing_right` is write-only.
  Delete all of it. About −130 lines.

- [ ] **4.3 `Root` install surface.** `Root::install` and `Root::install_with_inspector` have
  zero callers outside root.rs's own test; keep `install_app` and `install_app_with_inspector`.
  The layout/hidden setup at `root.rs:257-279` duplicates `sync_layout` (`:71-100`); call it.
  About −35 lines.

- [ ] **4.4 Shared small helpers.** `DropdownItem` and `SelectorItem` are the same trait with
  the same two impls; keep one `Label` trait. Both `content_size` computations, `Frame`'s title
  width (`frame.rs:150`), and `LogEntry` (`inspector/logs.rs:90`) size text by `.len()` (bytes);
  use `UnicodeWidthStr::width` (already a dependency). `List::item_metrics` takes an
  `available_width` it ignores. `Selector::handle_click` and `Dropdown::handle_click`
  re-implement their own `toggle`/`confirm` commands and return a bool the caller discards; call
  the commands. About −50 lines.

- [ ] **4.5 Editor structure.** `EditorController` (`editor/controller.rs`, 46 lines) and
  `EditorView` (`view.rs`, 53 lines) are single-user indirections with `pub(crate)` fields and
  three delegating wrappers each in `widget.rs`; fold them into `Editor` as fields. `widget.rs`
  (2,570 lines, no inline tests) holds ~1,100 lines of vi handling (`:888-1990`) while `vi.rs`
  holds only state enums; move the vi and prompt/search handlers next to `vi.rs`/`search.rs` as
  free functions over a small `ViContext<'a>` (the workspace enables
  `clippy::multiple_inherent_impl`). Target files: `editor/vi.rs` (state enums plus the vi
  handlers) and `editor/search.rs` (prompt and search handlers); `widget.rs` keeps the `Widget`
  impl, commands, and rendering. `util.rs` (12 lines) should own the
  `if grapheme == "\t" { tab_width(..) } else { grapheme_width(..) }` snippet that appears
  verbatim four times (`buffer.rs:594-598,608-612`, `layout.rs:322-326`, `widget.rs:2146-2150`).
  This item owns the editor file moves. Proof: `editor/tests.rs` passes unchanged. About −100
  lines and a navigable editor.

- [ ] **4.6 Widget-crate hygiene.** `Logs::poll` (`inspector/logs.rs:159-170`) installs a
  global tracing subscriber with `.init()`, which panics if the app already set one. Replace the
  `started: bool` with `enum InstallState { Unattempted, Active, Unavailable(String) }`; `poll`
  attempts `try_init()` only from `Unattempted`, moves to `Active` on `Ok` and to
  `Unavailable(err.to_string())` on `Err` (so it never retries every 100 ms), and the log panel
  renders the `Unavailable` message ("tracing subscriber already installed; inspector logs
  unavailable") so the failure is visible instead of a panic or a silently empty panel. Factor
  the transition into a pure `fn after_install(result) -> InstallState` with a unit test for both
  outcomes; cover the real global-subscriber conflict in one isolated integration-test process.
  (Moving subscriber ownership out of the widget is a design choice; see Considered and
  rejected.) `canopy_widgets::Box` shadows
  `std::boxed::Box` for any importer (`boxed.rs:106`, `lib.rs:54`); rename to `Border`.
  `terminal.rs:1305-1310` `focus_events_enqueue_focus_reports` asserts nothing; assert on the
  enqueued focus report.

- [ ] **4.7 One wrap helper.** `Frame::wrap` (`frame.rs:120-122`) and `Pad::wrap`
  (`pad.rs:30-36`) have zero callers and only forward to `wrap_with`; the two `wrap_with` bodies
  (`frame.rs:118-131`, `pad.rs:31-49`: create the wrapper detached, detach the child, attach it
  under the wrapper) are identical. Delete both `wrap` fns and replace both `wrap_with` bodies
  with one crate-level `pub fn wrap<W: Widget + 'static>(c: &mut dyn Context, child: NodeId,
  widget: W) -> Result<TypedId<W>>` in `lib.rs` (or a `wrap.rs` module); keep the callers
  (`examples/src/textgym.rs:117-118`, `fontgym.rs:205-233`) on the same `wrap_with` names or
  point them at the helper. About −20 lines.

## Stage 5: automation, examples, tooling

- [ ] **5.1 One demo binary.** Eight of the launchers in `crates/examples/examples/*.rs`
  (`chargym`, `editorgym`, `focusgym`, `fontgym`, `framegym`, `listgym`, `stylegym`, `termgym`)
  differ only by type name (`diff listgym.rs chargym.rs` shows six substitution lines);
  `intervals`, `textgym`, `pager`, `cedit`, and `imgview` add a small variation (a file argument
  or no inspector flag). Each repeats the clap flags, `--api` handling,
  `Root::install_app_with_inspector`, and the runloop call. Replace the 13 launchers with one
  `examples/demo.rs`: a clap `Subcommand` per demo (`demo listgym`, `demo pager <file>`,
  `demo cedit <file>`, `demo imgview <file>`, ...), shared `--api` and `--inspector` flags, and
  one `fn run_demo<T: Widget + Loader + 'static>(cnpy, app, inspector) -> Result<i32>` in
  `canopy_examples` that owns install and the `runloop` call (post-2.6 name). Keep
  `examples/widget.rs` as its own target. Delete the 14 `[[example]]` blocks in
  `crates/examples/Cargo.toml:25-79` (autodiscovery finds `demo.rs` and `widget.rs`). Delete
  `src/cedit.rs` (`Ed` is `WidgetEditor` plus one binding and a frame title) and the Luau block
  in `examples/widget.rs::setup_image_bindings` that duplicates `imgview::DEFAULT_BINDINGS`.
  Update the README/docs invocations to `cargo run -p canopy-examples --example demo -- <name>`.
  Proof: `cargo run --example demo -- --help` lists all 13 demos; each demo's `--api` output is
  unchanged from the old binary. About −600 lines.

- [ ] **5.2 canopyctl reuses canopy-mcp.** `crates/canopyctl/src/main.rs` re-implements the
  smoke discovery in `crates/canopy-mcp/src/smoke.rs` with two small semantic differences that
  the merge must settle: `collect_smoke_scripts` (`main.rs:1065-1090`) preserves the order of an
  explicit script list and sorts only discovered files, while `discover_scripts`
  (`smoke.rs:137-160`) sorts both; `fixture_for_script` (`main.rs:1117-1126`) accepts only a
  `Component::Normal` first path component, while `smoke.rs:128-135` accepts any component.
  Keep the canopyctl semantics (explicit order is meaningful for fail-fast runs; a non-normal
  first component is not a fixture name) as the single implementation in `smoke.rs`, make
  `discover_scripts`, `collect_luau_scripts`, and `fixture_for_script` `pub`, and add unit tests
  for explicit ordering, discovered-file sorting, and a `..` first component. Also delete the
  three inline copies of `json_tool_result` (`server.rs:22-26`; make it `pub`), the
  `EvalRequestPayload` mirror of `ScriptEvalRequest` (add `Serialize` to the latter), and the
  `FixtureInfo` mirror (add a direct `canopy` dependency to `canopyctl` and import
  `canopy::FixtureInfo`; do not re-export it from `canopy-mcp`), with their duplicate tests.
  `examples/todo/tests/smoke.rs` covers the in-process `run_suite` path. Then split the
  1,223-line file into `config.rs` (CLI config and `.canopyctl.toml` resolution, `:148-365,
  1018-1039`), `session.rs` (`Session` and `SessionManager`, `:489-695`), `replay.rs` (journal
  types and IO, `:399-476, 1139-1174`), and `main.rs` (CLI structs, proxy server, command
  handlers), and give `SessionManager` one `session()` accessor instead of the five-line
  lock/ensure preamble repeated in five methods. This item owns the canopyctl file split.
  Proof: `canopyctl` unit tests pass; `cargo xtask smoke` passes. About −120 lines.

- [ ] **5.3 canopy-mcp mirrors canopy types.** `canopy-mcp/src/script.rs:54-74`
  `ScriptDiagnostic` and `ScriptAssertion` mirror `canopy::script::ScriptCheckDiagnostic`
  (`script/mod.rs:106-119`) and `ScriptAssertion` (`:97-104`); canopy already depends on
  `schemars` and derives `JsonSchema` on `FixtureInfo`. Derive `Serialize`, `Deserialize`, and
  `JsonSchema` on both canopy types, use them directly in the MCP payloads, and delete the
  mirrors and converters (`:573-583, 674-687`) and `diagnostics_have_errors`
  (= `ScriptCheckResult::has_errors`). Protocol change to acknowledge: MCP diagnostics gain the
  `source: Option<String>` field the converter dropped; update the golden outputs and the
  `docs/agent-loop.md` payload description in the same commit. `AppEvaluator::evaluate`
  (`:295-349`) and `evaluate_live` (`:418-484`) share ~45 lines of
  typecheck→eval→timing→logs→assertions; factor one `evaluate_in(..)`. Delete
  `LaunchMode::Smoke`/`Command::Smoke` in `launch.rs:88-104` and
  `examples/todo/src/main.rs:49-66, 99-114` once canopyctl (`xtask smoke`) and `run_suite`
  (todo test) are the two remaining smoke paths. Proof: canopy-mcp tests pass with the updated
  goldens; `examples/todo/tests/smoke.rs` and `cargo xtask smoke` pass. About −130 lines.

## Stage 6: tests and test infrastructure

- [ ] **6.1 Delete the loom tests and dependency.** `poll.rs:367-428` and
  `terminal.rs:1389-1425` build self-contained models (local enums, channels, threads) inside
  `loom::model` and reference no production item; there is no `cfg(loom)` anywhere. Delete both
  tests, the `loom` dev-dep in both crates, and the `test(loom_)` step in `xtask dynamic` (do all
  three together; nextest fails on an empty filter). About −100 lines.

- [ ] **6.2 One integration-test binary for canopy.** `crates/canopy/tests/` has 15 files
  (3,419 lines, 67 tests), each linking the Luau-bearing lib (~39 MB per binary). Cargo treats
  every `tests/*.rs` file as its own integration crate, so the single root must be one file with
  its modules in a subdirectory: create `tests/it.rs` containing only `mod` declarations, and
  move the files under `tests/it/` with this exact map: `common.rs` (shared widgets and
  helpers), `commands.rs` (from `test_commands.rs` + `test_command_arg.rs` +
  `test_command_errors.rs`), `focus.rs` (`test_focus.rs`), `layout.rs` (`test_layout.rs`),
  `luau_check.rs` (`luau_check.rs`), `node_render.rs` (`test_node_render.rs`), `on_mount.rs`
  (`test_on_mount.rs`), `render.rs` (`test_render.rs`, until 6.7 merges it), `script.rs`
  (`test_script_framework.rs` + `test_script_commands.rs`), `tree.rs` (`test_tree.rs`),
  `viewport.rs` (`test_viewport_scrolling_simple.rs`). No other file may remain directly under
  `tests/`. Delete the duplicated
  `focus_first`/`focus_dir` helpers and the whole test `test_tree.rs:457-480`
  (= `test_focus.rs:201-224`); item 6.3 owns the `attach_grid` copies. Fold
  `test_arg_value_uint.rs` (one test) next to `uint_arg_round_trip` in `commands.rs` and
  `test_core_grid_dimensions.rs` into `testing/grid.rs`. The nextest filter
  `test(tracked_luau)` matches by name, so xtask is unaffected. Proof: `cargo nextest run
  -p canopy` runs every test that ran before except the one deleted duplicate (the two folded
  tests still run as unit tests), and `cargo test -p canopy --no-run 2>&1 | grep -c 'tests/it'`
  shows exactly one integration binary. About −60 lines and 14 fewer link steps.

- [ ] **6.3 `Grid::install` attaches itself.** Every functional caller (nine sites) follows
  `Grid::install` with the same three lines of attach + root layout + `set_root_size(expected)`;
  move that into `install`, derive `dimensions()` and `expected_size()` from one
  `cells_per_side()`, and delete both `attach_grid` copies (`test_focus.rs:46-57`,
  `test_tree.rs:297-308`).

- [ ] **6.4 Testing backends.** `CanvasRender`/`CanvasBuf` (`testing/backend.rs:108-202`) are
  constructed once and never read; `TestRender::create` returns an `Arc<Mutex<TestBuf>>` every
  caller discards; `TestRender::{styleman, buf_text, contains_text}` and
  `TestBuf::{is_empty, contains}` have zero callers. Delete `CanvasRender`, make `TestRender` a
  plain `struct { text: Vec<String> }`. About −140 lines.

- [ ] **6.5 Dead `BufTest`/`Harness` surface.** `BufTest::{with_null, with_any, dump_line,
  line_text, snapshot}` and `Harness::{with_size, mouse, render_snapshot, find_node}` have no
  external callers; delete them and their self-tests (`test_contains_functions` duplicates
  `test_buftest_instance_methods`). Make `Harness::new` delegate to `builder(root).build()` so
  both paths apply the root `Flex(1)` layout. About −120 lines.

- [ ] **6.6 One instrumented node in `ttree.rs`.** `leaf!`, `branch!`, and `R`
  (`testing/ttree.rs:69-290`) define the same handle, `Widget` impl, and `OutcomeTarget` impl three
  times; one `node!(Type [, name] [, command])` macro covers the seven uses. About −110 lines.

- [ ] **6.7 Test placement.** `world/tests.rs` (2,793 lines) mixes layout-engine tests
  (`:1208-2249`) that pin nine `layout_driver` internals as `pub(super)` with tree/lifecycle tests;
  move the layout tests beside `layout_driver.rs` so those items become private, add
  `fixed_leaf(core, w, h)`/`wrap_node(core)` helpers, and use `core.set_layout_of(x, L)` instead
  of 37 three-line `with_layout_of` closures (about −140 lines). `assert_error_context` is
  duplicated verbatim in `world/tests.rs:389-404` and `canopy/tests.rs:321-336`.
  (Item 2.2 landed the `tests/test_render.rs` merge: rewriting the render tests for the single
  constructor forced it, so the unique cases moved into the inline suite and the file and its
  `setup_render_test` are gone.) `testing/mod.rs:19-113` tests `Canopy`
  rendering already covered by `canopy/tests.rs::trender`. Move the largest inline `mod tests`
  blocks (`termbuf.rs` 1,255 test lines, `inputmap.rs` 707, `script/mod.rs` 677, `render.rs` 496)
  to sibling `tests.rs` files, the convention `world/`, `canopy/`, and `editor/` already use.

- [ ] **6.8 Tests that cannot fail.** `examples/todo/tests/basic.rs`: six of ten tests end
  without an assertion, `add`/`del_first`/`del_no_nav` take an unused `_next` parameter beside
  commented-out `expect_highlight` calls, and `#[should_panic]` has no `expected`. Give each of
  the six tests a `list_len` or `tbuf().contains_text` assertion on the state it drives, delete
  the `_next` parameters and commented lines, and add `expected = ".."`. Replace the four hand-rolled
  `env::temp_dir().join(format!("..{nanos}"))` helpers (`test_script_framework.rs:122-133`,
  `canopy-mcp/src/smoke.rs:196-202`, `todo/tests/basic.rs:16-24`, `todo/tests/smoke.rs:15-23`)
  with `tempfile` as a dev-dependency of `canopy`, `canopy-mcp`, and `todo` (automatic
  cleanup). Share the "check_script → join diagnostics → assert ok" block as
  `canopy::testing::luau::assert_typechecks(canopy: &mut Canopy, name: &str, source: &str)`
  (new `core/testing/luau.rs`, behind the existing `testing` feature; both consumers already
  enable it) and call it from `canopy-widgets/tests/luau_check.rs:38-51` and
  `todo/tests/luau_check.rs:49-57`; make `canopy_mcp::smoke::collect_luau_scripts` `pub` and use
  it from `todo/tests/luau_check.rs:12-26` (todo already depends on canopy-mcp).

## Stage 7: crate layout

Do these after the content changes above, so each move is a self-contained commit that changes
module boundaries only (new `mod` declarations, imports, and `pub(super)`/`pub(crate)`
visibility) and no behavior.

- [ ] **7.1 Repair the stale `canopy-core` doc.** `crates/canopy/src/core/error.rs:7` documents
  `Result` as "for canopy-core operations", a crate that no longer exists. Fix the sentence.
  (Flattening `core/` into `src/` was considered and is listed under Considered and rejected.)

- [ ] **7.2 Split `script/mod.rs` (4,938 lines).** Distinct responsibilities and current ranges:
  public types (71-190), host state (191-508), guards and the reentrant Canopy bridge (510-606,
  831-872), VM config/compile (623-829), NodeId host type (874-944), scoped value conversion
  (946-1213), state→record builders (1215-1740), command dispatch (1742-1834), arg parsing
  (1836-1964), async wait helpers (1966-2157), return helpers (2159-2211), error payload
  (2212-2385), marshaled value conversion (2387-2491), 47 `host_*` functions (2493-3299), module
  builders (3301-3369), ruau error adapters (3371-3529), `impl LuauHost` (3531-4260), tests
  (4262-4938). Target files: `mod.rs` (types, state, `LuauHost`), `bridge.rs`, `value.rs`,
  `records.rs`, `errors.rs`, `dispatch.rs`, `tests.rs`, and move every `host_*` fn, `ArgReader`,
  and wait helper into `base_api.rs` beside the table that registers them (`base_api.rs:11-22`
  currently imports ~45 `host_*` names from `super`). Visibility rule: items used only by
  `base_api.rs` become private there; items used by `LuauHost` or `Canopy` become `pub(super)`
  in their new module; nothing new becomes `pub`. Proof: the script test suite and `luau_check`
  pass unchanged, and `api-surface/canopy.rs` shows no new public items. (Items 4.5 and 5.2 own
  the editor and canopyctl splits.)

## Stage 8: final consistency check

- [ ] **8.1 Confirm the contracts agree with the code.** Each item already updated the
  documents it changed. Read `docs/architecture.md`, `docs/scripting.md`, `docs/agent-loop.md`,
  `docs/fixtures.md`, `api-surface/README.md`, and `README.md` once against the finished tree,
  fix any drift, and run `cargo xtask api` to confirm the skeletons are current.

## Considered and rejected

Listed so a later pass does not re-litigate them. The first group needs a product decision
before it could become an item.

- Installing a panic hook in `runloop` by default (2.6): `TerminalSession::drop` already
  restores the terminal on unwind; the hook only makes the panic message and backtrace readable.
  Worth doing, but it is a behavior change no caller has opted into.
- Trimming the `image` decoder list below today's set (0.6): which formats the viewer must open
  is a product choice.
- Moving tracing-subscriber installation out of `Logs::poll` into app setup (4.6): an ownership
  redesign; `try_init()` fixes the panic without it.
- Retaining a native Rust binding entry point (`Canopy::bind_input_mode`) after 3.1: zero
  callers and Luau covers it, but an app author might want it back; re-add on demand.
- A `Context::scroll_dir(Direction)` helper to collapse four local `Direction → scroll_*`
  matches: adds a fifth scroll alias to a budgeted trait for 40 lines.
- Windows support (0.2): the current CI matrix names Windows, but `canopyctl` and `canopy-mcp`
  use Unix-domain sockets without target gates, so no Windows leg can pass today. Supporting it
  means gating or replacing that transport; dropping it means updating the platform contract.
- Merging `Center`, `Modal`, `Pad`, `PaneColumn`, `MainPane`, and inspector `View` into one
  `Container { layout, name }`: all six render nothing and differ only by layout and node name
  (`Center` and `Modal` are twins apart from name and docs), so the merge would remove ~200
  lines of ceremony. Rejected: the type names carry the semantics at the use site
  (`key!(ModalSlot: Modal)`, `TypedId<Center>`, `Pad::wrap_with`), `Modal` is a semantic type
  with a usage convention that may grow behavior, and the reader would lose that for a modest
  saving. Every external review preferred distinct types. Item 4.1 keeps the one technical
  benefit (shells carry their layout).
- Flattening `crates/canopy/src/core/` into `src/`: `core/` is a fossil of the old
  `canopy-core` crate (every public item is re-exported twice, and modules import both
  `crate::core::x` and `crate::x`), but the flattening is a large path-only move with no line
  savings beyond `core/mod.rs`; one reviewer judged it churn. Do it only as a deliberate
  layout decision.
- Public capability removals justified by zero workspace callers (geometry methods in 2.1,
  effects in 2.11, `RunloopOptions` in 2.6, native bindings in 3.1, `Input::text` in 4.2, the
  MCP payload types in 5.3, and the example invocation in 5.1): the plan follows the settled
  zero-backcompat and minimal-surface intent, which the user reaffirmed to cover protocols,
  APIs, and example invocations, and `api-surface/README.md`'s "a smaller or clearer breaking
  surface is preferred". One external reviewer disputes that caller counts decide product value;
  veto individual removals if a capability must stay.
- Deleting the explicit `name()` overrides that equal the default type-name conversion, and the
  empty `#[derive_commands]` blocks on command-less widgets (~130 lines): consistent ceremony,
  not clearly wrong; a convention decision, not a clear win.
- Removing the inspector `Tabs` bar (decorative today; "Stats" is a stated future feature):
  product decision.
- `Line` vs `LineSegment`, `Rect` vs `RectI32`, `layout::Size` alias, `CanvasContext` newtype:
  distinct concepts or churn without structural gain.
- Merging `ArgValue::Int`/`UInt`; unifying scoped vs marshaled value conversion; replacing
  `CommandTypeSpec.rust` with `type_name`: semantic changes, not simplifications.
- A `Clone`-able `CoreState` sub-struct instead of `TreeStateSnapshot` capture/restore: −50
  lines but hundreds of `core.state.` sites.
- `Harness` vs `run_ttree` vs `DummyContext` vs raw `Core` in tests: different layers with one
  canonical use each; only `Grid` and `TestRender` needed tightening.
- Merging the two `#[mcp_server]` impls in `canopy-mcp/src/server.rs`: they differ in transport.
- `Box` widget into `Frame`; `edit.rs` into `buffer.rs`; `tokio` out of `Terminal`: cohesive as-is.
