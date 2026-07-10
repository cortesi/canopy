# Ruau adopter feedback

This live plan records shared Ruau work proven necessary while Canopy adopts the completed
embedding API expansion. Canopy-specific policy remains in Canopy, and new items belong here only
when adoption demonstrates a reusable Ruau API gap.

## 1. Preserve generated declaration documentation

`NativeModuleBuilder` couples public binding types to runtime registration, but the completed
surface cannot preserve documentation from an adopter's hand-written declaration fields. Canopy's
base API and owner commands both expose user-facing function documentation, so migration would
otherwise degrade the generated declaration.

1. [x] Add declaration documentation to public `NativeBinding` entries and reject documentation on
       hidden runtime-only bindings.
2. [x] Preserve documentation when generating both global declarations and library table fields,
       without weakening deterministic binding ordering or declaration/runtime validation.
3. [x] Cover documented declarations and hidden-binding rejection in the native-module builder
       tests, then run Ruau's no-default-features, tidy, and full test gates.
4. [x] Prove the API through Canopy's base and dynamic owner modules before considering the Ruau
       feedback complete.

## 2. Compose generated modules with shared declaration types

Canopy's dynamic owner modules use command argument and return types declared by the base module.
`ruau-decl::Builder` can mark those names as external, but `NativeModuleBuilder` cannot, so an
adopter must either duplicate shared declarations or keep a hand-written module.

1. [x] Add a target-neutral `NativeModuleBuilder::extern_ty` seam that participates in declaration
       validation without rendering or duplicating the shared type declaration.
2. [x] Normalize external type names deterministically and cover a generated binding whose type is
       supplied by another declaration module.
3. [x] Prove the seam through Canopy's command declaration registry and dynamic owner modules, then
       rerun the Ruau and Canopy validation gates.

## 3. Complete retained-runtime context and reentrancy seams

Canopy's retained scripts run async host functions with borrowed `&mut Canopy` state and can invoke
stored roots or callbacks from an already-live VM scope. `RetainedRuntime` supports borrowed
context for fresh synchronous steps, but cannot run an async root with borrowed context or resolve
a validated root/function handle inside an existing scope.

1. [x] Add async root execution with borrowed non-`Send` host context while preserving per-call
       options, execution counts, source-epoch invalidation, and stale-root errors.
2. [x] Add scope-branded root and function resolution methods that validate generational handles
       and let reentrant embedders call them without starting a nested VM step.
3. [x] Test borrowed async context, valid nested resolution, stale/released handles, and use with a
       foreign VM scope; keep arenas and raw registry storage private.
4. [x] Bind every compiled or prepared retained root to a fresh chunk environment and test that
       top-level assignments cannot leak between roots in the shared VM.
5. [x] Prove the seams by moving Canopy's raw VM, loaded roots, and closure stash ownership onto
       `RetainedRuntime`, then rerun both repositories' full gates.
