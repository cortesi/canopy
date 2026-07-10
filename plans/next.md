# Canopy core hardening plan

Canopy has a substantial test suite and a coherent retained-tree model, but several core
operations can currently publish partial state or accept invalid state. The next work should
stabilize those contracts before expanding the feature surface. This plan covers `canopy`,
`canopy-geom`, and the core seams used by `canopy-widgets`, `canopy-mcp`, and `canopyctl`.

Each stage is an independently reviewable batch. Keep the tree passing its stage gate, stop for
review before committing, and update this checklist immediately as findings change the work.

This is a clean-break redesign with zero backwards-compatibility constraints. Remove obsolete
APIs outright, update every in-repo consumer in the same stage, and add no deprecated aliases,
migration layers, or compatibility shims.

## Contracts this plan establishes

- A failed tree edit restores all core-owned state; hook-visible side effects follow an explicit
  lifecycle contract.
- Focus and capture never identify a missing or detached node.
- Rust and Luau node handles reject stale identities without panicking or silently succeeding.
- Script API finalization either publishes one ready runtime or leaves a retryable prior state.
- Public geometry is half-open and cannot panic or wrap for representable input values.
- A terminal grapheme is an atomic buffer value; writes, clipping, and cursors cannot split it.
- Registries produce deterministic results and reject a batch without partially applying it.
- Every background worker has explicit ownership, failure reporting, and deterministic tests.
- Terminal-session ownership is safe even when callbacks can replace the registered backend.
- The public API exposes intent-level operations, not the mutable `Core` implementation.

## Risks driving the order

- `Core::set_children` records snapshots without opening a transaction, so a mount error can
  leave the new topology installed.
- `replace_subtree` removes descendants incrementally, while
  `replace_widget_keep_children` skips the old widget's unmount lifecycle.
- rollback only restores selected structural fields and can unmount nodes that never mounted,
  while failing to unmount existing nodes whose mount completed before a later failure.
- `set_focus` and mouse capture accept detached IDs; later focus-path traversal indexes those IDs.
- `finalize_api` marks the script host finalized before later validation and compilation steps.
  A later error can make retries return success while `script_api()` still panics.
- command, input, keyed-child, and startup-hook batches can fail after partial mutation.
- geometry mixes unchecked, saturating, and narrowing arithmetic at signed/unsigned boundaries.
- terminal writes, cursor overlays, and right-edge clipping can orphan wide-cell continuations.
- the poll scheduler has no shutdown path and can leave a parked thread after `Canopy` drops.
- `TerminalSession` holds a raw pointer that backend replacement can invalidate during a run.
- sibling `ruau`, `itty`, and `tmcp` path dependencies prevent a standalone checkout from building.

## 1. Stage One: Make tree edits and lifecycle hooks atomic

The lifecycle contract makes `on_mount` and `pre_remove` fallible and `on_unmount` infallible. The
journal guarantees rollback only for core-owned state; fallible hooks must make external effects
repeatable or compensating. Nested hook edits join the active journal, and structural edits are
rejected while rollback is in progress.

1. [x] Add a reusable structural fault-injection harness.

   Record topology, keyed edges, mount state, focus, capture, helper indexes, and hook order before
   and after an edit. Support failures from `pre_remove`, `on_mount`, and nested context operations
   so every public tree mutation can share the same core-state rollback assertions.

2. [x] Replace the mount-only transaction with an explicit tree-edit journal.

   Journal every core-owned field a structural edit may change, including global focus and capture
   state. Make nested edits join the journal, reject structural edits during rollback, and unwind
   in deterministic reverse order without unmounting a node whose mount did not complete.

3. [x] Route every topology-changing operation through the transaction boundary.

   Cover `attach`, `detach`, `set_children`, `remove_subtree`, `replace_subtree`, and both child-add
   paths. Preflight IDs, cycles, duplicate keys, and borrowed widget slots. Run fallible
   `pre_remove` vetoes in a deterministic phase under the selected lifecycle contract before
   publishing topology or deleting arena entries.

