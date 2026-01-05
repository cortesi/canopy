# Text tab expansion plan

Add tab expansion to the `Text` widget with a default tab stop of 4, share the expansion helper
with `Input`, and update an example to exercise tab rendering. Validate with the standard project
checks.

1. Stage One: Scope and confirm current behavior

Review the current text rendering and tab handling paths to ground the changes.

1. [x] Inspect `crates/canopy/src/core/text.rs` and `crates/canopy-widgets/src/text.rs` to confirm
       where wrapping/slicing occurs and how cache invalidation works.
2. [x] Inspect `crates/canopy-widgets/src/input.rs` to confirm current tab expansion behavior and
       decide how to share the helper.
3. [x] Identify the example updates needed for a multi-Text demo, renaming `test_text` to
       `textgym` and exercising all `Text` variants.

2. Stage Two: Core tab expansion helper

Introduce a shared tab expansion helper in core text utilities.

1. [x] Add `text::expand_tabs` (and any small helper it needs) in
       `crates/canopy/src/core/text.rs`, using grapheme-aware widths and resetting column on
       newlines.
2. [x] Add unit tests in `crates/canopy/src/core/text.rs` covering:
       - default tab stop behavior (4 columns),
       - newline column reset,
       - wide grapheme alignment.

3. Stage Three: Text widget support

Add `tab_stop` to `Text` with a default of 4 and expand tabs before wrap/slice/measure.

1. [x] Add `tab_stop: usize` to `Text` in `crates/canopy-widgets/src/text.rs`, defaulting to 4, and
       add `with_tab_stop` (clamped to at least 1) that invalidates the wrap cache.
2. [x] Update `with_wrap_cache` to expand tabs via `text::expand_tabs` before wrapping, and ensure
       `max_width` reflects expanded lines.
3. [x] Update `measure` to compute raw width using expanded text.

4. Stage Four: Share helper with Input

Use the new helper to avoid duplicate tab expansion logic.

1. [x] Replace the local `expand_tabs` in `crates/canopy-widgets/src/input.rs` with
       `canopy::text::expand_tabs`, removing the duplicated helper and unused imports.

5. Stage Five: Example coverage and rename

Create a demo app that showcases multiple isolated `Text` instances and all `Text` variants.

1. [x] Rename `crates/examples/src/test_text.rs` to `crates/examples/src/textgym.rs`, updating the
       example launcher in `crates/examples/examples`, module references in
       `crates/examples/src/lib.rs`, and the `crates/examples/Cargo.toml` entry.
2. [x] Update the demo app to render multiple `Text` widgets in distinct sections, covering:
       default settings, `with_wrap_width`, `with_canvas_width` (view/intrinsic/fixed),
       `with_style`, `with_selected_style`, selection state, and `with_tab_stop` with default 4.
3. [x] Include tabbed lines in at least one `Text` instance so the default 4-space tab stop is
       exercised visually.
4. [x] Constrain the frame widths/heights in `textgym` so layout behavior is exercised.
5. [x] Remove duplicated titles from `textgym` content, leaving titles in the frames only.
6. [x] Add a `Pad` widget and wrap each text frame to add outer padding around the panes.
7. [x] Remove the extra interior padding in `textgym`, relying on the frame default inset.
8. [x] Use `Pad` in `cedit` to add outer spacing around the editor frame.

6. Stage Six: Validation and formatting

Run the standard project checks after the code changes.

1. [x] `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests --examples
       2>&1`
2. [x] `cargo nextest run --all --all-features` (or `cargo test --all --all-features` if needed).
3. [x] `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`.
