# Deslop Refactoring Plan

This plan captures the highest-value Rust cleanup work from the workspace deslop review. The
goal is to reduce panic-prone APIs, tighten unsafe and public boundaries, remove hot-path
allocation pressure, and simplify stateful widget internals without mixing unrelated changes.

1. Stage One: Repair Error And Safety Boundaries

Start with the changes that improve library correctness and make later refactors safer.

1. [ ] Replace the public `Color::rgb(&str)` panic path in
       `crates/canopy/src/core/style/color.rs` with a fallible parser
       (`try_rgb` or `TryFrom<&str>`), then migrate internal literal call sites to the `rgb!`
       macro where compile-time validation is sufficient.
2. [ ] Refactor `crates/canopy/src/core/script.rs` so script execution no longer depends on a
       thread-local raw `*mut Core` pointer spread across dispatch helpers; concentrate the unsafe
       boundary into a small scoped abstraction with explicit invariants, or remove it entirely.

2. Stage Two: Reduce API Surface Area

Collapse duplicate setup APIs so the external interface is smaller and the internal structure is
less exposed.

1. [ ] Introduce focused `Canopy` helpers for common app installation and style setup flows so
       examples and widgets no longer need to reach directly into `cnpy.core` and `cnpy.style`
       for routine setup.
2. [ ] Simplify the binding API around the existing generic `Canopy::bind(...)` entry point and a
       narrower `Binder` facade, removing the current key/mouse, script/command,
       fallible/panicking, and ID-returning wrapper matrix where it does not add unique value.

3. Stage Three: Remove Editor Hot-Path Allocations

Tend the editor buffer API so movement, layout, search, and rendering borrow from `ropey` where
possible instead of cloning whole lines into temporary `String`s.

1. [ ] Add borrowed line and range access helpers in
       `crates/canopy-widgets/src/editor/buffer.rs`, keeping owned-string APIs only where callers
       genuinely need ownership.
2. [ ] Update the editor hot paths in `buffer.rs`, `layout.rs`, `search.rs`, and `widget.rs` to
       use borrowed helpers for grapheme traversal, display-column mapping, layout rebuilds,
       rendering, and search.

4. Stage Four: Simplify Stateful Widget Internals And Validate

Finish by reducing internal bookkeeping complexity, then run the full repository validation
sequence once the staged refactors have landed.

1. [ ] Replace the parallel `columns` and `column_nodes` bookkeeping in
       `crates/canopy-widgets/src/panes.rs` with a single source of truth or a dedicated column
       state type so insert/delete/focus logic no longer depends on manual resynchronization.
2. [ ] Run the required validation and cleanup steps for the final patch set:
       `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests --examples`,
       `cargo nextest run --all --all-features` (or `cargo test --all --all-features` if
       `nextest` is unavailable), and `cargo +nightly fmt --all` with
       `--config-path ./rustfmt-nightly.toml` if available.