4. [x] Give widget replacement a complete lifecycle contract.

   When replacing an attached widget, run the old widget's removal lifecycle exactly once and
   mount the new widget exactly once. Specify failure behavior for the old hook, the new hook, and
   reentrant edits, and test both replacement variants with and without descendants.

5. [x] Make `KeyedChildren::try_reconcile` plan first and commit once.

   Validate duplicate and stale keys, create and update candidates, then atomically apply removal,
   visibility, order, and helper-map changes in deterministic order. Prune externally removed IDs,
   including retained `RemovePolicy::Hide` keys, and preserve prior state on every error.

6. [x] Pass the structural stage gate.

   Run invariant checks after successful and failed operations, exercise every injected failure
   point, and run the full unit, property, widget, and smoke suites before review.

## 2. Stage Two: Remove dead wrappers and make identity valid by construction

7. [x] Delete zero-consumer capability and focus-generation surfaces.

   Remove the six blanket capability traits, `node_focus_path_changed`, `last_focus_path`,
   `focus_changed`, `current_focus_gen`, and `Core::focus_generation`. Keep the heavily used
   `Context` convenience methods until the later API stage, after primitive semantics settle.

8. [x] Prevent callers from forging `TypedId<T>` values.

   Make its raw constructor crate-private and expose only checked conversions that verify the
   node exists and stores `T`. Audit all public APIs returning typed IDs and add stale-generation,
   wrong-type, and removed-node tests. Apply the same validation to Luau `NodeHandle` arguments so
   a retained script handle returns a structured script error after node removal.

9. [x] Replace unchecked focus assignment with one checked state transition.

   Accept only attached, existing nodes and return a structured result that distinguishes
   unchanged, changed, and rejected requests. Use the same transition for clearing and recovery,
   and replace panicking slotmap indexing in focus-path queries with checked traversal.

10. [x] Apply the same checked transition model to mouse capture.

   Require the requesting widget to be attached, clear capture during detach or removal through
   the central transition, and prevent event routing from observing a stale ID. Cover capture
   changes made inside callbacks and failed structural transactions.

11. [x] Replace silent boolean mutations with explicit outcomes where absence matters.

   In particular, make visibility and related node-targeted mutations distinguish a missing node
   from an unchanged value. Propagate typed errors through `Context` instead of allowing helper
   state to diverge silently.

12. [x] Pass the identity stage gate.

   Add a reference-model state machine that mixes attach, detach, remove, replace, focus, capture,
   visibility, Rust IDs, and script-held `NodeHandle`s. Assert the model and
   `validate_invariants()` agree after every step.

## 3. Stage Three: Make registries and script startup transactional

13. [x] Make command registration an atomic, conflict-aware batch operation.

   Preflight every ID and specification before insertion. Treat an identical full batch as
   idempotent, reject conflicting definitions, and guarantee that an error leaves `CommandSet`
   unchanged. Inject a mid-batch conflict, then retry the identical batch and require success
   without residue or duplicates.

14. [x] Make input-map changes validate before mutation.

   Compile path matchers and validate targets before removing an existing binding. Release both
   replaced and newly compiled Luau function handles on every error path. Use checked allocation
   for `BindingId`, script IDs, and closure IDs, and preserve the old binding on replacement error.

15. [x] Give registries one deterministic ordering contract.

   Stabilize `InputMode::bindings`, command availability, help snapshots, and diagnostic dumps by
   using ordered storage or explicit canonical sorting. Preserve outputs that already sort, and
   test the four unstable results across different insertion orders.

16. [x] Turn script API finalization into a prepare-and-publish state machine.

   Build the module source, definitions, runtime surface, declaration checks, default bindings,
   and startup scripts in temporary state. Stage `LuauHost::finalize`'s pending-script roots and
   the module source too; publish no runtime handle or source before `Ready`. A retry must discard
   only staged handles and preserve the pre-finalization script identities.

17. [x] Remove the pre-finalization panic from `script_api()`.

   Return `Result` or `Option` from the accessor, expose the finalization state where useful, and
   update downstream callers. Test access before setup, after success, after each injected failure,
   and after retry.

