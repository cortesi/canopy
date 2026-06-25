# Ruau API improvements for Canopy

Canopy now depends on the sibling Ruau checkout directly:
`../../../../private/ruau/crates/ruau`, with no crate version pin. The migration is
working, but the current integration still exposes several places where Canopy is
repeating logic that belongs in Ruau or using lower-level Ruau primitives directly.

This plan is for Ruau work first, in `/Users/cortesi/git/private/ruau`, followed by
small Canopy adoption patches in this repository. Each stage names the concrete
Canopy pattern that justifies the Ruau API change and the downstream proof that
the new API actually improves Canopy.

## 1. Root-source graph checking on `SurfaceSpec`

Canopy's retained checkers currently call `Checker::check_source` on one source
string in `crates/canopy/src/core/script/mod.rs`. That validates the root source
against the surface builtins and required globals, but it does not use the
surface's `ModuleSource`, so a root script's `require` graph is not checked through
the same path as runtime compilation. Ruau already has the right implementation
internally in `crates/ruau/src/runner/pipeline.rs`: it wraps the source in
`RootOverlaySource`, delegates non-root reads to `SurfaceSpec::module_source()`,
builds a `GraphChecker`, applies the surface analysis mode, and returns flattened
graph diagnostics. That private runner code should become a public surface API.

1. [ ] In Ruau, add `surface::RootSourceOptions` with at least:
       `root_id: ModuleId`, `display_name: String`, optional
       `root_requester: ModuleId`, parse options, syntax flags,
       `max_diagnostics`, and optional cancel flag.
2. [ ] In Ruau, add `surface::CheckedSourceGraph` with:
       `has_errors()` and `diagnostics() -> &[TypeDiagnostic]`. State whether it
       wraps, extends, or intentionally parallels the existing `CheckedModule`
       returned by `SurfaceSpec::check_source_bytes`; do not create overlapping
       result types without a migration story.
3. [ ] Move or share the runner helpers currently named like
       `checked_frontend_for_root`, `check_root_source_async`, and
       `source_front_door_check_from_frontend` behind public
       `SurfaceSpec::check_root_source_bytes` and
       `SurfaceSpec::check_root_source_bytes_async`.
4. [ ] Keep runner-only accounting internal unless another caller needs it. The
       existing runner count is `checker.arena().type_len() +
       checker.arena().pack_len()`; if it becomes public, expose it as optional
       advanced accounting, not as part of Canopy's required API.
5. [ ] Preserve the existing single-source fast path for sourceless binary input:
       when there is no module source and the root bytes are not UTF-8, call
       `check_source_bytes_with_config` and return the same result shape.
6. [ ] Add Ruau tests under `crates/ruau/tests/api.rs` proving that a root source
       requiring a dependency reports dependency diagnostics, honors
       `root_requester`, rejects synthetic-root id collisions when requested, and
       inherits required globals and analysis mode from the surface.
7. [ ] Update Ruau API snapshots with `cargo xtask api-check`, then run the
       relevant Ruau tests: `cargo test -p ruau --test api`,
       `cargo test -p ruau-source`, and the runner tests that used the private
       helper.
8. [ ] In Canopy, retain both the base `SurfaceSpec` and the startup
       `SurfaceSpec` clone created in `LuauHost::finalize`. The startup surface
       carries the additional `require_global` obligations; using only the base
       surface would drop the `setup` contract.
9. [ ] In Canopy, replace `LuauHost::check_script`,
       `LuauHost::check_startup_script`, and debug-build `maybe_typecheck` with
       the sync root-graph surface check. Preserve `strict_source` wrapping, use
       synthetic ids such as `@canopy/eval` and `@canopy/startup`, and use the
       startup surface for startup roots.
10. [ ] Add a Canopy test in `crates/canopy/tests/test_script_framework.rs` where
       a checked script `require`s a filesystem module containing a type error.
       Assert that `ScriptCheckResult` reports the dependency diagnostic before
       runtime execution. Keep the existing startup setup obligation test green.

## 2. Diagnostic rendering for embedders

Canopy manually converts Ruau type diagnostics in
`type_diagnostic_to_script`. It knows about `Payload::RequiredExport`, severity
names, zero-to-one-based line conversion, and fallback message rendering. That is
small but fragile: every new structured diagnostic Ruau adds requires embedders to
know another payload variant. Ruau should expose an embedder-oriented diagnostic
view that keeps the rich `TypeDiagnostic` available but centralizes the ordinary
display contract.

