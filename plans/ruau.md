# Ruau embedder API expansion

This is the first of two execution plans. It changes only
`/Users/cortesi/git/private/ruau`. The follow-up Canopy adoption is in
[`canopy-ruau.md`](canopy-ruau.md) and does not begin until this plan is complete.

Canopy, Verber, Itty, Hotki, Gonsh, Eguidev, Eguito, Porter, Ruau Sandbox, and the
private Workers surface were surveyed on 2026-07-10. Their repeated integration mechanics justify
significant Ruau work before another Canopy patch:

- The seven principal embedder families contain at least 91 hand-written
  `ScopedHostFunction` implementations and 30 repeated `NativeModule::declaration` and
  `NativeModule::build` pairs.
- Canopy and Verber repeat graph-checking, source-identity, and diagnostic adaptation work.
- Canopy, Hotki, and Gonsh each retain a raw VM, loaded chunks, and stashed closures. Verber wraps
  `SurfaceSession` and repeats part of the same execution pipeline.
- Canopy, Verber, Itty, and Gonsh independently classify Lua table layouts.
- Canopy, Gonsh, and Hotki compensate for partial per-call context behavior.
- Canopy already composes filesystem mounts, epochs, and reverse path lookup that would also make
  Itty and Hotki more capable embedders.

The execution boundary is strict:

- Embedders are read-only evidence while this plan is active. Do not modify Canopy or another
  consumer to make a Ruau stage pass.
- Ruau owns source identity, graph preparation, diagnostic records, call context, table-layout
  inspection, retained-VM mechanics, and declaration-coupled module authoring.
- Embedders retain domain values, host userdata meaning, startup order, journals, callback
  grouping, command semantics, application errors, and presentation.
- Lower-level APIs remain available. New APIs compose with `Surface`, `Vm`, `ModuleSource`, and
  `NativeModule` rather than replacing them.
- Target-neutral work stays out of `ruau-host`. Only native retained-runtime conveniences belong
  there; diagnostic, module-authoring, value, and VM APIs must continue to build for Workers.
- Public APIs remain provisional until the Canopy follow-up proves them. If adoption exposes a
  missing Ruau change, add it to this plan and complete it in Ruau before continuing Canopy work.

## 1. Fix the Ruau scope and consumer contracts

Capture the current public surface and turn the survey into explicit API requirements before
implementation.

1. [ ] Inspect `git status --short`, `git diff --stat`, and relevant diffs in Ruau before every
       batch. Preserve all unrelated changes and record the batch's owned files.
2. [ ] Treat Canopy, Verber, Itty, Hotki, Gonsh, Eguidev, Eguito, Porter, Ruau Sandbox, and Workers
       as read-only during this plan. Recheck their Ruau dependency revisions and enabled features
       only when an API assumption needs verification.
3. [ ] Use `ruskel` to capture the current public surfaces for `Surface`, `PreparedScript`,
       `SurfaceSession`, `DiagnosticView`, `ModuleDiagnosticView`, `ScopedHostFunction`,
       `NativeModule`, `ModuleBuilder`, `CallOptions`, `Vm`, `Table`, and `MarshaledValue`.
4. [ ] Add a Ruau design note mapping every proposed API to at least two surveyed consumers. For
       each mapping, record the repeated mechanism, policy that remains local, target constraints,
       and eventual adopter test.
5. [ ] Record API-layer ownership before coding: records in `ruau-typecheck`, graph artifacts in
       `ruau-surface`, table and call behavior in `ruau-vm`, filesystem mounts in `ruau-fs`,
       target-neutral module authoring in `ruau`, and retained runtime in `ruau-host`.
6. [ ] Add compile-fail or API tests for important negative boundaries, including declaration
       types in `ruau-vm`, Tokio in target-neutral crates, and host policy in shared value APIs.
7. [ ] Keep each stage a coherent Ruau changeset with its implementation, tests, documentation,
       API inspection, and checklist update. Do not commit until the user explicitly asks.

## 2. Add owned, presentation-neutral diagnostic records

`DiagnosticView` and `ModuleDiagnosticView` expose the right information but borrow checker-owned
storage. Canopy, Verber, and Ruau Sandbox copy that information into local records.

1. [ ] Add owned `DiagnosticRecord` and `ModuleDiagnosticRecord` types in `ruau-typecheck`.
       Preserve severity, category, code, one-based primary and secondary locations, rendered
       message, typed payload, module id, and display name where each field exists.
