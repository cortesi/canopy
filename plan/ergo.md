# Ergonomic Improvements Plan

Based on an analysis of the `focusgym` example, the following changes are proposed to improve the
developer experience and maintainability of Canopy applications. The sections below merge the
existing plan with a fuller set of ergonomics ideas, resolving overlaps and keeping the clearest
approach for each topic.

## 1. Lifecycle initialization hook

**Problem:** Widgets lack a reliable one-time initialization phase after being added to the tree.
`FocusGym` uses `ensure_tree` gates in `poll` and in command handlers, which is easy to forget and
makes widget state noisy.

**Solution:** Add an `on_mount`/`init` hook to `Widget` that is called exactly once when a node is
inserted into the tree and has a bound `Context`, before the first render and before command
execution. This is the canonical place to create children, seed state, and set initial focus.

**Example:**

```rust
impl Widget for FocusGym {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.build_tree(ctx)
    }
}
```

## 2. Typed widget access helpers

**Problem:** Mutating child widgets requires verbose `Any` downcasts and ad hoc error handling:

```rust
c.with_widget_mut(child_id, &mut |widget, _ctx| {
    let any = widget as &mut dyn Any;
    if let Some(block) = any.downcast_mut::<Block>() {
        // ...
    }
    Ok(())
})?;
```

**Solution:** Add typed accessors on `Context`, such as `with_widget_mut::<T>` and
`try_widget_mut::<T>`, that perform the downcast and produce a consistent error when the type does
not match.

**Example:**

```rust
ctx.with_widget_mut::<Block>(child_id, |block, ctx| {
    block.sync_layout(ctx)
})?;
```

## 3. Typed node IDs for widgets (optional, high leverage)

**Problem:** `NodeId` is untyped, which encourages unsafe downcasts and makes it unclear which IDs
belong to which widget type.

**Solution:** Add an optional typed wrapper like `WidgetId<T>` that can be created by `add_typed`
and converted into a `NodeId` when necessary. Typed IDs make intent explicit and reduce miscasts.

**Example:**

```rust
let block: WidgetId<Block> = ctx.add_typed(Block::new(true));
ctx.with_widget_mut(block, |b, ctx| b.sync_layout(ctx))?;
```

## 4. Child list management helper

**Problem:** Widgets often mirror the child list in their own fields and must manually keep it in
sync with Core (`children: Vec<NodeId>` plus `set_children` + custom `sync_layout`). This is easy
to forget and spreads tree-manipulation logic across widgets.

**Solution:** Provide a child-management helper that owns the list and keeps Core in sync, or a
`Context::replace_children` that returns the previous list. Another option is a `Children` handle
bound to a node that supports `push`, `retain`, and `replace` while updating Core automatically.

**Example:**

```rust
let mut children = ctx.children_handle();
children.replace(vec![left, right])?;
```

## 5. Expose the NodeBuilder via Context

**Problem:** Layout setup is verbose and split across repeated `with_style` closures. The existing
`NodeBuilder` is only reachable via `core`, which widget code does not have.

**Solution:** Expose `NodeBuilder` on `Context` so widget code can chain layout + hierarchy updates
without reaching into `core`. Consider also `add_child`/`add_children` helpers to combine `add` with
mounting in one call.

**Example:**

```rust
ctx.build(root_block)
    .flex_row()
    .add_child(left)
    .add_child(right);
```

## 6. Unified state + style management and flex helpers

**Problem:** Widgets mirror layout state (`flex_grow`, `flex_shrink`) in their own structs because
reading/updating `Style` is cumbersome, and the same flex fields are written repeatedly.

**Solution:**
1) Provide easy access to the current node's `Style`, including a read path (e.g.
   `ctx.style()` or `ctx.style_of(node)` returning a clone) and a write path that updates Taffy.
2) Add flex convenience helpers such as `Style::flex_item(grow, shrink)` or
   `Context::set_flex_item(node, FlexItem::auto(grow, shrink))` to apply common patterns.
3) Encourage the `Style` as the source of truth for layout values rather than duplicating them in
   widget state.

**Example:**

```rust
ctx.set_flex_item(node_id, FlexItem::auto(1.0, 1.0));
```

## 7. Focus traversal utilities

**Problem:** `delete_focused` rolls its own tree traversal to find the focused leaf and retarget
focus after removal. This logic is long, easy to duplicate, and distracts from widget intent.

**Solution:** Add reusable focus helpers like `focused_leaf(root)`, `focusable_leaves(root)`, and
`next_focus_after_remove(root, focused)` to centralize traversal logic.

**Example:**

```rust
let focused = ctx.focused_leaf(root)?;
let target = ctx.next_focus_after_remove(root, focused);
```

## 8. Geometry convenience methods

**Problem:** The code repeatedly constructs `Expanse` from `Rect` fields and manually unwraps view
options, which adds noise to size checks.

**Solution:** Provide helpers such as `Rect::expanse()`, `ViewContext::view_size()`, and
`ViewContext::node_view_size(node)`.

**Example:**

```rust
let size = ctx.view_size();
if self.size_limited(size) {
    return Ok(());
}
```

## 9. Type-safe command bindings

**Problem:** Key bindings are stringly typed (`"block::split()"`), which is brittle and not
refactor-friendly.

**Solution:** Provide a typed binding API, or emit command references from `derive_commands` that
the binder can consume. The binder should accept a strongly typed command instead of a string.

**Example:**

```rust
Binder::new(cnpy)
    .with_path(Block::path())
    .key_cmd('s', Block::split);
```

## 10. Render helpers for edge cleanup

**Problem:** `Block::render` includes manual viewport comparisons and edge clearing to keep block
fills flush against the screen. This mixes low-level rendering details into widget logic.

**Solution:** Add render helpers for overscan/edge cleanup, or expose a clear `ViewContext` accessor
(e.g., `screen_view()` or `root_viewport()`) so the intent is obvious and shared across widgets.

**Example:**

```rust
r.fill_block(bg, ctx.view())?;
r.clear_overscan(ctx)?;
```

## 11. Split helper widget or API

**Problem:** `Block` implements split/append logic, child synchronization, and flex setup all in one
place. This is a common pattern in examples and likely in real apps.

**Solution:** Introduce a small `Split` widget or helper API that encapsulates “split into two
children with flex layout.” This reduces per-widget boilerplate and makes intent explicit.

**Example:**

```rust
let split = ctx.add(Box::new(Split::row()));
split.set_children([left, right])?;
```
