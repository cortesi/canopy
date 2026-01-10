# Design Feedback Execution Plan

This plan verifies each feedback item in the current codebase and converts it into a staged
execution checklist. Update the checklist as work proceeds.

1. Phase 1: Immediate low-hanging fruit (high ROI, low risk)

1. [x] Add a WidgetSlotGuard RAII restore in `crates/canopy/src/core/world.rs` and refactor
    `with_widget_mut`/`with_widget_view` away from `take()` + restore (currently uses
    `take()` + `expect` and an unsafe shared core pointer in `world.rs`).
2. [x] Stop cloning the previous `TermBuf` on every render in `crates/canopy/src/core/canopy.rs`
    (current flow clones `prev`, copies `next`, diffs, then stores the clone).
3. [x] Switch poll scheduling to `Instant` in `crates/canopy/src/core/poll.rs` and update the
    `PendingHeap` tests (currently uses `SystemTime`).
4. [x] Reject duplicate children in `Core::set_children` in `crates/canopy/src/core/world.rs`
    (no duplicate check exists today; add an error variant).
5. [x] Make `Core::set_widget` fallible and replace it with explicit APIs for
    `replace_widget_keep_children` and `replace_subtree`, remove `set_widget`, and update call
    sites/tests (currently `set_widget` panics with `expect("Unknown node id")`).
6. [x] Return `TypedId<W>` from `Context::add_child*`, `add_child*_keyed`, `add_keyed*`, and
    `create_detached`, and typed `add_children*` helpers in `crates/canopy/src/core/context.rs`,
    then update call sites in `crates/canopy-widgets` (e.g., `List` uses
    `TypedId::new(ctx.add_child(..))`).
7. [x] Update `docs/src/commands.md` to the current command architecture
    (`CommandSpec`/`CommandSet`, `derive_commands`, `cmd_*().call()`, injections), replacing the
    outdated `dispatch` description.
8. [x] Fill doc gaps: add an end-to-end page (widget + command + binding + style layer), add a
    patterns page (keyed children, slots, reconciliation), and replace placeholders in
    `docs/src/styling.md` and `docs/src/bindings.md`; update `docs/src/SUMMARY.md`.
9. [x] Run `ruskel` for the modified APIs (`canopy::core::context`, `canopy::core::world`) and
    confirm the surface matches the intended intent.
10. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests \
    --examples 2>&1` and resolve any warnings.
11. [x] Run tests via `cargo nextest run --all --all-features` (fallback: `cargo test`).
12. [x] Format with `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml` (fallback:
    `cargo +nightly fmt --all`).
13. [ ] Review checkpoint: share the diff for approval before starting Phase 2.

2. Phase 2: Ergonomics multipliers

1. [ ] Add a `Slot<K: ChildKey>` helper in `canopy` core and refactor
    `crates/canopy-widgets/src/button.rs` to use it instead of
    `try_with_unique_descendant::<Text>`.
2. [ ] Introduce a keyed reconciliation helper (e.g., `KeyedChildren<K>`) in `canopy` core and
    refactor `crates/canopy-widgets/src/list.rs` to use it instead of manual `items` management
    and `set_children` syncing.
3. [ ] Add `BindingId` (monotonic `u64`) + `unbind -> bool` + binding introspection to
    `InputMap`/`Canopy`/`Binder`, and update examples/docs to prefer typed command bindings
    (`bind_key_command('q', "", Root::cmd_quit().call())`).
4. [ ] Run `ruskel` for the modified APIs (`canopy::core::context`, `canopy::core::inputmap`,
    `canopy-widgets::list`, `canopy-widgets::button`).
5. [ ] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests \
    --examples 2>&1` and resolve any warnings.
6. [ ] Run tests via `cargo nextest run --all --all-features` (fallback: `cargo test`).
7. [ ] Format with `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml` (fallback:
    `cargo +nightly fmt --all`).
8. [ ] Review checkpoint: share the diff for approval before starting Phase 3.

3. Phase 3: Larger design unlocks and refactors

1. [ ] Replace internal `process::exit` usage with cooperative shutdown: add an exit request flag
    on `Canopy`/`Core`, change `Context::exit` and `BackendControl::exit` signatures to return
    `()`, and make the runloop return an exit code (current exits in
    `crates/canopy/src/core/backend/mod.rs`, `core/context.rs`, and crossterm backend).
2. [ ] Make `Widget::on_event` fallible (`Result<EventOutcome>`) in
    `crates/canopy/src/widget/mod.rs`, bubble errors through core dispatch (`core/world.rs`) and
    public event entry points (`core/canopy.rs`), returning errors to the caller immediately.
3. [ ] Remove panicking command-arg conversions in `crates/canopy/src/core/commands.rs` (u64/usize
    `expect` and `Serialize` blanket `expect`) by introducing a fallible `SerdeArg` wrapper and
    updating call builders/tests.
4. [ ] Replace regex-based path matching in `crates/canopy/src/core/path.rs` with a custom
    component-glob matcher (`*` and `**`) plus explicit match scoring; update matcher tests
    accordingly.
