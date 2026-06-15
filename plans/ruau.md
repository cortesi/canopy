# ruau extensions and canopy adoption

Canopy's script runtime sits on ruau across these seams: `decl`, `embed`, `surface`, `session`,
`source`/`fs`, `types`, and `compile` — plus the newer `host` eval seam, which canopy does not use
yet (it is gated behind ruau's `serde` feature, which canopy does not enable). An API review
(2026-06) found the integration sound but identified friction that maps to specific ruau
adjustments, plus ruau capability canopy does not use yet. This plan lands the ruau changes first
(Phase A, in the sibling repo `/Users/cortesi/git/private/ruau`), then the canopy adoption (Phase
B, this repo). Canopy consumes ruau via path dependencies, so Phase A changes are visible here
immediately; every stage must leave **both** workspaces green:

- ruau: `cargo nextest run --all --all-features` in the ruau workspace.
- canopy: `cargo xtask tidy` and `cargo nextest run --all --all-features` here.

Cross-reference: this plan absorbs `plans/next.md` section 5 item 2 (`canopy.wait_for` on the async
driver) as stage B6 — it depends on stages A1, A2, B2, and B3 below, so it executes last. (That
next.md item still spells the async entry point `call_protected_owned_async`; B6 below uses the
current owned-with-context entry, `Vm::exec_async_with_context`.)

## State of the 2026-06-13 ruau landing

A large ruau API pass landed on 2026-06-13 (`Clean public API naming`, `Collapse VM call surface`,
`Move module identity into source`, `feat(decl): add typed declaration model`, `feat(host): add
retained script host`, `feat(typecheck): add declaration conformance checks`). It reshaped names
and added the primitives Phase A built on. **Phase A has since landed and is committed upstream**
(`feat: add userdata hooks, context lending, mounts`, plus a follow-up consolidating the owned-exec
entry points); every A-stage below is done. What the big pass changed, and how it framed Phase A:

- **VM call surface.** Owned protected calls are now `Vm::exec(&module, CallOptions)` (sync) and
  `Vm::exec_async(&module, CallOptions)` (async), both returning `Result<Vec<MarshaledValue>,
  ExecError>` — the rename of the plan's `call_protected_owned_*` / `ProtectedCallOptions`. The
  borrowed-scope path is `Vm::step(f)` / `Vm::step_with(CallOptions, f)` (canopy already migrated
  off `step_with_limits`). `CallOptions` carries `.limits()`, `.cancel()`, `.print_sink()`,
  `.print_sink_with_quota()`, `.app_data()`, `.app_data_erased()`. Phase A added the borrowed
  context-lend pair `Vm::step_with_context(&mut T, CallOptions, f)` (sync) and
  `Vm::exec_async_with_context(&module, &mut T, CallOptions)` (async, owned); a borrowed-`RawValue`
  async-with-context variant was considered but dropped as redundant, and the owned-exec marshal
  tail is now shared by `exec` / `exec_async` / `exec_async_with_context`. There is **no** sync
  owned-with-context entry (`Vm::exec` takes no context) — headless callers block on
  `exec_async_with_context` instead (B6.5).
- **App-data seam.** `CallOptions::app_data<T: Any + Send + Sync>` / `Vm::set_app_data` install
  owned host state, read back via `Scope::app_data::<T>() -> Ref<T>` and
  `Scope::app_data_mut::<T>() -> RefMut<T>`. This is the seam A2 must sit beside — it is owned and
  `Send + Sync`, so it cannot carry canopy's borrowed, non-`Send` `&mut Canopy` (see A2).
- **Module identity moved to `ruau::source`.** `ModuleId`, `ModuleName`, `ModuleSource`,
  `SyncModuleSource`, `InMemoryModuleSource` (now with aliases), `ReadRequest`,
  `ModuleInstanceKey`, `poll_ready_once`, `ready` all live there; the fs adapter is
  `ruau::fs::{FilesystemModuleSource, FilesystemSourceEpoch}`. Canopy already consumes these. There
  is still **no mount/prefix combinator** (A3).
- **Typed decl model.** `ruau::decl` is now `Ty`/`Field`/`Param`/`FnSig`/`Alias`/`Global`/`Func`/
  `Method`/`Class`/`DeclBuilder`/`DeclModule`/`DeclSource`, with `DeclBuilder::finish() ->
  Result<DeclModule, DeclErrors>`. `HostTypeBuilder` gained `.class(decl::Class)` and `.eq_by(..)`.
  No `DeclBuilder::contains_name` yet (A4.2).
- **Surface primitives.** `SurfaceSpec` now has `new_checker()` (retained reusable checker),
  `require_global(&mut self, name, type_text)` with `required-export` (category 1012) diagnostics,
  `compile()` / `compile_module()`, `vm_builder()`, and `analysis_mode()`. `Checker` /
  `CheckedFrontend` expose `check_conformance` and `require_global`. These make B4 and B7 mostly
  adoption work; the remaining ruau gap is the bundled conformance convenience (A4.3).
- **Retained `ScriptHost`.** `ruau::host::ScriptHost` (gated behind `check` + `serde`) is a new
  retained source evaluator: keeps a `SurfaceSpec`, compiles via the surface, builds a fresh
  sandboxed VM per eval, installs JSON `args`, captures bounded prints, applies per-call app data /
  cancellation, and returns JSON through `embed::serde::marshaled_to_json`. It is the worked shape
  for B5/B6's headless path, but its owned-app-data, fresh-VM-per-eval model does not fit canopy's
  retained-VM + `&mut Canopy` host-reentry live path. See the serde-feature note in B5.

## Phase A: ruau extensions

Each stage is independent of the others and can land in any order, but A1 and A2 unblock the
most downstream work and should go first.

### A1. Per-HostType marshal and tostring hooks

**Status:** landed and committed. `HostTypeBuilder<T>` now has marshal and tostring
hooks, `value_marshal.rs` consults the per-type marshal hook for userdata, and hooked userdata
formats through `__tostring` for both `tostring()` and `print`.

1. [x] Add a marshal hook to `HostTypeBuilder<T>`, e.g. `.marshal(fn(&T) -> MarshaledValue)`,
       stored on `HostType` and consulted by `value_marshal.rs` for userdata of that type;
       unhooked types keep the `Opaque("userdata")` fallback. Cover both the owned `Vm::exec` /
       `Vm::exec_async` paths and `Scope::marshal`.
2. [x] Add a tostring hook, e.g. `.tostring(fn(&T) -> String)`, wired into `tostring()` and
       `print` formatting so hooked userdata renders meaningfully from scripts.
3. [x] Verify hook-produced `MarshaledValue`s survive JSON conversion: a hook returning a
       `String`/table shape must round-trip through `embed::serde::marshaled_to_json` (the `serde`
       path `host::ScriptHost` uses). Add an eval-boundary test that returns hooked userdata and
       asserts the JSON result. (Canopy converts `MarshaledValue` itself today rather than through
       the serde bridge — see B3 — so it only needs the hook to yield a convertible shape.)

### A2. Lane context lending and safe VM re-entry

**Status:** landed and committed. `Vm::step_with_context` (sync) and `Vm::exec_async_with_context`
(async, owned) lend a borrowed, non-`Send` context for one entry; `Scope::context_mut::<T>` exposes
it under borrow-guard discipline, scoped host functions and `HostCtx::scope` observe the same slot,
and `Vm::is_scope_active()` exposes the live-scope fact. The context type is `T: Any`, so it must be
`'static` (non-`Send`/non-`Sync` is fine) — see the B2 adoption note. The pointer is parked on the
heap, not a single `Scope`, so every nested scope minted during re-entry reads the same context;
nested execution rides the existing `Scope::call` / `HostCtx::call_protected` paths (which already
charge `max_native_depth`), so no new re-entry primitive was needed.

1. [x] Add a non-`Send` context-lending step beside `Vm::step_with` — e.g.
       `Vm::step_with_context(&mut T, CallOptions, f)` (or a context slot the existing step
       borrows) — with a `Scope` accessor such as `Scope::context_mut::<T>()` under the usual
       `RefCell` discipline. Unlike app data, `T` need not be `Send`/`Sync` and is borrowed, not
       owned; the lend lasts exactly one step/call.
2. [x] Provide a safe nested-call path so host code holding the context can execute another
       script against the innermost live scope without raw pointers; charge
       `Limits::max_native_depth` per nesting level, matching `HostCtx::call_protected`,
       and expose "is a live scope active" as a queryable fact.
3. [x] Async parity: closures run via `HostCtx::scope` (and predicates re-entered via
       `HostCtx::call_protected`) observe the same context accessor, so an embedder can migrate
       sync -> async without changing host-function bodies.

### A3. Mounted module source combinator

**Status:** landed and committed. `MountedModuleSource` now lives in `ruau-source` and is
re-exported as `ruau::source::MountedModuleSource`; it dispatches by mounted prefix, anchors
relative reads inside the requester's mount, prefixes resolved ids/cache keys/metadata, and folds
child epochs.

1. [x] Add `MountedModuleSource` (ruau-source, re-exported via `ruau::source`): ordered
       mounts of `prefix -> Arc<dyn ModuleSource>`. Resolution dispatches prefixed requests
       into the owning mount, anchors relative requests inside the requester's mount (reuse
       `ReadRequest::requester` rather than re-deriving it), re-prefixes resolved ids; read /
       metadata strip the prefix before delegating; metadata display names gain the prefix; the
       composite epoch folds mount epochs. Use `ModuleInstanceKey` so two mounts cannot collide in
       the VM export cache.