Startup is fail-stop. Record success per script so retries skip completed scripts. On failure,
clean up only registrations owned by the failed script; preserve bindings and hooks installed by
earlier startup scripts or unrelated evaluations.

18. [x] Make startup execution and hook cleanup failure-safe.

   Enforce fail-stop execution, prevent successful scripts from being silently rerun, and release
   every drained function handle even when one hook fails. Scope cleanup to the failed script,
   preserve a deterministic error, and test unrelated bindings, callback side effects, queued
   hooks, and retry behavior.

19. [x] Pass the registry and script stage gate.

   Add table-driven fault injection for every finalization step and batch boundary, including a
   failure inside `LuauHost::finalize`'s pending-script loop. Run declaration conformance,
   default-binding, startup, MCP script, and end-to-end smoke tests before review.

## 4. Stage Four: Unify geometry and layout semantics

20. [ ] Specify one half-open geometry and overflow contract.

   Define rectangle edges, empty rectangles, point clamping, intersections, and signed conversion
   in `canopy-geom` documentation. Rename operations whose current inclusive behavior does not
   match that contract instead of preserving ambiguous names. Specify how containment and
   intersection agree for empty rectangles, and make `Rect::line` checked rather than panicking.

21. [ ] Replace unchecked coordinate arithmetic with deliberate operations.

   Use checked or saturating unsigned arithmetic and `i64` intermediates for signed geometry.
   Replace narrowing `as` conversions with `TryFrom` or explicit clamping in
   `RectI32::intersect_rect`, unsigned-to-signed `From` implementations, view origins, and render
   offsets. Repair unchecked `Point` addition and clamping, `Rect::contains_rect`, `Rect::line`,
   and `LineSegment::split_active`.

22. [ ] Enforce layout validity at the mutation boundary.

   Wire the existing `Layout::validate` and `LayoutValidationError` into the mutation boundary, or
   replace them with one checked application path. Reject contradictory bounds, zero flex weights,
   and overflowing padding; remove dead `clamp_weight`, scattered `.max(1)` normalization, and the
   test that currently codifies silent zero-weight repair.

23. [ ] Define and implement alignment for row and column layouts.

   Specify main-axis group alignment and cross-axis child alignment when free space exists, then
   apply `align_horizontal` and `align_vertical` consistently to row, column, and stack layouts.
   Cover fixed, flex, wrap, hidden, padded, gapped, and overflowing children.

24. [ ] Expand geometry and layout properties to boundary-biased inputs.

   Generate values around zero, `i32` limits, and `u32::MAX`, plus invalid layouts and mixed
   signed offsets. Assert no panic, half-open containment laws, containment/intersection agreement,
   conversion guarantees, monotonic placement, and stable results under equivalent construction
   orders.

25. [ ] Pass the geometry and layout stage gate.

   Run all `canopy-geom` and layout properties under debug and release arithmetic, then exercise
   widget snapshots and smoke flows at zero, tiny, ordinary, and maximum supported terminal sizes.

## 5. Stage Five: Make terminal cells and rendering invariant-safe

Use a small `RenderLimits` configuration with conservative per-axis and total-cell defaults for
the materialized `TermBuf`, explicit caller overrides, and no environment variables. These limits
apply only to the visible render target. They do not constrain virtual coordinates, canvases,
images, or other widget-owned off-screen data, which may be much larger than the viewport and use
lazy, tiled, or application-specific storage.

26. [ ] Make terminal-buffer allocation checked and fallible.

   Compute materialized render-target cell counts with checked `usize` arithmetic, apply
   `RenderLimits`, and use fallible reservation before initialization. Ensure indexing uses the
   same calculation and propagate construction errors through root sizing and renderer setup.
   Keep virtual canvas extents separate so off-screen content never drives `TermBuf` allocation.