1. [ ] In `ruau-typecheck`, inventory the existing rendering helpers before
       adding anything new: `TypeDiagnostic::user_message`,
       `diagnostic_snapshot`, `render_diagnostic_snapshot`,
       `render_diagnostic_summary`, and `wire_json`.
2. [ ] Add the smallest missing embedder report helper, either by extending the
       existing helpers or by adding a type such as `DiagnosticReport` with
       `severity`, one-based `line` and `column`, `message`, optional `category`,
       and an optional structured payload view for known diagnostics.
3. [ ] Format `RequiredExport` as Canopy currently does: missing globals name the
       global and required type, mismatches include the actual type when Ruau has
       it. Preserve or document Canopy's current severity mapping, where `Info`
       becomes `"warning"`.
4. [ ] Keep source filenames out of the core report. The caller should attach
       chunk labels or module display names because Canopy reports diagnostics in
       API-specific envelopes.
5. [ ] Add Ruau tests for required-export rendering, severity mapping, one-based
       positions, and fallback rendering for an ordinary type mismatch.
6. [ ] In Canopy, delete the custom payload match from
       `crates/canopy/src/core/script/mod.rs` and build `ScriptCheckDiagnostic`
       from the Ruau report. Keep Canopy's public struct unchanged unless the
       separate Canopy API decision is to expose `category`; otherwise drop
       `category` at the Canopy boundary.
7. [ ] Update `validate_script_module_declarations` in
       `crates/canopy/src/core/canopy/mod.rs` to use the same report helper so
       conformance errors do not keep a second hand-rolled diagnostic path.

## 3. Host function registration ergonomics

Ruau already has `ruau::vm::scoped_host_fn`, so Canopy no longer needs a new typed
wrapper from scratch. Canopy's dominant native shape is still
`MultiValue -> MultiValue`: `canopy_host_fn` is a local adapter around
`ScopedHostFunction`, and owner command declarations are rendered separately from
owner command registration. The high-value API work is declaration/registration
coupling for generated modules and deletion of local adapter code where existing
Ruau helpers already fit.

1. [ ] In Canopy, first try to delete `CanopyHostFn` and `canopy_host_fn` by
       calling the existing `ruau::vm::scoped_host_fn` directly for the
       `MultiValue -> MultiValue` handlers. If the existing generic bounds do not
       accept that shape, record the exact missing trait impl or helper in Ruau.
2. [ ] In Ruau `crates/ruau-vm/src/host_ext.rs`, add a builder helper such as
       `scoped_function_fn(name, binding, f)` only if item 1 proves the current
       `scoped_host_fn` plus `scoped_function` call is still materially noisy.
       Treat this as minor sugar, not the central API win.
3. [ ] Defer an analogous `typed_async_function` until a real callsite appears.
       Canopy already registers async functions with `async_host_fn` factories.
4. [ ] Add a small declaration-coupled registration helper in the umbrella
       `ruau` crate, not `ruau-vm`, so it can mention both `decl` and `vm`.
       Focus the first design on generated owner modules, where Canopy currently
       registers scoped functions in `OwnerCommandsModule::build` and renders
       matching declarations through `defs.rs`.
5. [ ] Keep the coupled helper optional. Lower-level embedders must still be able
       to build declarations and native modules independently.
6. [ ] Add Ruau API tests that register one `MultiValue -> MultiValue` scoped
       function, one strongly typed scoped function returning a scalar, and one
       function reading `Scope::context_mut`.
7. [ ] In Canopy, convert one owner-command registration path to the
       declaration-coupled helper. If the helper is expressive enough, also
       collapse one entry from `CANOPY_FUNCTIONS` so declaration and registration
       are authored in one place.

## 4. Value conversion policy over scoped and marshaled values

Canopy owns an `ArgValue` model for commands and automation. It currently has two
parallel Ruau conversion paths in `script/mod.rs`: scoped values to `ArgValue`
inside a live `Scope`, and owned `MarshaledValue` to `ArgValue` after async
execution. Those paths duplicate table shape rules, number handling, strings,
userdata handling, and unsupported-value errors. Ruau's serde bridge is useful,
but it is intentionally JSON-shaped and does not know Canopy's `NodeId` token
policy. Ruau should expose a reusable conversion traversal/policy API rather than
make embedders hand-walk both value representations.

