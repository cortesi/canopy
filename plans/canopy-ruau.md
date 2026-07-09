# Adopt the expanded Ruau APIs in Canopy

This plan begins only after [`ruau.md`](ruau.md) is complete and its Ruau changes have been
reviewed. It changes Canopy against the exact Ruau revision recorded at handoff. It does not add
substantial new Ruau facilities while a Canopy stage is in progress.

The validated Ruau code baseline is `7dc8f1b33`. The final plan-only commit may be newer, but
consumer dependency updates must select this code revision or a reviewed descendant that contains
the same API stack.

If adoption proves that a shared API is incomplete, add the missing work to `ruau.md`, return to
the Ruau checkout, complete and validate that stage, then resume this plan. Canopy-local policy
must not be used to widen a Ruau API without evidence from another surveyed embedder.

## Handoff notes from Ruau implementation

- `PreparedGraphScript` validates both module-source identity and source epoch. Build a fresh
  artifact after mount invalidation; do not try to reuse a checked graph with a replacement source
  object that happens to resolve the same ids.
- `RetainedRuntime` uses generational handles and deliberately does not reload roots implicitly.
  Source-epoch invalidation makes roots and stashes stale; Canopy must choose when to prepare,
  reload, and rebuild its callback groups.
- Complete `CallOptions` context is stack-scoped. Nested calls restore the outer print sink and app
  data, so Canopy should pass invocation state per call and delete persistent VM mutations only
  after reentrant tests prove equivalent behavior.
- `TableLayout` classifies structure only. Empty tables classify separately, and sparse, mixed, or
  unsupported-key details are available for Canopy to turn into its own path-aware errors.
- `NativeModuleBuilder` is deterministic and target-neutral, but dynamic command semantics and
  declaration types remain Canopy inputs. Keep the existing conformance audit after migration.
- `FilesystemMounts` rejects overlapping prefixes, duplicate canonical roots, traversal, and
  ambiguous reverse mappings. Canopy still owns discovery, user/project precedence, startup order,
  and which init file becomes a startup root.

Before editing Canopy, inspect the finalized surfaces with:

- `ruskel ruau-typecheck::DiagnosticRecord`
- `ruskel ruau-typecheck::ModuleDiagnosticRecord`
- `ruskel ruau-surface::PreparedGraphScript`
- `ruskel ruau-vm::ScopedHostFunction`
- `ruskel ruau-vm::TableLayout`
- `ruskel ruau-vm::CallOptions`
- `ruskel ruau::module::NativeModuleBuilder`
- `ruskel ruau-host::RetainedRuntime`
- `ruskel ruau-host::SurfaceSession`
- `ruskel ruau-fs::FilesystemMounts`
- `ruskel ruau-fs::FilesystemMountsBuilder`

## 1. Re-baseline the Canopy adoption boundary

Start from the completed Ruau surface and preserve unrelated Canopy work.

1. [ ] Inspect `git status --short`, `git diff --stat`, and relevant Canopy diffs. Record existing
       changes outside this plan before editing.
2. [ ] Record the Ruau revision, public API skeletons, features, and handoff limitations from the
       completed Ruau plan.
3. [ ] Verify Canopy sees that checkout with
       `cargo check -p canopy --all-targets --all-features`.
4. [ ] Use `ruskel` on the exact Ruau APIs touched by each Canopy stage before modifying call sites.
5. [ ] Preserve Canopy ownership of `NodeId`, command semantics, startup order, journals, callback
       groups, public error presentation, and reentrant `&mut Canopy` state.
6. [ ] Keep each stage a coherent Canopy changeset and update this checklist immediately as work
       completes. Do not commit until the user explicitly asks.

## 2. Adopt diagnostic records and prepared graphs

Replace single-source checking and local diagnostic rendering with the completed Ruau boundaries.

1. [ ] Add `source: Option<String>` to `ScriptCheckDiagnostic`. Preserve severity, line, column,
       and message meanings and the source-less display format.