27. [ ] Make grapheme replacement an atomic buffer operation.

   Before writing, clear the complete grapheme occupying the destination and every cell covered by
   the new grapheme. Reject width-two `char` values in `new`, `fill`, `fill_empty`, and frame fills,
   or make those paths grapheme-aware. Define zero-width behavior without forcing it to one cell,
   and maintain a canonical base-plus-continuation representation after every operation.

28. [ ] Make clipping and cursor overlays preserve canonical graphemes.

   Delete the production-unused `copy` and `copy_to_rect` APIs. Make cursor styling cover a complete
   grapheme without rewriting a continuation into a base, and never install a wide base when its
   continuation is clipped at the right edge. Keep the already grapheme-aware diff algorithm and
   verify it only receives canonical buffers.

29. [ ] Pass the rendering stage gate.

   Replace the narrow ASCII-only diff property with generated grapheme writes, fills, overwrites,
   right-edge clipping, resizes, and cursor moves. Replay every diff against a reference cell model
   and run widget snapshots plus a real-backend smoke test before review.

## 6. Stage Six: Own runtime resources and event-loop boundaries

30. [ ] Replace the parked poll thread with an owned scheduler.

   Use an explicit command channel or condition variable for schedule, reschedule, cancel, and
   shutdown. Join the worker on drop, detect and restart or reject a dead worker, use checked
   deadline arithmetic without poisoning shared state, and inject a clock so tests require no
   sleeps or wall-clock timing.

31. [ ] Make automation requests safe with respect to the UI thread.

   Detect or encode UI-thread ownership so synchronous requests cannot deadlock. Bound the work
   drained per event-loop turn, define backpressure, and make callback-driven state changes request
   a redraw through one explicit contract. Expose a supported servicing path for custom run loops
   instead of leaving `service_automation` crate-private.

32. [ ] Centralize terminal-session ownership.

   Eliminate `TerminalSession`'s raw pointer into replaceable `Core` storage and prevent widget
   contexts from starting or stopping the backend behind the session state. Unify control exit,
   panic cleanup, session stop/drop, and partial-start restoration in one safe RAII boundary,
   including balanced keyboard-enhancement push/pop behavior.

Use crossterm's async `EventStream` and make stream ownership part of the run loop so dropping the
loop cancels the reader. If a focused spike proves it unsuitable, use an OS wakeup primitive. A
permanently blocked detached reader thread is not an acceptable fallback.

33. [ ] Give backend event sources explicit cancellation.

   Implement cancellation so a run loop can stop cleanly without a terminal event. Surface reader
   failure to the main loop instead of logging and leaving `recv()` blocked, and test cancellation
   and reader death without arbitrary sleeps or timeouts.

34. [ ] Pass the runtime stage gate.

   Add deterministic scheduler and automation concurrency tests, repeated construct-run-drop loops,
   and thread-leak assertions. Run terminal restoration smoke tests before review.

## 7. Stage Seven: Reduce the public API to the core model

35. [ ] Reduce `Context` aliases and local/global method triplets.

   After the checked primitives settle, keep one honest `Context` and use an explicit scope value
   or extension helpers for derived behavior. Update every in-repo caller and delete the one-line
   aliases and local/global method triplets in the same change. Add no deprecated wrappers or
   replacement capability traits.

36. [ ] Close the concrete `Core` and low-level surface leaks.

   Replace widget installer `&mut Core` signatures with the smallest intent-level `Canopy` facade,
   remove the `core_mut` redraw side effect and escape hatch, decide privacy rather than relying on
   `#[doc(hidden)]` for low-level modules, and narrow the blanket `ruau` re-export to Canopy-owned
   integration types. Audit the already large `Canopy` inherent surface while adding the facade,
   and retain no transitional low-level entry points.

37. [ ] Replace string-bag and `anyhow`-leaking public errors with structured errors.

   Give each crate a `thiserror` error type with structured variants and sources. Preserve node,
   operation, script, and geometry context without making callers parse display strings.

38. [ ] Review the settled public API with `ruskel` and record an API budget.

   Inspect every workspace crate and the `Canopy` inherent methods, remove accidental exports and
   trivial accessors, and add a reviewable API-surface artifact for intent and complexity review.
   Treat the artifact as a design budget, not as a compatibility baseline.

