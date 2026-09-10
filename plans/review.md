# Canopy Deslop and Clear Wins Review

## Description

This is a combined structural simplification (deslop) and clear-wins survey of
the canopy workspace at commit `9bef547e` ("refactor: streamline public APIs",
2026-09-10). The tree was clean at that commit when measured. The survey read
every Rust source file in the workspace, verified consumer claims with
workspace-wide greps, and checked the code against `docs/architecture.md`,
`docs/api-budget.md`, `docs/scripting.md`, `docs/agent-loop.md`,
`docs/fixtures.md`, and `docs/styles.md`. No implementation edits were made.

Baseline at the reviewed commit:

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features` | clean |
| `cargo nextest run --workspace --all-features` | 738 passed, 1 skipped |
| `ncode api --check` | all 7 captures current |
| Rust lines (crates, examples, xtask) | 74,577 |
| `unsafe` blocks | 1 (`crates/canopy/src/core/script/bridge.rs:116`) |
| `#[allow]` sites | 9 |
| TODO/FIXME markers | 0 |

The first test run failed while compiling the sibling path dependency
`ruau-typecheck`, which was being edited concurrently. A retry passed. That is
an environment property, not a canopy defect.

### Assessment

The codebase is in good shape: lints are strict and clean, the architecture
document is detailed, and the recent API refactor removed most raw runtime
access from the public surface. The findings cluster into five themes.

**Dead paths left behind by migrations.** Several mechanisms were superseded
but not removed. The exclusive binding frame stack in `InputMap` has no
production caller since Root moved to modal scopes. The legacy replay journal
format has no producer. The terminal widget carries an itty driver bridge and
a per-widget tokio runtime that only an ignored test exercises. The reentrant
Canopy pointer in the script bridge is the workspace's only `unsafe` block and
exists to work around a borrow the design creates. The wake-handle machinery
and the script module root pipeline, both described at length in
`architecture.md`, have no production consumer.

**Duplicated policy and sources of truth.** Luau record declarations and their
runtime builders are maintained by hand in two files. Protocol label mappings
appear up to four times across three crates. Every vi edit is implemented twice
and the copies already disagree on undo grouping. The help modal's admitted
keys and its footer text live in different modules. Smoke-suite planning is
implemented in both `canopy-mcp` and `canopyctl`.

**Public surface beyond the budget.** `docs/api-budget.md` sets review
thresholds that the current captures already exceed under the document's own
counting rule. The overage is mostly aliases, test-only entry points, and two
`impl dyn` blocks that restate 33 extension-trait signatures. `Canopy` is under
its threshold, but 11 of its 47 methods serve only the in-crate Luau bridge or
tests. Several public items in `canopy-geom`, `canopy-widgets`, and
`canopy-mcp` have zero workspace consumers.

**Documentation drift after the API refactor.** `architecture.md` still
describes `SlotMap` storage, an unsafe access layer, `Root::install_app`, and
the exclusive frame help mechanism. `api-budget.md` carries line thresholds
copied from the deleted ruskel-era README. `agent-loop.md` documents a
bootstrap field that does not exist. `styles.md` documents an `Inactive` state
the code lacks.

**Tooling residue.** The `xtask dynamic` Miri task is referenced nowhere and two
of its three filters target safe code. `clippy.toml` sets one default value and
one threshold that cannot fire. Two shared dependencies are not hoisted to the
workspace manifest.

One latent runtime bug surfaced during the survey (C89): the fontgym focus
frame scrolls through the wrong widget type, so its scroll keys always return
an error.

### API budget measurements

Counts follow the rule in `docs/api-budget.md`: methods in the trait definition
or inherent implementation shown in `api/`, with extension traits counted
toward the surface they extend.

| Surface | Threshold | Actual | Status |
| --- | ---: | ---: | --- |
| `Canopy` | 56 | 47 (11 bridge- or test-only, C128) | under |
| `ViewContext` + `ViewContextExt` | 34 | 33 + 13 = 46 | over |
| `Context` + `ContextExt` | 60 | 53 + 20 = 73 | over |
| `Editor` | 24 | 17 (11 hand-written + 6 generated) | under |

| Capture | Threshold | Lines | Status |
| --- | ---: | ---: | --- |
| `canopy.rs` | 8,826 | 8,642 | under |
| `canopy-widgets.rs` | 2,050 | 1,948 | under |
| `canopy-mcp.rs` | 1,050 | 1,285 | over |
| `canopy-geom.rs` | 575 | 574 | 1 line of headroom |
| `canopy-examples.rs` | 1,050 | 705 | under |
| `todo.rs` | 225 | 74 | under |
| `canopy-derive.rs` | 40 | 50 | over |

Only the `canopy.rs` line threshold was recalibrated for the new capture
format. The other six were copied from `api-surface/README.md`, which the same
commit deleted. The method thresholds for the context traits fit the base
traits alone (33 < 34, 53 < 60), which contradicts the text that says extension
traits are counted. See C105.

### How to read the changes

Changes are grouped as follows. IDs are stable across the document.

- **Deslop batch (C1 to C10).** Ten structural simplifications recommended as
  one coherent batch. Each is evidenced and bounded. Some contain a small
  decision, called out inline.
- **Further structural candidates (C11 onward under that heading).** Structural
  changes that need a design decision or cross-repository work before they can
  be scheduled.
- **Clear wins.** Changes that can be implemented unattended and validated
  mechanically, grouped by area. Not capped.
- **Documentation and configuration.** Drift fixes and recalibration. Most are
  unattended. C105 needs one decision.

IDs are assigned in the order findings were consolidated, so they are not
contiguous within a group. Items inside `crates/canopy/src/core` that are `pub`
but not re-exported from `lib.rs` are already crate-only (`mod core` is private
and `clippy::redundant_pub_crate` is enabled), so this report does not propose
narrowing them.

Validation for every code change is the baseline suite: `cargo clippy
--workspace --all-targets --all-features`, `cargo nextest run --workspace
--all-features`, and `ncode api --check` (or `ncode api` to refresh captures
when public surface changes).

## Implementation checklist

Tick an item immediately when it lands. Record skips with a reason. Validate
with `ncode check`, `ncode test`, and `ncode api --check`.

### Stage 1: Unattended cleanups in `crates/canopy`

Core runtime:

- [x] C34: Invalid node IDs reported as `Error::Internal`
- [x] C35: Removal hook sequence duplicated between replace and remove
- [x] C36: Single-use one-line wrappers over `pub(crate)` fields
- [x] C37: `node_path_label` detects detachment through a rendered string

Scripting and commands:

- [x] C38: Two public script items with no external consumer
- [x] C39: `ZoomDirection` belongs to `ImageView`
- [x] C40: Parallel dispatch shims
- [x] C41: Gas budget literal
- [x] C42: Lossy UTF-8 conversion contradicts the strict policy
- [x] C43: `base_api.rs` and `script/mod.rs` micro-duplication
- [x] C44: `node_id_to_arg` aliases `ArgValue::Node`
- [x] C45: `new()` duplicating `Default`

Rendering, terminal buffer, input, layout:

- [x] C46: Infallible `Result` on input mode setters
- [x] C47: `TermBuf` computes the row-local grapheme span three times
- [x] C48: Unused `io::Write` supertrait on `TerminalOperations`
- [x] C49: Zero-consumer event surface
- [x] C50: `Constraint::max_bound` has zero consumers
- [x] C51: `Render::resolve_style_name_at` has one consumer
- [x] C52: Two replay models in termbuf tests
- [x] C53: `TestRender::render(&mut Canopy)` inverts the receiver
- [x] C54: Crossterm shift parameter names and coordinate casts

Core support files:

- [x] C117: 32 public theme colour constants with zero consumers
- [x] C118: Two copies of the completion-queue admission guard
- [x] C119: The dispatch boundary is hand-rolled at five sites
- [x] C120: `#[derive_commands]` on test nodes with no commands
- [x] C121: Help-panel styles spelled ten times
- [x] C122: `expand_tabs` allocates even when there are no tabs
- [x] C123: `normalize_filter` is `pub(crate)` with one in-file caller
- [x] C124: `BufTest::dump()` has no consumer beyond its own smoke test

Runtime facade:

- [x] C137: Journal recording duplicated with inconsistent clocks
- [x] C138: `refresh_snapshot` alias of `flush` removed (alias only)
- [x] C139: `ScriptErrorKind` labels live in two tables
- [x] C140: `Canopy::eval` is a one-line alias of `eval_headless`
- [x] C141: `View::content_to_screen` and `outer_to_content` zero consumers
- [x] C142: Impossible `Error::Internal` path in default binding compilation
- [x] C143: `BindingTarget` re-exported with no public-signature role
- [x] Refresh `api/canopy.rs` and `api/canopy-widgets.rs`

### Stage 2: Unattended cleanups in widgets, automation, examples, tooling

Widgets:

- [x] C55: `Editor.text_entry_transaction` duplicates buffer state
- [x] C56: Display-metric fallback scans the buffer twice
- [x] C57: `LineChange` and `take_change` leak the layout-cache protocol
- [x] C58: `Editor::selection()` and `Editor::new()` have zero consumers
- [x] C59: `handle_paste` duplicates `handle_insert_text`
- [x] C60: Three copies of the single-line newline policy
- [x] C61: `TerminalColors` is 110 lines for one fixed palette
- [x] C62: `matches_for_line` allocates per visible line per frame
- [x] C63: Two adapters over `ClickTracker::count`
- [x] C64: A field used as a return value, and a constant tuple element
- [x] C65: `font` module publishes items with no consumer
- [x] C66: `Root::HelpState` never returns to `Closed`
- [x] C67: `Root::install` re-enters the root widget to run `sync_layout`
- [x] C68: `Default` impls that exist only for the lint
- [x] C69: `BindingList` decides the gutter twice with two predicates
- [x] C70: `List::clear` redundant reset and `List::remove` visibility

Automation stack:

- [ ] C71: `Todo.pending` and an unreachable branch
- [ ] C72: `luau_check.rs` duplicates the smoke suite's typecheck gate
- [ ] C73: `serve_uds_with_context` is `serve_uds`
- [ ] C74: `SessionManager` forwarders and repeated `touch()`
- [ ] C75: `ReplayEntry` deserializes five unread fields (skip if C9 lands)
- [ ] C76: Replay option validation runs three times (skip if C9 lands)
- [ ] C77: Headless viewport validated twice with a `Size` round trip
- [ ] C78: `HeadlessSession` exists to carry a zero-sized `NopBackend`
- [ ] C79: Fixture re-rejected by an inner function
- [ ] C80: `AppFactory::bootstrap()` has one test consumer
- [ ] C81: Unused `Serialize`/`Deserialize` on smoke outcomes
- [ ] C82: Todo `main.rs` duplicates flags and exit handling
- [ ] C83: `validate_viewport` wraps `Viewport::validate`
- [ ] C84: Nine identical `ToolError::internal` closures
- [ ] C85: `open_adder` resolves `todo.input` twice
- [ ] C86: `script.rs` local redundancies
- [ ] C87: Duplicated test metadata literal
- [ ] C88: `.canopyctl.toml` restates three defaults

Supporting crates, examples, tooling:

- [ ] C89: fontgym `FocusFrame` scrolls through the wrong widget type
- [ ] C90: `canopy-geom` items with zero workspace consumers
- [ ] C91: Two trybuild `pass` cases duplicate unit-test coverage
- [ ] C92: Two type-shape matchers duplicated across derive files
- [ ] C93: `expand_command_arg` loops three times over one field list
- [ ] C94: Call builder re-parses parameter names from strings
- [ ] C95: `scroll`, `page`, `scroll_to` commands copied between two gyms
- [ ] C96: `TerminalStack` is defined twice
- [ ] C97: Two near-identical harness builders in the examples tests
- [ ] C98: Reading a layout through the mutable visitor
- [ ] C99: `print_luau_api` does not print
- [ ] C100: `renderer_from_font` is a misnamed identity wrapper
- [ ] C101: `FontBlock::set_text` and `set_effects` duplicate the mount split
- [ ] C102: Entry style rules duplicated between two demos
- [ ] C103: `futures` and `rand` are not hoisted
- [ ] C104: `canopy-examples` spells out the default widget feature set
- [ ] C110 first line: `clippy.toml` default cognitive-complexity threshold
- [ ] C111: remove local `vendor/` build output
- [ ] Refresh every `api/` capture

### Stage 3: Deslop batch items that need no decision

- [ ] C1: delete the exclusive frame stack
- [ ] C2: delete both `impl dyn` blocks
- [ ] C3 unattended part: five zero-consumer methods, two test-only methods,
      three `View` aliases and 47 call sites