1. [ ] In Ruau VM core, add a `ValueCodec` or `ValuePolicy` abstraction that is
       available without the `serde` feature. It should share table, number,
       string, and unsupported-value policy between `ScopedValue<'s>` and
       `MarshaledValue`, while acknowledging that the two value families do not
       carry identical information.
2. [ ] Make table policy explicit: strict arrays require integer keys `1..n`,
       maps require string keys, mixed tables are rejected unless the policy
       opts into a defined conversion, and empty tables have an explicit
       array/object decision.
3. [ ] Make number policy explicit: preserve `Integer`, accept integral
       `Number` as integer only when the policy asks for it, document that
       Canopy's inbound `ArgValue::UInt` currently flattens to Lua `number`, and
       reject non-finite floats.
4. [ ] Address the marshal-boundary identity problem explicitly. A live
       `ScopedValue::Userdata(NodeHandle)` can become `ArgValue::Node`, but an
       async `MarshaledValue` currently sees only the host-type marshal output,
       such as Canopy's `{ type = "NodeId", token = ... }` table. Either add a
       Ruau hook that preserves a host-supplied opaque identity across owned
       marshaling, or document the asymmetry and make token-table decoding the
       Canopy-owned contract.
5. [ ] Add policy hooks for live userdata, marshaled host-type output, and opaque
       values. Canopy should be able to say how `NodeHandle` is represented in
       both the scoped and owned result paths without forking the whole table
       traversal.
6. [ ] Reuse the path-prefix style from Ruau's serde bridge for errors, for
       example `actions[3].target: expected NodeId userdata`.
7. [ ] Add Ruau tests that run equivalent table/number/string policies over a
       live `ScopedValue` table and a `MarshaledValue` table, and add separate
       tests for the known userdata asymmetry or new identity-preserving hook.
8. [ ] In Canopy, replace outbound `scoped_to_arg_value`, `table_to_arg_value`,
       `marshaled_to_arg_value`, and `marshaled_table_to_arg_value` with one
       Canopy policy over Ruau's shared traversal. Keep `arg_value_to_scoped` as
       a separate inbound builder because it needs a live `Scope` to allocate Lua
       strings, tables, and userdata.
9. [ ] Add a Canopy async-result test where a module returns a node handle. The
       test must pin the chosen contract: either the owned path reconstructs
       `ArgValue::Node`, or it intentionally returns the documented token table.

## 5. Retained session host for non-JSON embedders

Ruau's current `host::Evaluator` is a good retained source-eval helper for JSON
arguments and JSON results, but Canopy's live path has a retained VM, a borrowed
`&mut Canopy` context, command dispatch, NodeId userdata, custom print capture,
startup script obligations, loaded script caches, and async result marshaling to
`ArgValue`. Canopy therefore still owns a lot of session plumbing in
`LuauHost`. Ruau should grow a lower-level retained session abstraction that is
not tied to JSON and does not force a fresh VM per eval.

1. [ ] In Ruau, add `host::Session` that owns a `SurfaceSpec`, a
       retained `Vm`, compile options, default `Limits`, and an optional module
       cache.
2. [ ] Provide explicit operations for Canopy's current lifecycle:
       `compile`, `load_named`, `LoadedModule` execution through
       `exec_async_with_context`, and borrowed-scope execution through
       `step_with_context`. Mirror the existing Ruau vocabulary instead of
       inventing names that obscure the VM operations being wrapped.
3. [ ] Accept per-call `CallOptions` by value or reference consistently with the
       current VM API. The session should not mutate the VM's global print sink
       for ordinary per-call capture; it should use `CallOptions` print sinks.
       Name the Canopy asymmetry being removed: `run_target` mutates the global
       sink, while `run_module_async` already uses `CallOptions`.
4. [ ] Return `Vec<MarshaledValue>` or a generic caller-selected result codec,
       not JSON. Keep the existing JSON `Evaluator` as a convenience wrapper over
       the session if that makes the layering simpler.
5. [ ] Expose a unified error enum that preserves compile, load, exec,
       cancellation, timeout, runtime, and marshal categories without hiding the
       original Ruau error types.
6. [ ] Add Ruau examples or tests showing a retained session with a borrowed
       context, an async host wait, print capture, and non-JSON userdata result
       marshaling.
