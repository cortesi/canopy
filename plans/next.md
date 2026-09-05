# SPEC: Validated Canopy runtime and API recommendations

## Description

This plan resolves the runtime and API contract problems identified in
`tmp/canopy-recommendations.md`. It retains changes supported by current source,
existing consumers, and observable acceptance criteria. It covers all 27
recommendations, with bounded implementations for 26 and one deferred proposal.

## Selection and compatibility

The R identifiers match the source report. Each retained item states the
validated problem and the selected implementation scope. Validation is static:
the source establishes these behaviors, but the proposed regressions have not
been executed. No performance improvement is claimed without measurement.

Existing script behavior remains available during migration. `cmd_on` keeps its
relative search. Omitted binding phases keep their current path-derived phase.
Existing owner functions keep legacy argument decoding. New explicit entry
points supply unambiguous behavior, and documentation marks legacy forms.

Rust interfaces may change together with all workspace consumers. The project
has no released compatibility policy requiring duplicate Rust interfaces.
Low-level assembly remains available where it supplies a distinct capability.

Path prefixes `core/` and `widget/` are relative to `crates/canopy/src/`.
Package prefixes such as `canopy-widgets/` are relative to `crates/`.
Other paths are repository-relative unless identified as dependencies.

## Items

### R01: Preserve parent placement during widget layout refresh

- Outcome: Parent size constraints survive child invalidation without replacing
  the child's padding or child arrangement.
- Evidence: `core/world/layout_driver/mod.rs::refresh_layouts` replaces
  `Node.layout` with `Widget::layout()`. `core/world/tree.rs::with_layout_of`
  changes that same cache. Todo's `ensure_modal` copies `Frame::layout()` before
  changing four size bounds.
- Change: Store a widget base layout and a persistent sparse `LayoutOverride`.
  Compute the effective layout by applying the override after the base.
  Add `set_layout_override_of(NodeId, LayoutOverride)` and
  `clear_layout_override_of(NodeId)`. Override fields distinguish inheritance
  from explicit values, including clearing optional size bounds. Existing full
  setters establish full overrides. Closure setters persist only changed fields.
- Constraints: Keep one layout engine. Validate base and effective layouts before
  committing them. Replacement clears the override. Detach and reattach preserve
  it. Refresh changes the base only. Structural rollback includes both values.
- Proof: A framed child retains fixed height and padding across two invalidations
  and renders. Clearing the override restores widget defaults. Invalid effective
  constraints leave the previous layout intact.

```rust
// Current: the parent must copy Frame internals.
let mut layout = Frame::new().layout();
layout.min_height = Some(3);
layout.max_height = Some(3);
ctx.set_layout_of(frame, layout)?;

// Proposed: only the parent constraint is replaced.
ctx.set_layout_override_of(frame.into(), LayoutOverride::new().fixed_height(3))?;
```

### R02: Track changes independently of event propagation

- Outcome: Every mutation path schedules publication when needed.
- Evidence: `core/canopy/routing.rs::route_input` maps both `Handle` and `Consume`
  to a render request. `Canopy::with_context` and
  `CoreContext::invalidate_layout` do not set `Canopy.render_pending`.
  `Harness::key`, `mouse`, and `script` force a render afterward.
- Change: Add a core-owned `ChangeSet` for layout, paint, cursor, and observation
  changes. Core mutators record changes. Add
  `Context::invalidate(Invalidation)` with `Layout`, `Paint`, and `Semantics`.
  `invalidate_layout()` remains shorthand for layout refresh plus publication.
  Treat `EventOutcome` as propagation information only.
- Constraints: Conservatively invalidate after mutation callback access, including
  failed callbacks that may have changed widget state. Initially this can repaint
  a consumed key. Exact minimal repainting and removal of `EventOutcome` variants
  are not acceptance requirements. Read and render access do not invalidate.
  Render callbacks can update paint caches but cannot schedule another frame.
- Proof: Equivalent key, native context, command, fixture, and automation
  mutations publish equal frames without a test-forced render. No-op core focus
  and visibility setters retain their `ChangeOutcome::Unchanged` behavior.

### R03: Define coordinate conversion and measurement overflow

- Outcome: Rendering and pointer handling use named conversions, and a child can
  request bounded measurement under an overflowing ancestor.
- Evidence: `RoutedInput::local_mouse` subtracts `View.content`, not
  `View.outer`. Editor and List then add scroll. Frame adds `content_origin`.
  `Layout::inherit_overflow` uses logical OR for both axes.
- Change: Add signed `View` conversions among screen, viewport-local,
  outer-local, and scrolled content points. Names are
  `screen_to_viewport`, `viewport_to_content`, `viewport_to_outer`,
  `content_to_screen`, and `outer_to_content`. Use `PointI32` at these conversion
  boundaries. Replace overflow booleans with per-axis
  `MeasureOverflow::{Inherit, Bounded, Unbounded}`. The default is `Inherit`.
- Constraints: Routed mouse coordinates remain viewport-local for existing
  consumers. Signed conversions precede checked conversion to unsigned drawing
  coordinates. Existing `overflow_x()` and `overflow_y()` builders select
  `Unbounded`. Default fields retain `Inherit`.

  New per-axis policy setters select `Bounded`. Clipping retains the existing
  ancestor/content clip.
  There is no new clipping mode or general coordinate type algebra.
- Proof: Nested padding, both scroll axes, captured pointer input, and a wide
  grapheme identify the same content cell. A `Bounded` descendant terminates
  inherited unbounded measurement without changing clipping.

For outer origin `(10,4)`, padding `(1,1)`, and scroll `(0,3)`, screen `(13,6)`
maps to viewport `(2,1)`, outer `(3,2)`, and content `(2,4)`. The source report's
description of the existing mouse location as outer-local is incorrect.

### R04: Separate exact command targets from relative origins

- Outcome: Discovery and execution agree on their declared target policy.
- Evidence: `commands.rs::CommandResolver::resolve_owner` searches the origin's
  subtree in preorder, then ancestors. `Canopy::eval_root` anchors at root.
  Bindings anchor at their route node. MCP bootstrap uses focus availability.
- Change: Add `CommandTarget::{Exact(NodeId), From(NodeId), Focus}` and one
  resolver used by invocation and discovery. Add native
  `dispatch_exact(node, invocation)` and `dispatch_from(origin, invocation)`.
  Add Luau `canopy.call_exact(node, id, ...)`, `call_from(node, id, ...)`, and
  `call_focus(id, ...)`. Extend discovery with matching target options.
- Constraints: Unqualified owner calls remain relative to the current script
  anchor. Bootstrap reports that default and both root and focus availability.
  Exact dispatch requires the specified node to own a node command. It rejects
  free commands, wrong owners, and stale nodes. Relative free commands retain
  their context origin. `Focus` is resolved at invocation time.
- Proof: Two editors, with the second focused, yield the documented target for
  bootstrap, discovery, bindings, top-level eval, and exact dispatch. Inserting
  another editor cannot redirect an exact action.

```luau
-- Existing relative behavior remains explicit and available.
canopy.call_from(canopy.root(), "editor::undo")
-- Only the supplied editor may receive this command.
canopy.call_exact(right_editor, "editor::undo")
```

### R05: Add optional scoped semantic identity

- Outcome: Application selectors survive decorative tree changes.
- Evidence: `Node.name` serves paths and command ownership. `child_keys` names
  direct children only. `script/records.rs::node_info_to_arg` exposes neither
  application keys nor roles. Todo smoke selectors use structural paths.
- Change: Add optional `SemanticIdentity { scope: NodeId, key: String }` on a
  node. Add `set_semantic_key(node, scope, key)`, `clear_semantic_key(node)`, and
  `find_key(scope, key) -> Result<Option<NodeId>>`. Luau uses
  `canopy.find_key(key, scope?)`, defaulting to root. Scope membership requires
  the node to remain in the scope subtree, including detached internal trees.