- [ ] C4: synchronous session input; remove runtime and two production deps
- [ ] C5: one `apply_edit`; single key destructure; undo-after-repeat assertion
- [ ] C6: remove the two checkpoint fields; trim two doc sentences
- [ ] C7: label methods, shared eligibility and target declarations
- [ ] C8: `List<Text>` logs, list in `on_mount`, remove `View`, private module
- [ ] C106: `docs/architecture.md` drift (unattended items)
- [ ] C107: `docs/agent-loop.md` and served guide text
- [ ] C108: `docs/scripting.md` drift (parts not tied to decisions)

### Stage 4: Items gated on decisions

- [ ] C3 twin policy decided, then C11 as decided; record in `api-budget.md`
- [ ] C9: `plan_suite`; legacy replay removal if confirmed
- [ ] C10 if approved
- [ ] C105: recalibrate `docs/api-budget.md`
- [ ] C109, C110 second line, C33 as decided
- [ ] C112, C113, C114, C115, C116 second part as decided
- [ ] C20 (a) or (b), then C125 to C136 as decided
- [ ] C126 and C127 taken together
- [ ] C138 visibility with C128
- [ ] Remaining structural candidates C12 to C32 as each decision is made
- [ ] C16 and C20 cross-repo changes coordinated with `ruau` and `itty`

## Changes

### Deslop batch

#### C1: Delete the legacy exclusive binding frame stack

`InputMap` carries two admission mechanisms: the token-based `exclusive_frames`
stack and `modal_bindings`, set by `Core::sync_modal_bindings`
(`crates/canopy/src/core/world/interaction.rs:137-144`). The field comment at
`crates/canopy/src/core/inputmap/mod.rs:282` already calls the frames
"legacy". Root opens help through `open_modal` with
`ModalBindings::Framework(HELP_BINDINGS)`
(`crates/canopy-widgets/src/root.rs:238-244`).
`Context::push_exclusive_bindings` and `pop_exclusive_bindings` are public
(`api/canopy.rs:3128-3139`) and have no caller outside canopy's own tests. The
tree-edit journal tracks `replaced_binding_owners` and `exclusive_frames_before`
(`crates/canopy/src/core/world/mod.rs:106-110`) solely to retire frames
(`crates/canopy/src/core/world/tree.rs:318-327`). Modal scopes already have
their own retirement in `interaction.rs`.

Change: delete `ExclusiveFrame`, `ExclusiveFrameToken`, `exclusive_frames`,
`next_token`, `push_exclusive_bindings`, `pop_exclusive_bindings`,
`retain_exclusive_owners`, `exclusive_frame_tokens`, and
`remove_replaced_exclusive_owners` in `inputmap/mod.rs`. Make
`active_exclusive_group` read only `modal_bindings`. Remove the two `Context`
trait methods, the `DummyContext` stubs (`testing/dummyctx.rs:198-207`), the
`lib.rs:39` and `core/mod.rs:80` re-exports, and the two journal fields with
their bookkeeping in `tree.rs`. Migrate the inputmap unit tests to
`set_modal_bindings`. Convert or drop the four frame-retirement tests in
`world/tests.rs:1437-1478`. Rewrite `docs/architecture.md:115, 256-257, 276-280`
and the comment at `root.rs:362` and test name at `root.rs:837` (see C106).
About 130 lines of runtime code plus public surface. Effort M. Unattended.

#### C2: Delete the `impl dyn ViewContext` and `impl dyn Context` forwarding blocks

`crates/canopy/src/core/context.rs:457-514` (13 methods) and `:1024-1152`
(20 methods) restate every `ViewContextExt` and `ContextExt` method as an
inherent method on the trait object, each body a one-line forward. The blanket
impls at `context.rs:455` and `:1022` already make every extension method
callable on `dyn` receivers when the trait is in scope. `prelude.rs:4-5`
exports both traits. No crate outside canopy names either trait directly, and
`canopy-derive` codegen emits no extension-method calls. The blocks are
invisible to the API capture, so the budget review cannot see them.

Change: delete both blocks and their `#[allow(missing_docs)]` attributes. Add
`use canopy::{ContextExt, ViewContextExt}` (or `prelude::*`) to the files the
compiler flags. The surveyor counted 48 files that call extension methods
without importing an extension trait or the prelude, so expect that many
one-line import edits. No behavior change. Effort M, mechanical and
compiler-guided. Unattended.

#### C3: Cull the context trait surface

`ViewContext` plus `ViewContextExt` has 46 methods against a threshold of 34.
`Context` plus `ContextExt` has 73 against 60. The overage is mostly aliases
and test-only entry points.

Unattended part:

- Zero workspace consumers: `Context::restore_mouse_capture`
  (`context.rs:608-609`, impl `:1334-1336`, stub `dummyctx.rs:184-186`),
  `Context::invalidate(Invalidation)` (`:680-683`, override `:1371-1376`),
  `ViewContextExt::all_in_tree` (`:375-381`),
  `ViewContextExt::focused_or_first_descendant` (`:430-443`),
  `ContextExt::try_with_typed_slot` (`:987-996`). Delete all five with their
  `NodeCtx` impls and stubs.
- Test-only with a mechanical replacement: `Context::clear_layout_override_of`
  (`:698-699`. Sole consumer `crates/canopy-widgets/tests/layout_override.rs:50`,
  replace with `set_layout_override_of(n, LayoutOverride::default())`) and
  `ContextExt::with_slot` (`:905-915`. Sole consumers
  `crates/examples/src/tests/framegym.rs:27-42`, which already define
  `FrameSlot` and `PatternSlot`. Switch to `with_typed_slot` and inline
  `with_slot` into it). Keep `Core::clear_layout_override_of` for in-crate
  layout tests.
- `View` aliases: `ViewContext::view_rect`, `view_rect_local`,
  `outer_rect_local` (`context.rs:170-183`) return `self.view().x()`. Delete
  them and rewrite the 47 call sites to `ctx.view().x()`. This is the alias
  category the budget document says earns no headroom.

These remove 10 methods (view surface 46 to 39, context surface 73 to 67).

Decision needed: the 15 `x()` / `x_of(node)` twins (`view/view_of`,
`layout/layout_of`, `children/children_of`, `is_focused/is_focused_of`,
`is_on_focus_path/_of`, `child_slot/_of`, `set_children/_of`, `set_hidden/_of`,
`with_layout/_of`, `set_layout/_of`, `add_child/add_child_to`,
`add_slot/add_slot_to`, `get_slot/_of`, `get_or_create_slot/_of`, and
`has_slot` = `get_slot().is_some()`) are the remaining overage. Either record
the current-node conveniences as accepted policy in `docs/api-budget.md` and
raise the thresholds, or drop the current-node forms. Five further methods are
test-only (`locate`, `typed_id`, `find_one`, `preorder`, `take_mouse_capture`,
see C11). Effort S for the unattended part.

#### C4: Drop the terminal widget's tokio runtime and itty-script dependency

Each `Terminal` builds a `tokio::runtime::Runtime` with one worker thread
(`crates/canopy-widgets/src/terminal.rs:88-92`) whose only job is
`queue_input`: spawn `handle.send_input(bytes)`, which travels through the itty
`DriverHost` command queue to the next `poll_nonblocking` up to 16 ms later.
The same file already calls the synchronous `session.send_key` (`:611`) and
`session.paste` (`:577`). itty exposes `Session::send_input_bytes` and
`Session::send_focus_report` for exactly the two `queue_input` callers
(`send_mouse_sequence` `:563`, `sync_focus` `:649`). The only tokio import in
the crate is `terminal.rs:29`. `itty-script` is a production dependency of the
`terminal-widget` feature (`Cargo.toml:14, 48`) but its only use is inside
`#[cfg(test)]` feeding one `#[ignore]`d test (`terminal.rs:1091, 1455`).
`docs/architecture.md` says "There is no eager scheduler thread per
application". This is one per widget.

Change: replace the two `queue_input` calls with the synchronous session
methods. Delete `queue_input`, `DriverRuntime::queue_input`, the `runtime`
field, and the tokio import. Move `tokio` (feature `rt-multi-thread`) and
`itty-script` to `[dev-dependencies]`. Drop `dep:tokio` and `dep:itty-script`
from the feature list. Update the feature table row in `architecture.md`.
Effort S. Unattended.

Decision needed (C21): the remaining `DriverHost`/`DriverHandle` bridge
(`terminal.rs:73-109, 335-374, 806-810`) is pumped every 16 ms per terminal
and has no production producer. Gate it behind `cfg(test)` or a feature, or
delete it with the ignored test.

#### C5: One implementation for vi edits

Every repeatable normal-mode edit in `crates/canopy-widgets/src/editor/vi.rs`
has a hand-written body in the key handler (`:215-597`) and a second in
`repeat_last_edit` (`:1249-1311`). The copies already disagree. Key paths open
the text-entry transaction before the edit (`o` `:258`, `O` `:273`, `C` `:578`,
`cc` `:665`) so the structural edit and typed text undo together. Repeat paths
open it after (`:1276, 1287, 1297, 1308`), so `.` leaves two undo steps. `x`
updates the yank register (`:559`) but its repeat `DeleteChar` does not
(`:1278-1280`). Only `Ctrl+r` checks modifiers (`:361-364`). Every other
`Char(..)` arm matches with `..`, so Ctrl and Alt chords act as the plain key,
unlike Text mode (`widget.rs:475-478`). The ~40 match arms each repeat the full
`Event::Key(key::Key { key: key::KeyCode::Char('x'), .. })` pattern, roughly
380 lines.

Change: one `apply_edit(&mut self, edit: &RepeatableEdit)` is the sole
implementation. Key handlers call `record_and_apply(edit)` and
`repeat_last_edit` calls `apply_edit` on the stored edit, with key-path
semantics as canonical. Destructure the key once, reject Ctrl and Alt up front
except for the explicit `Ctrl+r` arm, then match on `key.key`. Add an undo
assertion after `.` to `editor/tests.rs` (the existing repeat test at
`tests.rs:481-492` has none). Effort M. Unattended under the stated rule.
Optional and not unattended: a shared `Motion` enum for normal and visual
modes, which would add motions to visual mode.

#### C6: Stop checkpointing the command registry and scope stack

`TreeStateSnapshot` clones `commands: CommandSet` and `command_scope:
Vec<CommandScopeFrame>` on every tree edit including nested edits
(`crates/canopy/src/core/world/mod.rs:161-164`. Capture `tree.rs:34-35`,
restore `:53-54`). Neither can change during an edit. `Core.commands` is
mutated only by `Canopy::add_commands` (`core/canopy/mod.rs:1068`), which
needs `&mut Canopy` that no widget callback or Luau function can reach.
`command_scope` is pushed and popped in balanced pairs around each dispatch
(`context.rs:1435-1437`, `canopy/routing.rs:261, 276`,
`world/dispatch.rs:23-25, 51-53`).

Change: remove the two fields from `TreeStateSnapshot`, `capture`, and
`restore`. Trim "command registry and scope" from the checkpoint sentence in
`docs/architecture.md` and the `edit_structure` doc comment
(`context.rs:707-708`). The `StructuralSnapshot` test helper in
`world/tests.rs:54-107` can keep asserting `commands` unchanged. Effort S.
Unattended.

#### C7: Consolidate script protocol labels and record builders

Four label mappings are spelled out repeatedly across three crates:
`CommandRequirement` to string at `script/records.rs:357-361, 395-399` and
`canopy-mcp/src/script.rs:454-458`. `CommandStatus` at `records.rs:49-53,
338-341` and `canopy-mcp/src/script.rs:437-443`. `BindingPhase` at
`records.rs:283-286, 636-640, 648-652` with the reverse parse at
`base_api.rs:630-639`. Owner from `CommandDispatchKind` at `records.rs:419-422`,
`canopy-mcp/src/script.rs:433-436`, `commands.rs:1481-1484, 1488-1491`.
`BindingScope::label` and `BindingTarget::label` (`inputmap/mod.rs:100, 136`)
show the convention already exists. Within `defs.rs` the `command_target` table
is declared twice (`:218-227, 339-350`) and the eligibility block twice
(`:276-289, 351-367`). `records.rs` builds the eligibility block twice
(`:437-453, 659-673`). The external NodeId token record is built in
`bridge.rs:181-191` and again in `commands.rs:541-544`.
`Error::ScriptStructured { kind, command: None, owner, message }` is written
longhand seven times (`invocation.rs:162-167, 244-249, 254-259, 262-267`,
`canopy/mod.rs:580-585`, `turn.rs:525`).

Change: add `CommandRequirement::as_str`, `CommandStatus::label`,
`BindingPhase::label` and `parse`, and `CommandDispatchKind::owner`. Replace
all sites. Extract a `CommandTargetInfo` alias and one eligibility field list
in `defs.rs`, and one `insert_command_availability` in `records.rs`. Add one
`node_token_fields(NodeId)` used by both token builders. Add an
`Error::script_structured(kind, message)` constructor with `.with_owner`.
Effort S to M. Unattended.