2. [ ] Add `DiagnosticView::to_record`, `ModuleDiagnosticView::to_record`,
       `Diagnostics::records`, and `GraphDiagnostics::records`. Do not expose checker internals or
       require consumers to match payloads merely to render correct messages.
3. [ ] Decide whether `records` returns an iterator or collection by following the existing views
       API and avoiding a forced allocation when callers only stream diagnostics.
4. [ ] Ensure records own all text, paths, ids, and payload data needed after checker or graph
       storage is dropped. Do not hide lossy conversion behind `to_record`.
5. [ ] Add field-for-field tests for syntax errors, type errors, warnings, required exports,
       dependency errors, secondary locations, and diagnostics without source locations.
6. [ ] Add an example that converts source and graph diagnostics into a serializable
       application-owned envelope without matching Ruau diagnostic payload variants.
7. [ ] Inspect the final API with `ruskel`; keep borrowed views for zero-copy callers and avoid
       duplicate public types that differ only in naming.

## 3. Make checked module graphs reusable execution artifacts

`Surface::check_graph` already understands root overlays and requester-relative resolution.
`Evaluator::eval_checked` privately repeats graph check, compile, load, and run. Promote the
prepared graph, not `GraphChecker`, as the high-level boundary.

1. [ ] Add synchronous and asynchronous `Surface::prepare_graph` entry points that accept a real
       root `Source` and return a `PreparedGraphScript`. Keep `Surface::prepare` for source-only
       callers.
2. [ ] Make `PreparedGraphScript` own the root source, checked graph, compiled root chunk, required
       capabilities, and source-epoch evidence needed to identify stale preparation.
3. [ ] Expose read-only source, diagnostics, graph, chunk, capability, and epoch inspection without
       exposing mutable checker state or forcing callers back to `GraphChecker`.
4. [ ] Give the artifact load and run operations parallel to `PreparedScript`. Preserve the root
       `ModuleId` and display name through diagnostics, bytecode chunk naming, loading, and runtime
       tracebacks.
5. [ ] Define stale behavior explicitly. A prepared graph must be rejected or re-prepared when its
       source epoch changes; it must never run against dependencies different from those checked.
6. [ ] Refactor `Evaluator::eval_checked` to use `prepare_graph`. Preserve arguments, print
       capture, limits, cancellation, app data, JSON result, and timing behavior.
7. [ ] Share implementation between sync and async entry points without nesting runtimes or adding
       blocking sleeps and timeouts.
8. [ ] Test root overlays that also exist in the source, requester-relative sibling imports,
       dependency diagnostics, source-only preparation, stale epochs, capability failures, and
       matching diagnostic and traceback identities.
9. [ ] Add a graph-prepared example modeled on Canopy's named startup root and Verber's
       requester-aware catalog source, using only Ruau-owned fixtures.

## 4. Make borrowed scoped closures first-class

The typed closure helpers correctly cover owned argument and return shapes. Multiple embedders
still need lifetime-dependent `MultiValue<'s>` handlers and implement trivial adapter structs.

1. [ ] In `ruau-vm`, implement `ScopedHostFunction` for closures satisfying
       `for<'s> Fn(&Scope<'s>, MultiValue<'s>) -> Result<MultiValue<'s>, RuntimeError> + Send +
       Sync`.
2. [ ] Keep `scoped_host_fn` and `scoped_function_fn` restricted to owned shapes. Document when a
       caller should use the typed helper, the borrowed blanket implementation, or a custom trait
       implementation.
3. [ ] Verify coherence with current blanket implementations and function-pointer use before
       changing the trait. Do not introduce a second closure trait with the same purpose.
4. [ ] Add tests for borrowed tables and userdata, multiple returns, captures, function pointers,
       structured errors, reentrant calls, and coexistence with the typed owned helper.
5. [ ] Add a native-module example containing both an owned typed function and a borrowed
       `MultiValue` function without hand-written adapter structs.
6. [ ] Inspect the resulting trait and helper surface with `ruskel`, and remove any newly redundant
       Ruau-internal adapter without removing the explicit trait escape hatch.

## 5. Couple native-module declarations to runtime registration

Surveyed modules describe declarations separately from runtime bindings. Ruau already audits the
two products after the fact; a target-neutral authoring layer can prevent drift at construction.