- Constraints: Duplicate keys fail within one explicit scope. Root acts as the
  application scope. Keys are independent of type, command owner, and parent
  child keys. Moving within a scope preserves identity. Moving outside it clears
  the registration. Removing or replacing the keyed widget clears its identity.
- Proof: Todo's `new-item-input` remains addressable after wrapper insertion.
  Duplicate keys, cross-scope moves, replacement, rollback, and removal have
  deterministic results.

### R06: Generate typed calls and declarative application bindings

- Outcome: Rust argument mistakes fail during compilation, and simple key
  actions remain inspectable.
- Evidence: `canopy-derive/src/codegen.rs::accessor_tokens` emits parameterless
  `cmd_*` metadata accessors. `CommandCall` stores erased `CommandArgs`.
  `InputMap` already supports `BindingTarget::Command` for framework groups.
- Change: Generate `call_<command>(user_parameters...) -> CommandCall` beside
  each metadata accessor. Injected parameters are omitted. Add
  `CommandCall::with_target(CommandTarget)` and
  `Canopy::bind_command(key, BindingOptions, CommandCall) -> Result<BindingId>`.
  Add `canopy.bind_command(key, options, id, ...)` with positional arguments.
- Constraints: Keep one command registry. Declarative bindings have
  `BindingOwner::Application` and the same mode, replacement, and script-epoch
  cleanup as callbacks. Only simple one-command callbacks migrate. Buttons and
  List activation retain targets when storing calls. R04 and R09 define routing.
- Proof: `Todo::call_select_by(1)`, an explicit Luau call, and a declarative
  binding encode equal IDs and values. Wrong native argument types fail compile
  fixtures. Help exposes target policy and arguments without executing a closure.

### R07: Provide explicit positional and named script calls

- Outcome: A map's keys cannot change its argument convention on the new APIs.
- Evidence: `script/dispatch.rs::map_matches_named` guesses named arguments when
  every key matches a parameter. Empty tables become positional empty maps.
  MCP's `evaluate_applies_fixtures_and_named_optional_args` exercises legacy
  named calls, so removing the heuristic silently would break supported scripts.
- Change: All R04 `call_*` functions use positional values. Add
  `call_named(id, fields, target?)`, where `target` selects anchor-relative,
  exact, from, or focus resolution. Keep the existing owner functions, `cmd`,
  and `cmd_on` under their documented legacy decoding convention.
- Constraints: Generate accurate declarations for both conventions. Document
  explicit calls as the default for new structured arguments. No global mode
  can change the meaning of a stored callback. Native `CommandArgs` is unchanged.
- Proof: Cover empty maps, parameter-name collisions, nested maps, optional
  records, duplicate normalized names, extra arguments, and wrong named fields.
  Existing legacy tests continue to pass.

```luau
canopy.call_from(canopy.root(), "app::configure", { options = "dark" })
canopy.call_named("app::configure", { options = { options = "dark" } })
```

### R08: Report and enforce command eligibility

- Outcome: Discovery, help, and command buttons share a disabled reason.
- Evidence: `CommandAvailability` contains only a specification and resolution.
  Todo's `delete_item` succeeds without work when selection is absent. Required
  injected event values are checked only during invocation.
- Change: Add `CommandStatus::{Enabled, Disabled(String)}` and optional
  read-only status functions generated by `#[command(enabled = "can_delete")]`.
  A node status function receives `&self` and `&dyn ViewContext`, returning
  `Result<CommandStatus>`. Add typed read-only widget access to ViewContext so
  Todo can inspect its List selection without mutation. Discovery
  reports resolution, status, reason, and missing event requirements separately.
  Invocation rechecks status and returns a structured disabled-command error.
- Constraints: Status functions are cheap, read-only, and cannot promise external
  transaction success. Authorization remains separate. Default status is enabled.
  Calls to an active widget use the already borrowed target for eligibility,
  avoiding a second widget-slot borrow. R04 supplies the target policy.
- Proof: Todo delete eligibility changes with selection. Invocation rejects a
  formerly enabled action after its state changes. Missing mouse/list injection
  differs from disabled state and unknown commands.

### R09: Store input phase independently of the selector

- Outcome: A selector edit does not change an explicitly declared input phase.
- Evidence: `inputmap/mod.rs::binding_phase` derives phase from
  `PathMatch.anchored_end`. `routing.rs` permits pre-widget bindings only for
  keys. Help already reports `BindingPhase`.
- Change: Add optional `BindingOptions.phase` with `BeforeWidget` and
  `AfterIgnore`, serialized as `before_widget` and `after_widget` in Luau.
  Records store the option. Resolution prefers an explicit phase and otherwise
  uses the existing path rule. Migrate all workspace bindings to explicit phases.
- Constraints: Keep current scope, specificity, and insertion ordering. Reject
  explicit `before_widget` for mouse bindings. Do not reinterpret legacy omitted
  phases or add numerical priority. Application ownership remains unchanged.
- Proof: Identical selectors with different phases produce different route
  traces and matching help metadata. TermGym's F6 control still precedes terminal
  input, while shell navigation keys retain the established behavior.

### R10: Reuse one modal interaction scope

- Outcome: Nested modals coordinate bindings, focus, pointer capture, and owned
  visual effects.
- Evidence: `Root::show_help` and `hide_help` coordinate snapshots, tokens,
  capture, dimming, and focus. Todo's `sync_modal_state`, `enter_item`, and
  `cancel_add` separately coordinate a subset. Exclusive frames filter bindings,
  while ordinary widget routing still needs a modal subtree boundary.
- Change: Add `open_modal(ModalOptions) -> Result<InteractionToken>` and
  `close_modal(InteractionToken) -> Result<()>` to `Context`. Options specify
  owner, modal subtree, initial focus, optional background dim target, and
  binding admission. Admission accepts a framework group or application bindings
  evaluated only on the modal-to-owner route. Open captures focus ancestry,
  clears capture, reveals the modal, and records owned effects.
- Constraints: Only the top scope receives normal input. Pointer events outside
  its subtree are consumed. Focus setters stay inside it. Escape dismissal is
  an ordinary explicitly registered binding. Close removes only owned effects,
  hides the modal, and restores exact focus, then a surviving ancestor subtree.

  Successful close does not restore old pointer capture. Failed open restores it.
  Removing an owner retires its nested scopes. R11 handles callback teardown.
- Proof: Root help and Todo use the same scope implementation. Nested close,
  removed origin focus, background capture, outside clicks, failed open, and
  owner replacement preserve the declared focus and token rules.

### R11: Support teardown after callbacks return

- Outcome: A button can close its containing dialog without violating widget
  access rules.
- Evidence: `WidgetSlotGuard` temporarily removes the active widget.
  `world/tree.rs::plan_subtree_removal` rejects active subtrees before hooks.
  `Button::press` dispatches its containing action synchronously.
- Change: Add `Context::remove_after_dispatch(NodeId) -> Result<()>`. Store a
  node ID plus widget incarnation in a FIFO completion batch. Drain the batch
  after the outermost dispatch callback returns and before publication.
  Modal close uses the same completion boundary when callbacks are active.
- Constraints: A failed initiating dispatch discards its uncommitted batch.
  Immediate mutations already made are not undone. Nested dispatch uses queue
  checkpoints. Removal of an already removed target is an idempotent success.
  Replacement at the same ID makes the old request stale and prevents deletion.

  Completion failures return through the initiating turn. No arbitrary closures
  or deferred replacement API are required initially.
- Proof: Button and script teardown run hooks after widget restoration. A failed
  action leaves queued removal unapplied. Duplicate, ancestor/child, stale, and
  vetoed removals have deterministic results.

### R12: State the structural rollback guarantee precisely

