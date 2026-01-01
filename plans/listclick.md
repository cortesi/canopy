# List Click Handling with Built-in Hit Testing, Selection, Activation, Focus, and Capture

## 1. Scope

This specification defines list-native click handling to make click-to-select and optional activation ergonomic and consistent, eliminating manual hit-testing in callers. It includes:

* Rich list hit testing APIs (`hit_test`, `index_at_point`, `index_of_node`)
* Default-on click selection in `List` with opt-out via event handling
* Optional activation hook that dispatches typed commands
* Event payload and row context availability to commands via `Context` scope
* Focus and mouse capture invariants for predictable behavior

This spec assumes the typed command system (Spec 1) is implemented first.

---

## 2. Terminology

* **List**: `List<W>` widget that lays out a scrollable vertical collection of row widgets `W`.
* **Row**: One item widget instance in the list, associated with a logical item index.
* **Selection**: The list’s selected logical index (single selection).
* **Activation**: An optional command dispatch tied to a row click gesture.
* **Hit testing**: Mapping a screen point to list regions and (if applicable) an item index.
* **Target widget**: The deepest widget under the pointer as determined by the event dispatcher, or the capture owner.

---

## 3. Event routing invariants

### 3.1 Bubble model

* Input events are dispatched to a **target node** first.
* If the target widget returns `EventOutcome::Handle`, propagation stops.
* If it returns `EventOutcome::Ignore`, the event bubbles to the parent node, continuing until handled or root.

This invariant is required to implement “default-on with opt-out” list click handling:

* Child widgets opt out by handling the click.
* If no child handles it, the list handles it.

### 3.2 Current event scope

During event dispatch, `ctx.current_event()` returns the active `Event` snapshot for the duration of the dispatch stack, including commands invoked during handling.

---

## 4. Focus invariants

* There is a single focused node per window.
* Widgets must only change focus when they **handle** an input event.
* When the list handles a row selection click, it must:

  1. Update selection.
  2. Focus the selected row (via `focus_selected(ctx)`).
  3. Ensure the selected row is visible (via `ensure_selected_visible(ctx)`).

If a child widget handles the click, the list must not change selection or focus.

---

## 5. Mouse capture invariants

### 5.1 Capture semantics

* Mouse capture is per window and per button.
* While captured, all mouse events for that button are retargeted to the capture owner node and bubble from there.
* Capture is released when:

  * The owning widget releases it explicitly, or
  * The corresponding `mouse::Action::Up` occurs, or
  * The capture owner is removed from the tree.

### 5.2 Required Context API

`Context` must expose:

```rust
trait Context {
    fn capture_mouse(&mut self, node: NodeId, button: mouse::Button) -> Result<()>;
    fn release_mouse(&mut self, button: mouse::Button);
    fn mouse_capture_owner(&self, button: mouse::Button) -> Option<NodeId>;
}
```

---

## 6. Hit testing API

### 6.1 `ListHit` result

`List` exposes a rich hit test result that distinguishes item hits from non-item regions.

```rust
pub struct ListHit {
    pub kind: ListHitKind,
    pub index: Option<usize>, // Some for Item, None otherwise
    pub row_local: Point,     // valid when kind == Item; otherwise unspecified
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ListHitKind {
    Item,
    Gap,        // between items (e.g., spacing)
    Padding,    // list interior padding but not on an item
    Scrollbar,  // scrollbar track/thumb region if present
    Outside,    // outside list bounds (only if hit_test is asked to classify)
}
```

### 6.2 Methods

```rust
impl<W> List<W> {
    /// Maps a screen-space point to a semantic hit result using current layout metrics.
    pub fn hit_test(&self, ctx: &dyn Context, p_screen: Point) -> ListHit;

    /// Convenience: returns Some(index) iff the point hits an item row.
    pub fn index_at_point(&self, ctx: &dyn Context, p_screen: Point) -> Option<usize> {
        match self.hit_test(ctx, p_screen) {
            ListHit { kind: ListHitKind::Item, index: Some(i), .. } => Some(i),
            _ => None,
        }
    }

    /// Maps a realized row node id to its current logical index.
    pub fn index_of_node(&self, node: NodeId) -> Option<usize>;
}
```

