# Canopy Robustness, Correctness, and Ergonomics Roadmap

This roadmap is based on a design review of Canopy's public API surface with `ruskel`,
the core runtime, built-in widgets, scripting, MCP evaluation, rendering, layout, and
existing documentation. The main conclusion is that Canopy has a strong core model, but
too many important invariants are currently enforced by convention, dynamic borrowing,
panic paths, or duplicated logic.

The recommendations below intentionally include breaking and structural changes. They
are ordered so each stage can land as one or more coherent changes while keeping the
repository buildable and testable between stages.

## Review Findings

- The arena tree, widget lifecycle, render pass, and event dispatch rules are central
  APIs, but their invariants are mostly implicit in `crates/canopy/src/core/world.rs`.
- `Core`, `Canopy`, `Context`, and many modules expose broad public surfaces. App code can
  reach lower-level mechanisms that should probably remain internal implementation.
- Widget access relies on `RefCell<Option<Box<dyn Widget>>>`, temporary slot removal, and
  unsafe restoration through `NonNull<Core>`. This makes reentrancy hard to reason about.
- Layout and rendering sometimes convert internal failures into zero sizes or fallback
  canvases. That hides correctness bugs and makes failures difficult to diagnose.
- Command dispatch, command availability, help, input binding, and event routing repeat
  related resolution logic. The repeated logic is likely to drift.
- Script callbacks use a thread-local stack containing raw `*mut Canopy`; MCP timeouts
  spawn worker threads that may continue after the caller receives a timeout.
- Built-in widgets contain useful patterns, but keyed reconciliation, selection updates,
  editor transactions, and terminal driver ownership need stronger reusable contracts.
- The current mdBook documentation tree is too stale to maintain incrementally. Treat it
  as historical source material and replace it with a fresh plain Markdown tree.
- Core invariants are not yet captured as executable tests or snapshots.

## Guiding Principles

1. Public APIs should express invariants and make invalid states hard to construct.
2. Expected failures should return typed errors with node/path context, not panic.
3. The runtime owner and event-loop boundary should be explicit across widgets and scripts.
4. Layout, rendering, focus, and tree mutation should have property or differential tests.
5. Widget ergonomics should prefer typed builders and handles over raw strings and IDs.
6. Lower-level escape hatches should be private, `#[doc(hidden)]`, or clearly experimental.
7. Documentation should be plain Markdown under `docs/`, not generated mdBook source.

## 1. Stage One: Establish Runtime Contracts

Start by removing the stale docs and writing the core architecture contract the rest of
the roadmap will preserve. This stage should not significantly change public behavior
except where failures are currently hidden.

1. [x] Remove the obsolete mdBook tree: `docs/src`, `docs/book.toml`, and
       `docs/.gitignore`.
2. [x] Remove any local generated `docs/book` output so ignored stale docs do not linger.
3. [x] Write `docs/architecture.md` covering tree, lifecycle, layout, render,
       event routing, scripting ownership, and node ID validity.
4. [x] Add `Core::validate_invariants() -> Result<()>` and use it in tests and
       smoketests after structural mutations.
5. [x] Cover parent/child links, keys, focus, capture, init flags, layout state, and
       view state in invariant validation.
6. [x] Add property tests for `attach`, `detach`, `set_children`, `remove_subtree`,
       and `replace_subtree`.
7. [x] Replace hidden fallbacks in measurement, canvas computation, layout refresh, and
       runloop initialization with typed errors or explicit diagnostics.
8. [x] Define the panic policy for public crates: public APIs return `Result` or
       `Option`; panics are reserved for impossible internal bugs and tests.

## 2. Stage Two: Unify Routing and Command Resolution

Canopy needs one authoritative path for converting input into widget events and command
dispatch. This reduces correctness risk and makes binding behavior easier to explain.

1. [x] Extract a `CommandResolver` shared by dispatch, availability, help, and
       diagnostic reporting.
2. [x] Unify key and mouse routing into a single pipeline with explicit routing phases.
3. [x] Represent binding precedence with a typed priority or documented replacement rule.
4. [x] Promote input modes to first-class public commands and remove dead-code allowances.
5. [x] Add tests for command target resolution across descendants, ancestors, and owners.
6. [x] Fix and test hit-testing order so topmost rendered children receive pointer input.
7. [x] Add route tracing for diagnostics so a key or mouse event can explain its outcome.