2. [ ] Convert Ruau `DiagnosticRecord` and `ModuleDiagnosticRecord` through one Canopy adapter.
       Keep the severity policy: `Error` is `"error"`; `Warning` and `Info` are `"warning"`.
3. [ ] Delete Canopy's `Payload::RequiredExport` rendering and convert declaration-conformance
       diagnostics through the same adapter with their implementation path as source.
4. [ ] Replace retained base and startup `Checker` fields with finalized `Surface` values. Keep a
       distinct startup surface for required globals such as `setup`.
5. [ ] Add one source constructor that prefixes strict mode exactly once and uses the real config
       or startup `ModuleId`. Use a stable synthetic id only for public source-only checks.
6. [ ] Prepare named config and startup graphs before execution. Preserve the startup difference
       where checking sees the declaration and runtime execution appends `setup()`.
7. [ ] Test ordinary type errors, missing and mismatched startup globals, missing locations,
       conformance errors, bad dependencies, root overlays, relative imports, stale epochs, and
       matching diagnostic and traceback names.

## 3. Adopt borrowed closures and generated native modules

Remove Canopy adapters after the shared Ruau authoring path proves equivalent.

1. [ ] Replace `CanopyHostFn` and `canopy_host_fn` with closures or function pointers in the base
       API, owner commands, default bindings, and fixtures.
2. [ ] Retain `HostHandler` only if the function-pointer alias still expresses the static base API
       table more clearly than an inline type.
3. [ ] Route native command registration through `NativeModuleBuilder`. Make each dynamic owner
       command provide its name, declaration type, and runtime handler once.
4. [ ] Convert default bindings, base APIs, fixtures, host types, and support chunks without
       weakening their current declarations or runtime behavior.
5. [ ] Delete declaration/rendering and runtime-registration glue only after the generated module
       passes Ruau's declaration/runtime audit.
6. [ ] Compare `ruskel` output before and after adoption. Treat an unintended public declaration
       change as a failure, not a snapshot update.
7. [ ] Test script commands, owner dispatch, default bindings, fixtures, borrowed userdata,
       multiple returns, and structured errors.

## 4. Consolidate Canopy value policy over table layout

Use Ruau for representation mechanics while keeping Canopy's domain decisions explicit.

1. [ ] Add table-driven tests for scoped and marshaled values covering nil, booleans, integers,
       integral and fractional numbers, non-finite numbers, valid and invalid UTF-8 strings, empty
       tables, arrays, maps, sparse and mixed tables, unsupported keys, nested values, userdata,
       buffers, vectors, light userdata, and opaque values.
2. [ ] Use `TableLayout` for live and marshaled classification. Arrays are dense positive integer
       sequences; empty tables are maps; sparse, mixed, and unsupported-key tables are errors.
3. [ ] Share Canopy's numeric policy: finite integral values in `i64` range become
       `ArgValue::Int`, other finite values become `ArgValue::Float`, and non-finite values fail.
4. [ ] Preserve live `NodeHandle` userdata as `ArgValue::Node`, including when nested. Treat a
       marshaled `{ type = "NodeId", token = ... }` value as an external map because it cannot
       reconstruct process-local identity.
5. [ ] Add one Canopy-owned path builder so nested errors use the same syntax for scoped and
       marshaled values, such as `actions[3].target: expected NodeId userdata`.
6. [ ] Consolidate `scoped_to_arg_value`, `table_to_arg_value`,
       `marshaled_to_arg_value`, and `marshaled_table_to_arg_value` around shared layout mechanics.
       Keep `arg_value_to_scoped` separate because it allocates inside a live scope.
7. [ ] Use `Scope::marshal` only in equivalence tests for ordinary values. Do not use it for live
       conversion because it erases process-local `NodeHandle` identity.

## 5. Adopt complete call options and retained runtime

Replace VM-global call mutation and local retained-VM mechanics without moving Canopy orchestration
into Ruau.

1. [ ] Attach a per-invocation print capture and quota to `CallOptions` for synchronous stashed
       calls and async modules. Remove sync-path `Vm::set_print_sink_with_quota` mutation.
