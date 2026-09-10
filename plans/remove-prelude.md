# Remove the Canopy Prelude

## Description

Canopy exposes a glob convenience module, `canopy::prelude`
(`crates/canopy/src/prelude.rs`), that re-exports 38 items which already have a
home elsewhere. Every one of those items therefore has two public paths, for
example `canopy::Context` and `canopy::prelude::Context`, or
`canopy::event::Event` and `canopy::prelude::Event`.

The project rule is one canonical export location per item. Removing the prelude
leaves exactly one path per item and makes imports explicit.

This is a breaking public API change. `canopy` is at `0.0.1`, and
`docs/api-budget.md` states the project favors a smaller, clearer breaking
surface while the API evolves. No compatibility shim is proposed.

After removal the canonical locations are:

- Core app-author types stay at the crate root.
- Domain types are imported from their module.
- Derive macros stay at the crate root.

| Prelude item | Canonical path |
| --- | --- |
| `Canopy` | `canopy::Canopy` |
| `ChangeOutcome` | `canopy::ChangeOutcome` |
| `ChildSlot` | `canopy::ChildSlot` |
| `Context`, `ContextExt` | `canopy::Context`, `canopy::ContextExt` |
| `EventOutcome` | `canopy::EventOutcome` |
| `FocusDirection`, `FocusScope` | `canopy::FocusDirection`, `canopy::FocusScope` |
| `Loader` | `canopy::Loader` |
| `NodeId`, `TypedId` | `canopy::NodeId`, `canopy::TypedId` |
| `RenderLimits` | `canopy::RenderLimits` |
| `ViewContext`, `ViewContextExt` | `canopy::ViewContext`, `canopy::ViewContextExt` |
| `Widget` | `canopy::Widget` |
| `CommandArg`, `CommandEnum` | `canopy::CommandArg`, `canopy::CommandEnum` (derive macros) |
| `Event` | `canopy::event::Event` |
| `Key` | `canopy::event::key::Key` |
| `Point`, `Rect`, `Size` | `canopy::geom::{Point, Rect, Size}` |
| `Align`, `Constraint`, `Direction`, `Display`, `Layout`, `MeasureConstraints`, `MeasureOverflow`, `Measurement`, `Sizing` | `canopy::layout::{...}` |
| `Path`, `PathFilter` | `canopy::path::{Path, PathFilter}` |
| `Render` | `canopy::Render` |
| `NodeName` | `canopy::NodeName` |
| `StyleBuilder`, `StyleMap` | `canopy::style::{StyleBuilder, StyleMap}` |

A follow-up moved `Render`, `NodeName`, and `View` to the crate root and left
`canopy::render` with only the backend interfaces. See the note under Open
Decisions.

The `CommandArg` derive macro (`canopy::CommandArg`) shares a name with the
`canopy::commands::CommandArg` trait. Code that needs the trait imports it from
`canopy::commands`; the derive macro stays at the crate root.

Two details make the migration compiler-guided rather than a pure rename:

- Many items are extension traits. Files that call `ContextExt` or
  `ViewContextExt` methods through the prelude glob must import the trait
  explicitly, because trait methods require the trait in scope.
- Glob imports can be shadowed by explicit imports. For example,
  `examples/todo/src/lib.rs` imports `std::path::Path` and
  `std::fmt::Display`, which shadow the prelude `Path` and `Display`. In those
  files the correct fix is to import only the canopy items actually used;
  the shadowed names never were.

## Changes

### C1: Delete the prelude module

Remove `pub mod prelude;` from `crates/canopy/src/lib.rs:27` and delete
`crates/canopy/src/prelude.rs`. Every re-exported item remains reachable at the
canonical path in the table above, so no new re-exports are required.

### C2: Migrate workspace consumers to canonical imports

Replace each `canopy::prelude::*` import with explicit imports from the table
above. The consumers are:

- `crates/canopy-mcp/src/script.rs` and `crates/canopy-mcp/src/server.rs`
  (test modules).
- `crates/examples/examples/demo.rs`, `examples/widget.rs`.
- `crates/examples/src/{chargym,editorgym,focusgym,framegym,intervals,listgym,pager,stylegym,termgym,textgym,widget_editor}.rs`.
- `crates/examples/src/tests/{focusgym,framegym,listgym,stylegym,termgym}.rs`.
- `examples/todo/src/lib.rs` and `examples/todo/tests/basic.rs`.

Work file by file, add the imports the compiler asks for, and remove imports
the compiler reports as unused. Keep existing grouped `use canopy::{...}`
statements and add the module imports to them. This change is mechanical; the
main risk is extension-trait methods failing to resolve until the trait import
is added.

### C3: Update documentation

Rewrite `docs/architecture.md:11-16` so the public API surface section names the
crate root and module paths instead of `canopy::prelude::*`. State the
single-location rule as the reason. No README or `AGENTS.md` content references
the prelude. `plans/review.md` mentions it only in a historical proposal and
stays as written.

### C4: Refresh API captures and budget

Run `ncode api` and review the semantic diff. The `prelude` module
(`api/canopy.rs:2590`) disappears from the `canopy.rs` capture. Run
`ncode api --check` to confirm the captures are current. The reduction stays
inside the existing `canopy.rs` threshold, so no `docs/api-budget.md` edit is
needed unless the review flags one.

### C5: Confirm the canonical scheme is the intended one

Removing the prelude achieves one path per item under the current layout: core
types at the crate root and domain types in modules. This is a mixed scheme.

An alternative is to move the flat root re-exports into their source modules
(for example, `canopy::context::Context`, `canopy::canopy::Canopy`) so all
imports are module-based. That is a larger breaking change and is not part of
this plan. Decide whether the mixed scheme is acceptable before starting, since
the two changes touch the same import sites.

## Open Decisions

- Accept the mixed root/module scheme, or plan a follow-up that moves all
  public items under modules. Recommendation: accept it; the root surface is
  deliberate and documented as the app-author API.
- Follow-up applied: `Render`, `NodeName`, and `View` moved to the crate root,
  the `state` and `view` modules were removed, and `canopy::render` now exposes
  only `RenderBackend` and `NopBackend`. The crate root holds the facade traits
  and their handle types; value libraries stay in modules.
- Whether to add a guard against reintroducing a prelude. A repository policy
  line in `docs/architecture.md` is likely sufficient. An `xtask` check for
  `pub mod prelude` is possible but adds tooling for a rule already enforced by
  API review.