1. [ ] Add `ruau::module::NativeModuleBuilder` in the umbrella crate, where `ruau-decl` and
       `ruau-vm` already meet. One registration carries a binding's declaration and runtime
       implementation and builds an ordinary `Arc<dyn NativeModule>`.
2. [ ] Support globals, library fields, constants, hidden runtime-only bindings, scoped functions,
       typed scoped functions, async functions, leaf functions, host types, support chunks, and
       `ModuleExport` without reducing the current low-level builder's capability.
3. [ ] Generate the module declaration from registered public entries. Reject duplicate paths,
       declaration-only public bindings, runtime-only public bindings, and incompatible binding
       kinds before constructing a VM.
4. [ ] Define deterministic declaration ordering so generated declarations and snapshots do not
       depend on hash-map iteration or runtime registration order.
5. [ ] Preserve an explicit hand-written `NativeModule` escape hatch. Do not move declaration
       types into `ruau-vm` or make lower-level VM users depend on the umbrella crate.
6. [ ] Keep the builder target-neutral and usable with `default-features = false`; it must not
       depend on `ruau-host`, Tokio, filesystem support, or serde.
7. [ ] Add snapshot tests for declarations and runtime enumeration tests for every binding kind.
       Keep the existing declaration/runtime audit as a second line of defense.
8. [ ] Add three fixture modules modeled on real consumers: a dynamic Canopy-style command table,
       a Porter-style bridge, and an Itty-style module with host types and support code.
9. [ ] Compare the fixture declarations with equivalent hand-written `NativeModule`
       implementations and inspect the final authoring API with `ruskel`.

## 6. Add shared table-layout and value-inspection primitives

Ruau's serde layer has a private table classifier, while four embedders repeat related logic
without needing serde or a universal domain-value codec.

1. [ ] Add a target-neutral `TableLayout` API in `ruau-vm` with `Empty`, `Sequence`, `StringMap`,
       `Sparse`, `Mixed`, and `UnsupportedKey` outcomes. Keep the name distinct from internal
       bytecode and object table-shape types.
2. [ ] Define dense sequences as contiguous positive integer keys beginning at one. Do not use the
       Lua border operator as a host data-shape test or silently stringify numeric keys.
3. [ ] Make the classifier available for live `Table<'_>` entries and marshaled table pairs. Share
       the key-classification core so the representations cannot disagree.
4. [ ] Include structured detail for the first missing index or offending key. Avoid cloning or
       marshaling table values merely to classify their layout.
5. [ ] Reuse the public classifier inside Ruau's serde implementation where semantics match. Keep
       serde-specific map and sequence conversion policy in the serde layer.
6. [ ] Keep host userdata conversion, numeric coercion, invalid UTF-8 policy, path formatting, and
       domain output types outside `TableLayout`.
7. [ ] Do not add a universal `ValueCodec` until two embedders share a domain conversion rather
       than only traversal mechanics. Record this as an explicit non-goal in the API docs.
8. [ ] Add conformance tests for empty, dense, out-of-order dense, sparse, mixed, nested, string
       map, unsupported-key, negative-key, zero-key, fractional-key, and very large tables in live
       and marshaled forms.
9. [ ] Add an example that applies different application policies to one layout result, showing
       that Ruau supplies structure rather than dictating a domain value model.
10. [ ] Audit existing `ScopedValue`, `OwnedValue`, and `MarshaledValue` `type_name` and
        `display_lua` documentation so embedders can delete their duplicate helpers later.

## 7. Apply complete call context to every VM entry point

`Vm::step_with` applies effective limits and cancellation but not the `CallOptions` print sink or
app data. A call option must mean the same thing at every entry point that accepts it.

1. [ ] Inventory all VM entry points that accept `CallOptions` and write a field-by-entry-point
       behavior matrix before editing implementation code.
2. [ ] Make `Vm::step_with` and `Vm::step_with_context` install every applicable field with the
       same semantics as owned execution entry points.
3. [ ] Add one internal call-context guard that restores print sinks and app data after success,
       body error, runtime error, cancellation, panic poisoning, and nested calls.
4. [ ] Specify nested behavior: an inner override is visible only to the inner call, the outer
       override resumes afterward, and the VM default is restored when the outer call ends.
5. [ ] Preserve effective-limit composition, cancellation, GC boundaries, poisoning, and re-entry
       behavior. Do not fix isolation by forbidding valid nested calls.
6. [ ] Add tests for print quotas, VM defaults, per-call app data, borrowed context, nested calls,
       cancellation, runtime errors, panics, and sequential calls with different options.
