# Ergonomic Improvements Plan

This plan proposes API improvements to reduce boilerplate and improve the developer experience for
Canopy applications. Recommendations are ordered by impact and implementation priority.

---

## 1. Isolate Taffy from the public API

**Problem:** Taffy types leak through Canopy's public API, forcing users to depend directly on
`taffy` and coupling the framework to a specific layout engine. Currently, the `focusgym` example
imports directly from taffy:

```rust
use taffy::{
    geometry::Size,
    style::{AvailableSpace, Dimension, Display, FlexDirection, Style},
};
```

This leakage occurs in three places:

1. **Widget trait** - `measure()` and `canvas_size()` use `taffy::geometry::Size` and
   `taffy::style::AvailableSpace`
2. **Widget trait** - `configure_style()` uses `taffy::style::Style`
3. **Context trait** - `with_style()` uses `taffy::style::Style`

This means:
- Users must add `taffy` as an explicit dependency
- Taffy version upgrades are breaking changes for all users
- Switching layout engines would require rewriting all widget code

**Solution:** Create a `canopy::layout` module that re-exports and wraps Taffy types, making Canopy
self-contained:

```rust
// In canopy/src/layout.rs
pub use taffy::geometry::Size;
pub use taffy::style::{
    AvailableSpace,
    Dimension,
    Display,
    FlexDirection,
    Style,
    // ... other commonly used types
};
```

Then update the public API to use these re-exports:

```rust
// Widget trait uses canopy::layout types
use crate::layout::{AvailableSpace, Size, Style};

pub trait Widget: Any + Send + CommandNode {
    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32>;

    fn configure_style(&self, _style: &mut Style) {}
}
```

**Design considerations:**

1. **Re-export vs. wrap:** Start with re-exports for simplicity. Wrappers add indirection and
   maintenance burden without clear benefit unless we need to extend the types.

2. **Selective exposure:** Only re-export types that users actually need. Internal types like
   `TaffyTree` and `TaffyNode` should remain private.

3. **Convenience additions:** The `layout` module can add Canopy-specific helpers alongside the
   re-exports:

   ```rust
   // Convenience constructors
   impl Style {
       pub fn flex_row() -> Self { ... }
       pub fn flex_col() -> Self { ... }
   }

   // Or free functions
   pub fn flex_row() -> Style { ... }
   pub fn flex_col() -> Style { ... }
   ```

4. **Future flexibility:** If we ever need to switch layout engines, we can:
   - Create equivalent wrapper types in `canopy::layout`
   - Implement conversion from the new engine's types
   - Users code remains unchanged if they import from `canopy::layout`

**Migration:**

After this change, the focusgym example would import:

```rust
use canopy::layout::{Dimension, Display, FlexDirection, Size, Style};
```

The `taffy` crate becomes a private implementation detail of Canopy.

**Types to re-export:**

| Type | Used in | Notes |
|------|---------|-------|
| `Size<T>` | measure, canvas_size | Generic geometry |
| `AvailableSpace` | measure, canvas_size | Constraint enum |
| `Style` | configure_style, with_style | Full layout style |
| `Display` | Style field | Flex/Grid/Block/None |
| `FlexDirection` | Style field | Row/Column/RowReverse/ColReverse |
| `Dimension` | Style field | Points/Percent/Auto |
| `AlignItems` | Style field | Flex alignment |
| `JustifyContent` | Style field | Flex justification |
| `FlexWrap` | Style field | Wrap behavior |

---

## 2. Lifecycle initialization hook

**Problem:** Widgets lack a reliable one-time initialization phase after being added to the tree.
The `focusgym` example uses `ensure_tree` guards in both `poll` and command handlers, which is easy
to forget and makes widget state noisy.

**Solution:** Add an `on_mount` method to `Widget` that is called exactly once when a node is first
rendered, after it has a bound `Context`. This is the canonical place to create children, seed
state, and set initial focus.

**Design notes:**
- Called after `configure_style` but before the first `render`
- The node has valid layout information at this point
- Return `Result<()>` to allow fallible initialization
- Widgets that don't need initialization simply don't override the default empty impl

```rust
pub trait Widget: Any + Send + CommandNode {
    /// Called once when the widget is first mounted in the tree.
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }
    // ... existing methods
}
```