Decision needed (C15): whether to go further and define each Luau record once
in Rust so the declaration and the `ArgValue` come from the same field list.
`docs/scripting.md` claims "the text and the audited surface therefore cannot
drift apart", which is true for function signatures but not for record fields.

#### C8: Collapse the inspector logs subtree

`LogEntry` (`crates/canopy-widgets/src/inspector/logs.rs:24-105`) is a widget
that textwrap-wraps a string, draws a `█` bar when selected, and pads the text.
`Text` already wraps and caches, and `List::with_selection_indicator` already
draws a per-row indicator. `crates/examples/src/listgym.rs:182` and
`render_tests.rs:195-218` use exactly this combination. The inspector tree is
`Inspector -> Frame -> View -> Logs`, where `View` (`inspector/view.rs`, 33
lines) has no render, events, or commands. `Logs` builds its list lazily in
`poll` via `ensure_tree` (`logs.rs:176-178`) and every command re-checks the
slot through `with_list` (`:221-241`), which can return
`Error::Internal("logs list not initialized")`. `pub mod inspector` is public
(`lib.rs:34-36`) but `Inspector::new` is private and `install` is
`pub(crate)`, so the capture shows an unconstructible unit struct.

Change: replace `List<LogEntry>` with `List<Text>` plus the selection
indicator and `Text::with_wrap_width(78)`. Add `accept_focus` to `Logs`. Delete
`LogEntry`. Create the list in `on_mount` and delete `ensure_tree`. Make
`Logs` the frame's direct child and delete `view.rs`. Change `pub mod
inspector` to `mod inspector`. Add one buffer-snapshot test with a long log
line. The one behavior difference is that initial inspector focus lands on
`logs` rather than a row, which the `path = "logs"` bindings match either way.
Effort S. Unattended.

#### C9: One smoke-suite planner and one replay format

`crates/canopyctl/src/main.rs:328-363` re-implements
`canopy_mcp::run_suite` (`crates/canopy-mcp/src/smoke.rs:66-87`): discover
scripts, read source, derive fixture, build `ScriptEvalRequest`, break on
`fail_fast`. Only the evaluator and the reporting differ. `discover_scripts`
and `fixture_for_script` are public only so canopyctl can duplicate the loop.
Separately, `canopyctl replay` parses two on-disk formats
(`crates/canopyctl/src/replay.rs:53-59, 166-183, 204-286`). Only the
`canopy.replay/1` envelope has a producer (`replay.rs:192-202`). The legacy
`{"journal": [...]}` shape has none in the workspace and requires three CLI
flags (`--legacy`, `--viewport`, `--fixture`, `main.rs:130-143`) plus a
two-way conditional validator (`main.rs:470-499`) that runs three times
(`main.rs:381, 393, 396-398, 430-435`).

Change: add `plan_suite(config) -> Vec<SuiteScript>` in `canopy_mcp::smoke`
that returns path, fixture, and request. `run_suite` and canopyctl both iterate
it. Then make `discover_scripts` and `fixture_for_script` `pub(crate)` (the
only other consumer, `examples/todo/tests/luau_check.rs`, goes away with C72).
Delete the `Legacy` replay variant, the three flags, `validate_replay_options`,
`validate_fixture`, `ReplayInput`, `ReplayJournal`, `ReplayEntry`, their tests,
and the legacy paragraphs in `docs/scripting.md:412-419` and
`docs/agent-loop.md:139-143`. Effort M.

Decisions needed: the public shape of the plan type, and confirmation that
the legacy replay format is not wanted (it is documented but unused). If the
legacy format stays, C75 and C76 still apply.

#### C10: Keep scroll and canvas in one place on `Node`

`Node` stores `scroll` and `canvas` as fields (`crates/canopy/src/core/node.rs:52-61`)
and again inside `view: View`. Every writer updates both:
`layout_driver/mod.rs:197-198, 641-646, 707-711` and `context.rs:304-305`
(`update_scroll` writes `node.scroll` then `node.view.scroll = node.scroll`).
`validate_view_cache` (`tree.rs:613-642`, 30 lines) exists to detect drift
between the copies.

Change: drop `Node.scroll` and `Node.canvas`. Have `layout_node` write
`node.view.canvas` and clamp `node.view.scroll` in `update_canvas`. Delete the
sync line and the scroll and canvas branches of `validate_view_cache`.
Effort M.

Decision needed: whether `rect` and `content_size` also fold into `view`. They
are read before `update_views` runs, so they probably stay. The change touches
layout-pass ordering and the `view_has_cached_state` heuristic
(`tree.rs:605, 1209-1211`), so it is not unattended.

### Further structural candidates

These need a design decision, an API choice, or a change in a sibling
repository before they can be scheduled. Each states the decision.

#### C11: Test-only public context methods

`ViewContext::locate` (`context.rs:233-234`. Only `tests/it/tree.rs:154`),
`ViewContextExt::typed_id` (`:357-360`. Only `world/tests.rs` and
`canopy-widgets/src/render_tests.rs:203`), `ViewContextExt::find_one`
(`:383-392`. Only `render_tests.rs:203`), `ViewContextExt::preorder`
(`:362-365`. Only `tests/it/tree.rs:120`), and `Context::take_mouse_capture`
(`:605-606`. Only `#[cfg(test)]` code in `root.rs:744, 831` and
`list.rs:921, 1195`, all as a getter). Production hit-testing calls
`Core::locate_node` directly (`canopy/routing.rs:88, 103`,
`script/base_api.rs:1270`). Decision: drop them and move the tests in-crate, or
record them as deliberate API. For `take_mouse_capture`, a read-only
`mouse_capture()` query would serve the tests without a mutating primitive.
Effort S each.

#### C12: Replace `DummyContext` with a real-core test context

`crates/canopy/src/core/testing/dummyctx.rs` is a 340-line stub of about 50
methods returning `Ok(())`, `None`, or `Unchanged`. Four trait defaults exist
only for it (`modal_is_open` `context.rs:225-228`, `open_modal` and
`close_modal` `:615-628`, `invalidate` `:681-683`). `NodeCtx` overrides all
four. Every method added to `Context` costs a stub here. Consumers: 7 sites in
`canopy-derive/tests/derive.rs`, 6 in widget unit tests, 5 in
`tests/it/commands.rs`, 1 in `crates/examples/examples/widget.rs:319`.
Decision: provide `testing::with_test_context(|ctx| ...)` built on
`Core::new()` and `CoreContext::new`, port the 19 sites, delete the stub, and
make the four defaults required. The stub's "fake success" semantics mean each
test needs checking against real-core behavior (`widget.rs:319` expects
`on_mount` to fail against the dummy). Effort M.

#### C13: `find_nodes(&str)` hides filter parse errors

`ViewContext::find_nodes` (`context.rs:253-258`) returns an empty `Vec` for an
invalid filter string. `find_one` (`:383-392`) returns the parse error for the
same input. `docs/architecture.md`: "Do not hide internal errors behind
harmless defaults." Rust consumers: `root.rs:516, 814`,
`testing/harness.rs:191`, `crates/examples/src/tests/help.rs:30, 80`.
Decision: return `Result<Vec<NodeId>>`, or drop `find_nodes` and have callers
use `PathFilter::normalized(..)?` with `find_nodes_matching`, which also removes
one method. Effort S.

#### C14: One evaluation driver for `ScriptInvocation`

`LuauHost::execute` (`crates/canopy/src/core/script/mod.rs:1009-1055`) is a
self-contained synchronous driver with its own poll loop, deadline and abort,
gas accounting, diagnostics publication, and tokio current-thread bootstrap.
`turn.rs` implements the same four things again (`:283-330, 441-482, 555-568`).
`RuntimeBuilder::new_current_thread()` appears exactly twice in the crate
(`mod.rs:1044`, `turn.rs:563`). The drivers already differ: `execute` never
journals or prepares a frame, and admission checks diverge. Non-test callers of
`execute`: `canopy/builder.rs:170`, `canopy/mod.rs:726, 951`. Decision: route
startup, config, and builder scripts through the turn driver with a non-`Eval`
`ScriptOrigin`, or extract one `drive_invocation` on `Canopy` that both paths
call. The turn driver currently refuses to run during `finalize` and startup,
so the first option needs an admission rule. Effort L.

#### C15: Define each Luau record once

`script/defs.rs:85-494` declares record aliases (`NodeInfo`, `TreeNode`,
`BindingInfo`, `CommandInfo`, `ScreenCell`, `RouteTraceEntry`,
`AvailableBinding`, `BindingSnapshot`, `ScriptJournalEntry`, `NodeSnapshot`,
`FrameSnapshot`, `WidgetSemantics`, and more) and `script/records.rs:15-761`
builds the matching `BTreeMap` records. Nothing links a declared field to a
built key. A renamed key compiles and passes the golden test. Decision: a small
record builder that takes `(name, Type, value)` triples so declaration and value
come from one list, or reuse of the `CommandArg` derive path with explicit
`NodeId` handling. Minimum alternative: a test that walks each alias's declared
fields and asserts each key appears in the runtime record. Effort M to L.
Depends on C7.

#### C16: Remove the reentrant Canopy pointer