### 6.3 Metric requirements

`hit_test` must use list layout metrics (including scroll offset and per-row geometry). Fixed row height assumptions are not permitted.

Complexity:

* `index_at_point` and item-region mapping must be `O(log n)` or better for large lists (binary search over row boundaries or equivalent).

---

## 7. Row context availability

### 7.1 Row context type

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ListRowContext {
    pub list: NodeId,
    pub index: usize,
}
```

### 7.2 Context accessors

`Context` must expose:

```rust
trait Context {
    fn current_list_row(&self) -> Option<ListRowContext>;
}
```

### 7.3 Population rules

* For events whose target node is within a list row subtree, the event dispatcher must set `current_list_row` for the duration of dispatch.
* `current_list_row` is derived from list-maintained row metadata:

  * Each realized row root node must be annotated with its owning list node id and logical index.
  * The annotation must be updated when rows are re-ordered, inserted, removed, or when virtualization rebinds row nodes.

This enables item-level click handling without parent traversal or geometry math.

---

## 8. List click behavior

### 8.1 Default click-to-select (built-in)

`List<W>` handles click-to-select by default on primary button down, subject to event bubbling.

Normative behavior (inside `List<W>::on_event`):

* On `Event::Mouse(m)` where:

  * `m.button == mouse::Button::Left`
  * `m.action == mouse::Action::Down`
* The list performs:

  1. `hit = self.hit_test(ctx, m.location)`
  2. If `hit.kind != ListHitKind::Item`, return `EventOutcome::Ignore`
  3. `self.select(ctx, hit.index.unwrap())`
  4. `self.focus_selected(ctx)`
  5. `self.ensure_selected_visible(ctx)`
  6. Prepare activation tracking if configured (Section 8.2)
  7. Return `EventOutcome::Handle`

Because of the bubble model, this behavior runs only if no child widget handled the event first.

### 8.2 Optional activation hook

`List<W>` supports an optional activation command hook, dispatched on click release with a drag threshold.

#### 8.2.1 API

```rust
pub struct ListActivateConfig {
    pub command: &'static CommandSpec,
    pub drag_threshold_px: u32, // default: 4
}

impl<W> List<W> {
    pub fn with_on_activate(mut self, cfg: ListActivateConfig) -> Self;
    pub fn set_on_activate(&mut self, cfg: Option<ListActivateConfig>);
}
```

#### 8.2.2 Gesture rules

* Activation is evaluated on `mouse::Action::Up` for the left button.
* Activation requires:

  * The list previously handled the corresponding `Left Down` on an item row (i.e., it owns the gesture).
  * Pointer movement from down→up is within `drag_threshold_px`.
  * The up position hit-tests to the **same** item index as the down.

#### 8.2.3 Capture rules for activation

When the list handles `Left Down` on an item and activation is configured, it must:

* `ctx.capture_mouse(ctx.node_id(), mouse::Button::Left)` (or list node id as appropriate)
* Release capture on `Left Up` after activation evaluation.

This ensures the list receives the matching up event even if the pointer leaves the list while pressed.

#### 8.2.4 Dispatch rules

On activation:

* The list dispatches the configured command with the activated index as the first positional arg, using the existing index-as-`isize` convention:

```rust
let inv = cfg.command
    .call_with([index as isize])
    .invocation();
