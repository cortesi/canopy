# Ruau API cleanup for Canopy

Canopy depends on the sibling Ruau checkout directly:
`../../../../private/ruau/crates/ruau`, with `default-features = false`. This plan
is for the remaining API cleanup between Canopy and Ruau: use Ruau APIs that now
exist, add only the missing general APIs that Canopy proves it needs, and keep
Canopy's own scripting surface stable unless a later stage deliberately changes
that public contract.

This file was re-baselined against the live Canopy tree and
`/Users/cortesi/git/private/ruau` on 2026-07-09. The old plan was directionally
right, but several proposed Ruau APIs now exist under different names:

- `ruau::surface::Surface::check_graph` and `check_graph_async` return
  `CheckedGraph`.
- `ruau::typecheck::Diagnostics::views` and `GraphDiagnostics::views` expose
  conversion-friendly diagnostic views.
- `ruau::vm::ModuleBuilderExt::scoped_function_fn` and `async_function_fn`
  already cover typed closure registration.
- `ruau::host::SurfaceSession` exists as a retained, compiled-chunk session, but
  it does not yet cover Canopy's borrowed `&mut Canopy` execution path.
- `ruau::source::MountedSource` already has mount-local epochs; Canopy still owns
  root discovery and filesystem-path-to-module-id mapping.

The current Canopy integration still has useful adoption targets:

- Retained `Checker` fields in `LuauHost` still check one source at a time.
- `type_diagnostic_to_script` still matches `Payload::RequiredExport` manually.
- `CanopyHostFn` still wraps `MultiValue -> MultiValue` handlers locally.
- Live scoped values and owned `MarshaledValue`s still have separate
  `ArgValue` conversion traversals.
- `ScriptModuleRoots` and `ScriptModuleSource` still wrap Ruau filesystem
  sources for invalidation and path lookup.

## 1. Re-baseline the two repositories before implementation

Start every implementation pass by proving the local state that shaped the plan.
The Canopy checkout currently has unrelated dirty manifest/source edits, so the
first implementation batch must preserve that boundary.

1. [ ] In Canopy, record the current dirty baseline with `git status --short`
       and review the relevant diffs before editing. Keep unrelated `tmcp`,
       `itty`, and compatibility edits out of any Ruau-plan batch unless the
       user explicitly widens the scope.
2. [ ] In Ruau, inspect the public API skeleton for `ruau-surface`,
       `ruau-typecheck`, `ruau-vm`, `ruau-host`, `ruau-fs`, and `ruau-source`
       with `ruskel` before changing those crates.
3. [ ] Prove Canopy can see the sibling Ruau API with its real dependency
       settings. At minimum, run `cargo check -p canopy --all-targets
       --all-features` before the first adoption patch.
4. [ ] If either repository has pre-existing unrelated dirt, write down the
       intended ownership boundary in the implementation notes before touching
       code.

## 2. Adopt `Surface::check_graph` for script type checking

This is the highest-value cleanup. Canopy still type-checks retained snippets
with `Checker::check_source`, so a root script's `require` graph is not checked
through the same module-source path that runtime compilation and execution use.
Ruau now has the public graph-checking API the old plan asked for, so this stage
is mostly Canopy adoption.

1. [ ] In Canopy, replace the retained `checker` and `startup_checker` fields in
       `LuauState` with retained base and startup `Surface` values, or otherwise
       make `check_script` and `check_startup_script` go through the finalized
       surfaces instead of long-lived `Checker`s.
2. [ ] Keep the startup surface distinct from the base surface. It carries
       `require_startup_global` obligations such as `setup`; collapsing to one
       surface would silently drop the startup contract.
3. [ ] Add one Canopy helper that builds a strict `ruau::source::Source` for a
       checked root, including a stable `ModuleId` and display name. Use
       synthetic ids such as `@canopy/eval` for ad hoc eval/config snippets.
4. [ ] Before converting startup checks, test how `Surface::check_graph` behaves
       when the synthetic root id is also present in the mounted module source
       (`@user/init`, `@project/init`). If the existing root-overlay collision
       policy blocks real startup roots, either use a Canopy-only synthetic id
       for checking or add the smallest Ruau option needed to separate root id
       from requester identity.
5. [ ] Convert `LuauHost::check_script`, `LuauHost::check_startup_script`, and
       debug-build `maybe_typecheck` to `Surface::check_graph`. Convert returned
       `GraphDiagnostics::views()` into `ScriptCheckDiagnostic`s.
6. [ ] Keep `strict_source` wrapping exactly once. The checked source and the
       compiled source must agree on strictness and module identity.
7. [ ] Add a Canopy test where `check_script` catches a type error in a required
       filesystem module before runtime execution.
8. [ ] Add or extend startup tests so a user/project startup file requiring a
       bad dependency reports the dependency diagnostic, while the existing
       missing-`setup` and additional-startup-global tests remain green.
