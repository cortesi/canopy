# Public API design budget

This directory contains `ruskel` skeletons for every workspace target with a public Rust API. The
files are review artifacts for intent, cohesion, and complexity. They are not compatibility
baselines: Canopy has no backwards-compatibility constraint, and a smaller or clearer breaking
surface is preferred over preserving an old skeleton.

`canopyctl` and `xtask` are binary-only crates, so they have no public API artifact. The example
and Todo libraries are included because other workspace targets compile against them.

## Regeneration

Run these commands from the workspace root after a public API change:

```sh
ruskel crates/canopy > api-surface/canopy.rs
ruskel crates/canopy-derive > api-surface/canopy-derive.rs
ruskel crates/canopy-geom > api-surface/canopy-geom.rs
ruskel crates/canopy-mcp > api-surface/canopy-mcp.rs
ruskel crates/canopy-widgets > api-surface/canopy-widgets.rs
ruskel crates/examples > api-surface/canopy-examples.rs
ruskel examples/todo > api-surface/todo.rs
```

Review the semantic diff. Do not reject a change merely because the generated text changed.

## Intent-level budgets

The core budgets count methods in deep-path skeletons, avoiding duplicated definitions that
`ruskel` renders through convenience re-exports such as the prelude.

| Surface | Current | Budget | Review rule |
| --- | ---: | ---: | --- |
| `Canopy` | 66 methods | 70 | Add only app-lifecycle operations that cannot live on a context. |
| `ViewContext` | 32 methods | 32 | New queries must replace or generalize an existing query. |
| `Context` | 48 methods | 48 | New mutations must replace or generalize an existing mutation. |
| `Editor` | 22 methods | 24 | Keep editing policy on the editor and buffer mechanics on `TextBuffer`. |

The small headroom on `Canopy` is for a demonstrated cross-cutting lifecycle operation, not for
aliases. Exceeding a budget requires an explicit design note explaining why consolidation is not
clearer.

## Crate budgets

Generated line counts are coarse complexity signals because documentation and re-export expansion
affect them. Growth past these ceilings triggers review; shrinkage never requires compatibility
work.

| Artifact | Current lines | Review ceiling | Intended responsibility |
| --- | ---: | ---: | --- |
| `canopy.rs` | 6,384 | 6,500 | Retained tree, layout, input, rendering, scripting, runtime facade. |
| `canopy-widgets.rs` | 1,954 | 2,050 | Reusable widgets and the experimental editor. |
| `canopy-mcp.rs` | 1,012 | 1,050 | Automation protocol, evaluation, launch, and smoke helpers. |
| `canopy-geom.rs` | 536 | 575 | Geometry values and checked operations. |
| `canopy-examples.rs` | 978 | 1,050 | Demo application APIs used by example tests and binaries. |
| `todo.rs` | 223 | 225 | Todo example construction and store integration. |
| `canopy-derive.rs` | 28 | 40 | Command proc macros only. |

## Review findings

### Critical and major

None. The concrete arena `Core`, `InputMap`, backend controller, and broad Ruau namespaces are no
longer public. Widget installers use `Canopy` or `Context`, and terminal integration exposes only
run-loop policy and entry points.

### Moderate

- `Canopy` remains the largest intent-level surface. Its methods fall into runtime, tree setup,
  scripting, fixtures, input modes, and diagnostics. Keep those groups visible in future reviews;
  do not add root/local aliases or expose storage to shorten callers.
- `Context` and `ViewContext` are at their budgets. Extension behavior should be default methods or
  free helpers only when it composes existing primitives and does not create another synonym.
- `canopy::commands::declaration` is an intentional narrow Ruau declaration seam required by
  generated command implementations. Native-module registration names the Ruau trait in one method
  but no longer re-exports the embedding namespaces.

### Minor

- The example libraries are intentionally broad because their public nodes are reused by tests and
  binary wrappers. They are tracked separately so demo growth does not obscure core growth.
- `Input::text` returns the visible slice while `Input::value` returns the complete value. Their
  similar signatures represent different state and should retain explicit documentation.

## Concepts visible in the settled surface

- Retained structure: `NodeId`, `TypedId`, `ChildKey`, `KeyedChildren`, and checked context edits.
- Layout: `Layout`, constraints, measurements, canvas children, views, and geometry primitives.
- Input: typed key and mouse events, binding IDs/specifications, focus scopes, and input modes.
- Rendering: `Render`, `RenderBackend`, `TermBuf`, styles, cursor policy, and render limits.
- Scripting: command metadata, argument values, fixtures, checked scripts, startup roots, journals,
  structured script errors, and the narrow native-module registration seam.