- Outcome: A tree edit cannot be mistaken for a widget or database transaction.
- Evidence: `TreeStateSnapshot::capture` clones nodes that share widget slots.
  It explicitly excludes the binding registry. `Widget::on_mount` already warns
  that widget state and external effects require compensation. The public
  `Context::apply_tree_edit` still says only that mutations are atomic.
- Change: Rename the Rust helper to `edit_structure` and document the exact
  journal boundary. Preserve its immediate closure semantics. Add a restricted
  detached composition builder under R14, rather than a second general
  `TreePlan` representation.
- Constraints: The guarantee covers captured arena metadata, topology, keys,
  focus, capture, and other enumerated `TreeStateSnapshot` fields. Widget state,
  binding registration, and external effects remain outside it. New runtime-owned
  scope and lifetime changes commit only after structural success. Nested
  failures restore their structural checkpoint. Lifecycle cleanup is repeatable.
- Proof: Extend current nested-edit and mount-failure tests to distinguish restored
  structure from retained widget mutation. Document compensation for external
  effects and binding registration without claiming rollback.

### R13: Define node and attachment resource lifetimes

- Outcome: Detach, hide, replacement, and destruction have explicit effects on
  runtime-managed work.
- Evidence: `detach` preserves mounted nodes. Reattach skips completed mounts.
  `Terminal::on_mount` starts a session. `poll_node` tests node existence only,
  and replacement keeps `NodeId` while changing the widget slot.
- Change: Introduce `WorkLifetime::{Node, Attachment}` for R20 wake handles and
  scheduled polling. Node lifetime ends on widget replacement or removal.
  Attachment lifetime also ends when the owner becomes detached. Reattachment
  starts a new attachment generation. Hiding does not end either lifetime.
- Constraints: Existing widget polls default to node lifetime to preserve
  background terminal behavior. Add `Widget::poll_lifetime()` with that default.
  Attachment polling is reinitialized after reattach. Commit lifecycle changes
  after structural success. Mount/unmount semantics remain unchanged.
- Proof: Count poll and wake delivery across hide, detach, reattach, replacement,
  removal, and failed edits. No old completion reaches a replacement widget.

### R14: Add scoped detached composition

- Outcome: Application construction expresses nesting and applies layout before
  a newly built subtree mounts.
- Evidence: Todo's `ensure_tree` and `Root::install_app_with_inspector` repeatedly
  pass parent IDs through add/attach operations. The existing read-only
  `Context::children()` name is already occupied.
- Change: Add `Context::compose(parent, build)` through an inherent generic
  adapter on `dyn Context`. `ChildBuilder::child(widget, configure)` and
  `keyed::<K>(widget, configure)` return typed IDs. Each nested builder supplies
  layout overrides and semantic keys for its detached subtree. The whole subtree
  attaches after configuration succeeds.
- Constraints: Use existing creation and attachment operations inside
  `edit_structure`. A builder cannot dispatch commands or mutate existing
  widgets. External state captured by a closure remains application-owned.
  Keep typed keys and direct APIs. Defer wholesale context-trait replacement.
- Proof: Todo composition matches existing topology, keys, and geometry.
  Failed configuration leaves no attached partial subtree. Mount sees completed
  child structure and placement. Construction retains preorder mount behavior.

### R15: Enforce collection ownership and domain-key selection

- Outcome: Reconciliation preserves domain identity and cannot silently detach
  unrelated child widgets.
- Evidence: `KeyedChildren::reconcile` installs its managed IDs as the parent's
  entire child list. `List<W>` uses private monotonic `ListKey` values and exposes
  index-based updates. Todo rows already carry stable SQLite IDs.
- Change: Make whole-parent ownership explicit in `reconcile` documentation and
  reject unmanaged direct children before mutation. Use a dedicated rows container
  when headers or footers exist. Extend `List` to `List<W, K = AutoKey>`, with
  `K: Eq + Hash + Clone + ToArgValue + 'static`. Add `reconcile`, `selected_key`,
  `select_key`, and `item_for_key`. Retain append/insert conveniences for `AutoKey`.
- Constraints: Store selection by key and derive indices. Removing the selected
  key selects its next neighbor, then previous, then none. Duplicate desired keys
  fail before mutation. Reorder preserves node identity and pending activation
  follows its key. Reconcile's widget update closure has the R12 rollback limits.
- Proof: Todo uses `List<TodoEntry, i64>`. Reordering keeps selected domain and
  node IDs. A sibling header outside the dedicated container survives. Unmanaged
  direct children, duplicate keys, and update failure cannot corrupt the map.

### R17: Inject the Todo store into each application

- Outcome: Two Todo instances on one thread use independent databases.
- Evidence: `examples/todo/src/store.rs::STORE` is thread-local.
  `create_app_with_config` replaces it, while commands and fixture helpers call
  `current_store` again. `Store` already clones an `Rc<Connection>`.
- Change: Expose `Store::open`, add `Todo::new(Store)`, and store that handle on
  Todo. Add `create_app_with_store(Store, Option<&Path>)`. Keep path-based app
  constructors as wrappers. Fixture callbacks obtain the store through their Todo
  instance, rather than capture a non-`Send` global handle.
- Constraints: Remove `STORE`, `store::open`, `store::get`, and `current_store`.
  Keep SQLite schema and transaction behavior. Tests retain explicit Store handles.
  There is no new framework resource registry or additional `Rc<Store>` layer.
- Proof: Alternate commands and all three fixtures between two apps on one
  thread. Assert independent database records, row state, and modal state.

### R18: Drive live and headless execution through one turn boundary

- Outcome: Scripts can wait while input, timers, layout, and frame publication
  continue through the same runtime path.
- Evidence: `backend/crossterm.rs::runloop`, `Harness`, and MCP's
  `evaluate_in` independently decide when to render. `Canopy::render` performs
  initialization and startup work. `script/base_api.rs::wait_until` services
  automation and repeatedly yields. `LuauHost::run_root_async` holds runtime and
  Canopy borrows across `block_on`.
- Change: Add a single-threaded driver within `Canopy`, with
  `turn(Work) -> Result<TurnOutcome>`. Work covers input, timer/wake delivery,
  evaluation start, evaluation cancellation, and explicit preparation.
  `TurnOutcome` reports publication ID, completed eval results, and exit status.
  The order is dispatch, structural completion, initialization, layout, paint,
  snapshot publication, then wake satisfied waits for a later turn.
- Constraints: Keep one active top-level eval. Reject another eval as busy while
  permitting input and bounded native automation work. Parked waits use runtime
  wake signals and deadlines, not yield loops. Each resumed segment retains its
  original script anchor and checks whether that anchor is live. Focus-targeted
  calls deliberately resolve current focus. A synchronous native callback remains
  cooperative and cannot be preempted.
- Proof: The same controlled timer/input trace satisfies a pending wait through
  terminal, native harness, and MCP execution. Verify cancellation, busy errors,
  publication before successful completion, and no recursive top-level eval.

The current Ruau dependency already supplies `create_root_invocation`,
`poll_invocation_with_context_and_result`, and `abort_invocation` in
`../../private/ruau/crates/ruau/src/session/retained.rs`. A pending poll retains
the invocation but releases borrowed host context. The driver uses this API
instead of extending `run_with_context` or modifying Ruau.

Published frames and terminal output have separate ownership. The driver owns
prepared frame data. Each output adapter tracks its last successfully emitted
frame. A snapshot query cannot advance the terminal's diff baseline.

### R19: Publish coherent semantic snapshots

- Outcome: Screen cells, geometry, identity, and semantic state refer to one
  completed frame.
- Evidence: Screen queries can call `refresh_snapshot` and render through
  `NopBackend`. Node queries read cached geometry. `node_info_to_arg` reports
  `visible = !hidden`, ignoring attachment and ancestors.
