# Mouse focus latency: spec and implementation plan

This plan covers three complementary options to reduce mouse‑driven focus latency. The spec
sections describe intended behavior and trade‑offs; the staged checklist proposes incremental,
contained changes so we can evaluate impact after each option.

## Spec

### Option 1: Coalesce mouse‑move events

Goal: prevent `Mouse::Moved` storms from delaying click handling.

Design:
- Coalesce consecutive `Mouse::Moved` events so only the most recent move is processed.
- Preserve ordering for non‑move events; never drop `Down`, `Up`, `Drag`, or scroll events.
- Keep coalescing localized to the event source / runloop so widgets remain unchanged.

Trade‑offs:
- Some widgets (e.g., terminal mouse reporting) may expect full move fidelity. Coalescing will
  reduce the frequency of move events. We should decide whether to gate coalescing when a widget
  needs full fidelity or accept the reduction as a default behavior.

### Option 2: Centralized render‑dirty gating

Goal: avoid a full render when an event is ignored and no state changes occurred, especially for
mouse‑move noise.

Design:
- Add a centralized `render_pending` flag in `Canopy` (or `Core`) that defaults to true on first
  frame and is set when an event is *handled* or a command/script executes.
- In the runloop, skip `cnpy.render` when `render_pending` is false.
- Mark `render_pending = true` in a small number of centralized places:
  - after `EventOutcome::Handle` / `Consume` in `Canopy::mouse` / `Canopy::key`
  - after a binding dispatch (script or command)
  - on `Event::Resize`, `Event::Poll`, `Event::Paste`, `Event::FocusGained`, `Event::FocusLost`
  - when style or layout mutations are applied (existing code paths already centralized)

Intrusiveness:
- This can be centralized in `Canopy` and the event loop without adding “render taint” calls
  throughout widgets. No widget changes required as long as they return `Handle` when mutating.
- Risk: if a widget mutates state but returns `Ignore`, its changes may not render. That is already
  a correctness bug; this change will make it visible.

### Option 3: Cache editor highlight spans

Goal: reduce the per‑frame cost of syntax highlighting during repeated renders (especially when
content is unchanged).

Design:
- Cache highlight spans per line keyed by `(buffer revision, line index)`.
- If the buffer revision changes, clear the cache (simple, safe first pass).
- Rendering uses cached spans when available; recompute only when cache miss.

Trade‑offs:
- A revision‑level cache is coarse (clears all lines on any edit) but still removes highlight
  recomputation for focus or mouse‑move renders.
- Future refinement: invalidate only changed lines using `LineChange` if we need finer granularity.

---

1. Stage One: Mouse‑move coalescing

Implement coalescing in the event source or runloop so only the last consecutive move is handled.

1. [x] Add a coalescing `EventSource` wrapper in `core/backend/crossterm.rs` that collapses
   consecutive `Event::Mouse(Action::Moved)` into the most recent move.
2. [x] Ensure non‑move events are never dropped or reordered, and confirm drag/scroll are not
   coalesced.
3. [x] Add a brief doc note in the backend module describing the coalescing behavior and its
   rationale.

2. Stage Two: Centralized render‑dirty gating

Add a centralized `render_pending` flag to avoid rendering when events are ignored.

1. [x] Introduce `render_pending: bool` in `Canopy` and initialize it to `true` before the first
   render.
2. [x] Set `render_pending = true` in `Canopy::mouse` / `Canopy::key` when an event is handled or a
   binding executes, and for non‑input events that can mutate state.
3. [x] Update the crossterm runloop to call `cnpy.render` only when `render_pending` is true, and
   reset the flag after a successful render.
4. [x] Add a short test in `core/canopy.rs` to confirm ignored mouse moves do not trigger renders.

3. Stage Three: Editor highlight caching

Cache highlight spans per line to reduce render time when content is unchanged.

1. [x] Add a highlight‑span cache to `widgets/editor/widget.rs` keyed by buffer revision + line
   index.
2. [x] Clear the cache when the buffer revision changes and use cached spans in `render_line`.
3. [x] Add a unit test in `widgets/editor/tests.rs` ensuring cached spans are reused when the text
   and revision are unchanged.

4. Stage Four: Validation and latency checks

Run full lint/test/format and verify that mouse focus updates are immediate in editorgym.

1. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests --examples 2>&1`.
2. [x] Run `cargo nextest run --all --all-features` (or `cargo test --all --all-features`).
3. [x] Run `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`.
4. [ ] Manually verify focus latency in `editorgym` after sustained mouse movement.
