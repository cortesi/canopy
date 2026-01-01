# Extract canopy-widgets Crate

Separate the widget implementations from the core canopy crate into a new `canopy-widgets` crate.
This allows users to use canopy's core infrastructure without pulling in the built-in widgets, and
keeps the widget implementations in a focused location.

## 1. Stage One: Create canopy-widgets crate structure

Set up the new crate with proper Cargo.toml and initial module structure.

1. [x] Create `crates/canopy-widgets/` directory
2. [x] Create `crates/canopy-widgets/Cargo.toml` with dependencies:
       - canopy (path = "../canopy") for core types
       - canopy-derive for macros
       - Widget-specific deps: textwrap, pad, syntect, ropey, alacritty_terminal, portable-pty,
         image, unicode-width, unicode-segmentation
3. [x] Create `crates/canopy-widgets/src/lib.rs` with module declarations and re-exports

## 2. Stage Two: Move widget files

Move all widget implementations from canopy to canopy-widgets.

1. [x] Move `crates/canopy/src/widgets/*.rs` to `crates/canopy-widgets/src/` (all widget files:
       boxed, button, center, dropdown, frame, image_view, input, list, modal, panes, root,
       selector, tabs, terminal, text, vstack)
2. [x] Move `crates/canopy/src/widgets/editor/` directory to `crates/canopy-widgets/src/editor/`
3. [x] Move `crates/canopy/src/widgets/inspector/` directory to `crates/canopy-widgets/src/inspector/`
4. [x] Remove the now-empty `crates/canopy/src/widgets/` directory and its mod.rs

## 3. Stage Three: Update imports in canopy-widgets

Fix all imports in the moved widget files to use canopy's public API.

1. [x] Update all `crate::` imports to `canopy::` in widget files
2. [x] Update editor widget imports
3. [x] Ensure canopy-widgets/src/lib.rs properly re-exports all public widget types
4. [x] Verify all widget dependencies are correctly referenced

## 4. Stage Four: Update canopy crate

Remove the widgets module from canopy.

1. [x] Remove `pub mod widgets;` from `crates/canopy/src/lib.rs`
2. [x] Add public APIs for testing: `compile_script()` and make `render()` public

## 5. Stage Five: Update examples crate

Update the examples to use the new canopy-widgets crate.

1. [x] Add `canopy-widgets` dependency to `crates/examples/Cargo.toml`
2. [x] Update imports in all example source files to use `canopy_widgets::` instead of
       `canopy::widgets::`
3. [x] Update test files in `crates/examples/src/tests/` similarly
4. [x] Update `examples/todo` to use canopy-widgets

## 6. Stage Six: Consolidate editor module

Move the editor primitives (TextBuffer, TextPosition, Selection, etc.) from canopy core into
canopy-widgets, making them internal to the editor widget.

1. [x] Move `crates/canopy/src/editor/` files to `crates/canopy-widgets/src/editor/`:
       - buffer.rs (TextBuffer, LineChange)
       - edit.rs (Edit, Transaction)
       - position.rs (TextPosition, TextRange)
       - selection.rs (Selection)
       - util.rs (tab_width)
2. [x] Update canopy-widgets editor mod.rs to include the new modules
3. [x] Update all imports in canopy-widgets to use local editor module
4. [x] Remove `pub mod editor;` from canopy's lib.rs
5. [x] Delete `crates/canopy/src/editor/` directory
6. [x] Add proptest as dev-dependency for buffer tests

## 7. Stage Seven: Verify and cleanup

Ensure everything compiles and works correctly.

1. [x] Run `cargo clippy --fix` and address all warnings
2. [x] Run `cargo +nightly fmt --all`
3. [x] Run `cargo nextest run --all --all-features` and verify all tests pass (300 tests passed)