## 3. Stage Three: Make Reentrancy and Ownership Explicit

The current slot-removal model permits powerful widget callbacks, but it spreads a risky
ownership pattern across the core. Narrowing the unsafe boundary should be a priority.

1. [x] Replace direct mutable widget extraction with an explicit widget access module.
2. [x] Keep any remaining unsafe code inside that module with audited invariants.
3. [x] Make widget restoration panic-safe and test nested access, panic, and early return.
4. [x] Split read-only layout/render access from mutation hooks where practical.
5. [x] Evaluate a command-buffer or `TreeMutator` model for widget-triggered tree edits.
6. [x] Convert `with_script_context` to an RAII guard that always pops thread-local state.
7. [x] Replace raw script `*mut Canopy` access with an explicit execution context if viable.
8. [x] Add deterministic tests for nested script callbacks, release, unbind, and dispatch.

Stage Three keeps the current callback mutation model, but narrows it behind
`widget_access`. A command buffer or `TreeMutator` would be a larger API break and
should wait until layout/render error contracts are strengthened; the new access layer is
the intended migration point if that model becomes worthwhile.

## 4. Stage Four: Finalize Widget View and Layout Contracts

Stage Three gave Canopy explicit widget access modes. Before deeper rendering work, make
those modes part of the internal contract and make layout failures carry useful context.

1. [x] Rename or split `with_widget_view` so render-time mutable access cannot be confused
       with read-only widget queries.
2. [x] Add a small helper for attaching node ID, node path, and operation name to widget
       access, layout refresh, measure, canvas, and render errors.
3. [x] Decide whether `Widget::measure` and `Widget::canvas` should become fallible APIs.
4. [x] Keep measure and canvas infallible for now, with fallible slot access reported as
       contextual layout errors.
5. [x] If measure and canvas stay infallible, document that policy and ensure all access
       failures still surface with node context.
6. [x] Add tests proving read, render, layout refresh, measure, and canvas failures report
       the affected node and operation.
7. [x] Update `docs/architecture.md` with the three widget access modes: read, render, and
       mutation callback.

## 5. Stage Five: Strengthen Layout, Rendering, and Geometry

Once widget view failures are explicit, tighten layout and rendering themselves. The goal
is to make geometry and terminal output mechanically testable rather than visually
inspected.

1. [x] Add `Layout::validate()` or introduce a validating `LayoutSpec` builder.
2. [x] Keep fixed sizing as `fixed_width()` and `fixed_height()` constraints rather than
       adding `Sizing::Fixed(u32)` or a `Length` enum.
3. [x] Add property tests for `Rect`, `LineSegment`, clipping, `View`, and scroll clamping.
4. [x] Add layout property tests for flex allocation, hidden nodes, display modes, padding,
       and extreme sizes.
5. [x] Build a differential render test comparing full repaint and diff output states.
6. [x] Centralize grapheme writing and clipping in `TermBuf`.
7. [x] Test wide graphemes, continuation cells, row shifts, line shifts, and stale cells.
8. [x] Add compact render snapshots for failure cases that property tests find hard to
       explain.

## 6. Stage Six: Decide the Tree Mutation Boundary

Stage Three kept immediate mutation during callbacks because the current behavior is useful
and now better contained. Revisit buffering only after layout and render contracts can
report failures clearly.

1. [x] Audit every `Context` method that mutates tree structure, focus, capture, input
       bindings, script state, or layout state during callbacks.
2. [x] Write `plans/tree-mutator.md` comparing immediate mutation, command buffering, and
       a `TreeMutator` API against Canopy's callback use cases.
3. [x] Add regression tests for callbacks that remove or replace the current node, parent,
       sibling, focused node, and mouse-capture node.
4. [x] Decide whether mutation remains immediate or moves behind a buffered boundary.
5. [x] If mutation remains immediate, document allowed reentrancy and add tests for the
       documented edge cases.
6. [x] Defer buffered mutation and record the migration triggers in
       `plans/tree-mutator.md`.