7. [ ] Refactor existing owned execution methods onto the same guard where that reduces duplicate
       restoration logic without changing their public error behavior.
8. [ ] Update `CallOptions` and VM entry-point documentation to state the complete, uniform
       contract, then inspect the public surface with `ruskel`.

## 8. Extract a lock-free retained-runtime core

Three embedders retain raw VMs and stashed closures. `SurfaceSession` adds synchronization and a
blocking Tokio runtime around a related state machine. Extract mechanics without imposing a host
concurrency model.

1. [ ] Add `RetainedRuntime` to `ruau-host`. It owns a `Surface`, `Vm`, source-epoch snapshot,
       loaded roots, stable stashed-value handles, module-cache state, and execution count.
2. [ ] Make the core require `&mut self` and contain no mutex, background task, arbitrary timeout,
       or application-global registry. Support non-`Send` borrowed host context through existing
       scoped execution APIs.
3. [ ] Provide explicit prepare, load, run, step, stash, fetch, release, unload, and invalidate
       operations. Avoid trivial accessors that expose internal storage rather than intent.
4. [ ] Use typed generational handles so release, unload, or reload makes stale handles fail
       deterministically instead of referring to a new value.
5. [ ] Evaluate a well-maintained generational-arena crate before implementing handle allocation.
       Keep the dependency private and add it with `cargo add` if it materially reduces code.
6. [ ] Tie loaded roots and handles to source epochs. Define which values survive dependency
       invalidation, which roots reload lazily, and how callers receive stale-handle errors.
7. [ ] Make `SurfaceSession` a synchronization and blocking-runtime wrapper around
       `RetainedRuntime`. Preserve its public behavior and error categories while deleting the
       duplicated state machine.
8. [ ] Keep script ids, callback grouping, journals, startup hooks, render policy, and application
       state outside the core. Provide composition seams instead of callbacks for those policies.
9. [ ] Add direct-core tests modeled on Canopy, Hotki, and Gonsh, plus wrapper tests modeled on
       Verber. Use Ruau fixtures rather than importing consumer code.
10. [ ] Test stale handles, explicit release, root unload, epoch invalidation, nested calls,
        callback-style cleanup, VM poisoning, async execution, and repeated reload without
        registry growth.
11. [ ] Add examples for a direct retained runtime and a thread-safe `SurfaceSession` host so the
        concurrency boundary is visible in public documentation.

## 9. Provide multi-root filesystem module sources

Canopy composes `MountedSource`, `FilesystemSource`, filesystem epochs, reverse path lookup, and
per-root invalidation. Those mechanics belong in `ruau-fs`; Canopy startup policy does not.

1. [ ] Add a `FilesystemMounts` builder and source in `ruau-fs`. Map `ModuleId` prefixes to
       filesystem roots while retaining access to the composable lower-level source primitives.
2. [ ] Support mount creation, one-mount invalidation, all-mount invalidation, a composite epoch,
       and reverse `module_id_for_path` lookup for supported Luau files.
3. [ ] Define duplicate-prefix, overlapping-prefix, duplicate-root, extension, normalization,
       symlink, traversal, and outside-root behavior before implementing reverse lookup.
4. [ ] Follow existing `MountedSource` precedence where compatible and reject ambiguous reverse
       mappings. Do not silently choose a filesystem root based on insertion order.
5. [ ] Return a root `Source` with the same `ModuleId` and display path used for graph checking,
       compilation, loading, and tracebacks.
6. [ ] Keep root discovery, user/project precedence, startup order, and `init.luau` conventions out
       of Ruau.
7. [ ] Add tests for relative imports, nested modules, unknown mounts, reverse lookup, per-root
       epochs, overlapping prefixes, duplicate roots, symlink escape, traversal, and outside files.
8. [ ] Add a file-runner example with two roots and relative sibling imports. Include invalidation
       and re-preparation so the filesystem and prepared-graph contracts are tested together.
9. [ ] Inspect the final `ruau-fs` surface with `ruskel` and keep `MountedSource` and
       `FilesystemSource` independently usable.

## 10. Consolidate the Ruau embedding story

Prove that the new APIs form a coherent stack before handing them to Canopy.

1. [ ] Add or update focused examples for owned diagnostics, graph preparation, borrowed closures,
       generated native modules, table layout, complete call context, direct retained runtime,
       `SurfaceSession`, and multi-root filesystems.
