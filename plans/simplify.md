# Simplify listgym by extending core APIs

This document proposes core-library improvements that make
`crates/examples/src/listgym.rs` smaller and easier to understand.

Goals and constraints:
- Primary goal: shrink listgym and reduce concepts for new users, even if core APIs grow.
- Higher-level convenience APIs are fine as long as base primitives remain available.

## Proposal 1: List-managed selection indicator + richer Text widget

### Problem
Listgym creates a bespoke `TextBlock` widget just to get wrapping, selection state, and
intrinsic width for horizontal scroll.

### Proposal
1) Add a list-level selection indicator, e.g.:
   - `List::with_selection_indicator(style: &str, glyph: char, width: u32)`
   - or `List::set_selection_indicator(...)`
2) Extend `Text` to support styling + intrinsic canvas width:
   - `Text::with_style("red/text")`
   - `Text::with_canvas_width(CanvasWidth::Intrinsic)`

We should support both patterns: list-level indicators for convenience, and widget-based indicators
for custom layouts.

For Text sizing, prefer a small enum such as:
`CanvasWidth::{View, Intrinsic, Fixed(u32)}`.

### Before
```rust
let list_id = c.add_orphan(List::<TextBlock>::new());
// ...
list.append(ctx, TextBlock::new(i))?;
```

### After
```rust
let list_id = c.add_orphan(List::<Text>::new()
    .with_selection_indicator("list/selected", '█', 1));
// ...
list.append(ctx, Text::new(TEXT)
    .with_style(item_style(i))
    .with_canvas_width(CanvasWidth::Intrinsic))?;
```

## Proposal 2: Panes column introspection + focus helpers

### Problem
Listgym manually walks the tree to find column lists and manage column-to-column focus.

### Proposal
Add helpers/commands on `Panes`:
- `Panes::column_nodes(ctx) -> Vec<NodeId>`
- `Panes::focused_column_index(ctx) -> Option<usize>`
- `Panes::focus_next_column(ctx)` / `focus_prev_column(ctx)`
- commands `panes::next_column()` / `panes::prev_column()`

Focus helpers should target the first focusable leaf under the column.

### Before
```rust
fn shift_column(&self, c: &mut dyn Context, forward: bool) -> Result<()> {
    let panes_id = Self::panes_id(c).expect("panes not initialized");
    let columns = Self::column_lists(c, panes_id);
    // compute next_idx, set focus
    c.set_focus(columns[next_idx]);
    Ok(())
}
```

### After
```rust
#[command]
pub fn next_column(&mut self, c: &mut dyn Context) -> Result<()> {
    c.with_widget(self.panes_id(c)?, |panes: &mut Panes, ctx| {
        panes.focus_next_column(ctx)
    })??;
    Ok(())
}
```

## Proposal 3: Path-based node selection for tests (reuse existing matcher)

### Problem
The tree structure is now authoritative, and widget-held child references may not reflect the
actual layout tree. In listgym tests, we walk the arena manually to locate panes, frames, and
lists. This is noisy and fragile.

We already have a path-based selection mechanism used for input bindings (`PathMatcher`). We can
repurpose the same path syntax and matcher to select nodes for tests and light app logic.

### Proposal
Add helpers that locate nodes by path filter (same syntax as bindings), such as:
- `Context::find_node(path_filter: &str) -> Option<NodeId>`
- `Context::find_nodes(path_filter: &str) -> Vec<NodeId>`
- `Harness::find_node(path_filter: &str) -> Option<NodeId>`

Implementation can reuse `core::path::PathMatcher` and `core::world::node_path`.

The selection API should live on `Context` (for app code and tests), with a `Harness` helper that
delegates to it. `find_node` should return the first pre-order match, and `find_nodes` should return
all matches. Path matching should be relative to the current node by default.

### Before (listgym tests)
```rust
fn list_id(harness: &Harness) -> NodeId {
    let panes_id = panes_id(harness);
    let panes_children = &harness
        .canopy
        .core
        .nodes
        .get(panes_id)
        .expect("panes node missing")
        .children;
    let column_id = *panes_children.first().expect("pane column not initialized");
    let column_children = &harness
        .canopy
        .core
        .nodes
        .get(column_id)
        .expect("column node missing")
        .children;
    let frame_id = *column_children.first().expect("frame not initialized");
    let frame_children = &harness
        .canopy
        .core
        .nodes
        .get(frame_id)
        .expect("frame node missing")
        .children;
    *frame_children.first().expect("list not initialized")
}
```

### After
```rust
let list_id = harness
    .find_node("list_gym/*/frame/list")
    .expect("list not initialized");
```