- Change: Add owned `FrameSnapshot { frame_id, viewport, nodes, cells, focus }`.
  `Canopy::snapshot()` and `canopy.snapshot()` return the last publication without
  running hooks. Add explicit `canopy.flush()` to request preparation after native
  dispatch has unwound. Legacy screen getters retain their refresh behavior
  through that explicit driver path. Add `Widget::semantics(&self, view)` for
  role, label, optional value, selection, and action status.
- Constraints: Include `attached`, `displayed`, and `intersects_viewport` fields.
  Displayed means attached with no hidden or `Display::None` ancestor.
  Intersection uses accumulated clipping and the viewport. Occlusion is omitted.

  Roles and values are optional. Input value exposure is configurable and
  sensitive values are omitted. No arbitrary widget serialization is permitted.
  R05 keys, R08 status, and R18 publication feed this snapshot.
- Proof: Hidden ancestry, display suppression, detached nodes, offscreen nodes,
  and scrolling produce distinct flags. Reading a snapshot changes neither
  lifecycle counts nor frame IDs. Terminal diff output remains complete after
  in-script screen observation.

### R20: Add cancellable node wake handles

- Outcome: Background producers can wake one owning widget without retaining a
  generic UI-thread mutation closure.
- Evidence: `Poller` schedules raw NodeIds through a worker thread. Terminal
  polls every fixed interval. `AutomationHandle` marshals whole-Canopy closures.
  Replacement can preserve a NodeId while changing the widget incarnation.
- Change: Add `Context::wake_handle(WorkLifetime) -> Result<NodeWakeHandle>` and
  `NodeWakeHandle::wake() -> Result<WakeOutcome>`, where outcomes distinguish
  queued, coalesced, and expired work. A wake requests one owner poll. Handles
  bind node, widget incarnation, and optional attachment generation. Dropping
  the owner expires its handles and cancels scheduled polls.
- Constraints: Coalesce pending wakeups per owner. Store no unbounded result
  queue in Canopy. Producers own bounded channels and cooperative cancellation.
  Keep the current timer/poll API. A general async job executor and terminal
  driver notification redesign require a concrete producer API and remain
  deferred. R13 defines lifetime policy, and R18 provides delivery.
- Proof: Deliver a fake worker result through a bounded channel and wake handle.
  Detach or replace its owner before delivery. Assert expiration, bounded pending
  work, cancellation, and no mutation of the replacement.

### R21: Configure terminal interrupt policy

- Outcome: Embedded terminals can receive Ctrl+C through the live adapter.
- Evidence: Crossterm `runloop` intercepts every Ctrl+C before dispatch and
  returns 130. `Terminal::on_event` otherwise forwards keyboard input to Itty.
- Change: Add `RunOptions { interrupt_policy, emergency_exit }` and
  `runloop_with_options(Canopy, RunOptions)`. Policies are `Exit130` and
  `RouteToApplication`. An optional emergency key is a separate exact key match.
  Keep `runloop` as the existing `Exit130` default. Configure TermGym for routed
  Ctrl+C and an explicit Ctrl+Alt+Q emergency exit.
- Constraints: Preserve `TerminalSession` cleanup on exit and failure. Do not
  change process signal handlers. Adapter policy runs before R18 normal input.
  Headless adapter tests exercise the same decision function.
- Proof: Ctrl+C reaches a test terminal session under routed policy without host
  exit. Legacy policy exits with 130. The configured emergency key restores the
  terminal once and bypasses widget dispatch.

### R22: Gate heavy widgets behind capability features

- Outcome: Basic forms avoid image, font, syntax-highlighting, and terminal
  emulator dependencies.
- Evidence: `canopy-widgets/Cargo.toml` has no feature gates for `image`,
  `fontdue`, `syntect`, or `itty-core`. Input imports the editor's shared text
  buffer. Root always installs Inspector, even when hidden.
- Change: Keep basic widgets unconditional. Add `editor`, `terminal-widget`,
  `graphics`, and `devtools` features, enabled by default. Make their heavy
  dependencies optional. Move Input's shared buffer, position, edit, and selection
  machinery into a feature-independent text module. Keep existing editor exports
  through re-exports when `editor` is enabled. Devtools controls Inspector code
  and construction.

  Root help remains available without devtools.
- Constraints: Luau and the Crossterm core remain standard dependencies.
  Default features preserve the complete current bundle. Minimal widgets still
  use Ropey. No optional core-scripting or core-terminal feature is introduced.
  Full API generation remains authoritative. Feature checks build minimum and
  full profiles, plus each independent capability.
- Proof: Minimal Input/List/Root builds exclude the four heavy dependency groups.
  Full builds include all demos. Root without devtools creates no inspector nodes.
  Feature changes preserve basic widget interaction and layout results.

### R23: Add an explicit setup builder

- Outcome: Registration, finalization, assembly, and startup have one documented
  order and a consuming failure boundary.
- Evidence: Todo setup loads two command groups, registers fixtures, finalizes,
  evaluates bindings/config, and then installs Root. `ensure_finalized` can seal
  the API implicitly. Startup hooks run later during rendering.
- Change: Add `CanopyBuilder` with `configure(FnOnce(&mut Canopy) -> Result<()>)`,
  `bindings(name, source)`, `config(path)`,
  `assemble(FnOnce(&mut Canopy) -> Result<()>)`, and `build() -> Result<Canopy>`.
  Configuration closures run before finalization. Binding/config sources run
  after finalization. Assembly runs afterward. `build` consumes the builder.
- Constraints: Reuse `Loader`, fixtures, module roots, and finalization rollback.
  The first R18 preparation runs app, user, and project startup setup in current
  order, then `on_start` after valid geometry and before first publication.
  The existing low-level path remains supported. Failed setup publishes no app,
  but cannot undo native or database effects. Per-layer hot reload is deferred.
- Proof: Convert Todo and shared example setup. Fail each setup phase and build
  a fresh instance again. Confirm no duplicate bindings or successful startup
  effects within one instance, and accurate API output without starting a UI.

### R24: Declare stable style roles and states

- Outcome: Stock widget themes can target documented roles without depending on
  decorative child structure.
- Evidence: Button pushes `button/active` or `button/inactive`.
  The `active` state is independent of command eligibility.
  `StyleManager` already resolves hierarchical
  string paths and supports inherited effects.
- Change: Add stock role constants and a small `WidgetState` mapping to existing
  string layers. Start with Button label/border and Input text/cursor roles.
  States distinguish focused, selected, disabled, and pressed/active. Use R08
  status for disabled command buttons. Document each role in `docs/styles.md`.
- Constraints: Preserve current fallback and custom string extensions. Existing
  active/inactive paths remain meaningful. Do not add another cascade or a CSS
  engine. Inspector rule provenance is deferred until the resolver exposes it.
- Proof: Resolve stock roles through changed decorative wrappers and compare
  styles. Disabled and selected states remain separate from focus. Existing
  theme and rendering goldens continue to pass unless intentionally updated.

### R25: Declare the trusted-local scripting boundary

- Outcome: Applications explicitly choose whether to execute local scripts and
  expose live automation, without claiming VM isolation limits native authority.
- Evidence: `set_user_script_root` and `set_project_script_root` register startup
  modules against the full command surface. MCP `serve_uds` exposes live eval
  without an application permission profile. Native modules remain trusted code.
- Change: Add `ScriptTrust::{Disabled, TrustedLocal}` to builder script-root
  registration and `AutomationPolicy::{Disabled, TrustedLocal}` to launch
  configuration. Disabled roots are not mounted, required, or executed. Disabled
  automation creates no listener. Existing explicit low-level calls remain
  documented trusted-local operations.
- Constraints: Preserve current behavior for apps that explicitly select
  `TrustedLocal`. Document socket placement and host filesystem permissions as
  deployment responsibilities. Do not claim read-only eval, command allowlists,
  or a first-use approval UI. Those require an application threat model covering
  commands, input, fixtures, modules, and nested native calls.