ctx.dispatch_command(&inv)?;
```

During this dispatch:

* `ctx.current_event()` is the `MouseUp` event.
* `ctx.current_list_row()` is `Some(ListRowContext { list: <list_id>, index })`.

### 8.3 Opt-out rules

A row widget (or any descendant) can prevent list selection/activation by returning `EventOutcome::Handle` for the relevant mouse event(s):

* Handle `Left Down` to prevent selection.
* Handle `Left Up` to prevent activation (if selection did not already capture the mouse, it won’t see the up).

If a child wants custom behavior but still wants default selection, it should dispatch its own commands and return `EventOutcome::Ignore` so the click bubbles to the list.

---

## 9. Public helper usage patterns

### 9.1 Parent widget does not manual hit-test

Typical parent code no longer computes indices from geometry.

```rust
fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
    // Parent no longer performs list hit-testing for selection.
    EventOutcome::Ignore
}
```

### 9.2 Item-level click handling without parent math

A row can dispatch using `current_list_row`:

```rust
impl Widget for TermEntry {
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if let Event::Mouse(m) = event
            && m.button == mouse::Button::Left
            && m.action == mouse::Action::Down
        {
            let Some(row) = ctx.current_list_row() else { return EventOutcome::Ignore; };

            let _ = ctx.dispatch_command(
                &TermGym::cmd_select_terminal()
                    .call_with([row.index as isize])
                    .invocation()
            );
            return EventOutcome::Handle; // prevents list selection if desired
        }
        EventOutcome::Ignore
    }
}
```

---

## 10. Compatibility and migration

* Existing manual hit-testing in termgym becomes redundant and must be removed.
* Existing command calls using `call_with([index as isize])` remain valid.
* The list activation hook uses the same index argument convention and does not require bespoke APIs.

---

## Addendum B — Staged implementation plan (checklist)

### Stage 1 — Prerequisite: command extensions (Spec 1)

* [ ] Implement Spec 1 through at least: `ArgValue`, `#[command]` signature extraction, `dispatch_command` returning `ArgValue`, `ctx.current_event()`, and injected `&mouse::Event` / `Option<&mouse::Event>`.
* [ ] Add `ctx.current_list_row()` plumbing in command scope (may return `None` until Stage 4 here).
* [ ] Add Rhai bridge and serde user-type support if list activation or list commands require it.

### Stage 2 — List hit testing surface

* [ ] Implement `ListHitKind`, `ListHit`, and `List::hit_test(ctx, screen_point)`.
* [ ] Implement `List::index_at_point` via `hit_test`.
* [ ] Ensure hit testing uses list metrics (scroll offset + row boundaries) with `O(log n)` lookup.
* [ ] Add tests for variable row heights and scrolling.

### Stage 3 — Default click-to-select in `List`

* [ ] Implement list-native selection in `List<W>::on_event` for `Left Down` using `hit_test`.
* [ ] Call `select(ctx, index)`, `focus_selected(ctx)`, and `ensure_selected_visible(ctx)` in that order.
* [ ] Ensure list ignores clicks on `Scrollbar`, `Padding`, `Gap`, and `Outside`.
* [ ] Add tests: click selects correct row under scroll; clicks on non-item regions do not select.

### Stage 4 — Row metadata + `current_list_row`

* [ ] Add internal per-row metadata registration so realized row roots can be mapped to `(list_id, index)`.
* [ ] Add dispatcher logic to set `current_list_row` for events targeting nodes within a row subtree.
* [ ] Add tests: `current_list_row` visible inside row and inside commands dispatched during row event handling.

### Stage 5 — Activation hook with capture and command dispatch

* [ ] Add `ListActivateConfig` and `with_on_activate` / `set_on_activate`.
* [ ] Track pending activation state on list-handled `Left Down` (index + down position).
* [ ] If activation configured, capture mouse on down; release on up.
* [ ] On `Left Up`, if movement <= threshold and same row index, dispatch command with `[index as isize]`.
* [ ] Add tests: drag beyond threshold does not activate; release on different row does not activate; capture ensures up is received.

### Stage 6 — termgym migration and cleanup

* [ ] Remove termgym manual hit-testing code and any fixed row height assumptions used only for hit testing.
* [ ] Configure sidebar list with built-in selection and activation hook (if term selection should activate).
* [ ] Update any row widgets that need custom click handling to use `ctx.current_list_row()`.

### Stage 7 — Regression and performance validation

* [ ] Bench large list hit testing and selection handling; confirm no `O(n)` behavior in `hit_test`.
* [ ] Stress test virtualization/rebinding (if applicable): ensure row metadata stays correct.
* [ ] Add end-to-end tests: selection + focus + ensure-visible with scrolling and dynamic content.