## Proposal 4: VStack widget for fixed + flex rows

### Problem
Listgym sets several layouts manually, which distracts from the UI structure (panes + status bar).

### Proposal
Add a `VStack` widget that accepts children with fixed or flex sizing. This feels generally useful
beyond listgym.

### Before
```rust
c.with_layout(&mut |layout| {
    *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
})?;

c.with_layout_of(status_id, &mut |layout| {
    *layout = Layout::row().flex_horizontal(1).fixed_height(1);
})?;
```

### After (sketch)
```rust
let stack_id = c.add_child(VStack::new()
    .push_flex(panes_id, 1)
    .push_fixed(status_id, 1))?;
```

## Proposal 5: Frame wrapping helper

### Problem
Wrapping a list in a `Frame` requires manual orphan + mount steps.

### Proposal
Add a `Frame::wrap` helper.

### Before
```rust
let list_id = c.add_orphan(List::<TextBlock>::new());
let frame_id = c.add_orphan(frame::Frame::new());
c.mount_child_to(frame_id, list_id)?;
```

### After
```rust
let list_id = c.add_orphan(List::<Text>::new());
let frame_id = frame::Frame::wrap(c, list_id)?;
```

## Listgym After Sketch (combined)

This is a rough idea of how listgym could look once the above are in place.

```rust
fn ensure_tree(&self, c: &mut dyn Context) -> Result<()> {
    if !c.children().is_empty() {
        return Ok(());
    }

    let panes_id = c.add_child(Panes::new())?;
    let status_id = c.add_child(StatusBar)?;
    c.add_child(VStack::new()
        .push_flex(panes_id, 1)
        .push_fixed(status_id, 1))?;

    let (frame_id, list_id) = Self::create_column(c)?;
    c.with_widget(panes_id, |panes: &mut Panes, ctx| {
        panes.insert_col(ctx, frame_id)
    })?;
    c.focus_first_in(list_id);

    Ok(())
}

fn create_column(c: &mut dyn Context) -> Result<(NodeId, NodeId)> {
    let list_id = c.add_orphan(List::<Text>::new()
        .with_selection_indicator("list/selected", '█', 1));
    let frame_id = frame::Frame::wrap(c, list_id)?;

    c.with_widget(list_id, |list: &mut List<Text>, ctx| {
        for i in 0..10 {
            list.append(ctx, Text::new(TEXT)
                .with_style(item_style(i))
                .with_canvas_width(CanvasWidth::Intrinsic))?;
        }
        Ok(())
    })?;

    Ok((frame_id, list_id))
}
```

Target: keep this sketch as the goal; it is much cleaner and easier to understand.

## Implementation Checklist

1. Stage One: Path-based node selection for tests and app code

2. [x] Add core traversal helpers to collect nodes under a root and match with
    `PathMatcher`, using `Core::node_path` for path construction.
3. [x] Add `Context::find_node` and `Context::find_nodes` that match relative to the current
    node and return first pre-order match or all matches, respectively.
4. [x] Add `Harness::find_node` and `Harness::find_nodes` that delegate to the context helpers.
5. [x] Update listgym tests to use the new path selection helpers instead of manual tree walks.
6. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
    --examples 2>&1`, then `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`,
    then `cargo nextest run --all --all-features`.

7. Stage Two: Text sizing + list selection indicator

8. [x] Introduce `CanvasWidth::{View, Intrinsic, Fixed(u32)}` in `Text`, update measure/canvas
    behavior, and add `Text::with_style` plus a width-setting builder method.
9. [x] Add list-level selection indicator support (style, glyph, width) while keeping item-level
    selection possible and unchanged.
10. [x] Update listgym items to use `Text` directly and remove `TextBlock`.
11. [x] Update any listgym tests that reference `TextBlock` or selection state.
12. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
    --examples 2>&1`, then `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`,
    then `cargo nextest run --all --all-features`.

13. Stage Three: Panes helpers, VStack, and Frame::wrap

14. [x] Add `Panes::column_nodes`, `Panes::focused_column_index`, and focus helpers that move
    focus to the first focusable leaf in the target column; add commands for next/prev.
15. [x] Implement a `VStack` widget with a fluent API like `push_flex` / `push_fixed`.
16. [x] Add `Frame::wrap` to wrap a child node in a frame.
17. [x] Update listgym to use `VStack`, `Frame::wrap`, and `Panes` helpers to simplify layout and
    column navigation.
18. [x] Run `cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
    --examples 2>&1`, then `cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml`,
    then `cargo nextest run --all --all-features`.