- Proof: Disabled project/user roots cannot execute startup or required modules.
  Disabled live automation creates no socket. Trusted-local examples document
  that scripts and automation can exercise all exposed native actions.

### R26: Describe sessions and version replay metadata

- Outcome: Automation reports whether state persists and validates replay
  assumptions before executing steps.
- Evidence: `AppEvaluator` creates `HeadlessSession` per request at 120 by 40.
  `evaluate_live` reuses an app and rejects an inline fixture. Bootstrap has an
  API digest but no execution mode. `canopyctl/src/replay.rs` stores source/results
  without viewport, API identity, or reset semantics.
- Change: Add execution mode, session ID, viewport, and domain reset policy to
  bootstrap and eval responses. Add optional headless viewport to requests.
  Add `canopy.replay/1` envelopes with app identity, API digest, execution mode,
  viewport, fixture/reset metadata, and ordered source/expected-success steps.
- Constraints: Keep `fresh-app-per-eval` and `live-session` as the two modes.
  Replay steps follow the declared mode. Multi-step fresh scenarios use one eval
  source. Do not introduce persistent headless sessions.

  App configuration
  supplies reset policy as `external`, `fixture`, or `isolated`. The factory does
  not infer isolation from a new UI instance. Strict replay rejects mismatches.
  An explicit `--allow-mismatch` permits compatibility overrides.

  Legacy journals require explicit legacy mode.
  Node tokens never become durable references.
- Proof: Check viewport, digest, mode, missing fixture, and persistent-database
  mismatches before mutation. Verify independent eval IDs and stable live session
  IDs. CLI, proxy, direct MCP, and smoke requests preserve the metadata.

### R27: Test contracts through the shared driver

- Outcome: The harness exposes scheduling defects instead of repairing them with
  an unconditional render.
- Evidence: `Harness::{key, mouse, script}` explicitly render after actions.
  Current tree property tests, lifecycle tests, route tests, and buffer diff
  equivalence tests already cover the lower layers.
- Change: Route public Harness action helpers through R18 turns. Add controlled
  clock/wakeup input and snapshot assertions. Keep `Harness::render` as explicit
  widget-output tooling. Share a small contract suite across native, terminal,
  and MCP adapters, with real `Root -> MainPane -> app` composition where relevant.
- Constraints: Retain existing property, ABI, Unicode, and diff-equivalence tests.
  Do not claim deterministic native side effects or depend on sleeps. Tests
  distinguish adapter interrupt policy and headless fixture rules explicitly.
- Proof: Permanent cases cover layout persistence, exact targets, argument
  shapes, modality, teardown, app isolation, waits, observations, interrupt
  policy, and replay compatibility. Each case maps to its originating R item.

## Rejected and deferred scope

| Recommendation | Decision | Evidence and reconsideration condition |
| --- | --- | --- |
| R16 | Defer the data-list implementation. | `List` intentionally supports interactive widget rows. ListGym creates ten rows, and current benchmarks do not establish a large data-list bottleneck. Require a real dataset, measured cost, and logical-item automation contract first. |
| R01 | Defer separate `Placement` and `ContentLayout` types. | Base layouts plus sparse overrides solve the demonstrated ownership problem within the existing engine. |
| R02 | Defer a new event enum and perfect minimal invalidation. | Conservative mutable-access invalidation establishes correctness without a second callback API. |
| R03 | Correct the existing mouse coordinate claim. | The current adapter produces viewport-local content coordinates. New clipping modes and pervasive wrappers are not justified. |
| R12 | Defer a general structural plan/commit language. | An existing journal and a restricted detached builder cover validated consumers. Existing rollback warnings remain applicable. |
| R13, R20 | Defer attachment hooks and a general job executor. | Runtime-owned poll/wake lifetimes address current ownership failures. No application search-job or subscription abstraction exists to justify a new executor. |
| R14 | Defer a context-trait reorganization. | Scoped composition has concrete callers. Several new capability adapters would otherwise only forward existing methods. |
| R22 | Defer optional core Luau/Crossterm. | The current product treats scripting and terminal operation as standard capabilities. Heavy widget bundles have clear manifest boundaries. |
| R23 | Defer component registries and per-layer hot reload. | Existing Loader registration can populate a consuming builder without another ownership registry. |
| R24 | Defer a new theme cascade and provenance inspector. | Stable roles can use the current resolver. Provenance is a separate diagnostic surface. |
| R25 | Defer restricted eval and first-use approval. | A trustworthy capability policy needs an application threat model and complete mutation coverage. Trusted-local behavior can be stated precisely now. |
| R26 | Defer persistent headless scenarios. | Explicit current modes and replay metadata solve the demonstrated ambiguity. |

## Implementation contracts

### Runtime ownership and failure

`Core` owns the tree, widget incarnations, change records, modal scopes, and
structural completion batches. `Canopy` owns scheduling, script execution,
startup state, and immutable frame publication. Adapters own terminal I/O or MCP
transport, never widget borrows. A runtime turn does not make widget mutations
transactional.

`Work` has `Input(Event)`, `Wake`, `StartEval(EvalRequest)`,
`CancelEval(EvalId)`, and `Prepare` variants. `EvalRequest` owns source, timeout,
and script anchor. `TurnOutcome` contains an optional new `FrameId`, completed
eval outcomes, and an optional exit code. Native context access records changes.
The caller then submits `Prepare`, or the containing driver turn prepares them.

An eval ID belongs to one Canopy instance. A start while another eval is active
returns `Busy`. Cancellation calls Ruau's `abort_invocation` and completes its
journal once. It does not undo completed commands or published frames.
Reload during active evaluation
returns `Busy` instead of invalidating retained handles.

Each VM poll establishes the original script anchor for that segment. Each
input callback establishes its route anchor independently. Pending VM work holds
no widget, Canopy, or retained-runtime borrow. Host futures signal a coalesced
waker. Predicate waits subscribe to publication changes and optional deadlines.
They recheck immediately before parking, preventing a lost publication wake.

Read-only predicates do not create a fresh frame simply by being checked.

Input and native automation callbacks can run between VM segments. Callbacks
that require Luau use a fresh runtime scope after the previous segment releases
it. They cannot start another top-level eval. A turn services at most the
existing `AUTOMATION_SERVICE_BUDGET` callbacks before yielding to other work.
The driver owns deadlines, with a monotonic clock supplied by live or test code.
Binding callbacks remain synchronous and cannot suspend. Pending top-level eval
does not make an ordinary binding callback fail the top-level busy check.

Detached polling preserves the existing total script resource limits. The
driver subtracts each `InvocationPollUsage.gas_spent` from the invocation budget.
It applies the remaining budget to each VM segment. An absolute deadline bounds
parked work as well as VM execution. Cancellation, deadline expiry, and normal
completion each release the invocation, wait registrations, and retained
temporary values exactly once.

Every successful outer native dispatch drains its teardown before returning to
its caller. A top-level eval is not one long structural dispatch. Its command
calls each complete their native dispatch before the next script statement.
An input binding owns one dispatch through its synchronous Luau callback.
Nested native dispatch joins that batch, with failure checkpoints.

No teardown batch crosses a VM suspension. Unrelated input while an eval is
parked owns a separate batch. An initiating callback failure discards its batch.
A drain executes FIFO removals until the first failure, then discards the
remaining requests. Earlier successful removals remain committed. Lifecycle
hooks cannot enqueue additional teardown while a batch drains.

Completion failure returns to the command or input caller. A subsequent script
error or cancellation does not reverse an earlier successful command's teardown.
Completed immediate work remains observable. A preparation or paint error keeps
the previous snapshot and pending change flags. A backend write failure does not
advance that backend's emitted-frame baseline.

Live eval requests use a typed automation message, not a blocking eval inside
`AutomationHandle::request`. Add `submit_eval(EvalRequest) -> Result<EvalTicket>`
and `cancel_eval(EvalId) -> Result<ChangeOutcome>`. A ticket contains its eval ID
and a completion receiver. MCP awaits that receiver outside the UI thread.
The driver sends the final result after the required publication succeeds.