5. [ ] Optional refactor (if approved, otherwise defer): move widget storage into a separate arena
    (node stores a widget key) to eliminate `unsafe` in `with_widget_view` and reduce borrow
    workarounds.
6. [ ] Run `ruskel` for the modified APIs (`canopy::widget`, `canopy::core::world`,
    `canopy::core::commands`, `canopy::core::path`).
7. [ ] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests \
    --examples 2>&1` and resolve any warnings.
8. [ ] Run tests via `cargo nextest run --all --all-features` (fallback: `cargo test`).
9. [ ] Format with `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml` (fallback:
    `cargo +nightly fmt --all`).
10. [ ] Review checkpoint: share the diff for approval before starting Phase 4.

4. Phase 4: Testing + diagnostics hardening

1. [ ] Add snapshot tests for core widgets (`Frame`, `List`, `Text`, `Button`) using the existing
    harness (`crates/canopy/src/core/testing`) and record stable snapshots in
    `./tests/snapshots`.
2. [ ] Add property tests for the new path matcher and `TermBuf::diff` invariants (idempotence,
    no-ops on identical buffers) using `proptest` (already in dev-deps).
3. [ ] Add `debug_assert!` invariants in core mutation points (no duplicate children, parent
    pointers consistent, `child_keys` subset of `children`, focus attached or `None`).
4. [ ] Add a diagnostic inspector command to dump tree, focus path, and active bindings for the
    current node (ties into the existing command system).
5. [ ] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests \
    --examples 2>&1` and resolve any warnings.
6. [ ] Run tests via `cargo nextest run --all --all-features` (fallback: `cargo test`).
7. [ ] Format with `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml` (fallback:
    `cargo +nightly fmt --all`).
8. [ ] Review checkpoint: share the diff for approval before committing.

# Appendix: Consolidated Implementation Guidance (by Phase)

The notes below rationalize the feedback into concrete implementation sketches that align with the
checklist above. Each item maps to a checklist line so it can be applied mechanically without
re-reading the original review.

1. Phase 1: Immediate low-hanging fruit

1. Panic-safe widget access guard

   - Current `Core::with_widget_mut` and `with_widget_view` in
     `crates/canopy/src/core/world.rs` use `take()` and restore, which leaves a `None` if user code
     panics.
   - Add a small RAII guard that always restores the widget slot on drop.

   ```rust
   struct WidgetSlotGuard<'a> {
       slot: &'a mut Option<Box<dyn Widget>>,
       widget: Option<Box<dyn Widget>>,
   }

   impl<'a> WidgetSlotGuard<'a> {
       fn new(slot: &'a mut Option<Box<dyn Widget>>) -> Self {
           Self { widget: slot.take(), slot }
       }

       fn widget_mut(&mut self) -> &mut dyn Widget {
           self.widget
               .as_deref_mut()
               .expect("Widget missing from node")
       }
   }

   impl Drop for WidgetSlotGuard<'_> {
       fn drop(&mut self) {
           if self.slot.is_none() {
               *self.slot = self.widget.take();
           }
       }
   }
   ```

   - Refactor `with_widget_mut` to use the guard and avoid manual restoration.
   - `with_widget_view` can also use the guard; the `unsafe` shared core pointer remains until the
     widget arena split is done.

2. Stop cloning `TermBuf` every frame

   - In `Canopy::render` (`crates/canopy/src/core/canopy.rs`), replace:

   ```rust
   let mut screen_buf = prev.clone();
   screen_buf.copy(&next, root_size.rect());
   screen_buf.diff(prev, be)?;
   self.termbuf = Some(screen_buf);
   ```

   - With a direct diff against `prev` and store `next`:

   ```rust
   if let Some(prev) = &self.termbuf {
       next.diff(prev, be)?;
       self.termbuf = Some(next);
   } else {
       next.render(be)?;
       self.termbuf = Some(next);
   }
   ```

   - Optional follow-up: keep a reusable scratch buffer to avoid allocations.

3. Poller should use `Instant`

   - Replace `SystemTime` in `crates/canopy/src/core/poll.rs` with `Instant`.
   - Update `PendingHeap::_add`, `_current_wait`, and `_collect` to use `Instant::now()` and
     `duration_since` without fallible error handling.
   - Update tests in the same file accordingly.

4. Duplicate child rejection in `set_children`

   - Add a duplicate check at the start of `Core::set_children` in
     `crates/canopy/src/core/world.rs`.
   - Prefer a new `Error::DuplicateChild { parent, child }` variant if acceptable, otherwise use an
     `Error::Internal` message.

5. Replace `set_widget` with explicit APIs

   - `Core::set_widget` currently panics on unknown node id.
   - Replace with two explicit APIs:

     - `replace_widget_keep_children(node, widget) -> Result<()>`
     - `replace_subtree(node, widget) -> Result<()>` (remove/detach descendants first)

   - Remove `set_widget` (prefer explicit intent) and update call sites/tests.

