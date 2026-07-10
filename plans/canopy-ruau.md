# Adopt the expanded Ruau APIs in Canopy

This plan began after the original Ruau plan was completed and removed. New adopter feedback is
tracked in [`ruau-next.md`](ruau-next.md) and is implemented inline when Canopy proves that a shared
surface is incomplete.

The validated Ruau code baseline is `7dc8f1b33`. The final plan-only commit may be newer, but
consumer dependency updates must select this code revision or a reviewed descendant that contains
the same API stack.

Canopy adoption uses Ruau checkout revision `218d57a3e`. Its only change from the validated code
baseline is `plans/next.md`; the implementation and public API stack are identical to
`7dc8f1b33`.

Adoption then proved the shared gaps recorded in [`ruau-next.md`](ruau-next.md). Those completed,
uncommitted Ruau changes now form the live dependency surface validated by this plan.

If adoption proves that a shared API is incomplete, add the missing work to `ruau-next.md`,
complete and validate it in the Ruau checkout, then resume Canopy adoption. Canopy-local policy
must not widen Ruau without concrete adopter evidence.

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

1. [x] Inspect `git status --short`, `git diff --stat`, and relevant Canopy diffs. Record existing
       changes outside this plan before editing.
2. [x] Record the Ruau revision, public API skeletons, features, and handoff limitations from the
       completed Ruau plan.
3. [x] Verify Canopy sees that checkout with
       `cargo check -p canopy --all-targets --all-features`.
4. [x] Use `ruskel` on the exact Ruau APIs touched by each Canopy stage before modifying call sites.
5. [x] Preserve Canopy ownership of `NodeId`, command semantics, startup order, journals, callback
       groups, public error presentation, and reentrant `&mut Canopy` state.
6. [x] Keep each stage a coherent Canopy changeset and update this checklist immediately as work
       completes. Do not commit until the user explicitly asks.

## 2. Adopt diagnostic records and prepared graphs

Replace single-source checking and local diagnostic rendering with the completed Ruau boundaries.

1. [x] Add `source: Option<String>` to `ScriptCheckDiagnostic`. Preserve severity, line, column,
       and message meanings and the source-less display format.
2. [x] Convert Ruau `DiagnosticRecord` and `ModuleDiagnosticRecord` through one Canopy adapter.
       Keep the severity policy: `Error` is `"error"`; `Warning` and `Info` are `"warning"`.
3. [x] Delete Canopy's `Payload::RequiredExport` rendering and convert declaration-conformance
       diagnostics through the same adapter with their implementation path as source.
4. [x] Replace retained base and startup `Checker` fields with finalized `Surface` values. Keep a
       distinct startup surface for required globals such as `setup`.
5. [x] Add one source constructor that prefixes strict mode exactly once and uses the real config
       or startup `ModuleId`. Use a stable synthetic id only for public source-only checks.
6. [x] Prepare named config and startup graphs before execution. Preserve the startup difference
       where checking sees the declaration and runtime execution appends `setup()`.
7. [x] Test ordinary type errors, missing and mismatched startup globals, missing locations,
       conformance errors, bad dependencies, root overlays, relative imports, stale epochs, and
       matching diagnostic and traceback names.

## 3. Adopt borrowed closures and generated native modules

Remove Canopy adapters after the shared Ruau authoring path proves equivalent.

1. [x] Replace `CanopyHostFn` and `canopy_host_fn` with closures or function pointers in the base
       API, owner commands, default bindings, and fixtures.
2. [x] Retain `HostHandler` only if the function-pointer alias still expresses the static base API
       table more clearly than an inline type.
3. [x] Route native command registration through `NativeModuleBuilder`. Make each dynamic owner
       command provide its name, declaration type, and runtime handler once.
4. [x] Convert default bindings, base APIs, fixtures, host types, and support chunks without
       weakening their current declarations or runtime behavior.
5. [x] Delete declaration/rendering and runtime-registration glue only after the generated module
       passes Ruau's declaration/runtime audit.
6. [x] Compare `ruskel` output before and after adoption. Treat an unintended public declaration
       change as a failure, not a snapshot update.
7. [x] Test script commands, owner dispatch, default bindings, fixtures, borrowed userdata,
       multiple returns, and structured errors.

## 4. Consolidate Canopy value policy over table layout

Use Ruau for representation mechanics while keeping Canopy's domain decisions explicit.

1. [x] Add table-driven tests for scoped and marshaled values covering nil, booleans, integers,
       integral and fractional numbers, non-finite numbers, valid and invalid UTF-8 strings, empty
       tables, arrays, maps, sparse and mixed tables, unsupported keys, nested values, userdata,
       buffers, vectors, light userdata, and opaque values.
2. [x] Use `TableLayout` for live and marshaled classification. Arrays are dense positive integer
       sequences; empty tables are maps; sparse, mixed, and unsupported-key tables are errors.
3. [x] Share Canopy's numeric policy: finite integral values in `i64` range become
       `ArgValue::Int`, other finite values become `ArgValue::Float`, and non-finite values fail.