`ScriptTaskState` adds `Cancelled`. Timeout remains `TimedOut`, while dispatch
and preparation errors produce `Failed`. Terminal failure completes waiting
request receivers with an error before shutdown. Synchronous eval conveniences
drive a headless loop only when no adapter turn is active. Reentry returns a
structured error instead of starting a nested loop.

Layout preparation runs after lifecycle initialization. Startup `setup` runs
once after assembly and before the first prepared frame. `on_start` runs after
initial valid geometry, with one subsequent preparation before publication.
Repeated observation cannot run startup hooks again. Public `render` remains an
explicit prepare-and-output convenience. Ordinary adapters call the shared
driver and emit its published frame.

### Public interface details

`LayoutOverride` mirrors every `Layout` property with explicit inheritance.
Optional bounds use `Option<Option<u32>>`: outer `None` inherits, while
`Some(None)` clears a bound. Convenience builders such as `fixed_height` set
both corresponding bounds. A closure setter compares effective fields before
and after the closure, merging changed fields into the persistent override.
Calling it with equal values does not add an override.

Named coordinate conversions use `PointI32` inputs and return
`Result<PointI32>`. Intermediate arithmetic uses `i64`, and out-of-range results
return a geometry error. Negative coordinates remain valid until a drawing or
hit-test operation requires a checked unsigned point. The existing unsigned
mouse boundary retains its current clamping behavior during compatibility
migration. New signed helpers do not claim to recover already-clamped values.

`BindingOptions` contains path, scope/mode, description, source, and optional
phase. Declarative actions store both invocation and optional target policy.
An omitted action target means the binding's route origin. A targeted button
retains the target when converting its call to a stored action. List's injected
row context retains its existing index and adds the stable key as `ArgValue`.

`call_named` takes `{ kind = "exact", node = id }`,
`{ kind = "from", node = id }`, or `{ kind = "focus" }` as its optional target.
Omission means the current script anchor. `canopy.commands(target?)` uses the
same shape and default. Bootstrap includes separate `default_commands` and
`focus_commands`, with `default_target = "root"`. Its existing `commands` field
remains a documented focus-availability alias during script-client migration.

`CommandStatus` hooks apply to node commands in this batch. Free commands are
enabled by default. Generated erased status functions accept `&dyn Any` plus
`&dyn ViewContext`, returning `Result<CommandStatus>`. They run after target
resolution and before mutable command execution. Status does not replace
argument or required-injection validation.
Help command entries and semantic Button observations expose the same status.

Add object-safe `ViewContext::read_widget(NodeId, callback)` with an immutable
widget callback. An inherent `with_widget_read<W, R>(TypedId<W>, callback)`
performs checked downcasting and returns the callback result. Both methods use
`Core::with_widget_read`, without extracting the widget or invalidating it.
Borrow failures remain errors. Todo's eligibility hook reads List selection
through this interface.

Add `ViewContext::command_status(target, invocation)` for semantic action
widgets. Generate explicit event, mouse, and list-row requirements in command
metadata, including optional injection. Report missing requirements from the
current `CommandScopeFrame`. Do not infer injection kind from Rust type-name
strings or turn inspection errors into disabled results.

Semantic keys use an explicit arena scope ID, not implicit nearest-container
scope inference. The runtime index maps `(scope, key)` to one node and follows
structural rollback. A snapshot includes every live arena node, including
detached nodes, with parent and child IDs. `find_key` searches the requested
scope's index. Reusable application components choose their own scope root.

Scope membership changes become final at the outermost successful structural
edit. Wrapper insertion around a keyed node uses one `edit_structure` that
includes detach, wrap, and reattachment. A detached subtree moved outside its
semantic scope in a completed edit loses those scoped registrations.

`ModalOptions` names the owner and modal subtree separately. The owner must
contain the modal, and initial focus must be inside it. Nested modals must be
inside the currently admitted interaction region. Focus restoration remains
inside the next surviving modal scope. Closing a non-top token closes its nested
scopes first. Removal and replacement retire scopes after structural commit.

Native exact commands retain their explicit authority beyond normal input
modality. This is an interaction rule, not a script security boundary.

`ChildBuilder` can add detached children and set their layout or semantic
identity. It has no mutable access to existing widgets. Configured child trees
attach in source order. Its methods are generic conveniences over the existing
object-safe `Context`, without exposing `Core`.

`FrameSnapshot` owns its cells and semantic node data. Rust accesses it through
`Arc<FrameSnapshot>`, and Luau receives detached snapshot records. Initial
`snapshot()` returns `None`/`nil` until preparation publishes a frame. The first
headless session turn prepares before evaluating user source. `flush()` fails
with `InvalidPhase` if a widget mutation callback still holds an empty slot.
It can run at a top-level Luau host-call boundary after commands return.
Non-displayed nodes have no current screen rectangle. Offscreen displayed nodes
retain their computed rectangle and report no viewport intersection. Old
snapshots retain their owned values even after their live NodeIds expire.

`NodeWakeHandle` is `Send + Sync` and never contains a widget or `&mut Canopy`.
It uses an incarnation token separate from SlotMap generation, because
`replace_subtree` preserves NodeId. Attachment handles additionally capture an
attachment generation. Expired handles return `Expired` without enqueuing work.

Node polling continues while detached by default. Attachment polling pauses and
reinitializes on reattachment. Hiding alone never pauses polling.

Builder closures are owned `FnOnce` values and run in insertion order within
their phase. Script-root methods take `(PathBuf, ScriptTrust)`. Their default is
disabled.

Disabled automation is the launch default. Explicit MCP launch modes
select `TrustedLocal`. Raw `serve_uds` remains an explicitly trusted low-level API.
The builder publishes no Canopy value on failure.

Rebuilding requires a fresh builder and application-owned resources with
suitable retry behavior.

### Replay schema

```json
{
  "schema": "canopy.replay/1",
  "app": "todo",
  "api_digest": "recorded-api-digest",
  "execution": "fresh-app-per-eval",
  "viewport": { "width": 120, "height": 40 },
  "fixture": "with_items",
  "reset": "isolated",
  "steps": [
    {
      "source": "todo.select_first(); todo.delete_item()",
      "expect": { "success": true }
    }
  ]
}
```

The application declares its identity and domain reset contract when it creates
the evaluator or launch configuration. Todo's `:memory:` factory declares
`isolated`. A file database declares `external` unless a fixture reset is
explicitly requested. Live replay accepts `external` or an explicitly applied
fixture. It never claims that reconnecting resets application state.

CLI replay accepts either an explicit live socket or a headless command.
`--allow-mismatch` reports each compatibility override before execution.
Malformed schemas, missing fixture implementations, and invalid viewport sizes
remain errors. `--legacy` accepts old journals with explicit fixture/viewport
options and no claim of complete reproduction. Expected failure steps are
executed when explicitly selected and compared with their recorded expectation.

## Execution Plan

Each stage is a coherent implementation and review boundary. Stages run in the
order below. Checkboxes remain unchecked until implementation and proof pass.
Without separate commit authority, review the uncommitted changes before the
next stage. Stage boundaries do not authorize commits.

The native broad gate is `ncode test`, followed by `ncode tidy --check` and
`cargo xtask smoke`. `ncode tidy --check` includes the configured feature, API,
Luau, and benchmark compilation hooks. `cargo xtask api` regenerates public API
skeletons. `cargo xtask checks` checks skeletons and tracked Luau.
`cargo xtask ci` no longer exists. The hosted workflow still calls it and also
requires unavailable sibling dependencies on hosted runners.

Local path dependencies are `../itty`, `../tmcp`, and `../../private/ruau`.
Implementation uses their existing APIs and changes only Canopy. The shared
Ruau `script_api` catalog schema remains unchanged. The workspace includes all
application and adapter consumers named below. Final proof checks these
consumers together. It does not require speculative sibling migrations.

