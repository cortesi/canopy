# Style layer scoping plan

Fix the style-layer leak caused by `StyleManager::pop`, then add a scoped helper for layer usage
so widget code makes layer intent explicit. This plan covers correctness, ergonomics follow-up, and
validation.

1. Stage One: Confirm behavior and lock in a regression test

Confirm the current render traversal and add a focused unit test that fails on the leak.

1. [x] Review `StyleManager::pop` in `crates/canopy/src/core/style/mod.rs` and the render traversal
       in `crates/canopy/src/core/canopy.rs` to confirm level/layer semantics.
2. [x] Add `pop_pops_all_layers_at_level` in `crates/canopy/src/core/style/mod.rs` to verify that
       multiple layers pushed at the same level are all removed on `pop`.

2. Stage Two: Correctness fix in `StyleManager`

Make `pop` remove every layer that was pushed at the current render level.

1. [x] Update `StyleManager::pop` in `crates/canopy/src/core/style/mod.rs` to loop while
       `layer_levels.last() == Some(&self.level)` before decrementing `level`.
2. [x] Run the new unit test locally to confirm the regression is fixed.

3. Stage Three: Ergonomics decision - keep the API minimal

Avoid adding new render APIs when core traversal guarantees are sufficient.

1. [x] Remove `Render::with_layer` from `crates/canopy/src/core/render.rs`.
2. [x] Revert call sites back to `push_layer` in `crates/canopy-widgets/src/button.rs`,
       `crates/canopy-widgets/src/inspector/logs.rs`, `crates/canopy-widgets/src/inspector/mod.rs`,
       `crates/examples/src/termgym.rs`, `crates/examples/src/listgym.rs`,
       `crates/examples/src/intervals.rs`, and `examples/todo/src/lib.rs`.

4. Stage Four: Validation and formatting

Run the standard project checks after the code changes.

1. [x] `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests --examples
       2>&1`
2. [x] `cargo nextest run --all --all-features` (or `cargo test --all --all-features` if needed).
3. [x] `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`.