4. [x] Preserve live `NodeHandle` userdata as `ArgValue::Node`, including when nested. Treat a
       marshaled `{ type = "NodeId", token = ... }` value as an external map because it cannot
       reconstruct process-local identity.
5. [x] Add one Canopy-owned path builder so nested errors use the same syntax for scoped and
       marshaled values, such as `actions[3].target: expected NodeId userdata`.
6. [x] Consolidate `scoped_to_arg_value`, `table_to_arg_value`,
       `marshaled_to_arg_value`, and `marshaled_table_to_arg_value` around shared layout mechanics.
       Keep `arg_value_to_scoped` separate because it allocates inside a live scope.
7. [x] Use `Scope::marshal` only in equivalence tests for ordinary values. Do not use it for live
       conversion because it erases process-local `NodeHandle` identity.

## 5. Adopt complete call options and retained runtime

Replace VM-global call mutation and local retained-VM mechanics without moving Canopy orchestration
into Ruau.

1. [x] Attach a per-invocation print capture and quota to `CallOptions` for synchronous stashed
       calls and async modules. Remove sync-path `Vm::set_print_sink_with_quota` mutation.
2. [x] Route VM ownership, prepared roots, loaded roots, source epochs, stashed closures, explicit
       release, and execution count through `RetainedRuntime`.
3. [x] Keep script ids, callback groups, `active_eval`, startup hooks, journals, and reentrant
       `&mut Canopy` state in Canopy.
4. [x] Map preparation, stale artifact, load, execution, cancellation, timeout, runtime, stale
       handle, and marshal failures into Canopy's existing public error categories.
5. [x] Define closure cleanup for script unload, replacement, startup failure, and host shutdown.
       Do not rely on process lifetime or VM destruction for routine cleanup.
6. [x] Test sync and async print capture, quotas, sequential isolation, nested calls, reentrant host
       calls, stale handles, explicit release, unload, epoch invalidation, failure recovery, and
       repeated reload without registry growth.

## 6. Adopt multi-root filesystem sources

Replace generic filesystem mechanics while retaining Canopy namespace and startup policy.

1. [x] Build `FilesystemMounts` from Canopy's discovered user and project roots with explicit
       `ModuleId` prefixes.
2. [x] Replace Canopy's generic mount, invalidation, composite epoch, and reverse path lookup with
       Ruau facilities.
3. [x] Keep root discovery, user/project precedence, namespace policy, startup order, and
       `init.luau` selection in Canopy.
4. [x] Use one root `Source` identity for graph checking, compilation, loading, and tracebacks.
5. [x] Preserve explicit-root imports and relative imports within a mount. Reject traversal,
       unknown mounts, ambiguous reverse mappings, and files outside configured roots.
6. [x] Test user and project roots, relative imports, per-root and global invalidation, reverse
       lookup, startup layering, symlinks, traversal, unknown mounts, and source display names.

## 7. Remove obsolete Canopy integration layers

Complete the adoption by deleting mechanics rather than leaving parallel paths.

1. [x] Remove superseded checkers, host-function adapters, declaration builders, table
       classifiers, VM stash registries, epoch composition, mount wrappers, and print-sink mutation.
2. [x] Remove forwarding methods that exist only to preserve the old internal layering. Update
       callers to express intent through the new Ruau or Canopy-owned API directly.
3. [x] Re-run `ruskel` over Canopy's public and private script API. Keep domain concepts visible
       and Ruau implementation details encapsulated.
4. [x] Confirm that public Canopy scripting behavior changed only where this plan explicitly says
       so. Document any deliberate improvement in its proper user-facing reference.
5. [x] Search for duplicate `type_name`, Lua display, table-shape, diagnostic-rendering, and stash
       helpers and delete those now provided by Ruau.

## 8. Validate the Canopy adoption

Run focused gates after every stage and the full Canopy gates before review.

1. [x] For diagnostics and graphs, run Canopy script typecheck, startup, declaration-conformance,
       and `test_script_framework` tests.
2. [x] For closures and native modules, run Ruau's declaration audit through Canopy plus
       `test_script_commands`, owner dispatch, default binding, and fixture tests.
3. [x] For table layout, run command argument, async result, nested userdata, invalid UTF-8,
       sparse table, mixed table, and path-error tests.
4. [x] For retained execution, run startup/config, stashed closure, async module, print capture,
       quota, reentrant call, reload, unload, stale handle, and failure-recovery tests.
5. [x] For filesystems, run module-source unit tests and filesystem-backed script framework tests.
6. [x] After each stage, run `cargo check -p canopy --all-targets --all-features`.
7. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
       --examples 2>&1`, resolve every warning, and rerun focused tests after each fix batch.
8. [x] Format with Canopy's repository formatter after the final code change.
9. [x] Run `cargo xtask tidy`, `cargo xtask test`, and `cargo xtask smoke` from this repository.
10. [x] Run `git diff --check`, inspect the exact Canopy diff and final `ruskel` surface, and verify
        the Ruau checkout contains only the completed `ruau-next.md` work atop the handoff.
11. [x] Stop for review before committing. Keep Ruau and Canopy commits separate if the user later
        asks to commit.