9. [ ] In Ruau, add new graph-checking API only if the startup collision spike
       proves `Surface::check_graph` cannot represent Canopy's real root/requester
       needs. Do not reintroduce the old `SurfaceSpec::check_root_source_bytes`
       or `CheckedSourceGraph` names.

## 3. Replace hand-rendered diagnostics with Ruau views

Ruau now exposes `DiagnosticView` and `ModuleDiagnosticView`, including
payload-aware messages and one-based locations. Canopy should consume those views
instead of matching individual payload variants.

1. [ ] Add a Canopy conversion helper from `DiagnosticView` to
       `ScriptCheckDiagnostic`. Preserve Canopy's public fields and its current
       severity policy: `Error` stays `"error"`, while `Warning` and `Info`
       become `"warning"`.
2. [ ] For graph diagnostics, include the module display name only where Canopy's
       current envelope needs it. Keep `ScriptCheckDiagnostic` unchanged unless a
       separate Canopy API decision adds a source/module field.
3. [ ] Delete the `Payload::RequiredExport` match from
       `crates/canopy/src/core/script/mod.rs` and use Ruau's rendered message.
       Verify that missing and mismatched required globals still produce useful
       text.
4. [ ] Update `validate_script_module_declarations` in
       `crates/canopy/src/core/canopy/mod.rs` to use the same conversion helper,
       so conformance errors do not keep a second diagnostic rendering path.
5. [ ] Add focused tests for required startup globals, declaration conformance
       failures, ordinary type mismatches, and dependency diagnostics produced by
       graph checking.
6. [ ] Add Ruau diagnostic helpers only if Canopy still has to repeat a general
       transformation that belongs in `ruau-typecheck`.

## 4. Delete the local scoped host-function adapter

Ruau already provides `scoped_host_fn` and
`ModuleBuilderExt::scoped_function_fn`. Canopy should first try to delete its
local adapter before adding more Ruau sugar.

1. [ ] Replace `CanopyHostFn` and `canopy_host_fn` call sites with
       `builder.scoped_function_fn` or direct `ruau::vm::scoped_host_fn`, keeping
       the existing `MultiValue -> MultiValue` command-dispatch shape.
2. [ ] If `MultiValue` cannot be used directly through the generic helper,
       capture the exact missing trait bound or impl in Ruau and add only that
       minimal support.
3. [ ] Convert owner command registration in `OwnerCommandsModule::build` first,
       including the `default_bindings` branch.
4. [ ] After the adapter is gone, re-evaluate the declaration/registration split
       between `OwnerCommandsModule::build` and `defs::render_owner_declaration`.
       Add a declaration-coupled Ruau helper only if it can author one generated
       owner command in one place without making lower-level embedders pay for
       the abstraction.
5. [ ] If a coupled helper is justified, put it in the umbrella `ruau` crate or
       another layer that can mention both `decl` and `vm`. Do not put
       declaration-aware code in `ruau-vm`.
6. [ ] Prove the conversion with `test_script_commands`, one owner-command
       dispatch test, default-bindings coverage, and the Ruau API tests for
       scoped functions.

## 5. Share value conversion policy across scoped and marshaled values

This is still genuine Ruau API work. Canopy currently has two conversion
traversals for outbound script values: live `ScopedValue` inside a `Scope`, and
owned `MarshaledValue` after async execution. The two paths disagree in edge
cases, especially mixed array/map tables, userdata, and host-type marshaling.

1. [ ] In Canopy, first write tests that pin the intended `ArgValue` policy for
       live and async results: arrays, maps, empty tables, sparse arrays, mixed
       tables, integral floats, non-finite floats, strings, userdata, and opaque
       values.
2. [ ] Decide Canopy's table policy explicitly. Do not keep the current
       accidental split where live mixed tables are rejected while marshaled
       mixed tables become maps with numeric string keys unless that is the
       documented contract.
3. [ ] In Ruau VM core, add a `ValuePolicy`, `ValueCodec`, or visitor-style
       traversal that is available without the `serde` feature and can walk both
       `ScopedValue<'_>` and `MarshaledValue`.
4. [ ] Make number policy explicit: preserve `Integer`, optionally accept
       integral `Number` as integer, reject non-finite floats, and document that
       Canopy's inbound `ArgValue::UInt` still flattens to Lua `number`.
5. [ ] Add hooks for live userdata, marshaled host-type output, and opaque
       values. Canopy must be able to map live `NodeHandle` userdata and choose a
       documented owned-result representation after marshaling.
6. [ ] Reuse Ruau's existing path-prefix style for conversion errors, for
       example `actions[3].target: expected NodeId userdata`.
7. [ ] Add Ruau tests that run equivalent policies over live and marshaled table
       shapes, plus separate tests for the chosen userdata/host-type asymmetry.
8. [ ] In Canopy, replace `scoped_to_arg_value`, `table_to_arg_value`,
       `marshaled_to_arg_value`, and `marshaled_table_to_arg_value` with one
       Canopy policy over Ruau's shared traversal.
9. [ ] Keep `arg_value_to_scoped` separate unless a later inbound-builder API is
       clearly useful. It allocates Lua strings, tables, and userdata inside a
       live `Scope`, so it is not the same problem as outbound traversal.