`script/bridge.rs:88-128` keeps a thread-local stack of `NonNull<Canopy>`.
`host_send_key` (`base_api.rs:1331, 1346, 1388`) runs inside
`with_current_canopy`, which holds the scope's `&mut Canopy`, pushes the raw
pointer, then calls `canopy.key(..)`. Routing may call a Luau binding whose host
functions cannot get the scope context (the outer borrow is live), so
`with_reentrant_canopy` dereferences the pointer into a second live
`&mut Canopy` (`bridge.rs:116`, the workspace's only `unsafe`). Two live
`&mut Canopy` is aliasing under Rust's model even though it works in practice.
Decision: release the scope context around routing and re-install it for nested
callbacks (needs a ruau "take context / restore" API), or thread `&mut Canopy`
from `Canopy::key(Some(scope), ..)` into `call_function_in_scope` so host
functions receive it through the scope. Then delete `REENTRANT_CANOPY`,
`ReentrantCanopyGuard`, and `with_reentrant_canopy`. Effort L, cross-repo.

#### C17: Drop `NodeInfo.visible`

`records.rs:187-188` and `defs.rs:556-559` carry `visible` as a documented
legacy inverse of `hidden` (`docs/scripting.md:350`). No `.luau` file or Rust
string reads it. Decision: remove the field, declaration, and doc sentence. Typed
scripts that read `.visible` would fail typecheck, so this is a script API
removal. Effort S.

#### C18: Drop or document `kind = "anchor"` command targets

`base_api.rs:971-973, 988` and `defs.rs:140` accept `kind = "anchor"`. Omitting
the target already means anchor. `docs/scripting.md` lists only `exact`, `from`,
`focus`. No consumer uses `"anchor"`. Decision: remove it from the literal union
and parser, or document it. Removal is the simpler contract. Effort S.

#### C19: Remove unbindable media and modifier key codes

`KeyCode::Media` and `KeyCode::Modifier` with their 27-variant enums
(`event/key.rs:80-142, 201-204, 475-476`) are mirrored from crossterm
(`backend/crossterm.rs:760-790`), but `parse_key_code` (`key.rs:347-391`)
cannot produce them, so no binding can target them, and no widget matches them.
About 95 lines of public enum surface. Decision: drop them at translation like
key releases and delete the enums, or add parse names if terminal passthrough
is planned. Effort S.

#### C20: Wake-handle machinery: give it a consumer or delete it

`core/wake.rs` (353 lines), `Context::wake_handle`, `NodeWakeHandle`,
`WakeOutcome`, `WorkLifetime::Attachment`, `Node.attachment_generation`,
`WorkStamp.attachment`, `Widget::poll_lifetime`, and the provisional and commit
registration protocol threaded through tree edits and `Drop for Canopy`
implement cross-thread producer wakes with attachment-scoped expiry. No widget
in the workspace uses any of it. `wake_handle` callers are the trait, its impl,
`dummyctx.rs:303`, and tests. `NodeWakeHandle`, `WakeOutcome`, and
`WorkLifetime` have zero hits outside `crates/canopy/src`. No widget overrides
`poll_lifetime`, and all seven `poll` implementations are timer-based.
`docs/architecture.md` names "detached terminal polling" as the motivating
case, yet `Terminal::poll` (`terminal.rs:806-810`) returns `Some(16 ms)`
unconditionally, including when the session failed to mount and after the
child exited. itty has the hook shape (`RepaintNotifier`,
`Session::add_repaint_notifier`) but the method is `pub(crate)` in itty.

Decision: (a) give the feature its consumer: make the itty notifier public,
install a notifier in `Terminal::on_mount` that calls `wake.wake()`, return
`None` from `poll` (or a slow keepalive while the child lives), and wire `Logs`
the same way. Or (b) delete `WakeRegistry`, `NodeWakeHandle`, `WakeOutcome`,
`WorkLifetime::Attachment`, `Node.attachment_generation`,
`Widget::poll_lifetime`, reduce `WorkStamp` to node and incarnation, simplify
`poll_runtime_wake` and `turn_inner`, and cut the three architecture
paragraphs. Unattended sub-step now: return `None` from `Terminal::poll` when
`self.session.is_none()`. Effort L, and (a) is cross-repo.

#### C21: The itty driver bridge in `Terminal` has no production producer

`terminal.rs:73-109, 335-374, 806-810`: `mount_session` calls
`driver::attach`, and every poll runs `host.poll_nonblocking`. The only way to
obtain the `DriverHandle` that feeds it is `#[cfg(test)] driver_handle()`
(`:336-339`), used by the ignored test at `:1464`. Per terminal, per tick,
canopy pumps a queue nothing enqueues into. Decision: is scripted driving of
canopy terminals through `itty-script` a supported capability? If not, delete
the bridge and the ignored test. If so, gate `DriverRuntime` and `attach`
behind `cfg(test)` or a feature. Effort M. Follows C4.

#### C22: `Input` should scroll through the runtime view

`InputBuffer` (`input.rs:18-153, 244-276`) keeps its own `scroll` and
`view_width`, re-derives them each render, clamps in its own
`ensure_cursor_visible`, and slices the visible window itself. `Editor`
(including `multiline=false`) reports a canvas and calls `ctx.scroll_to`
(`editor/widget.rs:274-331`). Two scroll models in one crate, about 60 lines of
bookkeeping. Decision: adopt canvas scrolling for `Input` (changes how a
fixed-width parent interacts with `measure`, which today reports full text
width). Effort M.

#### C23: `TerminalConfig::with_on_exit` has no non-test consumer

`terminal.rs:257-258, 283-290, 311-312, 353, 617-631, 808`: `on_exit`,
`exit_notified`, and `sync_exit_status` (run every poll) invoke a callback once
when the child exits. Only the ignored test uses it (`:1458`). The widget
already paints the exit status itself (`:690-703`). Decision: remove the
capability, or keep it and add a real consumer such as termgym closing the tab.
Effort S.

#### C24: `SharedClipboard` is a write-only sink

`terminal.rs:45-71, 350, 567-570, 600-603`: `copy_selection` stores the
selection into a private `Mutex<String>` no canopy code reads, so
Ctrl+Shift+C appears to work and does nothing. itty ships `SystemClipboard`
under its default feature. Decision: install `SystemClipboard` (this also lets
OSC 52 from the child write the user's clipboard) or drop the chord and shim.
Effort S.

#### C25: `image` feature list is the default set minus `avif`

`crates/canopy-widgets/Cargo.toml:25-41` declares `default-features = false`
then enables 14 of the 15 default formats plus `rayon`. The only consumer is
`image::open(path)` in `image_view.rs:342-347`. `exr`, `tiff`, `qoi`,
`image-webp`, `rayon`, and `crossbeam-*` enter the graph only through this
list. Decision: the supported format list is a product choice. A minimal set
would be `jpeg`, `png`, `gif`, `webp`, without `rayon`. Effort S.

#### C26: `Panes` keeps parallel vectors and leaks deleted panes

`panes.rs:30-35, 48-63, 97-161`: `columns: Vec<Vec<NodeId>>` and
`column_nodes: Vec<NodeId>` are kept in sync by hand, with a truncating
`.take(columns.len())` and guards. `delete_focus` drops IDs from the vectors,
and `sync_layout` then calls `set_children_of`, which detaches (`parent =
None`, `tree.rs:938-943`) but never removes. The pane subtree and its column
node stay in the arena with their widget slots and node-lifetime work. Only
consumer: `crates/examples/src/listgym.rs:249`. Decision: one
`Vec<Column { node, panes }>` with `delete_focus` calling `remove_subtree`, or
rename to `detach_focus` and document caller ownership. Effort M.

#### C27: Help controls are described in two modules

`root.rs:363-394` registers the 13 admitted help keys. `help/panel.rs:48-55`
hard-codes the footer summary text. The footer test (`help/tests.rs:308-340`)
asserts strings, not bindings. Rebinding a help key leaves the footer stale.
Decision: move `register_help_bindings` and `HELP_BINDINGS` into `help/` and
drive both from one `HELP_CONTROLS` table with an optional footer label per
entry. Which keys the footer summarizes and how aliases (j/k, g/G, Space) group
is the decision. Effort S to M.

#### C28: `Dropdown` and `Selector` share row-list mechanics

`dropdown.rs:80-91, 106-119, 134-150, 176-199, 216-226` and
`selector.rs:75-86, 117-127, 138-147, 181-213, 218-228` independently
implement max-label-width sizing, click-row lookup, the
`saturating_add_signed(delta).min(len-1)` clamp (a third copy is in
`list.rs:437-443`), and the visible-row render loop. About 60 parallel lines.
Decision: a private `rows.rs` with `max_label_width`, `clicked_row`,
`clamp_offset`, and a visible-row iterator. Single-select and multi-select
behavior stays distinct. Effort M.

#### C29: `Button` composes three child nodes for a bordered label

`button.rs:21-23, 113-131, 148-150`: `Border -> Center -> Text` slots built by
`sync_label` in `on_mount`, with no re-sync path because `label` is immutable.
`Frame` draws its border inline with `BoxGlyphs::draw` (`frame.rs:100-142`),
and `BoxGlyphs::draw` is `pub(crate)`. Three arena nodes and three slot types
per button. Decision: render border, fill, and centered label directly in
`Button::render`. Delete the slots, `sync_label`, `on_mount`, and the
`button_label_role_survives_an_extra_center` test (`:307-346`) whose purpose is
to defend the subtree's style layering. Whether that layering is a contract is
the decision. Effort M.

#### C30: `LineSegment` and the slice family serve one private computation

`canopy-geom/src/linesegment.rs` (244 lines), `Rect::{hslice, hextent, vslice,
vextent}` (`rect.rs:65-110`), and the error variants `ZeroLengthWindow`,
`WindowOutsideView`, `ExtentOutsideRect` (`error.rs:18-36`) are public API used
only by `View::vactive` and `View::hactive`
(`crates/canopy/src/core/view.rs:120-150`). `LineSegment` is never named
outside canopy-geom. About 60 lines of `api/canopy-geom.rs`. Decision: move
the scrollbar math into geom as one axis-agnostic method and make
`LineSegment` private, or keep it crate-private and expose only the
`vactive`/`hactive` shape. The three error variants collapse into one or into
`Option`. Effort M.

#### C31: `derive_commands` ignores the command method's visibility

`canopy-derive/src/codegen.rs:602-633` hard-codes `pub fn` for every
`cmd_*` accessor and `call_*` builder. In the workspace 25 command methods are
`pub(crate)` and 18 are private, yet their builders are public: 46 generated
pub fns in `api/canopy-examples.rs` alone belong to `pub(crate)` commands. No
external consumer of any non-`pub` command's accessor exists (the only
cross-crate accessor use is `Button::call_press`, whose method is `pub`).
Related: `ParamKind::Context { mutable }` (`model.rs:16-19`, `parse.rs:123-138`,
`codegen.rs:189-192, 212-220`) distinguishes `&dyn Context` from `&mut dyn
Context` parameters. All 156 workspace commands outside the derive tests use
`&mut dyn Context` or none. Decision: emit accessors with the method's own
`vis` (available at `parse.rs:305`), and treat any context parameter as the
single mutable binding. Both change the macro's contract. Effort M.

#### C32: `evaluate_live` is a test-only live path that diverges from production

`crates/canopy-mcp/src/script.rs:496-525` (`#[cfg(test)] pub fn evaluate_live`)
shares `validate_live_request` with the production path but then calls
`evaluate_in`, the headless body, instead of `automation.submit_eval`. Four
tests named `evaluate_live_*` and `direct_live_context_*`
(`:1159-1188, 1371-1424`) assert live behavior on a path no client executes.
Decision: drive them through `live_canopy_mcp_server(..).script_eval` with the
existing turn-loop pattern (`server.rs:778-860`), or reduce them to unit tests
of `validate_live_request` and `LiveContext::metadata` and delete
`evaluate_live`. Effort M.

#### C33: `cargo xtask dynamic` is unreferenced and mostly targets safe code

`xtask/src/main.rs:30-31, 48-75` runs Miri with filters
`widget_slot_restores` (`world/tests.rs:832-891`, safe code),
`core::backend::tests` (`backend/mod.rs`, safe code), and
`reentrant_canopy_guard_restores_nested_stack` (`script/tests.rs:183`, covers
the one `unsafe` block). Nothing in `ci.yml`, `docs/`, `README.md`,
`AGENTS.md`, or the `tidy-hooks` list references `dynamic` or Miri. A filter
matching zero tests passes silently. Decision: delete `Task::Dynamic` and
`MIRI_TOOLCHAIN`, or keep only the bridge filter and add the task to docs or
CI. Trimming the two stale filters is unattended either way (and moot if C16
lands). Effort S.

#### C112: `dump.rs` uses `termcolor` and leaks ANSI colour into script strings

`crates/canopy/src/core/dump.rs:3, 15, 21-35, 59-97` writes into
`termcolor::Buffer::ansi()` unconditionally. Consumers: the crossterm Ctrl+C
and render-error path (`backend/crossterm.rs:837`) and
`Canopy::diagnostic_dump` (`canopy/mod.rs:1478`), whose string reaches Luau
through `canopy.diagnostic_dump` (`base_api.rs:1714-1718`). `termcolor` is used
by this one file (`Cargo.toml:16`). Every write is wrapped in
`.map_err(dump_error)?` because the buffer is an `io::Write`. Scripts and MCP
receive raw escape sequences inside a diagnostic string. Decision: is a
colourless Ctrl+C tree dump acceptable? If yes, write into a `String` with
`fmt::Write`, drop `termcolor`, and delete `dump_error`, the colour pairs, and
the indicator colour match (`:81-95`). Effort S.

#### C113: `value.rs` maintains two parallel table-conversion pipelines

`script/value.rs:113-139, 181-227` (scoped) and `:328-388` (marshaled)
implement the same algorithm over two key and value representations: classify
the layout, fill a `vec![None; len]` with a missing-index check, or insert into
a `BTreeMap` with `path.field(key)`. About 60 duplicated lines, and the numeric
index and UTF-8 key policies are stated twice. Decision: whether one
`assemble_table(layout, pairs, convert_value)` over a small key enum is clearer
than the duplication. The existing tests (`script/tests.rs:226-393`) are the
guard. Effort M.

#### C114: `Palette` and `theme()` are public with no consumer

`style/mod.rs:17` re-exports `palette::{Palette, theme}`. `palette.rs:13-53`
defines an 18-field public struct and a builder. No file outside the four
built-in themes names either (`api/canopy.rs:5395-5432, 5543`, about 40
lines). The module doc (`palette.rs:8-11`) presents it as the theme extension
point. Decision: is "define your own `Palette`-based theme" a supported
app-author feature? If not, make the palette items `pub(crate)`. Apps can still
build themes with `StyleMap::rules()`. Effort S.

#### C115: `WidgetSlotGuard::drop` silently discards widgets in forbidden states

`widget_access.rs:112-120` restores the widget only when
`self.slot.try_borrow_mut()` succeeds and the slot is empty. Both fall-through
conditions are unreachable by invariant: `plan_subtree_removal`
(`tree.rs:1011-1016`) fails with `ReentrantWidgetBorrow` when the slot is empty,
and no `Ref` can outlive the callback. `docs/architecture.md` says panics are
for impossible internal bugs and that internal errors must not hide behind
harmless defaults. A vanished widget is that failure mode. Decision:
`debug_assert!` both conditions, or `tracing::error!` in release, and whether
to add a test that drives the state through `Core` internals. Effort S.

#### C116: Two display-width policies

`core/text.rs:54-59` `grapheme_width` caps at terminal cell widths and drives
rendering (`termbuf`, `render`, `crossterm`), but the string form
`display_width` (`:94-98`) is `#[cfg(test)]`. `canopy-widgets/src/text.rs:140-144,
163-167` measures canvas width with raw `UnicodeWidthStr::width`, and
`fontgym.rs:397, 414, 685` measures by slicing with `usize::MAX` because no
width function is exported. Measured and rendered widths come from different
functions, so any grapheme where they disagree yields a canvas wider or
narrower than what is drawn. Unattended part: make `text::display_width`
public, replace the three fontgym probes, and use it in the `Text` widget's
`raw_width` and `max_width`. Decision: whether to sweep the other 11 widget
files that call `UnicodeWidthStr::width`, after checking none relies on the
uncapped value. Effort S for the first part, M for the sweep.

#### C125: Three overlapping invalidation mechanisms

Pending work is tracked three ways: `Core::changes: ChangeSet` with four flags
(`core/change.rs:17-63`), `Canopy::render_pending: bool` (`canopy/mod.rs:122`
and 9 set sites), and blanket `invalidate(Layout)` calls. `prepare_frame`
(`canopy/rendering.rs:272-301`) runs the full pipeline whenever any flag is
set. The four flags are read only through `is_pending()` and in
`world/change_tests.rs`, so `Invalidation::Paint` versus `Layout` has no
behavioral effect. `Core::with_widget_dyn_mut` (`world/mod.rs:298`) records
`Layout` on every mutable widget access and `Canopy::with_context`
(`canopy/mod.rs:872`) records `Layout` before running any closure, so nearly
every turn is a full relayout. `ChangeSet` and `Invalidation` are re-exported
(`lib.rs:37, 40`) with zero external consumers. Decision: collapse to one
pending flag on `Core`, delete `render_pending` (route `style_mut` through
`core.invalidate`), and drop the re-exports. Or keep the flags and make
`prepare_frame` honor them while `with_widget_dyn_mut` stops recording
`Layout`. Either way the `ChangeSet` paragraph in `architecture.md` must
match. Effort M. Related: C3 deletes `Context::invalidate`.

#### C126: The script module root pipeline has zero production consumers

`CanopyBuilder::user_script_root` and `project_script_root` with `ScriptTrust`
(`canopy/builder.rs:13-21, 58-61, 105-141`), `Canopy::invalidate_script_modules`,
the fields `script_module_roots`, `script_module_source`,
`completed_startup_modules`, `startup_module_scripts`, the filesystem
startup-module loop, `.d.luau` declaration-pair validation, and the
config-inside-roots branch (`canopy/mod.rs:83-97, 546-599, 668-705, 934-949,
1077-1083, 1103-1105, 1199-1271, 1494-1541`) total roughly 250 lines plus
`script::ScriptModuleRoots`. No app, example, harness, or tool enables a root.
The setters are `cfg(testing)`. No `.luau` file references `@user` or
`@project`. `ScriptTrust` appears 11 times in `api/canopy.rs`. Decision: add one
real consumer (for example a project root in `examples/todo`) so the feature is
exercised end to end, or delete the pipeline, `ScriptTrust`, and
`invalidate_script_modules`, and remove the module-reload sentence from
`architecture.md`. Effort L.

#### C127: Startup progress has two representations and two loops

Inline startup scripts live in `Vec<StartupScript { name, source, script_id,
ran }>` and filesystem modules in a `HashSet<PathBuf>` plus a `HashMap<PathBuf,
ScriptId>` (`canopy/mod.rs:89-94, 259-269, 645-707, 1090-1094, 1119-1122,
1349-1360`). `run_startup_scripts_inner` runs one loop per representation.
Inline scripts compile eagerly at finalize with rollback, module scripts
compile lazily without. Decision: one `Vec<StartupScript>` with a
`StartupSource::{Inline, Module}` variant and one loop. If C126 deletes module
roots this collapses for free. Effort M.

#### C128: Eleven `Canopy` methods have no consumer outside the Luau bridge or tests

`unbind`, `clear_bindings`, `push_input_mode`, `pop_input_mode`,
`set_input_mode`, `diagnostic_dump`, `invalidate_script_modules`,
`set_script_journal_limit`, `require_startup_global`,
`register_script_module`, and `next_deadline` (`canopy/mod.rs:578, 603, 639,
922, 1003, 1024, 1044-1054, 1405`, `turn.rs:377`) are called only from
`core/script/base_api.rs`, `core/script/invocation.rs`, the in-crate crossterm
adapter, or tests. `set_input_mode` also appears in a canopyctl test fixture.
`next_deadline` cannot serve an external adapter because `poll_runtime_wake`,
`publication_watch`, `emit_frame`, `event`, and `service_automation` are
`pub(crate)`. Decision: make the eleven `pub(crate)`, taking `Canopy` from 47
to 36. `architecture.md` lists input modes as a `Canopy` responsibility, so the
mode trio is the one judgment call. Effort S. Related: C46.

#### C129: `cancel_eval` is a dead, untested cancellation path

`AutomationHandle::cancel_eval` (`turn.rs:541-549`), `AutomationMessage::Cancel`
(`:65-66, 261-262`), and `Driver.cancel` (`:348-358, 423-429`) form a
cross-thread path parallel to `Work::CancelEval` (`:413-419`), and the
`driver.cancel` branch duplicates the `Work::CancelEval` branch line for line.
`cancel_eval` has zero callers and zero tests, while `architecture.md`
documents it. Decision: delete the path and the sentence, or keep it, add one
test through `AutomationHandle`, and implement it as a submitted closure that
sets `driver.cancel` so the message variant and the duplicate block go. Effort S.

#### C130: `EvalOutcome.result` is an `Arc` for a reader that does not exist

`turn.rs:93-111, 500-509`: a completion is cloned into the ticket sender and
pushed to `TurnOutcome.completed`, so `EvalOutcome` derives `Clone`, wraps the
result in `Arc`, and `into_result` carries an "impossible" `Error::Internal`.
Production readers of `.completed` are only `eval_headless`, which never has a
ticket. The crossterm runloop reads `.exit_code` and `.frame`, and canopy-mcp
reads `.completed` only in a test. Decision: send to the ticket when one
exists, else push, and make `result: Result<ArgValue>`. This changes a
documented `TurnOutcome` contract. Effort S.

#### C131: `EventOutcome::Consume` is indistinguishable from `Handle`

`widget/mod.rs:20-29` documents `Consume` as "processed without a state
change". Both dispatch sites (`canopy/routing.rs:213`, `world/dispatch.rs:35`)
match `Handle | Consume` identically. Only `testing/ttree.rs:37-38`
distinguishes them, for a label. Eight widget sites return `Consume`
(`frame.rs`, `help/mod.rs`, `help/binding_list.rs`). Decision: remove the
variant and use `Handle`, or make routing honor the hint after C125 removes
`render_pending`. Effort S.

#### C132: `PendingHeap` is a heap, map, and compaction scheme for seven pollers

`core/poll.rs:33-119, 171-196`: a `BinaryHeap` with reversed `Ord`, an
authoritative `HashMap<NodeId, PendingNode>`, `compact`, `discard_stale`, and a
collect-then-cancel `retain`. The workspace has seven `poll` implementations,
most of which never coexist. A single `HashMap` with `min_by_key` on the
deadline and `retain` gives identical semantics in about 25 lines and removes
the stale-entry invariant. Decision: a complexity-versus-scale judgment. Effort
S.

#### C133: `NodeSnapshot` flattens `View` into four fields

`core/snapshot.rs:52-59, 115-118`: `rect`, `content_rect`, `scroll`, `canvas`
are exactly `View { outer, content, scroll, canvas }` with the first two made
`Option` by `displayed`. Readers: `script/records.rs:91-98, 195-196` and
`examples/src/tests/help.rs:107`. Decision: `view: Option<View>` and emit the
four fields from it in `records.rs`. Today `scroll` and `canvas` are exposed for
non-displayed nodes (stale caches). Decide whether that is wanted. Related:
C10. Effort S.

#### C134: Filesystem failures flattened into `Error::Invalid(String)`

`canopy/mod.rs:678-681, 931, 940, 1078, 1210-1221, 1505-1524` wrap `io::Error`,
`DirectoryMountsError`, and root-validation errors in `Error::Invalid(format!(..))`,
losing the source error and path. Decision: add a typed
`Error::ScriptSource { path, source }` variant, or let C126 remove most of these
sites. Effort S.

#### C135: Two full-screen `TermBuf` clones per published frame

`canopy/rendering.rs:297-299, 322` and `core/snapshot.rs:129`: `snapshot::capture`
clones `buffer.cells` and `emit_frame` clones the whole `TermBuf` into
`emitted_buf`, so three copies of the same cells exist after emit. The rendered
buffer is immutable after `render_pass`. Decision: share one `Arc<TermBuf>`
between `termbuf`, `FrameSnapshot.buf`, and `emitted_buf`. Three readers change
from `.cells[i]` to `.buf.cells[i]`. The benefit is structural, not measured, and
the change touches a public field. Effort M.

#### C136: `register_fixture` ignores setup when deciding idempotence

`canopy/mod.rs:817-825` compares `description` only, so a different closure
under the same name and description is silently dropped.
`register_startup_script` and `register_default_bindings` compare the source.
Decision: reject any duplicate name, or compare `Arc::ptr_eq` on the setup and
error otherwise. Effort S.

### Clear wins

Each item can be implemented unattended and validated with the baseline suite.
Items that shrink public surface also need `ncode api` to refresh the captures.

#### Core runtime

#### C34: Invalid node IDs reported as `Error::Internal`

`layout_driver/mod.rs:40` and `:897` return `Error::Internal("missing root
node")` and `Error::Internal("missing node")` for a stale `NodeId`.
`dump.rs:45-48` does the same and discards the ID.
`docs/architecture.md` classifies invalid node IDs as expected failures. Change
all three to `Error::NodeNotFound(id)`, the variant `widget_access.rs:90` and
`world/mod.rs:327` already use for this condition. No test asserts the strings.

#### C35: Removal hook sequence duplicated between replace and remove

`tree.rs:188-191` and `:986-989` both run `run_pre_remove_plan`,
`validate_removal_plan`, `run_unmount_plan`, `validate_removal_plan`. Extract
`run_removal_hooks(&mut self, plan)` and call it from both.

#### C36: Single-use one-line wrappers over `pub(crate)` fields

`world/mod.rs:244-256`: `take_exit_request`, `request_diagnostic_dump`,
`take_diagnostic_dump_request` each wrap one field operation and have one
caller (`canopy/turn.rs`, `context.rs:1538`, `canopy/rendering.rs`). Inline
them. Keep `request_exit` (`:237-241`), which has first-wins logic.

#### C37: `node_path_label` detects detachment through a rendered string

`world/mod.rs:378-388` renders the path to a `String` and compares with `"/"`.
`path_of` returns `Path::empty()` for unreachable nodes (`tree.rs:1195-1197`)
and `Path` is `PartialEq`. Compare with `Path::empty()` and drop the redundant
root guard.

#### Scripting and commands

#### C38: Two public script items with no external consumer

`pub type ScriptId` (`script/mod.rs:84`) appears in `api/canopy.rs:4840, 4983`
only as its own definition, and no public method takes or returns it.
`ScriptCheckResult::from_diagnostics` (`:172`) has one caller, inside the crate
(`canopy/mod.rs:1226`). Make both `pub(crate)`. Other `pub` items inside the
private `script` submodules (`modules.rs`, `invocation.rs`, `bridge.rs`,
`errors.rs`, and the `LuauHost` methods) are already crate-only because `mod
core` is private (`lib.rs:22`) and only selected items are re-exported.
Narrowing them would trip `clippy::redundant_pub_crate`, so leave them.

#### C39: `ZoomDirection` belongs to `ImageView`

`commands.rs:47-54` defines a `CommandEnum` whose only consumer is
`canopy-widgets/src/image_view.rs:7, 352-359`. It costs about 35 lines of
`api/canopy.rs`. Move it into `image_view.rs` under the `graphics` feature.
`derive(CommandEnum)` works from any crate.

#### C40: Parallel dispatch shims

`script/dispatch.rs:59-72` calls `commands::dispatch`, a one-line forward to
`dispatch_target` with one caller (`commands.rs:1418-1424`).
`dispatch_explicit` (`:93-109`) builds a `CoreContext` only to call
`.dispatch()`, itself a one-line forward. `CommandResolver::new`
(`commands.rs:884-887`) has two callers that could use `for_target`. Call
`commands::dispatch_target` directly from both dispatch functions. Delete
`commands::dispatch` and `CommandResolver::new`. Also remove the duplicate spec
lookup (`dispatch.rs:82` then `commands.rs:1450`).

#### C41: Gas budget literal

`turn.rs:324` writes `gas: 500_000_000`. `SCRIPT_GAS_LIMIT`
(`invocation.rs:22`) holds the same value and is already re-exported to the
crate. Use the constant.

#### C42: Lossy UTF-8 conversion contradicts the strict policy

`script/value.rs:13-24` `scoped_value_to_string` uses `from_utf8_lossy`.
`scoped_to_arg_value` (`:160-167`), ruau's `String: FromLua`, and
`docs/scripting.md` ("Strings must be valid UTF-8") are strict. Callers:
`base_api.rs:592` (option fields) and `:1107` (assert message). Delete the
helper. Read `Option<String>` through `FromLua`.

#### C43: `base_api.rs` and `script/mod.rs` micro-duplication

`host_focus_next`, `host_focus_prev`, `host_focus_dir` share one body
(`base_api.rs:1277-1320`). `read_opt_node_id` has an unused `_name` parameter
(`:686-690`. Callers `:1163, 1713, 1727`). Four copies of
`scope.context_mut::<Canopy>().ok_or_else(..)` (`:776-778, 808-810, 851-853,
880-882`. Also `bridge.rs:122-128`). `WaitForNodeArgs` and
`WaitForScreenTextArgs` are identical modulo field name (`:723-755`).
`host_bind`/`host_bind_mouse` (`:1501-1553`) and `host_cmd`/`host_cmd_on`
(`:941-963`) differ only in the parsed spec. `load_script` and `loaded_root`
both invalidate, clear roots, and clear closures (`mod.rs:964-968, 987-996`).
`check_startup_source` and `typecheck_startup_source` have one caller each
(`:662-681`). Extract one helper per pair.

#### C44: `node_id_to_arg` aliases `ArgValue::Node`

`bridge.rs:202-204` wraps the enum constructor. 23 uses across `records.rs`,
`base_api.rs`, `value.rs`. Replace with `ArgValue::Node`, which works in both
call and function-pointer position.

#### C45: `new()` duplicating `Default`

`CommandSet::new` (`commands.rs:1068-1072`, derive at `:1060`, one caller
`world/mod.rs:229`) and `ScriptModuleRoots::new` (`modules.rs:27-29`, body
`Self::default()`). Delete both.

#### Rendering, terminal buffer, input, layout

#### C46: Infallible `Result` on input mode setters

`InputMap::set_mode` and `push_mode` (`inputmap/mod.rs:653-667`) and
`Canopy::set_input_mode` and `push_input_mode` (`canopy/mod.rs:1043-1051`)
only push to a `Vec<String>` and always return `Ok(())`. Six call sites `?` a
no-op (`base_api.rs:1471, 1485`, `help.rs:225`, `inputmap/tests.rs:123-124`,
`canopyctl/src/main.rs:758`). Return `()`. Refresh `api/canopy.rs`.

#### C47: `TermBuf` computes the row-local grapheme span three times

`termbuf/mod.rs:242-256, 337-352, 355-382` each compute `row_start = idx /
width * width`, clip to the row, and call `grapheme_range`. `restyle_grapheme`
lacks the `width == 0` guard the other two have. Add `grapheme_span(index) ->
Option<Range<usize>>` and use it in all three. About 25 lines removed.

#### C48: Unused `io::Write` supertrait on `TerminalOperations`

`backend/crossterm.rs:297` requires `Write`. No trait consumer uses it, and the
`Stderr` impl satisfies `execute` on its own. It forces a nine-line dummy
`Write` impl for `FakeTerminal` (`:1105-1113`). Drop the bound and the dummy.

#### C49: Zero-consumer event surface

`FromStr for Key` (`event/key.rs:306-312`) and `FromStr for Mouse`
(`event/mouse.rs:118-124`) forward to `parse_spec`, which every consumer (27
sites) calls directly. `pub fn Action::is_button` (`mouse.rs:50-64`) is used
only at `mouse.rs:98`. Delete the `FromStr` impls. Make `is_button` private.

#### C50: `Constraint::max_bound` has zero consumers

`layout.rs:644-650`. Delete.

#### C51: `Render::resolve_style_name_at` has one consumer

`render/mod.rs:166-174` composes `resolve_style(name).resolve_at(bounds,
point)`. The only caller is `font_banner.rs:148`. Inline and delete.

#### C52: Two replay models in termbuf tests

`termbuf/tests.rs:301-437` `ReplayBackend` re-implements `shift_chars` and
`shift_lines` on `Vec<Vec<char>>` while `ModelBuffer` (`:439-750`) implements
the same with style tracking and drives the proptest. Drive the two
interior-shift tests (`:962, 972`) through `ModelBackend`. Keep only the
narrow-wide text path for `render_repositions_after_wide_graphemes` (`:1054`).

#### C53: `TestRender::render(&mut Canopy)` inverts the receiver

`testing/backend.rs:16-20`. Two consumers (`canopy/tests.rs:435, 439`). Every
other test calls `canopy.render(&mut backend)`. Replace and delete.

#### C54: Crossterm shift parameter names and coordinate casts

`crossterm.rs:687-692` names parameters `_top` and `_bottom` and then reads
them. `:677` truncates with `as u16` while `text()` uses checked
`u16::try_from` (`:570-582`). Rename, and share one `cell_coord(u32) ->
io::Result<u16>` helper.

#### Widgets

#### C55: `Editor.text_entry_transaction` duplicates buffer state

`editor/widget.rs:46-47, 184, 456-470`: a `bool` flag wraps
`buffer.begin_transaction()` and `commit_transaction()`, which are already
idempotent (`text_buffer/buffer.rs:171-188`). The flag goes stale after
`set_text` (`:213-220`) and after the transaction guards in `vi.rs:1045, 1085`.
Delete the field. Forward the two helpers to the buffer.

#### C56: Display-metric fallback scans the buffer twice

`editor/widget.rs:250-272, 1042-1071`: when the layout cache has no metrics,
`display_line_count` and `display_line_width` each walk every line.
`display_line_width` calls `layout_line` with `WrapMode::None` solely for
`display_width`, which is wrap-independent (`layout.rs:365`). Add one
`metrics(buffer, wrap_mode, wrap_width, tab_stop) -> (lines, width)` in
`layout.rs` and delete the two widget functions.

#### C57: `LineChange` and `take_change` leak the layout-cache protocol

`text_buffer/mod.rs:14`, `buffer.rs:15-24, 159-168`,
`api/canopy-widgets.rs:635-643,
835-839`. Consumers: `editor/layout.rs` only. A consumer calling `take_change`
would silently break the editor's layout cache. Make the re-export and method
`pub(crate)`.

#### C58: `Editor::selection()` and `Editor::new()` have zero consumers

`editor/widget.rs:167-170, 196-225`. `selection()` forwards
`buffer.selection()` and forces `text_buffer::Selection` into the Editor
surface. `new(text)` is `with_config(text, default())`. External consumers use
`with_config` and `set_highlighter` only. Delete both. Editor goes from 17 to
15 inherent methods.

#### C59: `handle_paste` duplicates `handle_insert_text`

`editor/widget.rs:384-392, 445-454`. Both normalize, check `read_only`, insert,
and update the preferred column. One caller discards the return value. Make
`handle_insert_text` return the normalized text and delete `handle_paste`
(`vi.rs:788` uses the return value).

#### C60: Three copies of the single-line newline policy

`input.rs:78-83, 302-305` and `editor/widget.rs:394-401` all replace `\n` and
`\r` with a space. Add `single_line(text)` in `text_buffer/util.rs` and use it
in all three.

#### C61: `TerminalColors` is 110 lines for one fixed palette

`terminal.rs:139-248`: a private struct with `pub` fields, a `Default` of 19
`rgb!` literals, and a converter, used only as `TerminalColors::default()` at
`:673, 833`. Replace with `default_palette() -> PaletteConfig` and a
`DEFAULT_BACKGROUND` constant.

#### C62: `matches_for_line` allocates per visible line per frame

`editor/search.rs:100-109` builds a `Vec` per rendered line. The caller
(`widget.rs:719, 762-765`) then scans it per grapheme. Matches are produced in
ascending order (`:156-176`). Return `&[TextRange]` through two
`partition_point` calls.

#### C63: Two adapters over `ClickTracker::count`

`editor/widget.rs:990-1009, 620-640` wraps the count in a `ClickType` enum.
`terminal.rs:432-435, 507` wraps it in a one-line `selection_type_for_click`.
Delete both adapters and match the count directly, or return one shared enum
from `ClickTracker::count`.

#### C64: A field used as a return value, and a constant tuple element

`editor/widget.rs:40-41, 287-291, 305`: `cursor_point` is written in
`update_layout` and read once immediately after. Return it instead.
`search.rs:449-467, 395-402, 434-437`: `replace_match` takes `Vec<TextRange>`
by value to `.get(index)` and always returns `(updated, 0)`. Take the target
range and return the vector.

#### C65: `font` module publishes items with no consumer

`font.rs:45-61` `Glyph`, `:185-205` `FontCell` and `FontLayout`, `:231-237`
`FontRenderer::layout`, `:524-534` `align_offset` (byte-identical to core's
private `layout_driver/mod.rs:861`). All have zero external consumers. Make
them private or `pub(crate)`. Optional adjunct: `ImageView` is reachable as
`canopy_widgets::font::ImageView` (`font.rs:14`). Re-export it at the crate
root under `graphics` and update `demo.rs:15` and `widget.rs:20`.

#### C66: `Root::HelpState` never returns to `Closed`

`root.rs:51-71, 144, 222, 246, 262-271`: the only assignment is `Open { token
}`. `hide_help` calls `close_modal` and never resets. Core is the source of
truth for openness. Replace with `help_token: Option<InteractionToken>`.

#### C67: `Root::install` re-enters the root widget to run `sync_layout`

`root.rs:97-109, 176-193, 303-308`: after `replace_root`, a second
`with_root_context` and `with_widget_mut` reach `inspector_active`. `sync_layout`
also rewrites the app layout on every inspector toggle. Read the flag into a
local before `replace_root`, do both writes in the first closure, have
`show_inspector` and `hide_inspector` call `set_hidden_of` directly, and delete
`sync_layout`.

#### C68: `Default` impls that exist only for the lint on crate-private types

`pad.rs:28-32` (`Pad::default` yields a zero pad and is not lint-required),
`help/panel.rs:22-26, 106-110`, `help/binding_list.rs:39-43`,
`inspector/logs.rs:314-318`. None is called. Delete `Pad`'s. Make the other
`new` fns `pub(crate)` and delete their `Default` impls.

#### C69: `BindingList` decides the gutter twice with two predicates

`help/binding_list.rs:104-135, 191-208, 245-250`: `viewport_lines` uses
`lines.len() > view.h && view.w > 2`, `render` uses `canvas.h > viewport.h &&
content.w > 2`. `binding_lines` has a `style` parameter always `"help/label"`.
Return the text width from `viewport_lines` and drop the parameter.

#### C70: `List::clear` redundant reset and `List::remove` visibility

`list.rs:321-327`: `clear` sets `selected = None` after `reconcile_order`
already does. `pub fn remove` (`:310-319`) has no consumer outside the crate.
Delete the statement. Make `remove` `pub(crate)` or record it as deliberate.

#### Automation stack

#### C71: `Todo.pending` and an unreachable branch

`examples/todo/src/lib.rs:150-151, 159-176, 197-198, 566`: `ensure_tree`
re-reconciles `pending` when the slot exists, but the slot is created only by
`ensure_tree`, which clears `pending`. The field also forces `Todo::new ->
Result`. Load todos at slot creation, delete `pending`, and make `new`
infallible.

#### C72: `luau_check.rs` duplicates the smoke suite's typecheck gate

`examples/todo/tests/luau_check.rs:11-25` discovers the same scripts and
asserts `check_script(..).is_ok()`. `run_suite` typechecks each script before
execution with the same predicate (`canopy-mcp/src/script.rs:747-758,
846-869`) and `smoke.rs` fails on any typecheck error. Delete the file.

#### C73: `serve_uds_with_context` is `serve_uds` with the caller building the context

`canopy-mcp/src/server.rs:261-340`. Both non-test callers pass
`LiveContext::new(metadata)` (`launch.rs:55`, `server.rs:677`). Merge into
`serve_uds`. Drop the `LiveContext` import in `launch.rs`.

#### C74: `SessionManager` forwarders and repeated `touch()`

`canopyctl/src/session.rs:187-222`: five one-line forwarders over `session()`.
`main.rs:184-265`: `last_activity` lives beside `sessions` and every tool calls
`touch()` first. Make `session()` public, move `last_activity` into
`SessionManager`, update it inside `session()`, `connect_live()`, and
`disconnect()`. Delete the forwarders and `touch`.

#### C75: `ReplayEntry` deserializes five unread fields

`canopyctl/src/replay.rs:212-241`: `id`, `error`, `logs`, `assertions`,
`duration_ms` are never read. `Serialize` on `ReplayJournal` and `ReplayEntry`
is never used. Delete the fields and derives. Subsumed by C9 if the legacy
format goes.

#### C76: Replay option validation runs three times

`canopyctl/src/main.rs:381, 393, 396-398, 430-435`. Delete the inline repeats
at `396-398` and `430-435`. Subsumed by C9.

#### C77: Headless viewport validated twice with a `Size` round trip

`canopy-mcp/src/script.rs:306-309, 318-332, 802-808`: callers validate the
`Viewport`, convert to `Size`, and `HeadlessSession::new` converts back and
validates again. Take `Viewport` in the constructor and delete the second
validation.

#### C78: `HeadlessSession` exists to carry a zero-sized `NopBackend`

`script.rs:793-798, 818-820, 344-345, 740-746, 762-764`. Replace with
`build_headless(..) -> Result<Canopy>` and `evaluate_in(.., render: bool)`.

#### C79: Fixture re-rejected by an inner function

`script.rs:609-616` repeats the check `validate_live_request` (`:533-537`)
already made for the only caller (`:598`), with a duplicated message. Delete.

#### C80: `AppFactory::bootstrap()` has one test consumer

`script.rs:299-302`. Consumer `:1217`. Delete, or rename
`bootstrap_with_request` to `bootstrap` (three call sites).

#### C81: Unused `Serialize`/`Deserialize` on smoke outcomes

`smoke.rs:41, 52`: `SuiteOutcome` and `ScriptOutcome` are never serialized.
Drop the derives and the import. Four impl blocks leave the capture.

#### C82: Todo `main.rs` duplicates flags and exit handling

`examples/todo/src/main.rs:24-26, 43-46` (two `config` options), `:71-87` and
`:107-109` (two exit blocks), `:102-104` (usage error exits 0). Use a global
`config` flag, fold `--api` into the `match`, and `bail!` on the usage error.

#### C83: `validate_viewport` wraps `Viewport::validate`

`canopyctl/src/replay.rs:161-164`. `?` already converts the error. Replace the
four calls and delete.

#### C84: Nine identical `ToolError::internal(error.to_string())` closures

`canopy-mcp/src/server.rs:86-200`. Canopyctl already has `tool_error`
(`main.rs:718-721`). Add the same helper to `server.rs`. Replace the two
constructor wrappers `canopy_mcp_server` and `live_canopy_mcp_server`
(`:52-75`) with struct literals.

#### C85: `open_adder` resolves `todo.input` twice

`examples/todo/src/lib.rs:324-330`, with a third copy of the error string
(`:272, 283, 330`). Add `input_id` and `list_id` helpers used by `with_input`,
`with_list`, `can_delete`, and `open_adder`.

#### C86: `script.rs` local redundancies

`script.rs:343, 753, 767, 771` use `.elapsed().as_millis() as u64` while
`elapsed_ms` (`:722-724`) exists. `:624-628` hand-builds `live_timing(start,
start)`. `:568-588` has a match with an `unreachable!()` arm that
`.map_err().and_then()` removes. `:356-358, 367` clone the whole API text to
compute a digest afterwards. Compute the digest first.

#### C87: Duplicated test metadata literal

`canopy-mcp/src/metadata.rs:141-162`: `test_app_factory` and
`AppMetadata::test()` both spell `{ app: "canopy-test", reset: Isolated }`.
Compose one from the other.

#### C88: `.canopyctl.toml` restates three defaults

`examples/todo/.canopyctl.toml:4, 6-7, 9-10`: `cwd`, `suite`, and
`idle_shutdown_after_secs` equal the defaults in `config.rs:116, 176, 203`.
Reduce to the `[app]` section. Skip this if the file is meant to document the
schema for new examples (`docs/fixtures.md:70-73` points at it).

#### Supporting crates, examples, tooling

#### C89: fontgym `FocusFrame` scrolls through the wrong widget type

`crates/examples/src/fontgym.rs:224` stores `TypedId<List<FontBlock>>`, but
`scroll_list` (`:239`) calls `with_widget_mut(self.list_id, |_: &mut
List<Text>, ..|)`. `with_widget_mut` erases the `TypedId` and runtime-checks
the type (`context.rs:318-331`), so every Up, Down, PageUp, and PageDown in the
fonts frame returns `Err(NodeTypeMismatch)`. No test covers `FocusFrame` keys.
Fix the closure type and add a harness test that presses PageDown and asserts
the scroll changed.

#### C90: `canopy-geom` items with zero workspace consumers

`RectI32::ZERO` (`rect_i32.rs:16-20`), `RectI32::translate` (`:42-48`),
`TryFrom<Rect> for RectI32` and `TryFrom<RectI32> for Rect` (`:129-151`),
`PointI32::ZERO` (`point_i32.rs:29`), `PointI32::new` (`:32-34`),
`LineSegment::new` (`linesegment.rs:14-16`). Only geom's own tests use them.
Delete them and their test lines. Regenerate `api/canopy-geom.rs`. This is the
cheapest headroom under the 575-line threshold.

#### C91: Two trybuild `pass` cases duplicate unit-test coverage

`canopy-derive/tests/compile.rs:8-9`: `valid_typed_call.rs` proves what
`tests/derive.rs:228-297` already compiles and exercises.
`conditional_commands.rs` only needs `commands().len() == 1`. Each pass case
builds an isolated dependency graph (see `.config/nextest.toml`). Move the
second into `derive.rs` as a `#[test]`, delete the first, keep the four
`compile_fail` cases.

#### C92: Two type-shape matchers duplicated across `parse.rs` and `codegen.rs`

`codegen.rs:69-89` `is_immutable_view_context` vs `parse.rs:123-138`
`is_context_ref`. `codegen.rs:93-116` `is_command_status_result` vs
`parse.rs:97-120` `extract_result_type`. The copies already differ on accepted
generic-argument counts. Generalize the parse helpers and use them from codegen.

#### C93: `expand_command_arg` loops three times over one field list

`canopy-derive/src/lib.rs:77-149`: after matching `Fields::Named`, one loop
`expect`s an ident and another returns `syn::Error` for a missing one. One
loop filling three vectors, plus one `bounded_generics` helper for the two
where-clause blocks.

#### C94: Call builder re-parses parameter names from strings

`codegen.rs:620-624` does `syn::parse_str(&param.name).expect(..)` on a name
that was an `Ident` at `parse.rs:271`. Store `user_ident` on `ParamMeta`.

#### C95: `scroll`, `page`, `scroll_to` commands copied between two gyms

`crates/examples/src/editorgym.rs:134-161` and `framegym.rs:42-69` are
character-identical apart from doc comments. Add `scroll_in` and `page_by`
helpers in `lib.rs`. The commands stay on each owner and delegate.

#### C96: `TerminalStack` is defined twice

`termgym.rs:109-123` and `widget/term.rs:49-67`. One `pub(crate)` definition.

#### C97: Two near-identical harness builders in the examples tests

`src/tests/mod.rs:12-25` `root_harness` and `src/tests/help.rs:9-22`
`wrapped_harness` differ only in the assemble closure and a hard-coded size.
`tests/stylegym.rs:18-21` builds a third by hand. Parameterize `root_harness`
on the mount strategy.

#### C98: Reading a layout through the mutable visitor

`focusgym.rs:70-78` and `src/tests/focusgym.rs:73-79` use `with_layout_of` to
read `direction`. `ViewContext::layout_of` (`context.rs:143`) returns it
directly. Mutable widget access records invalidation, so a read should not go
through it.

#### C99: `print_luau_api` does not print

`crates/examples/src/lib.rs:52-55` returns `script_api().map(str::to_owned)`.
two callers (`examples/demo.rs:81`, `examples/widget.rs:149`) print it. Replace
with `builder.build()?.script_api()?` and delete.

#### C100: `renderer_from_font` is a misnamed identity wrapper

`crates/examples/src/widget/font.rs:93-96`, one call at `:90`, doc claims
"demo glyph settings" that do not exist. Inline.

#### C101: `FontBlock::set_text` and `set_effects` duplicate the mount split

`fontgym.rs:305-328`: two 10-line methods differing only in the setter. One
`with_banner(ctx, f)` helper.

#### C102: Entry style rules duplicated between two demos

`intervals.rs:259-280` and `termgym.rs:425-455` build identical
normal/selected rule sets. One `selectable_entry_styles(rules, prefix)` in
`lib.rs`.

#### C103: `futures` and `rand` are not hoisted

`futures = "0.3.32"` in `crates/canopy/Cargo.toml`, `crates/canopy-mcp/Cargo.toml`,
`crates/canopyctl/Cargo.toml`. `rand = "0.10.1"` in `crates/canopy/Cargo.toml`
and `crates/examples/Cargo.toml`. Every other multi-crate dependency is in
`[workspace.dependencies]`. Hoist both.

#### C104: `canopy-examples` spells out the default widget feature set

`crates/examples/Cargo.toml:12` sets `default-features = false` and then lists
exactly the default set from `crates/canopy-widgets/Cargo.toml:12`. Use the
default.

#### Core support files

#### C117: 32 public theme colour constants with zero consumers

`style/dracula.rs:10-36` (12 `pub const`) and `style/gruvbox.rs:10-54` (20
`pub const`) are read once inside `dracula()` and `gruvbox_dark()`. No file
outside the two names them, and five gruvbox constants (`DARK0_HARD`,
`DARK0_SOFT`, `DARK3`, `LIGHT2`, `LIGHT4`) are unused even there. About 116
lines of `api/canopy.rs`. Make them private and delete the five. `solarized`
constants have 30 or more external consumers and stay public.

#### C118: Two copies of the completion-queue admission guard

`world/teardown.rs:100-124` `remove_after_dispatch` and `:127-145`
`close_modal_after_dispatch` run the same reject-if-draining, capacity check,
push, and idle-drain sequence. Only the payload and messages differ. Extract
`enqueue_completion(request, what)`. Keep the removal message exact, since
`teardown.rs:214` asserts it.

#### C119: The dispatch boundary is hand-rolled at five sites

`canopy/routing.rs:136-145, 355-358`, `canopy/mod.rs:871-878`,
`canopy/rendering.rs:67-72`, and `commands.rs:1432-1441` each call
`begin_dispatch`, run work, call `finish_dispatch(checkpoint, result.is_ok())`,
and combine the two results in a different idiom. The boundary is what makes
`remove_after_dispatch` safe. Add
`Canopy::with_dispatch_boundary(|c| ..) -> Result<R>` and use it at the four
`Canopy` sites, and optionally a `Core` twin for `commands.rs`.

#### C120: `#[derive_commands]` on test nodes with no commands

`testing/grid.rs:4, 31` (`GridNode`) and `testing/harness.rs:205`
(`TestNode`) carry the derive with no `#[command]` method, emitting an empty
command array and a `CommandNode` impl that nothing registers. `Widget` and
`Loader` have no `CommandNode` bound. Remove the attributes and imports.

#### C121: Help-panel styles spelled ten times

`style/palette.rs:112-145` has ten `.style(path, ..)` calls with three
distinct bodies. `StyleRules::style_all` (`style/mod.rs:604-611`) is identical
to `style` per path and is already used in the examples. Three `style_all`
calls replace them.

#### C122: `expand_tabs` allocates even when there are no tabs

`core/text.rs:62-81` builds a new `String` grapheme by grapheme. `Input`
calls it on every render (`input.rs:72-74`, plus a second `to_string`) and the
`Text` widget calls it twice (`text.rs:140, 158`). Return `Cow<'_, str>` and
short-circuit when the input has no tab. The call sites already only borrow the
result.

#### C123: `normalize_filter` is `pub(crate)` with one in-file caller

`core/path.rs:191-198`, sole caller `:152`. Make it private or inline it into
`PathFilter::normalized`.

#### C124: `BufTest::dump()` has no consumer beyond its own smoke test

`testing/buf.rs:138-161`. The test at `:200-212` only checks it does not panic.
`assert_matches_with_context` already prints expected and actual on failure.
Delete it, or keep it as a debugging aid. Low value either way.

#### Runtime facade

#### C137: Journal recording duplicated with inconsistent clocks

`turn.rs:479-499` hand-builds a `ScriptJournalEntry` and calls
`enforce_script_journal_limit` instead of `record_script_journal`
(`canopy/mod.rs:1276-1316`). It measures duration with the driver clock while
`record_script_journal` uses wall-clock `elapsed()`, so under `ManualClock` the
two paths disagree. `turn.rs:622-631` feeds a driver-clock start into the
wall-clock path. `logs` and `assertions` are cloned twice. Make
`begin_script_journal` and `record_script_journal` use `self.now()` and replace
the hand-built entry with one `record_script_journal` call. Existing journal
tests cover it.

#### C138: `refresh_snapshot` is an alias of `flush`

`canopy/rendering.rs:41-44` forwards to `flush` (`canopy/mod.rs:516-521`).
Callers: `base_api.rs:1687`, `rendering_tests.rs:101`. Delete it and call
`flush`. Decision, separate from the alias removal: `flush` is a third public
preparation path that `architecture.md` does not name. Its one external
consumer is `canopy-widgets/tests/minimum.rs:33`. Either make it `pub(crate)`
and switch that test to `turn(Work::Prepare)`, or document it.

#### C139: `ScriptErrorKind` labels live in two tables

`core/error.rs:83-140` defines the protocol string per variant through serde
attributes, and `:142-171` defines them again in `as_str`. Both are live
(`canopy-mcp/src/script.rs:137` and `script/errors.rs:50`,
`base_api.rs:987`). Derive serde from one label table so a rename cannot
desynchronize them. Protocol strings unchanged.

#### C140: `Canopy::eval` is a one-line alias of `eval_headless`

`canopy/mod.rs:533-536` forwards to `turn.rs:555`, which is `pub(super)` with
no other caller. Rename `eval_headless` to `eval` and delete the wrapper.

#### C141: `View::content_to_screen` and `View::outer_to_content` have zero consumers

`core/view.rs:48-66` with tests at `:179-183, 189-191, 213-214`. The other
three conversions are used. Delete both.

#### C142: An impossible `Error::Internal` path in default binding compilation

`canopy/mod.rs:1333-1346` collects sorted owner keys, then `get_mut` on the
same map with a stringly fallback and a `String` allocation per owner. Iterate
`iter_mut()`, sort the entries, and compile in place.

#### C143: `BindingTarget` is re-exported with no public-signature role

`lib.rs:35-44` re-exports `BindingTarget`, which appears in no public fn
signature and has zero consumers outside canopy. Drop it from `lib.rs` and
`core/mod.rs`. (`ChangeSet`, `Invalidation`, `ScriptTrust`, `NodeWakeHandle`,
`WakeOutcome`, `WorkLifetime` also have zero consumers but are decided by
C125, C126, and C20.)

### Documentation and configuration

#### C105: Recalibrate `docs/api-budget.md`

The document was written in `9bef547e` alongside the new `api/` captures, but
only the `canopy.rs` line threshold was recalibrated (6,500 to 8,826). The
other six thresholds were copied from the deleted `api-surface/README.md`,
whose ruskel skeletons were a different format (`canopy-mcp` was 666 lines
there and is 1,285 now). Two captures already exceed their thresholds
(`canopy-mcp.rs` 1,285 > 1,050 and `canopy-derive.rs` 50 > 40) and
`canopy-geom.rs` has one line of headroom. The method thresholds for the
context traits (34 and 60) fit the base traits alone (33 and 53) but not the
combined counts the text says to use (46 and 73).

Decision needed: whether extension traits count toward the context surfaces.
Then set every threshold from the current numbers with the same headroom used
for `canopy.rs` (about 2 percent), after the surface reductions in C3, C39,
C58, C90 land. Also record the accepted `ruau` coupling: `canopy::commands`
re-exports `ruau::declaration` (`commands.rs:8`), `CommandType::luau_ty`
returns a ruau type, `DeclRegistry` wraps `ruau::module::Builder`,
`Canopy::register_script_module` takes `Arc<dyn ruau::vm::NativeModule>`, and
`From<Error> for ruau::vm::RuntimeError` is public. All are required by
`canopy-derive` codegen (`derive/src/lib.rs:143-175`), so the coupling is
accepted in practice but the document lists only the `futures` receiver.

#### C106: `docs/architecture.md` drift

Unattended fixes:

- Line 19: `Root::install_app` is `Root::install` (`root.rs:279`). The test
  name `install_app_preserves_app_layout_direction` (`root.rs:543`) carries the
  same stale name.
- Line 50: "`Core` stores `Node`s in a `SlotMap<NodeId, Node>`" is now
  `NodeArena<Node>` over `SlotMap<RawNodeId, Node>` (`id.rs:15-16`,
  `world/mod.rs:60`). The wrapper is what stops apps forging IDs, which is
  worth stating.
- Line 147: "The access layer owns the unsafe restoration boundary".
  `widget_access.rs` has no `unsafe`. Slot take and restore are safe code
  through `WidgetSlotGuard`. The only `unsafe` is the script bridge (C16).
- Invariants list omits the semantic-key index and pending-diagnostic-target
  checks that `validate_invariants_with` runs (`tree.rs:422-423`,
  `semantic.rs:102`).
- Widget capabilities: "Editor retains its existing buffer re-exports when
  enabled" is false. `editor/mod.rs:20` re-exports only `display_width`
  (`pub(crate)`). Fix the sentence or restore re-exports. C58 makes it moot.
- Widget capabilities: `graphics` is "Images, fonts", but `ImageView` is
  reachable only as `canopy_widgets::font::ImageView` (see C65).

Fixes tied to batch items:

- Lines 115, 256-257, 276-280: the exclusive framework frame and frame token
  description of the help modal. Root uses `open_modal` with
  `ModalBindings::Framework` and `close_modal` (`root.rs:238-244, 265`).
  Rewrite around `ModalOptions` with C1.
- Structural Edits: "restores ... command registry and scope" goes with C6.
- Runtime Turns: "There is no eager scheduler thread per application" holds
  after C4.
- Widget capabilities table: `terminal-widget` becomes `itty-core` only after
  C4.
- Node Lifecycle: `Context::wake_handle` is described as the mechanism for
  background producers, and no widget uses it (C20).
- Runtime Turns: "Adapters wait on terminal input, runtime notifications, and
  `Canopy::next_deadline()`", but `poll_runtime_wake`, `publication_watch`,
  `emit_frame`, `event`, and `service_automation` are `pub(crate)`
  (`turn.rs:366-377`, `rendering.rs:311`, `routing.rs:338, 354`), so no
  external adapter can exist. Expose an adapter surface or reword (C128).
- Runtime Turns: "`ChangeSet` tracks layout, paint, cursor, and observation
  invalidation" and "Read-only automation does not request a redraw". The flags
  are never consumed individually, and `with_context` always records `Layout`
  (C125).
- Runtime Turns names `turn(Work::Prepare)` and `render`. `Canopy::flush` is a
  third public path (C138).
- Scripting Ownership documents `cancel_eval(id)`, which has no caller or test
  (C129).

#### C107: `docs/agent-loop.md` and the served guide text

- Line 58 (also 60-61): "The legacy `commands` field remains an alias for
  `focus_commands`." `BootstrapResponse` (`canopy-mcp/src/script.rs:213-234`)
  has no such field or serde alias, and `server.rs:527` reads
  `focus_commands`. Delete the sentence.
- Line 46: bootstrap "includes ... generated API text". The payload carries
  `api_sources`, and `server.rs:518` asserts there is no `api` key. Say "an API
  source inventory".
- `canopy-mcp/src/script.rs:396`: the guide tells agents to "Request
  `canopy.fixtures`". The Luau surface declares a global `fixtures()`
  (`base_api.rs:566-571`). A `script_api` filter for `canopy.fixtures` returns
  `not_found`. Change the instruction.

#### C108: `docs/scripting.md` drift

- "Generated API" item 2 omits `SemanticIdentity`, `SemanticActionStatus`,
  `WidgetSemantics`, `NodeSnapshot`, `FrameSnapshot` (`defs.rs:111-117,
  439-494`).
- "Commands" lists `exact`, `from`, `focus`. Code also accepts `anchor` (C18).
- "The text and the audited surface therefore cannot drift apart" holds for
  function signatures only (C15).
- Line 350 documents `visible` (C17).
- Lines 412-419 describe the legacy replay format (C9).

#### C109: `docs/styles.md` documents an `Inactive` state the code lacks

Lines 11, 31, 33-34 list `Inactive -> inactive`, give `button/inactive/border`
as an example, and say Button pushes `active` or `inactive`. `WidgetState`
(`style/mod.rs:37-60`) has no `Inactive`. `button.rs:152-163` pushes `active`
only when active. No theme defines an `inactive` rule. The examples work
around it with literal `"inactive/border"` layers (`widget/term.rs:156, 158`).
Decision: remove the rows from the doc, or add `WidgetState::Inactive` and
push it in `Button::render`. Removing is smaller and matches the resolver
fallback story.

#### C110: `clippy.toml` sets a default and an unreachable threshold

`cognitive-complexity-threshold = 25` is clippy's default. Delete the line.
`too-many-lines-threshold = 800` (default 100) makes the `too_many_lines` warn
lint unable to fire in practice. Decision: lower it to a value that binds, or
drop the lint and the threshold.

#### C111: `vendor/mlua` is 173 MB of local build output

`vendor/` is git-ignored and contains only `mlua/target`. No manifest,
`[patch]`, or `.cargo/config.toml` entry references it and `Cargo.lock` has no
`mlua`. Nothing in the repository changes. `rm -rf vendor` on this machine.

## Execution Plan

Validate every stage with `cargo clippy --workspace --all-targets
--all-features`, `cargo nextest run --workspace --all-features`, and `ncode api
--check`. Run `ncode api` whenever a stage changes public surface, and review
the capture diff before committing. Stages 1 to 3 need no decisions. Stage 4
starts after the decisions listed in C3, C9, C10, and C105 are made.

### Stage 1: Unattended cleanups in `crates/canopy`

- [ ] C34, C35, C36, C37 in `core/world` and `core/world/layout_driver`.
- [ ] C38, C39, C40, C41, C42, C43, C44, C45 in `core/script` and
      `core/commands.rs`. C39 also touches `canopy-widgets/src/image_view.rs`.
- [ ] C46, C47, C48, C49, C50, C51, C52, C53, C54 in `core/inputmap`,
      `core/termbuf`, `core/backend`, `core/event`, `layout.rs`,
      `core/render`, `core/testing`.
- [ ] C117 to C124 in `core/style`, `core/world/teardown.rs`, `core/canopy`,
      `core/text.rs`, `core/path.rs`, `core/testing`. C122 also touches
      `canopy-widgets`.
- [ ] C137, C138 alias removal, C139, C140, C141, C142, C143 in `core/canopy`,
      `core/error.rs`, `core/view.rs`, `lib.rs`.
- [ ] Refresh `api/canopy.rs` and `api/canopy-widgets.rs`.

### Stage 2: Unattended cleanups in widgets, automation, examples, tooling

- [ ] C55 to C70 in `crates/canopy-widgets`.
- [ ] C71 to C88 in `crates/canopy-mcp`, `crates/canopyctl`, `examples/todo`.
      Skip C75 and C76 if Stage 4 takes C9 with the legacy replay removal.
- [ ] C89 (fix plus a harness test for `FocusFrame` paging), C95 to C102 in
      `crates/examples`.
- [ ] C90 in `crates/canopy-geom`. C91 to C94 in `crates/canopy-derive`.
- [ ] C103, C104 in the manifests. C110 first line in `clippy.toml`. C111
      locally.
- [ ] Refresh every `api/` capture.

### Stage 3: Deslop batch items that need no decision

- [ ] C1: delete the exclusive frame stack. Migrate inputmap and world tests.
      rewrite the three `architecture.md` passages and the `root.rs` comment
      and test name.
- [ ] C2: delete both `impl dyn` blocks. Add extension-trait imports where the
      compiler asks.
- [ ] C3 unattended part: the five zero-consumer methods, the two test-only
      methods with replacements, and the three `View` aliases with their 47
      call sites.
- [ ] C4: synchronous session input. Remove the runtime and the two production
      dependencies. Update the feature table in `architecture.md`.
- [ ] C5: one `apply_edit`. Destructure keys once. Add the undo-after-repeat
      assertion.
- [ ] C6: remove the two checkpoint fields. Trim the two doc sentences.
- [ ] C7: label methods, shared eligibility and target declarations, one token
      builder, one `ScriptStructured` constructor.
- [ ] C8: `List<Text>` logs, list created in `on_mount`, `View` node removed,
      `mod inspector` private, one wrapping snapshot test.
- [ ] C106 unattended items and C107, C108 (the parts not tied to open
      decisions).

### Stage 4: Items gated on decisions

- [ ] C3 twin policy, then C11 as decided. Record the outcome in
      `docs/api-budget.md` and recalibrate every threshold (C105).
- [ ] C9: `plan_suite` in `canopy_mcp::smoke`. Legacy replay removal if
      confirmed. `discover_scripts` and `fixture_for_script` to `pub(crate)`.
- [ ] C10 if approved.
- [ ] C109, C110 second line, C33 as decided.
- [ ] C112, C114, C115, C116 second part as decided, and C113 if the shared seam
      is judged clearer.
- [ ] C20 (a) or (b), then C125 to C136 as decided. Take C126 and C127
      together, and C138 visibility with C128.
- [ ] Remaining structural candidates C12 to C32 as each decision is made. C16
      and C20 need coordinated changes in `ruau` and `itty`.