39. [ ] Pass the API stage gate.

   Build every downstream crate and example against the facade. Repair the corrupted `ChildKey`
   and `key!` rustdoc, convert the four ignored examples into compiling doctests, and verify the
   `ruskel` skeleton expresses the retained-tree, layout, input, rendering, and scripting concepts
   without exposing their storage.

## 8. Stage Eight: Make the validation discipline match the risks

Use publishable version or pinned public-git dependencies in committed manifests. Developers doing
cross-repository work can override them through an untracked Cargo patch configuration. Do not make
the normal CI and source archive depend on specially provisioned sibling checkouts.

40. [ ] Add a non-mutating `cargo xtask ci` gate.

   Resolve external dependencies under the committed dependency policy, then run format checking,
   Clippy with warnings denied, default-feature and all-feature builds, doctests, nextest, smoke
   tests, and benchmark compilation. Keep `cargo xtask tidy` as the local fixing command rather
   than using a mutating command in CI.

41. [ ] Make local and CI test semantics identical.

   Pin the local Rust toolchain and nextest version, run doctests separately, and remove the current
   local-nextest versus CI-`cargo test` fallback difference. Add workspace lints to `examples/todo`,
   repair or remove its nine unlabeled ignored tests, and give the remaining flaky or platform-only
   tests explicit owners and commands.

42. [ ] Add strict Luau checking to the repository gate.

   Validate checked-in scripts and generated declarations with source paths and diagnostics suited
   to editors and CI. Include this check in `cargo xtask ci` so Rust and Luau API changes cannot
   drift independently.

43. [ ] Add model and fault-injection suites for the core state machines.

   Extend the structural, identity, and rendering harnesses from earlier stages to registries and
   scripting, then unify their minimized failure output. Generate failures at every fallible
   boundary rather than building a second set of overlapping model tests.

44. [ ] Add targeted dynamic checks for unsafe and concurrent code.

   Run Miri on the widget-slot guard, terminal-session, and script-reentrancy tests. Use a
   maintained deterministic concurrency checker only for the terminal driver and poller, and deny
   `unsafe_code` in crates that currently contain none. Keep these suites small and diagnostic
   instead of adding broad sanitizer jobs without a demonstrated target.

45. [ ] Add performance baselines for core operations.

   First declare the core Criterion target with `harness = false`; its current auto-discovered
   target runs an empty libtest harness. Add it to validation, then measure tree edits, layout,
   render diff, command resolution, and script startup after correctness semantics settle.

46. [ ] Replace the beta-only CI matrix with intentional toolchain coverage.

   Use the repository-pinned stable Rust as the required cross-platform gate, retain beta or
   nightly as an advisory early-warning job where useful, and make formatting, lint, smoke, docs,
   and benchmark jobs visible instead of relying on `cargo xtask test` alone.

47. [ ] Pass the repository release gate.

   Run the new CI command on macOS and Linux, preserve the Windows build and test job, run smoke
   suites against the packaged binaries, and confirm a clean source archive can build docs,
   examples, tests, and benches without undeclared local state.

## Appendix A: Low-hanging features after the core contracts settle

48. [ ] Add a first-class automation redraw operation.

   Let an automation callback return or request a redraw without reaching into core internals.
   Build this on the Stage Six request and backpressure contract.

49. [ ] Add scoped input-mode guards.

   Provide a small RAII helper for temporary mode pushes so modal widgets cannot leak a mode when
   an event handler returns early. Keep explicit push and pop primitives available underneath.

50. [ ] Add a deterministic core snapshot for bug reports.

   Serialize the public tree shape, focus, capture, layout, active modes, and registered command
   names without widget internals or unstable map order. Reuse it in model-test failure output and
   MCP diagnostics.

Larger features such as a full overlay or interactive inspector should wait until these mutation,
rendering, and API boundaries are stable; they would otherwise cement the current internals.