## 6. Evaluate and fill retained-session gaps

The old plan asked Ruau to create a retained session. Ruau now has
`ruau::host::SurfaceSession`, but Canopy still needs borrowed host context,
script ids, loaded-script caches, closure stashes, startup orchestration,
journals, and reentrant Canopy state. This stage should adopt only the parts
that actually reduce local plumbing.

1. [ ] Audit `SurfaceSession` against Canopy's `run_module_async` path:
       retained VM ownership, module-source epoch invalidation, load target,
       `CallOptions`, print capture, cancellation, `execution_count`, and error
       categories.
2. [ ] Decide whether `SurfaceSession` needs a borrowed-context execution
       method over `Vm::exec_async_with_context`. If so, design it around
       non-`Send` host context and the existing blocking runtime instead of
       forcing Canopy state into global app data.
3. [ ] If the borrowed-context method lands, adopt it first for
       `run_module_async`, where Canopy already uses owned async execution and
       per-call `CallOptions`.
4. [ ] Do not move Canopy-local orchestration into Ruau in this stage: script
       ids, the loaded-script cache policy, closure registry, `active_eval`,
       `on_start` hooks, journals, startup obligations, and reentrant Canopy
       state remain in `LuauHost`.
5. [ ] Fix the sync stashed-call path's print capture. `run_target` still mutates
       the VM-global sink with `set_print_sink_with_quota`; if `step_with_context`
       ignores `CallOptions::print_sink_with_quota`, add Ruau support there or
       document why the global sink is still required.
6. [ ] Preserve Canopy's current public error shapes while mapping any new
       Ruau session errors to compile, load, exec, cancellation, timeout,
       runtime, and marshal categories.
7. [ ] Prove this stage with startup/config script tests, async module execution
       tests, print-capture tests, and at least one reentrant host-call test.

## 7. Move filesystem mount conveniences toward Ruau

Ruau's `MountedSource`, `MountEpoch`, `FilesystemSource`, and `FilesystemEpoch`
already provide most low-level pieces. Canopy still owns a useful embedder
convenience layer: namespace roots, per-root invalidation, composite epoch, and
filesystem-path-to-module-id lookup.

1. [ ] Decide the Ruau ownership boundary first. A helper that creates
       filesystem-backed mounts belongs with `ruau::fs`; lower-level
       `MountedSource` must remain in `ruau::source`.
2. [ ] Add a filesystem mount helper that owns a `MountedSource` plus per-prefix
       epoch handles. It should mount `ModuleId` prefixes, not plain strings.
3. [ ] Support mounting `prefix -> root path`, invalidating one prefix,
       invalidating all prefixes, and returning the composite epoch.
4. [ ] Add `module_id_for_path(path)` for on-disk `.luau` files under a mounted
       root. Keep Canopy-specific startup discovery and `init.luau` layer order
       in Canopy.
5. [ ] Keep `MountedSource` and `FilesystemSource` public and composable. The new
       helper is only the common filesystem case, not a replacement for custom
       `ModuleSource` implementations.
6. [ ] Add Ruau tests mirroring Canopy's current cases: user/project roots,
       explicit root imports only, relative imports within a mount, unknown
       mount errors, path-to-id mapping, and per-root invalidation changing the
       epoch.
7. [ ] In Canopy, replace most of
       `crates/canopy/src/core/script/modules.rs` with the new helper, leaving
       only root discovery, namespace policy, and startup-root selection locally.

## 8. Validation and rollout

Land each stage as a Ruau patch plus the smallest Canopy adoption patch that
proves the API. Do not commit either repository until the user explicitly asks.

1. [ ] In Ruau, run focused tests for the touched crates after each stage, then
       run `cargo xtask api-check` before considering any public API stable for
       Canopy.
2. [ ] In Canopy, after each adoption patch, run `cargo check --workspace
       --all-targets --all-features`.
3. [ ] Run the repository clippy command and fix all warnings:
       `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty
       --tests --examples 2>&1`.
4. [ ] Run focused Canopy tests for the stage:
       Stage 2 runs `test_script_framework` plus script typecheck tests.
       Stage 3 runs typecheck and declaration-conformance tests.
       Stage 4 runs `test_script_commands` and owner-command dispatch tests.
       Stage 5 runs command marshaling and async-result tests.
       Stage 6 runs startup/config, async module, print-capture, and reentrant
       host-call tests.
       Stage 7 runs module-source unit tests and filesystem-backed script
       framework tests.
5. [ ] Before final review, format Canopy with the repository formatter and run
       `cargo nextest run --all --all-features`. If `nextest` is unavailable,
       run `cargo test --all --all-features`.
6. [ ] Before final review, run the corresponding Ruau full gate:
       `cargo xtask api-check` and `cargo xtask test` if available, otherwise
       `cargo nextest run --all --all-features`.
7. [ ] Keep commits separate by repository and review the exact diff in each
       repository before staging.