2. [ ] Route VM ownership, prepared roots, loaded roots, source epochs, stashed closures, explicit
       release, and execution count through `RetainedRuntime`.
3. [ ] Keep script ids, callback groups, `active_eval`, startup hooks, journals, and reentrant
       `&mut Canopy` state in Canopy.
4. [ ] Map preparation, stale artifact, load, execution, cancellation, timeout, runtime, stale
       handle, and marshal failures into Canopy's existing public error categories.
5. [ ] Define closure cleanup for script unload, replacement, startup failure, and host shutdown.
       Do not rely on process lifetime or VM destruction for routine cleanup.
6. [ ] Test sync and async print capture, quotas, sequential isolation, nested calls, reentrant host
       calls, stale handles, explicit release, unload, epoch invalidation, failure recovery, and
       repeated reload without registry growth.

## 6. Adopt multi-root filesystem sources

Replace generic filesystem mechanics while retaining Canopy namespace and startup policy.

1. [ ] Build `FilesystemMounts` from Canopy's discovered user and project roots with explicit
       `ModuleId` prefixes.
2. [ ] Replace Canopy's generic mount, invalidation, composite epoch, and reverse path lookup with
       Ruau facilities.
3. [ ] Keep root discovery, user/project precedence, namespace policy, startup order, and
       `init.luau` selection in Canopy.
4. [ ] Use one root `Source` identity for graph checking, compilation, loading, and tracebacks.
5. [ ] Preserve explicit-root imports and relative imports within a mount. Reject traversal,
       unknown mounts, ambiguous reverse mappings, and files outside configured roots.
6. [ ] Test user and project roots, relative imports, per-root and global invalidation, reverse
       lookup, startup layering, symlinks, traversal, unknown mounts, and source display names.

## 7. Remove obsolete Canopy integration layers

Complete the adoption by deleting mechanics rather than leaving parallel paths.

1. [ ] Remove superseded checkers, host-function adapters, declaration builders, table
       classifiers, VM stash registries, epoch composition, mount wrappers, and print-sink mutation.
2. [ ] Remove forwarding methods that exist only to preserve the old internal layering. Update
       callers to express intent through the new Ruau or Canopy-owned API directly.
3. [ ] Re-run `ruskel` over Canopy's public and private script API. Keep domain concepts visible
       and Ruau implementation details encapsulated.
4. [ ] Confirm that public Canopy scripting behavior changed only where this plan explicitly says
       so. Document any deliberate improvement in its proper user-facing reference.
5. [ ] Search for duplicate `type_name`, Lua display, table-shape, diagnostic-rendering, and stash
       helpers and delete those now provided by Ruau.

## 8. Validate the Canopy adoption

Run focused gates after every stage and the full Canopy gates before review.

1. [ ] For diagnostics and graphs, run Canopy script typecheck, startup, declaration-conformance,
       and `test_script_framework` tests.
2. [ ] For closures and native modules, run Ruau's declaration audit through Canopy plus
       `test_script_commands`, owner dispatch, default binding, and fixture tests.
3. [ ] For table layout, run command argument, async result, nested userdata, invalid UTF-8,
       sparse table, mixed table, and path-error tests.
4. [ ] For retained execution, run startup/config, stashed closure, async module, print capture,
       quota, reentrant call, reload, unload, stale handle, and failure-recovery tests.
5. [ ] For filesystems, run module-source unit tests and filesystem-backed script framework tests.
6. [ ] After each stage, run `cargo check -p canopy --all-targets --all-features`.
7. [ ] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
       --examples 2>&1`, resolve every warning, and rerun focused tests after each fix batch.
8. [ ] Format with Canopy's repository formatter after the final code change.
9. [ ] Run `cargo xtask tidy`, `cargo xtask test`, and `cargo xtask smoke` from this repository.
10. [ ] Run `git diff --check`, inspect the exact Canopy diff and final `ruskel` surface, and verify
        the Ruau checkout still matches the reviewed handoff revision.
11. [ ] Stop for review before committing. Keep Ruau and Canopy commits separate if the user later
        asks to commit.