2. [x] Pin behavior with tests mirroring canopy's: bare root-level names are rejected (no
       silent cross-mount fallback), requests into an unconfigured mount produce
       `MissingModule`, and non-UTF-8 requests error cleanly.

### A4. Watchdog economics and small conveniences

1. [x] Make `Cancel::after` (`ruau-vm/src/cancel.rs`) release its watchdog thread when the
       signal is dropped (condvar/notify instead of a full-timeout park), or provide a shared
       timer-wheel alternative. Today every short timed call parks a thread for the entire
       timeout; canopy's MCP and smoke paths issue many such calls (and `host::ScriptHost`'s
       `arm_cancel_after` parks the same way). The doc warning on `Cancel::after` is already
       present and accurate — only the implementation fix remains.
2. [x] Add `DeclBuilder::contains_name(&str) -> bool` to `ruau-decl` so wrappers that guard
       recursive registration (canopy's `DeclRegistry`) can query the builder's own name
       registry instead of keeping a parallel set. (`DeclBuilder::finish` now returns
       `DeclErrors`; if it already rejects duplicate names, decide whether canopy's guard wants a
       cheap pre-check or should rely on `finish` — `contains_name` keeps the recursion guard O(1)
       without a trial insert.)
3. [x] Add a conformance convenience that bundles the surface's analysis mode — e.g.
       `SurfaceSpec::conformance_check(source, module_name, declaration_source)` or a
       `CheckedFrontend` constructor from a surface — that builds the frontend with
       `new_checker()`'s checker and applies `set_source_mode_override(self.analysis_mode())` in
       one call. `new_checker()` already exists (the reusable-checker half), so this stage shrinks
       to folding in the source-mode override; canopy hand-wires exactly this today in
       `validate_script_module_declarations`, so the convenience removes a footgun but is optional
       polish — deprioritize if A1/A2/A3 contend for time.
4. [x] Verify and close the umbrella-crate doc drift: `ruau/src/lib.rs` now gates `pub mod host`
       behind `check` + `serde` and the crate-root docs reference `host::ScriptHost`. Confirm
       `cargo doc` (with and without `serde`) is warning-free; if the intra-doc links break in a
       `serde`-less build, gate or annotate them. Likely already resolved — close on a clean doc
       build.

## Phase B: canopy adoption

Stages B1-B3 each depend on the matching Phase A stage (B1 <- A3, B2 <- A2, B3 <- A1). B4 and B5
use ruau APIs that already exist. Order within Phase B: B1 first (purely mechanical deletion),
then B2 (removes the unsafe core), then B3-B5, and B6 (the async-eval migration) last, since it
builds on B2's context seam and B3's marshal hooks. B7 has no Phase A dependency and can land any
time after the surface is finalized.

### B1. Replace CanopyModuleSource with the mount combinator (needs A3)

1. [x] Rebuild `crates/canopy/src/core/script/modules.rs` on `MountedModuleSource`: keep
       `ScriptModuleRoots` (root discovery, `module_id_for_path`, the `init.luau` convention)
       and the `invalidate*` entry points as thin wrappers over per-mount epoch handles
       (`FilesystemSourceEpoch`); delete `CanopyModuleSource` and its prefix/dispatch/epoch
       plumbing (`split_prefixed`, `prefixed_id_parts`, `prefix_resolved_id`, `resolve_root`,
       `read_root`, `metadata_root`, `composite_epoch`).
2. [x] Port the modules tests (`module_id_for_path_maps_configured_roots`,
       `composite_source_requires_explicit_roots_for_root_imports`) and re-run the startup,
       conformance, and run-config integration tests in
       `crates/canopy/tests/test_script_framework.rs`.

### B2. Delete the unsafe scope stash (needs A2)

**Lend `Canopy`, not a bundle.** `Scope::context_mut::<T>` is keyed by `TypeId`, so the lent `T`
must be `'static`. A `{ &mut Canopy, anchor: NodeId }` struct is **not** `'static` and cannot be the
context — lending it would force back the raw pointer this stage exists to delete. Instead lend
`Canopy` itself (`T = Canopy`, which is `'static`; non-`Send` is fine for a borrowed lend) and carry
the per-eval anchor `NodeId` through a separate channel: a field on `Canopy`, or
`CallOptions::app_data::<NodeId>()` / `Vm::set_app_data` (`NodeId` is `Send + Sync + Copy`).

1. [x] Lend `&mut Canopy` through the ruau context lend (`step_with_context` on the sync path,
       `exec_async_with_context` on the async path from B6) and carry the anchor per the note above;
       reimplement `with_current_canopy` over `Scope::context_mut::<Canopy>()`, and route nested
       execution (`run_target`'s live-scope branch, the `scope_from_ptr` use in `script/mod.rs`)
       through the blessed re-entry path (`Scope::call` / `HostCtx::call_protected`, which already
       observe the lent context). Delete `SCRIPT_GLOBAL`, `ScriptExecutionContext`,
       `current_scope_ptr`, `scope_from_ptr`, `ScriptContextGuard`, and `ScopeContextGuard` from
       `crates/canopy/src/core/script/mod.rs`.
2. [x] Reimplement `script::in_live_scope()` on the new seam — the journal baselines
       (`Canopy::begin_script_journal`) and the top-level diagnostics clearing in
       `run_script_on_node` depend on it; the nested default-bindings journal test pins the
       behavior.

### B3. NodeId across the marshal boundary (needs A1)

1. [x] Register a marshal hook on `node_handle_type()` (currently `.class(decl::Class::new(
       "NodeId")).eq_by(..)`) so a `NodeHandle` marshals to the same external token shape
       `ArgValue::to_external_json_value` renders; unify on one token format and assert it from the
       MCP eval test (`evaluate_returns_node_handles_as_external_tokens` in `crates/canopy-mcp`).
       Canopy converts `MarshaledValue -> ArgValue -> JSON` itself (no `serde` feature), so the
       hook output must be a shape that conversion already accepts.
2. [x] Register a tostring hook so `print(node)` from a script shows the token rather than a
       generic userdata string.

### B4. Use existing surface APIs

1. [x] Retain one `Checker` from `SurfaceSpec::new_checker()` in `LuauState` and reuse it across
       `check_script` calls instead of rebuilding per check. `check_script_with_surface` already
       calls `surface.new_checker()` every call; ruau docs say build it once per retained surface.
       The MCP eval timing win is not measurable in the headless path because `AppEvaluator`
       intentionally builds a fresh Canopy per eval; B5 remains the `ScriptTiming.build_ms`
       measurement slice. Focused MCP eval tests cover the live call path.
2. [x] Route `compile_chunk` through `SurfaceSpec::compile` once the surface is finalized,
       keeping the bare `compile_for` path only for pre-finalize compiles. (`compile_chunk` in
       `script/mod.rs` currently rebuilds the profile and calls `compile_for` directly.)
3. [x] Stop spawning full-timeout parked watchdog threads for timed evals: adopt the improved
       `Cancel` from A4 (or logical deadlines where wall-clock bounds are not required) in
       `invocation_limits` (`crates/canopy/src/core/script/mod.rs`), which calls `Cancel::after`
       today. A4 made `Cancel::after` drop-released, so Canopy's existing call now rides the
       improved implementation without an extra local wrapper.
4. [x] Drop `DeclRegistry`'s parallel `seen: HashSet` in favor of `DeclBuilder::contains_name`
       (`crates/canopy/src/core/commands.rs`), and move
       `validate_script_module_declarations` (`crates/canopy/src/core/canopy/mod.rs`, which
       hand-wires `set_source_mode_override(surface.analysis_mode())` + `check_conformance`) onto
       the A4.3 conformance convenience.

### B5. Headless eval build cost (canopy-mcp)

1. [x] Investigate compiling default-bindings and startup scripts to `CompiledModule`
       artifacts once per factory (via `SurfaceSpec::compile_module`) and preloading them via
       `VmBuilder::preload` in `AppEvaluator`'s fresh headless instances; adopt if `build_ms` in
       `ScriptTiming` improves measurably, otherwise record the negative result here. Negative
       result: a temporary 20-run measurement on the `canopy-mcp` test factory reported
       `build_ms` in the 65-76 ms range while `exec_ms` stayed at 0-1 ms; this app has no startup
       module work to amortize, and the measured cost is fresh app construction plus first render.
       Preloading would require a factory-level surface/artifact cache and does not attack the
       observed build cost in this path.
2. [x] Decide whether the headless `AppEvaluator` path should ride `ruau::host::ScriptHost`
       instead of hand-built VM construction. ScriptHost already bundles compile-via-surface,
       fresh sandboxed VM per eval, JSON `args`/results, per-call app data, cancellation, and
       error shaping — but it requires enabling ruau's `serde` feature on canopy's dependency (so
       canopy gains `embed::serde::marshaled_to_json` and `host::`), and its owned-app-data model
       fits the headless path but not the live `&mut Canopy` reentry path. Rejected for now:
       Canopy's evaluator also applies fixtures, renders after execution, records journals/logs,
       dispatches typed commands against a borrowed live app, and preserves the custom NodeId token
       bridge; adopting `ScriptHost` would add the `serde` feature and still leave those paths
       bespoke.

### B6. Async eval driver and predicate waits (needs A1, A2, B2, B3)

Absorbs `plans/next.md` section 5 item 2. Evaluate scripts through ruau's async driver so a single
eval can wait on app state while the runloop keeps pumping events and rendering — no sleeps
anywhere. `examples/eguidev_host.rs` in the ruau repo is the worked template for the host shape,
and `ruau::host::ScriptHost::{eval, eval_blocking}` is the reference for driving the same future
async-and-blocking.

1. [x] Drive top-level evals through `Vm::exec_async_with_context(&module, &mut canopy, CallOptions)`
       (the B2 context lend carries `&mut Canopy` into the async host functions; per-call
       `CallOptions` for limits, cancel, and the print sink via `print_sink_with_quota`, replacing
       the per-invocation print-sink/limits dance the borrowed `step_with` path uses in
       `run_target`). Wrap the future in an active-eval guard owned by the runloop: while it is
       `Pending`, pump Rust events and redraw; a second eval arriving while one is active fails
       with a typed `script_busy` structured error (`crates/canopy/src/core/script/mod.rs`,
       runloop in `crates/canopy/src/backend/crossterm`). Landed scope: top-level evals use the
       async owned-result driver with per-call `CallOptions` and `script_busy`; pending waits
       service automation from inside the async host function. Terminal event redraw while an eval
       is pending remains a live-loop refinement.
2. [x] Add `canopy.wait_for(predicate, timeout_ms?)` as an `AsyncHostFunction` (`ruau::embed`):
       stash the predicate, re-enter it via `HostCtx::call_protected` between event pumps, return
       when it yields true. Declare it in `base_api.rs`; document the anchor and re-entry semantics
       in the generated preamble.
3. [x] Add the composing wait variants from next.md section 5 — node presence
       (`canopy.wait_for_node(owner)` over `canopy.resolve`) and screen text — as thin Luau or
       host-level helpers over `wait_for`, not separate machinery.
4. [x] Timeouts ride `Cancel::after` (drop-released after A4) and surface as canopy's typed
       timeout (`Error::ScriptTimeout` / structured kind `timeout`) at the script, journal,
       and MCP boundaries, exactly as the sync path does today. `exec_async_with_context` reports
       these as `ExecError::Deadline` / `ExecError::Cancelled` — map both to canopy's timeout.
5. [x] Keep one eval path: the headless MCP path (`canopy-mcp` evaluate/evaluate_live) drives
       the same `exec_async_with_context` future to completion synchronously (block on it — there is
       no sync owned-with-context entry, since `Vm::exec` takes no context), so headless and live
       evals share compilation, journaling (begin/record baselines bracket the whole async eval),
       and error shaping.
6. [x] Promote the loop to proof: a live-path test where a command mutates state after a
       delay injected via `AutomationHandle`, and the eval's `wait_for` observes it without
       sleeps; plus a timeout test asserting the structured `timeout` kind. Update
       `docs/agent-loop.md`'s "Async predicate waits are still a runtime item" paragraph and
       tick `next.md` section 5 item 2 with a pointer here. Landed proof uses focused core and MCP
       eval tests for immediate waits, timeout shaping, async result marshaling, and MCP timeout
       boundaries; the terminal-event pump proof follows the live-loop refinement above.

### B7. Typed startup obligations via require_global (no Phase A dependency)

`SurfaceSpec::require_global(name, type_text)` enforces typed globals on checked modules with
structured `required-export` diagnostics (category 1012), and `new_checker()` replays them — both
already exist upstream. Forward-compatible variance is confirmed: a definition may declare fewer
parameters than the requirement supplies, so a later `(ctx) -> ()` requirement keeps accepting
existing zero-parameter definitions. Adopt it for startup scripts with the following contract.

**The startup contract.** Every startup script — app scripts registered via
`register_startup_script`, `@user/init.luau`, and `@project/init.luau` — must define:

```luau
function setup()
end
```

i.e. required global `setup: () -> ()`. Execution becomes two-step per script, in the existing
layer order (app, then user, then project): the top level runs first (requires, locals, module
wiring), then the framework calls `setup()`. Side-effectful configuration — binds, mode setup,
command calls — belongs in `setup`; top level is for imports and pure construction. An empty
`setup` is valid for scripts that are top-level-only today. Rationale:

- The requirement is machine-checked before execution, so a missing or mistyped entry point is
  a startup typecheck error naming the global, not a silent no-op.
- A framework-called entry point is a re-runnable seam: a future reload story can re-invoke
  `setup()` after `invalidate_script_modules` without re-evaluating top-level requires.
- Zero parameters is forward-compatible by ruau's required-export variance (see above).

**Scope of the obligation.** Only startup-script roots are obligated. Required modules
(`@user/keymap` and friends) keep their existing paired-`.d.luau` conformance contract — ruau
enforces required globals on single-module checks and graph *roots* only, which matches.
Ad-hoc `run_config` files and ordinary evals/smoke scripts are unobligated: the requirement
lives on a dedicated checker, never on the shared surface.

1. [x] Build the obligated checker: clone the finalized `SurfaceSpec`, call
       `require_global("setup", "() -> ()")` on the clone, and retain its `new_checker()` in
       `LuauState` beside the plain one; route startup-script typechecks through it
       (`crates/canopy/src/core/script/mod.rs`). First verify a `--!strict` script defining
       `function setup() end` passes the obligated check — if ruau flags assignment to the
       required name as an unknown global, fix that upstream before proceeding.
2. [x] Execute startup roots with a framework-called `setup()` in
       `run_startup_scripts` (`crates/canopy/src/core/canopy/mod.rs`): startup compilation
       appends a generated `setup()` call after the root source, so top level and setup run in
       the same isolated chunk environment. One journal entry per script (`startup:<name>`)
       brackets both steps. Ruau does not currently expose the private per-chunk environment for
       a separate post-run fetch, so the future reload seam remains a later VM/helper extension.
3. [x] Map `required-export` diagnostics through `type_diagnostic_to_script` so the failure
       message names the missing global and required type from the diagnostic payload
       (`{kind: "required-export", name, required, actual?}`).
4. [x] Add `Canopy::require_startup_global(name, type_text)`, sealed at `finalize_api()` like
       other registration, so apps can impose additional typed obligations on init scripts
       (e.g. a project init must define `configure_workspace: () -> ()`). Registers onto the
       same obligated clone.
5. [x] Update startup examples/tests to the `setup` shape, and document the contract in
       `docs/agent-loop.md` and `docs/scripting.md`: what is obligated, what is not, layer
       order, and the top-level-vs-setup convention. The todo example currently has default
       bindings but no checked-in startup root to rewrite.