7. [ ] Keep Canopy-local orchestration explicitly out of the first Ruau session
       slice: script ids, the loaded-script cache policy, closure registry,
       `active_eval`, `on_start` hooks, journals, startup obligations, and
       reentrant Canopy state remain in `LuauHost` unless a later stage proves
       they belong in Ruau.
8. [ ] In Canopy, adopt the session first for `run_module_async`, which already
       uses owned async execution and per-call `CallOptions`. Then migrate the
       sync stashed-call path enough to remove `vm.set_print_sink_with_quota` in
       favor of per-call print sinks. Preserve Canopy's public API and error
       shapes.

## 6. Mount invalidation and path lookup conveniences

Ruau's `MountedSource`, `FilesystemSource`, and `FilesystemEpoch` already removed
most of Canopy's custom module-source implementation. Canopy still keeps thin
wrappers in `ScriptModuleRoots` and `ScriptModuleSource` for namespace-specific
invalidation and path-to-module-id lookup. Those are common embedder needs for
editor/watch integrations and should live beside the source combinator.

1. [ ] In Ruau, decide the ownership boundary first. Either add
       `ruau::fs::FilesystemMounts` in the `ruau` crate, where
       `FilesystemSource` and `FilesystemEpoch` already live, or first move those
       filesystem types down into `ruau-source`.
2. [ ] Add the mounted-filesystem helper at that resolved boundary. It should own
       a `MountedSource` plus per-prefix `FilesystemEpoch` handles.
3. [ ] Support mounting `prefix -> root path`, invalidating one prefix,
       invalidating all prefixes, and returning the current composite epoch.
4. [ ] Provide `module_id_for_path(path)` that returns the mounted `ModuleId`
       for an on-disk `.luau` file when the path belongs to a configured root.
       Keep startup discovery and `init.luau` root selection in Canopy; path
       lookup should stay a general root-relative mapping.
5. [ ] Keep the lower-level `MountedSource` API public. The new helper is for
       the common filesystem case, not a replacement for arbitrary custom
       `ModuleSource` implementations.
6. [ ] Add tests in the crate that hosts the helper, mirroring Canopy's current
       cases: user/project roots, explicit root imports only, relative imports
       within a mount, unknown mount errors, path-to-id mapping, and per-root
       invalidation changing the epoch.
7. [ ] In Canopy, replace most of
       `crates/canopy/src/core/script/modules.rs` with the new helper, leaving
       only Canopy namespace discovery and startup-root selection locally.

## 7. Validation and rollout

Each Ruau stage should be landed with a small Canopy adoption patch before moving
on, so the API is shaped by real use rather than hypothetical convenience.

1. [ ] In Ruau, run the focused crate tests for the touched crates after each
       stage, then run `cargo xtask api-check` before considering the Ruau API
       stable for Canopy.
2. [ ] Prove every new Ruau API used by Canopy is reachable with Canopy's actual
       dependency settings: `ruau = { default-features = false }`. At minimum,
       `cargo check -p canopy --all-targets --all-features` must see the new API.
3. [ ] In Canopy, after each adoption patch, run `cargo check --workspace
       --all-targets --all-features`, `cargo clippy -q --fix --all --all-targets
       --all-features --allow-dirty --tests --examples 2>&1`, and the focused
       tests named in the stage.
4. [ ] Use this per-stage Canopy proof matrix:
       Stage 1 runs `test_script_framework`, the existing script typecheck unit
       tests, and the new require-graph diagnostic test. Stage 2 runs script
       typecheck and conformance-diagnostic tests. Stage 3 runs
       `test_script_commands` and one owner-command dispatch test. Stage 4 runs
       command marshaling tests and the async node-result test. Stage 5 runs
       startup/config script tests and async module execution tests. Stage 6 runs
       module-source unit tests and filesystem-backed script framework tests.
5. [ ] Before final review, format Canopy with the repository formatter and run
       `cargo nextest run --all --all-features`. If `nextest` is unavailable,
       run `cargo test --all --all-features`.
6. [ ] Before final review, run the corresponding Ruau full gate:
       `cargo xtask api-check` and `cargo xtask test` if available, otherwise
       `cargo nextest run --all --all-features`.
7. [ ] Keep commits separate by repository. Do not commit either repository until
       explicitly asked.