**Example:**

```rust
impl Widget for FocusGym {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let root_block = ctx.add(Box::new(Block::new(true)));
        let left = ctx.add(Box::new(Block::new(false)));
        let right = ctx.add(Box::new(Block::new(false)));

        ctx.set_children(ctx.node_id(), vec![root_block])?;
        // ... setup children
        self.root_block = Some(root_block);
        Ok(())
    }
}
```

---

## 3. Typed widget access

**Problem:** Accessing child widgets requires verbose `Any` downcasts with ad-hoc error handling:

```rust
c.with_widget_mut(child_id, &mut |widget, _ctx| {
    let any = widget as &mut dyn Any;
    if let Some(block) = any.downcast_mut::<Block>() {
        flex = (block.flex_grow, block.flex_shrink);
    }
    Ok(())
})?;
```

**Solution:** Add typed accessors that perform the downcast internally and produce a clear error on
type mismatch.

```rust
pub trait Context: ViewContext {
    /// Execute a closure with typed mutable access to a widget.
    /// Returns an error if the node doesn't exist or the widget type doesn't match.
    fn with_widget<T, F, R>(&mut self, node: NodeId, f: F) -> Result<R>
    where
        T: Widget,
        F: FnOnce(&mut T, &mut dyn Context) -> Result<R>;

    /// Try to execute a closure with typed mutable access.
    /// Returns `Ok(None)` if the type doesn't match, `Ok(Some(R))` on success.
    fn try_with_widget<T, F, R>(&mut self, node: NodeId, f: F) -> Result<Option<R>>
    where
        T: Widget,
        F: FnOnce(&mut T, &mut dyn Context) -> Result<R>;
}
```

**Example:**

```rust
ctx.with_widget::<Block, _, _>(child_id, |block, ctx| {
    block.sync_layout(ctx)
})?;

// Or when the type might not match:
if let Some(flex) = ctx.try_with_widget::<Block, _, _>(child_id, |block, _| {
    Ok((block.flex_grow, block.flex_shrink))
})? {
    // use flex
}
```

---

## 4. Style read access

**Problem:** Reading a node's layout style requires a closure even for simple reads, and there's no
direct read path. Widgets end up duplicating style values (like `flex_grow`) in their own fields
because reading from the layout engine is awkward.

**Solution:** Add a read accessor that returns a clone of the style:

```rust
use crate::layout::Style;

pub trait ViewContext {
    /// Return a clone of the layout style for a node.
    fn style(&self, node: NodeId) -> Option<Style>;
}
```

This makes Style the source of truth for layout values, eliminating the need to mirror them in
widget state.

**Example:**

```rust
// Before: duplicate state
pub struct Block {
    flex_grow: f32,    // mirrored from Style
    flex_shrink: f32,  // mirrored from Style
}

// After: read directly
if let Some(style) = ctx.style(child_id) {
    let flex = (style.flex_grow, style.flex_shrink);
}
```

---

## 5. Combined add and mount

**Problem:** Adding a child widget requires two steps: `add` to create the node, then `mount_child`
or `set_children` to attach it. This is error-prone and verbose.

**Solution:** Add a combined method that creates and mounts in one call:

```rust
pub trait Context: ViewContext {
    /// Add a widget as a child of the specified parent. Returns the new node's ID.
    fn add_child(&mut self, parent: NodeId, widget: Box<dyn Widget>) -> Result<NodeId>;

    /// Add multiple widgets as children of the specified parent.
    fn add_children(
        &mut self,
        parent: NodeId,
        widgets: Vec<Box<dyn Widget>>,
    ) -> Result<Vec<NodeId>>;
}
```

**Example:**

```rust
// Before
let left = ctx.add(Box::new(Block::new(false)));
let right = ctx.add(Box::new(Block::new(false)));
ctx.set_children(parent, vec![left, right])?;

// After
let children = ctx.add_children(parent, vec![
    Box::new(Block::new(false)),
    Box::new(Block::new(false)),
])?;
```

---

## 6. Focus traversal utilities

**Problem:** Widgets implement their own tree traversal logic to find focused nodes and collect
focusable leaves. The `focusgym` example has `find_focused` and `collect_focusable` methods that
are generally useful.

**Solution:** Add focus utilities to `ViewContext`:

```rust
pub trait ViewContext {
    /// Find the focused leaf node within a subtree, if any.
    fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

    /// Collect all focusable leaf nodes within a subtree in pre-order.
    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;
}
```

For deletion scenarios, add a helper to `Context`:

```rust
pub trait Context: ViewContext {
    /// Suggest the next focus target after removing a node from a subtree.
    /// Returns the node at the same index, or the last node if index exceeds bounds.
    fn suggest_focus_after_remove(&self, root: NodeId, removed: NodeId) -> Option<NodeId>;
}
```

**Example:**

```rust
// Before: 25 lines of manual traversal
fn find_focused(&self, c: &dyn ViewContext, node: NodeId, parent: Option<NodeId>)
    -> Option<(Option<NodeId>, NodeId)> { ... }
fn collect_focusable(&self, c: &dyn ViewContext, node: NodeId, out: &mut Vec<NodeId>) { ... }

// After
let focused = ctx.focused_leaf(root_block);
let target = ctx.suggest_focus_after_remove(root_block, focused);
```

---

## 7. Expose NodeBuilder via Context

**Problem:** The `NodeBuilder` fluent API exists but is only accessible via `Core::build()`. Widget
code doesn't have access to `Core`, so it can't use the builder for layout configuration.

**Solution:** Expose `NodeBuilder` through `Context`:

```rust
pub trait Context: ViewContext {
    /// Start a builder chain for a node's layout configuration.
    fn build(&mut self, node: NodeId) -> NodeBuilder<'_>;
}
```

**Example:**

```rust
// Before: repeated with_style closures
ctx.with_style(node_id, &mut |style| {
    style.display = Display::Flex;
    style.flex_direction = FlexDirection::Row;
})?;
ctx.with_style(node_id, &mut |style| {
    style.flex_grow = 1.0;
})?;

// After: fluent builder
ctx.build(node_id)
    .flex_row()
    .style(|s| s.flex_grow = 1.0);
```

Consider also adding common presets to NodeBuilder:

```rust
impl NodeBuilder {
    /// Configure as a flex item with grow/shrink factors.
    pub fn flex_item(self, grow: f32, shrink: f32) -> Self;

    /// Set size to fill parent.
    pub fn fill(self) -> Self;
}
```

---

## 8. Typed node IDs (optional)

**Problem:** `NodeId` is untyped, which encourages unsafe downcasts and makes it unclear which IDs
refer to which widget types.

**Solution:** Add an optional typed wrapper that provides compile-time type safety for widget IDs:

```rust
/// A node ID that knows the widget type it refers to.
pub struct TypedId<T: Widget> {
    id: NodeId,
    _marker: PhantomData<T>,
}

impl<T: Widget> TypedId<T> {
    /// Convert to untyped NodeId.
    pub fn id(&self) -> NodeId { self.id }
}

impl<T: Widget> From<TypedId<T>> for NodeId {
    fn from(typed: TypedId<T>) -> NodeId { typed.id }
}
```

Add a typed add method:

```rust
pub trait Context: ViewContext {
    /// Add a widget and return a typed ID.
    fn add_typed<T: Widget>(&mut self, widget: T) -> TypedId<T>;
}
```

**Example:**

```rust
let block: TypedId<Block> = ctx.add_typed(Block::new(true));

// Type-safe access - no downcast needed
ctx.with_widget(block, |block, ctx| {
    block.sync_layout(ctx)
})?;

// Still works with untyped APIs
ctx.set_focus(block.id());
```

**Note:** This is optional and additive. Existing code using `NodeId` continues to work unchanged.

---

## 9. Type-safe command bindings

**Problem:** Key bindings use string paths like `"block::split()"`, which are brittle and not
refactor-friendly.

**Solution:** Provide a typed binding API where command references are generated by
`derive_commands`:

```rust
// derive_commands generates:
impl Block {
    pub fn cmd_split() -> CommandRef { CommandRef::new("block::split") }
}

// Usage
Binder::new(canopy)
    .with_path(Block::path())
    .key('s', Block::cmd_split());
```

Alternatively, accept a method reference directly:

```rust
Binder::new(canopy)
    .with_path::<Block>()
    .key('s', Block::split);  // Macro magic to extract command path
```
