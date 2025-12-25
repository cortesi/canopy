# Ergonomic Improvements Execution Plan

Convert the ergonomic improvements proposal into staged, testable changes that keep the public
API clean while improving widget authoring and example readability.

1. Stage One: Taffy isolation and public API audit

Audit current Taffy exposure, add a stable `canopy::layout` surface, and update call sites.

1. [x] Inventory current public Taffy usage in `crates/canopy/src/widget/mod.rs`,
       `crates/canopy/src/core/context.rs`, `crates/canopy/src/core/builder.rs`,
       `crates/canopy/src/widgets/frame.rs`, `crates/canopy/tests/test_layout.rs`,
       `crates/examples/src/focusgym.rs`, and `crates/examples/src/framegym.rs`.
2. [x] Add `crates/canopy/src/layout.rs` with curated re-exports and expose it in
       `crates/canopy/src/lib.rs`.
3. [x] Replace public trait signatures and example imports to use `crate::layout` types.
4. [x] Remove direct `taffy` imports from public-facing code and examples once the re-exports
       compile.
5. [x] Use `ruskel canopy` to confirm no `taffy` types remain in the public API surface.
6. [x] Run the full test and lint suite for this stage and resolve any failures or warnings.

2. Stage Two: Lifecycle initialization hook

Introduce `on_mount` and ensure it is called exactly once per node after context binding.

7. [x] Add `Widget::on_mount` with a default `Ok(())` implementation in
       `crates/canopy/src/widget/mod.rs`.
8. [x] Call `on_mount` from the mount path in `crates/canopy/src/core/world.rs` and plumb
       failures through existing `Result` handling.
9. [x] Move example initialization from `ensure_tree`-style guards to `on_mount` in
       `crates/examples/src/focusgym.rs`.
10. [x] Add or update tests to prove `on_mount` is called once and after context binding.
11. [x] Run the full test and lint suite for this stage and resolve any failures or warnings.

3. Stage Three: Typed widget access and style read access

Reduce downcast boilerplate and make `Style` the source of truth for layout values.

12. [x] Add `Context::with_widget` and `Context::try_with_widget` in
        `crates/canopy/src/core/context.rs` and implement them in the core context.
13. [x] Add `ViewContext::style` returning a cloned `Style` based on cached node style in
        `crates/canopy/src/core/node.rs`.
14. [x] Replace downcast boilerplate in examples/tests with typed accessors where applicable.
15. [x] Remove redundant widget fields that merely mirror `Style` in example code.
16. [x] Run the full test and lint suite for this stage and resolve any failures or warnings.

4. Stage Four: Mount helpers, focus utilities, and builder exposure

Add child-mount helpers, focus traversal utilities, and a `NodeBuilder` entry point in `Context`.

17. [x] Add `Context::add_child` and `Context::add_children` plus implementations in
        `crates/canopy/src/core/world.rs` or related context types.
18. [x] Add `ViewContext::focused_leaf`, `ViewContext::focusable_leaves`, and
        `Context::suggest_focus_after_remove` with shared traversal helpers.
19. [x] Expose `Context::build` to return `NodeBuilder` and add any agreed helpers such as
        `flex_item` or `fill` in `crates/canopy/src/core/builder.rs`.
20. [x] Update focus-heavy examples to use the new utilities and builder chain where helpful.
21. [x] Run the full test and lint suite for this stage and resolve any failures or warnings.

5. Stage Five: Optional typed IDs and command bindings

Implement optional type-safe ergonomics for node IDs and command bindings.

22. [ ] Add `TypedId<T>` and `Context::add_typed` in canopy core, plus example coverage.
23. [ ] Extend `derive_commands` or `Binder` to support typed command references in
        `crates/canopy-derive/src/lib.rs` and `crates/canopy/src/core/binder.rs`.
24. [ ] Update one or two examples to demonstrate typed command bindings without removing
        string-based bindings.
25. [ ] Run the full test and lint suite for this stage and resolve any failures or warnings.

6. Stage Six: Documentation and final validation

Document the new APIs and validate the repository end-to-end.

26. [ ] Update README/docs or crate-level docs to mention `canopy::layout`, `on_mount`, and the
        new context helpers.
27. [ ] Run clippy, formatting, and the full test suite; fix any remaining warnings or failures.
