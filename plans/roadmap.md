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

1. [ ] Extract a `CommandResolver` shared by dispatch, availability, help, and
       diagnostic reporting.
2. [ ] Unify key and mouse routing into a single pipeline with explicit routing phases.
3. [ ] Represent binding precedence with a typed priority or documented replacement rule.
4. [ ] Promote input modes to first-class public commands and remove dead-code allowances.
5. [ ] Add tests for command target resolution across descendants, ancestors, and owners.
6. [ ] Fix and test hit-testing order so topmost rendered children receive pointer input.
7. [ ] Add route tracing for diagnostics so a key or mouse event can explain its outcome.

## 3. Stage Three: Make Reentrancy and Ownership Explicit

The current slot-removal model permits powerful widget callbacks, but it spreads a risky
ownership pattern across the core. Narrowing the unsafe boundary should be a priority.

1. [ ] Replace direct mutable widget extraction with an explicit widget access module.
2. [ ] Keep any remaining unsafe code inside that module with audited invariants.
3. [ ] Make widget restoration panic-safe and test nested access, panic, and early return.
4. [ ] Split read-only layout/render access from mutation hooks where practical.
5. [ ] Evaluate a command-buffer or `TreeMutator` model for widget-triggered tree edits.
6. [ ] Convert `with_script_context` to an RAII guard that always pops thread-local state.
7. [ ] Replace raw script `*mut Canopy` access with an explicit execution context if viable.
8. [ ] Add deterministic tests for nested script callbacks, release, unbind, and dispatch.

## 4. Stage Four: Strengthen Layout, Rendering, and Geometry

Layout and rendering are correctness-heavy and hard to inspect manually. The goal is to
turn silent behavior into explicit contracts and test render output mechanically.

1. [ ] Add `Layout::validate()` or introduce a validating `LayoutSpec` builder.
2. [ ] Consider `Sizing::Fixed(u32)` or a `Length` enum instead of encoding fixed sizes
       as min equals max.
3. [ ] Propagate measure and canvas errors through `Core::update_layout` with node context.
4. [ ] Add property tests for `Rect`, `LineSegment`, clipping, `View`, and scroll clamping.
5. [ ] Add layout property tests for flex allocation, hidden nodes, and extreme sizes.
6. [ ] Build a differential render test comparing full repaint and diff output states.
7. [ ] Centralize grapheme writing and clipping in `TermBuf`.
8. [ ] Test wide graphemes, continuation cells, row shifts, line shifts, and stale cells.

## 5. Stage Five: Harden Scripting and MCP Evaluation

Scripting and MCP are user-facing automation boundaries. They should fail predictably,
surface diagnostics, and avoid background work continuing beyond the caller's contract.

1. [ ] Replace thread-per-timeout MCP evaluation with cooperative cancellation or process
       isolation.
2. [ ] If hard cancellation is impossible, document timeout semantics and expose task state.
3. [ ] Make Luau typechecking APIs stable across targets by returning unavailable
       diagnostics instead of removing methods.
4. [ ] Split script host responsibilities into compiler, typechecker, command binding,
       closure registry, and diagnostics components.
5. [ ] Add golden tests for generated `.d.luau`, command enums, named args, and fixtures.
6. [ ] Add script ABI tests for optional args, error reporting, callbacks, and unbinding.
7. [ ] Write `docs/scripting.md` from the generated API and keep it snapshotted.

## 6. Stage Six: Improve Widget Composition APIs

Built-in widgets should model the app-author API Canopy wants users to write. Shared
composition machinery will also reduce bugs in child lifecycle and selection handling.

1. [ ] Add reusable keyed-child reconciliation in `canopy`.
2. [ ] Move `List` reconciliation to the shared reconciler and remove internal panics.
3. [ ] Make `List` selection updates return errors instead of swallowing access failures.
4. [ ] Add focus and selection invariant tests for `List`, `Selector`, and `Dropdown`.
5. [ ] Split `Editor` into clearer buffer, view, and controller responsibilities.
6. [ ] Add strict `TextBuffer` accessors alongside current clamping helpers.
7. [ ] Replace manual editor undo transactions with an RAII transaction guard.
8. [ ] Encapsulate terminal driver threading in a private driver runtime module.
9. [ ] Remove sleeps from terminal tests by waiting on explicit driver or session events.
10. [ ] Audit widget constructors and replace complex public fields with config builders.

## 7. Stage Seven: Narrow the Public API Surface

This is the main breaking-change stage. It should happen after the internal contracts are
tested so public API reductions can be made confidently.

1. [ ] Decide the stable app-author surface: prelude, widget trait, context, layout,
       commands, styles, and selected widgets.
2. [ ] Make `Canopy` fields private and expose focused methods for core and style access.
3. [ ] Split `Context` into smaller capability traits or extension traits.
4. [ ] Hide lower-level core modules that app authors should not depend on directly.
5. [ ] Replace raw string path APIs with typed `Path`, `PathFilter`, and `NodeName`.
6. [ ] Validate path components and scripting path strings at the boundary.
7. [ ] Review `canopy-widgets` exports and mark experimental APIs before release.
8. [ ] Update examples and docs so they use only the intended public surface.

## 8. Stage Eight: Expand Tooling and CI Guardrails

Once the core contracts are clearer, automate checks that keep regressions from returning.

1. [ ] Extend `cargo xtask tidy` to flag `unwrap`, `expect`, and `panic!` in library code.
2. [ ] Keep a small allowlist for justified panics with a rationale next to each entry.
3. [ ] Add CI coverage for `cargo xtask smoke`, docs, and generated Luau API snapshots.
4. [ ] Add MCP headless and live evaluation fixture tests.
5. [ ] Add benchmark coverage for layout, render diffing, text buffers, and large trees.
6. [ ] Add a fixture inventory so CLI, MCP, and widget smoke tests cover real workflows.
7. [ ] Move completed historical work out of `plans/next.md` or clearly mark it archived.

## Suggested First Batch

The first implementation batch should be Stage One items 1 through 3. That deletes the
stale mdBook tree and replaces it with a plain Markdown architecture document before any
larger implementation refactors. After that, the invariant checker and tree mutation
property tests will give later changes a better safety net.