### Stage 1: Layout ownership, structural guarantees, and app isolation

R01, R03, R12, and R17 establish local invariants before new runtime surfaces.
This stage has no prerequisites.

- [x] Add `LayoutOverride` and `MeasureOverflow` in `crates/canopy/src/layout.rs`.
  Implement the field precedence, validation, and compatibility builders specified
  above.
- [x] Update `core/node.rs`, `core/world/tree.rs`, and
  `core/world/layout_driver/mod.rs` for base layouts and persistent overrides.
  Include creation, replacement, refresh, snapshots, and invariant validation.
- [x] Add Context override methods and migrate Todo's modal sizing.
  Update `core/testing/dummyctx.rs` and `core/testing/ttree.rs` to preserve the
  same contract.
- [x] Add signed conversions in `core/view.rs`.
  Replace duplicated conversions in `routing.rs`, Frame, List, and Editor where
  doing so preserves the existing mouse boundary.
- [x] Rename `apply_tree_edit` to `edit_structure` in Context and all workspace
  callers, including `core/children.rs` and tree integration tests.
  State the exact rollback boundary in rustdoc and `docs/architecture.md`.
- [x] Inject Store in `examples/todo/src/{store.rs,lib.rs}`.
  Update all three fixture callbacks, setup helpers, and constructors.
  Remove thread-local lookup and retain explicit handles in
  `examples/todo/tests/basic.rs`.
- [x] Add regression cases to `core/world/layout_driver/tests.rs`,
  `core/world/tests.rs`, and `crates/canopy/tests/it/{layout,viewport,tree}.rs`.
  Cover persistent overrides, invalid merges, bounded descendants, coordinate
  edge cases, and structural versus widget-state rollback.
- [x] Add two-instance database and fixture isolation tests in
  `examples/todo/tests/basic.rs`.
  Retain production Root composition in layout and app tests.
- [x] Validate with `ncode test -p canopy -p canopy-widgets -p todo`.
  Regenerate API skeletons with `cargo xtask api`.
  Run `cargo xtask checks` and review the completed stage diff.

### Stage 2: Command targets, arguments, eligibility, and bindings

R04, R06, R07, R08, and R09 establish an inspectable action contract after
Stage 1.

- [x] Add target policy, exact dispatch, and read-only status hooks in
  `core/commands.rs` and `core/context.rs`.
  Extend `CommandError`, `core/script/errors.rs`, and MCP error conversion for
  wrong owner, disabled action, and missing event requirements.
- [x] Extend `canopy-derive/src/{parse,model,codegen}.rs` for typed `call_*`
  builders and `enabled` attributes.
  Preserve generics, optional user arguments, ignored returns, and injection.
- [x] Add explicit call functions and target-aware discovery in
  `core/script/{dispatch,base_api,records,defs}.rs`.
  Keep legacy decoder behavior under legacy entry points.
- [x] Add stored phases and application command targets in `core/inputmap/mod.rs`.
  Update routing, help snapshots, binding diagnostics, removal, and epoch cleanup.
  Preserve one winner per input under the existing resolver ordering.
- [x] Preserve targeted actions in `canopy-widgets/src/{button,list}.rs`.
  Add Todo delete eligibility and show status in Help's binding list.
- [x] Migrate workspace binding strings and tracked Luau to explicit phases.
  Preserve prior phase at each selector.
  Convert only callbacks that invoke one command to declarative bindings.
- [x] Update bootstrap in `canopy-mcp/src/script.rs` and consumers in
  `canopyctl/src/{main,session}.rs` for explicit default/focus availability.
  Update `docs/scripting.md` and `docs/agent-loop.md` with migration examples.
- [x] Extend `crates/canopy/tests/it/commands.rs`, `core/script/tests.rs`,
  `core/inputmap/tests.rs`, `core/help.rs`, and derive tests.
  Add two-owner target, explicit-map, eligibility transition, and phase cases.
  Add negative compile fixtures for typed native calls and invalid status hooks.
- [x] Run package tests with `ncode test` for `canopy`, `canopy-derive`,
  `canopy-widgets`, `todo`, `canopy-mcp`, and `canopyctl`.
  Run TermGym's F6/navigation tests in `canopy-examples`.
  Regenerate API skeletons and run `cargo xtask checks`.
  Review the completed stage diff before changing runtime scheduling.

### Stage 3: Shared turns, teardown, resource lifetimes, and scheduling tests

R02, R11, R13, R18, R20, and the driver portion of R27 unify progress after
Stage 2.

- [x] Add `ChangeSet` and invalidation in `core/change.rs`, `core/world/mod.rs`,
  Context, facade mutators, and mutable widget access.
  Include failed mutations, style changes, scrolling, focus, and tree changes.
- [x] Add incarnation and attachment generations to nodes.
  Commit generation changes, wake expiration, and poll cancellation after
  successful structural edits in `core/world/tree.rs`.
- [x] Add the bounded post-dispatch removal batch and nested checkpoints.
  Integrate callback depth with `core/widget_access.rs`, command dispatch,
  native context entry, and script segment completion.
- [x] Add runtime work/outcome types and the shared driver in
  `core/canopy/turn.rs`.
  Move preparation and frame construction out of adapter-owned scheduling.
  Separate published frames from backend diff baselines in `rendering.rs`.
- [x] Replace `LuauHost::run_root_async` with detached invocation polling in
  `core/script/mod.rs`.
  Use Ruau's existing invocation API and release borrowed state between polls.
  Preserve script anchors, closure synchronization, print capture, and journal
  completion on success, failure, cancellation, and timeout.
- [x] Replace `wait_until`'s yield loop in `core/script/base_api.rs` with
  publication/deadline wake registration.
  Remove recursive automation servicing from wait helpers.
  Reject concurrent top-level eval and reload with a structured busy result.
- [x] Move deadlines into the shared driver and reuse `PendingHeap` in
  `core/poll.rs`.
  Add poll cancellation and `NodeWakeHandle` delivery with per-owner coalescing.
  Replace one eager poll-worker thread per Canopy with adapter waiting on the
  driver's next deadline and wake receiver.
- [x] Adapt Crossterm `EventSource` and runloop to drive VM wakes, terminal events,
  and timer deadlines fairly.
  Adapt `AutomationHandle` requests and `canopy-mcp/src/{script,server}.rs` so
  active eval does not block ordinary input or native automation progress.
- [x] Route Harness and synchronous eval conveniences through the same driver.
  Add a manually advanced clock and controllable wake source under `testing`.
  Preserve explicit `Harness::render` for isolated rendering tests.
- [x] Add `crates/canopy/tests/it/turn.rs` and register it in the integration target.
  Cover error invalidation, button teardown, stale incarnation, lifetime expiry,
  wait progress, cancellation, busy requests, and fair servicing.
- [x] Add MCP pending-eval tests and terminal adapter tests with the same trace.
  Assert published state and lifecycle order without unconditional rendering or
  timing sleeps.
- [x] Run package tests with `ncode test` for `canopy`, `canopy-widgets`,
  `canopy-mcp`, `canopy-examples`, and `todo`.
  Run `cargo xtask dynamic` for changed widget-access and script-context safety
  boundaries when the configured Miri toolchain is available.
  Record any unavailable dynamic check as a validation limitation.
- [x] Regenerate API skeletons and update runtime/wait documentation.
  Review buffer diff equivalence and script ABI results before accepting the
  completed stage.

Dynamic validation limitation: `cargo xtask dynamic` could not run because
`cargo-miri` is not installed for `nightly-2026-07-01`.

### Stage 4: Scoped composition, keyed collections, and modal interaction

R05, R10, R14, and R15 use the structural and turn boundaries from Stage 3.