6. Typed IDs as default return type

   - In `crates/canopy/src/core/context.rs`, return `TypedId<W>` from:
     `add_child`, `add_child_to`, `add_child_keyed`, `add_child_to_keyed`, `add_keyed`,
     `add_keyed_to`, `create_detached`, and typed `add_children*` helpers.
   - Update call sites to drop `TypedId::new(...)` wrapper in widgets (notably
     `crates/canopy-widgets/src/list.rs`).

7. Commands doc refresh

   - Update `docs/src/commands.md` to match the current command architecture:
     `CommandSpec`/`CommandSet`, `derive_commands`, `cmd_*().call()`, and injections such as
     `Injected<T>`, `Event`, `MouseEvent`, `ListRowContext`.

8. Fill docs gaps and add a Patterns page

   - Replace placeholder docs in `docs/src/styling.md` and `docs/src/bindings.md`.
   - Add a real end-to-end example doc page and a Patterns page (slots, keyed children,
     reconciliation, focus conventions). Update `docs/src/SUMMARY.md` to include new pages.

9. Ruskel review

   - Use `ruskel` to validate API surface for modified modules. Confirm naming and intent.

2. Phase 2: Ergonomics multipliers

1. Slot helper for keyed children

   - Add `Slot<K: ChildKey>` helper in `canopy` core with typed caching and
     `get_or_create`/`with` helpers.
   - Refactor `crates/canopy-widgets/src/button.rs` to use `Slot<LabelSlot>` instead of
     `try_with_unique_descendant::<Text>`.

   ```rust
   pub struct Slot<K: ChildKey> {
       id: Option<TypedId<K::Widget>>,
       _phantom: PhantomData<K>,
   }

   impl<K: ChildKey> Slot<K> {
       pub fn get_or_create(
           &mut self,
           ctx: &mut dyn Context,
           make: impl FnOnce() -> K::Widget,
       ) -> Result<TypedId<K::Widget>> { /* ... */ }

       pub fn with<R>(
           &mut self,
           ctx: &mut dyn Context,
           f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
       ) -> Result<R> { /* ... */ }
   }
   ```

2. Keyed reconciliation helper

   - Extract the list reconciliation logic into a helper (e.g., `KeyedChildren<K>`) in
     `canopy` core that keeps a `HashMap<K, NodeId>` and `Vec<K>` for ordering.
   - `List` should use this to build and update rows, then call `ctx.set_children(...)` once.

3. Bindings: IDs, unbind, introspection

   - Add `BindingId` as a monotonic `u64` and store bindings in a map keyed by id.
   - Track a secondary index by `(mode, input)` for fast resolution.
   - Add `unbind(id) -> bool` and `bindings_for(mode, path) -> Vec<Binding>`.
   - Update examples to prefer typed command bindings.

3. Phase 3: Larger design unlocks and refactors

1. Cooperative shutdown

   - Replace internal `process::exit` usage with a cooperative `exit_requested: Option<i32>` on
     `Canopy` or `Core`.
   - Change `Context::exit`/`BackendControl::exit` to return `()` and set the exit request.
   - Make the runloop return `Result<i32>` when exit is requested.

2. Fallible `on_event`

   - Change `Widget::on_event` to return `Result<EventOutcome>`.
   - Update dispatch in `crates/canopy/src/core/world.rs` to bubble errors out.
   - Adjust `Canopy::key`/`Canopy::mouse` to propagate the error immediately.

3. Command arg conversion de-panicking

   - Remove panicking `u64`/`usize` conversions and `Serialize` blanket impl.
   - Add a fallible `SerdeArg<T>` wrapper.
   - Update call builders and tests accordingly. Consider adding `ArgValue::UInt(u64)` if needed.

4. Path matcher refactor

   - Replace regex matching in `crates/canopy/src/core/path.rs` with a custom component-glob
     matcher: `*` (one component), `**` (zero or more components), anchored start/end.
   - Implement explicit match scoring (literals, depth, anchored end).
   - Update matcher tests to cover the new semantics.

5. Optional: widget storage arena

   - Split widgets into a dedicated `SlotMap<WidgetId, Box<dyn Widget>>` and store `widget_id` in
     nodes. This removes `unsafe` from `with_widget_view` and reduces borrow workarounds. Defer
     unless borrow pain shows up elsewhere.

4. Phase 4: Testing + diagnostics hardening

1. Snapshot tests

   - Use existing harness (`crates/canopy/src/core/testing`) to snapshot `Frame`, `List`, `Text`,
     `Button` output.
   - Store snapshot files in `./tests/snapshots`.

2. Property tests

   - Use `proptest` for path matcher invariants and `TermBuf::diff` idempotence.

3. Core invariants

   - Add `debug_assert!` checks at mutation points: no duplicate children, parent pointers
     consistent, child_keys subset of children, focus attached or `None`.

4. Diagnostics

   - Add an inspector command that dumps tree + focus path + active bindings.