## 7. Stage Seven: Harden Scripting and MCP Evaluation

The script context stack is now RAII-protected, but scripting and MCP remain user-facing
automation boundaries. They should fail predictably, surface diagnostics, and avoid
background work continuing beyond the caller's contract.

1. [x] Replace thread-per-timeout MCP evaluation with cooperative cancellation or process
       isolation.
2. [x] If hard cancellation is impossible, document timeout semantics and expose task state.
3. [x] Audit `ScriptExecutionContext` and thread-local use for cross-thread assumptions and
       document the supported execution model.
4. [x] Make Luau typechecking APIs stable across targets by returning unavailable
       diagnostics instead of removing methods.
5. [x] Split script host responsibilities into compiler, typechecker, command binding,
       closure registry, and diagnostics components.
6. [x] Add golden tests for generated `.d.luau`, command enums, named args, and fixtures.
7. [x] Expand script ABI tests for optional args, error reporting, nested callbacks,
       deferred release, unbind, and dispatch.
8. [x] Write `docs/scripting.md` from the generated API and keep it snapshotted.

## 8. Stage Eight: Improve Widget Composition APIs

Built-in widgets should model the app-author API Canopy wants users to write. Shared
composition machinery should build on the chosen mutation boundary rather than adding
another ownership pattern.

1. [x] Review existing `Slot`, keyed child, and typed ID helpers before adding new
       reconciliation APIs.
2. [x] Add reusable keyed-child reconciliation in `canopy`.
3. [x] Move `List` reconciliation to the shared reconciler and remove internal panics.
4. [x] Make `List` selection updates return errors instead of swallowing access failures.
5. [x] Add focus and selection invariant tests for `List`, `Selector`, and `Dropdown`.
6. [x] Split `Editor` into clearer buffer, view, and controller responsibilities.
7. [x] Add strict `TextBuffer` accessors alongside current clamping helpers.
8. [x] Replace manual editor undo transactions with an RAII transaction guard.
9. [x] Encapsulate terminal driver threading in a private driver runtime module.
10. [x] Remove sleeps from terminal tests by waiting on explicit driver or session events.
11. [x] Audit widget constructors and replace complex public fields with config builders.

## 9. Stage Nine: Narrow the Public API Surface

This is the main breaking-change stage. It should happen after the internal contracts are
tested so public API reductions can be made confidently.

1. [x] Use `ruskel` to capture the public API skeleton for each workspace crate before
       changing exports.
2. [x] Decide the stable app-author surface: prelude, widget trait, context, layout,
       commands, styles, and selected widgets.
3. [x] Make `Canopy` fields private and expose focused methods for core and style access.
4. [x] Split `Context` into smaller capability traits or extension traits.
5. [x] Hide lower-level core modules that app authors should not depend on directly.
6. [x] Replace raw string path APIs with typed `Path`, `PathFilter`, and `NodeName`.
7. [x] Validate path components and scripting path strings at the boundary.
8. [x] Review `canopy-widgets` exports and mark experimental APIs before release.
9. [x] Update examples and docs so they use only the intended public surface.

## 10. Stage Ten: Expand Tooling and CI Guardrails

Once the core contracts are clearer, automate checks that keep regressions from returning.

1. [ ] Extend `cargo xtask tidy` to flag `unwrap`, `expect`, and `panic!` in library code.
2. [ ] Keep a small allowlist for justified panics with a rationale next to each entry.
3. [ ] Add CI coverage for `cargo xtask smoke`, docs, and generated Luau API snapshots.
4. [ ] Add MCP headless and live evaluation fixture tests.
5. [ ] Add benchmark coverage for layout, render diffing, text buffers, and large trees.
6. [ ] Add a fixture inventory so CLI, MCP, and widget smoke tests cover real workflows.
7. [ ] Add an API-surface diff check using `ruskel` output for intentional public changes.
8. [ ] Move completed historical work out of `plans/next.md` or clearly mark it archived.

## Suggested Next Batch

The next implementation batch should be Stage Ten items 1 through 3: add library panic
guardrails to `cargo xtask tidy`, keep any justified panic allowlist small and documented,
and wire the existing smoke, docs, and generated Luau checks into CI.