2. [ ] Add one end-to-end example that combines `FilesystemMounts`, `Surface::prepare_graph`,
       `NativeModuleBuilder`, `RetainedRuntime`, and per-call options without using consumer code.
3. [ ] Ensure examples use the lower-level APIs where appropriate and do not imply that every
       embedder must adopt the highest-level host stack.
4. [ ] Run `ruskel` over each touched crate and the umbrella crate. Remove duplicate concepts,
       forwarding methods, leaky implementation types, and inconsistent names before adoption.
5. [ ] Verify error ownership. Every fallible crate with public `Result` methods must retain a
       focused error type, and high-level errors must preserve useful lower-level categories.
6. [ ] Update the design note with the final API names, examples, consumer mappings, known
       limitations, and explicit Canopy follow-up items.
7. [ ] Mark an API provisional where Canopy must still settle ergonomics. Do not add deprecations
       or compatibility wrappers before a real adopter has exercised the replacement.

## 11. Validate Ruau and hand off to Canopy

Run focused gates after each coherent batch and repository-wide gates at the end. This stage ends
with Ruau ready for adoption, not with consumer repositories modified.

1. [ ] After every stage, run the smallest relevant crate tests and
       `cargo check -p ruau --no-default-features`.
2. [ ] Run target-neutral crate tests and examples for `wasm32` using the repository's existing
       Workers-compatible target gate. Keep `ruau-host` and Tokio outside that build.
3. [ ] Run documentation tests and compile every changed example with both the default workspace
       configuration and the umbrella crate's `default-features = false` contract.
4. [ ] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
       --examples 2>&1`, resolve every warning, and rerun focused tests after each fix batch.
5. [ ] Format with Ruau's repository formatter after the final code change.
6. [ ] Run `cargo xtask tidy` and `cargo xtask test` from
       `/Users/cortesi/git/private/ruau`.
7. [ ] Run `git diff --check`, inspect the exact Ruau diff and public API skeletons, and verify that
       no consumer repository changed during this plan.
8. [ ] Update every checklist item immediately as it completes and add newly discovered Ruau work
       to the appropriate earlier stage rather than deferring it silently to Canopy.
9. [ ] Record the exact Ruau revision and final `ruskel` surfaces in
       [`canopy-ruau.md`](canopy-ruau.md), then stop for user review before beginning adoption.

## Expected embedder outcomes

These are the expected results after each embedder adopts the completed Ruau APIs. The migrations
remain outside this Ruau-first plan.

- **Canopy:** named config and startup scripts use one checked-graph pipeline; diagnostics retain
  source identity; command modules declare and install bindings once; value conversion shares
  table-layout mechanics; retained scripts use stable handles and per-call context; user and
  project script roots use the shared filesystem source.
- **Verber and Verber Connect:** direct `GraphChecker` orchestration and copied diagnostic records
  shrink to prepared graphs and owned records; retained execution builds on the simplified
  `SurfaceSession`; standard-library modules can eliminate declaration/runtime duplication.
- **Itty:** the terminal module becomes a single declaration-coupled definition; borrowed handlers
  lose adapter structs; option parsing uses reliable table layouts; file execution can support
  relative sibling imports through mounted filesystem roots.
- **Hotki:** render and event callbacks use uniform per-call options and stable retained handles;
  its native module has one source of truth; modular configuration can grow without creating a
  second filesystem-source layer.
- **Gonsh:** callback storage and cleanup move from a raw VM and hash map to typed retained handles;
  callback state becomes per-call rather than persistent VM data; sequence detection stops probing
  the Lua border manually.
- **Eguidev:** its large native module can replace hand-written scoped function structs and paired
  declarations with closure registrations, while existing Ruau type and display helpers replace
  local value-description code.
- **Eguito:** its smaller scripted surface gains the same closure and module-authoring path, and
  can delete local scoped-value type-name duplication without adopting a larger host runtime.
- **Porter and Subagent:** the bridge module declares and installs bindings together; its JSON
  wrapper retains JSON policy but delegates lifetime plumbing to borrowed closures; evaluator app
  data remains the normal route for per-evaluation state.
- **Ruau Sandbox:** graph preparation and owned diagnostics reduce local orchestration and DTO
  copying, while shared value inspection improves result and error presentation.
- **Workers:** target-neutral diagnostics, module authoring, table inspection, and VM changes remain
  available on `wasm32` without pulling in `ruau-host`, Tokio, or native filesystem/runtime policy.