- [ ] Add semantic identity metadata and the `(scope, key)` index in Core.
  Extend tree validation, edit snapshots, replacement, reparenting, and removal.
  Add Context and Luau key lookup methods.
- [ ] Add `ChildBuilder` in `core/children.rs` and the `compose` Context adapter.
  Restrict construction to detached nodes and apply overrides before mounting.
- [ ] Add `List<W, K = AutoKey>` and keyed reconciliation in
  `canopy-widgets/src/list.rs`.
  Reject unmanaged children in `KeyedChildren::reconcile`.
  Preserve selection and pending activation by key, including injected row keys.
- [ ] Migrate Todo to `List<TodoEntry, i64>` and scoped composition.
  Assign semantic keys to the Todo list and new-item input.
  Update `examples/todo/tests/basic.rs` typed lookups and smoke selectors.
- [ ] Add the modal scope stack in `core/world/interaction.rs`.
  Integrate binding admission, route boundaries, focus constraints, pointer
  targeting, token-owned effects, and structural retirement.
  Use R11 completion for closes requested during dispatch.
- [ ] Replace Root help and Todo modal state coordination with interaction tokens.
  Preserve Root's pre-open help snapshot and application binding ownership.
  Keep existing commands and Escape/help bindings as application-facing controls.
- [ ] Extend tree and key tests for wrapper changes, scope moves, and rollback.
  Exercise `canopy-widgets/src/wrap.rs` inside a complete structural edit.
  Add List tests for reorder, selected-key removal, duplicate keys, and unmanaged
  children beside a dedicated row container.
- [ ] Extend Root/Todo tests for nested modals, outside pointer input, capture,
  failed open, focus recovery, and owner replacement.
  Verify that scope close preserves unrelated effects.
- [ ] Validate with `ncode test -p canopy -p canopy-widgets -p todo -p canopy-examples`.
  Run `cargo xtask smoke`, regenerate API skeletons, and run `cargo xtask checks`.
  Review the completed stage diff and the production Root layout cases.

### Stage 5: Semantic observations and stock style contracts

R19 and R24 publish stable observations using Stage 4 identity and interaction state.

- [ ] Add snapshot and semantic data types in `core/snapshot.rs` and
  `Widget::semantics` in `widget/mod.rs`.
  Capture immutable node data and cells after successful preparation and paint.
- [ ] Implement precise attachment, display, and clip-intersection flags.
  Add snapshot and flush APIs in `core/canopy/mod.rs` and
  `core/script/{base_api,records,defs}.rs`.
  Keep legacy node/screen query behavior clearly documented.
- [ ] Add Input, Button, and List semantic hooks with configurable value exposure.
  Include labels, selected keys, and command status.
  Exclude sensitive values and unsupported occlusion claims.
- [ ] Add stock role constants and `WidgetState` mappings using the current style
  resolver in `core/style/mod.rs`.
  Apply them in Button and Input and document them in `docs/styles.md`.
- [ ] Update Todo smoke assertions to use one snapshot and semantic keys.
  Add hidden-ancestor, detached, offscreen, old-snapshot, and flush-phase tests.
  Compare terminal output after in-eval observation with full repaint output.
- [ ] Add wrapper-independent role tests and disabled/focused/selected state tests.
  Preserve existing theme goldens unless the new disabled state changes output
  intentionally.
- [ ] Validate with `ncode test -p canopy -p canopy-widgets -p todo`.
  Run `cargo xtask smoke`, regenerate API skeletons, and run `cargo xtask checks`.
  Review the completed stage, including snapshot allocation and value exposure.

### Stage 6: Setup, capability features, trust declarations, and terminal policy

R21, R22, R23, and R25 complete application assembly after Stage 5.

- [ ] Add `CanopyBuilder` in `core/canopy/builder.rs` and export it through the
  crate's intentional public paths.
  Implement consuming phases and disabled/trusted script-root options.
- [ ] Convert Todo setup and `crates/examples/src/lib.rs` shared setup helpers.
  Preserve app/user/project startup order and API-only construction behavior.
  Update demos that register defaults or roots through those helpers.
- [ ] Add `RunOptions` and the pure interrupt decision function in the Crossterm
  adapter.
  Add `runloop_with_options` and pass options through MCP launch configuration.
  Configure TermGym for routed Ctrl+C and explicit emergency exit.
- [ ] Add launch automation policy in `canopy-mcp/src/{launch,server}.rs`.
  Keep disabled launch paths free of listeners.
  Document trusted-local native authority and socket deployment requirements in
  `docs/scripting.md` and `docs/agent-loop.md`.
- [ ] Add widget capability features in `canopy-widgets/Cargo.toml` and gate
  modules/re-exports in `canopy-widgets/src/lib.rs`.
  Make `image`, `fontdue`, `syntect`, `itty-core`, and feature-specific runtime
  dependencies optional where their callers permit it.
- [ ] Extract shared buffer support from `canopy-widgets/src/editor/` into
  `canopy-widgets/src/text_buffer/`.
  Update Input and Editor imports while preserving public editor buffer exports.
  Gate Inspector construction and commands in Root behind `devtools`.
- [ ] Update feature-dependent tests, benchmarks, and demo manifests.
  Make `xtask::run_default_check` verify minimum, full, and each independent
  widget capability without feature unification from example dependencies.
  Keep full-feature `cargo xtask api` generation unchanged.
- [ ] Add builder phase-failure and disabled-root tests.
  Add adapter Ctrl+C delivery, legacy exit, and emergency cleanup tests.
  Add minimum-feature Input/List/Root tests with no Inspector nodes.
- [ ] Validate minimum and each capability using package-scoped Cargo checks.
  Inspect `cargo tree -p canopy-widgets --no-default-features -e normal`.
  Run package tests with `ncode test` for `canopy`, `canopy-widgets`,
  `canopy-mcp`, `canopy-examples`, and `todo`.
- [ ] Regenerate API skeletons and run `cargo xtask checks`.
  Review feature boundaries, setup failure ownership, and the completed stage diff.

### Stage 7: Replay contracts and final integration

R26 and the cross-adapter portion of R27 complete the spec after Stage 6.

- [ ] Add execution/session metadata, app identity, reset policy, and viewport
  request support in `canopy-mcp/src/script.rs`.
  Report metadata from headless and live servers without implying database
  isolation from app construction alone.
- [ ] Add the versioned replay envelope in `canopyctl/src/replay.rs`.
  Extend CLI arguments and replay routing in `canopyctl/src/main.rs`.
  Validate mode, digest, viewport, fixture, and reset contract before execution.
- [ ] Propagate request/response fields through `canopyctl/src/{session,main}.rs`
  and MCP smoke helpers.
  Add live replay selection, explicit legacy parsing, mismatch reports, and
  expected-success comparison.
- [ ] Update Todo factories and `docs/{agent-loop,fixtures,scripting}.md` with
  fresh/live behavior, reset guarantees, and reproducible replay examples.
  Preserve source-only smoke scripts as the normal durable scenario format.
- [ ] Complete the shared scenario suite across native, terminal, direct MCP,
  proxy MCP, and CLI replay boundaries.
  Cover the permanent R27 cases with explicit adapter-policy differences.
- [ ] Update `.github/workflows/ci.yml` to use the current native gate commands.
  Retain its documented sibling-dependency blocker until hosted checkout inputs
  are actually supplied. Do not claim hosted CI success from local validation.
- [ ] Run `ncode tidy` once after implementation settles.
  Review its changes, regenerate API skeletons, and run focused tests for any
  resulting corrections.
- [ ] Run `ncode test`, `ncode tidy --check`, and `cargo xtask smoke` sequentially.
  Confirm every focused filter selected tests.
  Check `api-surface/` and generated Luau declarations for intended public changes.
- [ ] Run `rumdl check` on this spec and the five changed documentation files.
  Review the full uncommitted diff and run `git diff --check`.
  Record completed stage proof and any environment blockers before handoff.
