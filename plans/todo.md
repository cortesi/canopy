# Correctness Fix Plan

This plan addresses the correctness issues validated from `plan/feedback.md` and
leaves larger architectural changes for later.

1. Stage One: Error visibility and terminal correctness

Improve error displays, raw mode handling, and cursor visibility.

1. [x] Update `crates/canopy/src/core/error.rs` to include payloads in `#[error]` strings,
       add `#[source]` for wrapped errors where appropriate, and adjust callers if needed.
2. [x] Fix `CrosstermControl::enter` in `crates/canopy/src/core/backend/crossterm.rs` to
       leave raw mode enabled and ensure `exit` remains the sole place to disable it.
3. [x] Add a `TermBuf` helper to overlay a cursor cell and use it from
       `crates/canopy/src/core/canopy.rs` `post_render` to draw the focused cursor (virtual
       cursor only).

2. Stage Two: Command derive dispatch safety

Make `#[derive_commands]` robust against missing args and ignore-result behavior.

1. [x] In `crates/canopy-derive/src/lib.rs`, keep `Return::result` true for `Result` returns
       even with `ignore_result`, but coerce successful values to `ReturnValue::Void`.
2. [x] In `crates/canopy-derive/src/lib.rs`, error on unsupported reference args instead of
       silently skipping them, with a clear diagnostic message.
3. [x] In `crates/canopy-derive/src/lib.rs`, replace `cmd.args[i]` with `.get(i)` and return
       `Error::Invalid` when args are missing/wrong types.
4. [x] Add/adjust derive macro tests to cover `ignore_result + Result`, missing args, and
       reference-arg rejection.

3. Stage Three: ScriptHost error handling

Remove `unwrap`/`assert` from scripting and surface recoverable errors.

1. [x] In `crates/canopy/src/core/script.rs`, replace `unwrap` and `assert` with `Result`
       paths that include script id, node id, and command name; include line/offset when
       available from Rhai errors.
2. [x] In `compile`, preserve Rhai parse error details (including location data if
       available) instead of mapping to an empty `ParseError`.
3. [x] Add tests that exercise script compile and runtime failures without panicking.

4. Stage Four: Mouse event coordinate semantics

Align `MouseEvent` meaning with implementation.

1. [x] Keep `MouseEvent.location` as local coordinates; update docs in
       `crates/canopy/src/core/event/mouse.rs` accordingly.
2. [x] Update `Canopy::mouse` in `crates/canopy/src/core/canopy.rs` to make local semantics
       explicit and consistent.
3. [x] Add helper mapping functions or docs if screen-to-local conversion needs to be
       discoverable for widget authors.

5. Stage Five: Unicode-safe input editing

Make `TextBuf` operate on grapheme or scalar boundaries.

1. [x] Add `unicode-segmentation` and `unicode-width` via `cargo add`, and rework `TextBuf`
       in `crates/canopy/src/widgets/input.rs` to track cursor positions by grapheme
       cluster for better UX.
2. [x] Update cursor movement and windowing logic to use the new indexing model.
3. [x] Add tests covering non-ASCII input insert, delete, and cursor movement.

6. Stage Six: Hidden node cleanup

Remove or implement the feature to avoid dead state.

1. [x] Either add `Context::set_hidden` / `show` / `hide` and wire them through
       `crates/canopy/src/core/context.rs` + `core/world.rs`, or remove `hidden` and its
       checks from `core/node.rs`, `core/canopy.rs`, and `core/world.rs`.
2. [x] Update docs/tests for the chosen path.

7. Stage Seven: Quality gates

Run lint, format, and tests after the changes.

1. [x] `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests --examples`
2. [x] `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml` (or fallback).
3. [x] `cargo nextest run --all --all-features` (or fallback to `cargo test`).
