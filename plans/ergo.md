# API Ergonomics Improvements

Analysis of `focusgym.rs` reveals several API friction points. This document proposes
changes to make the API more coherent, DRY, and intuitive.

## Current Pain Points (from focusgym `on_mount`)

```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    // 1. Unnecessary guard - on_mount is guaranteed to run once
    if Self::root_block_id(c).is_some() {
        return Ok(());
    }

    // 2. Redundant: passing c.node_id() when already in context
    let root_block = c.add_child(c.node_id(), Block::new(true))?;

    // 3. add_widget doesn't attach to tree - manual step required later
    let left = c.add_widget(Block::new(false));
    let right = c.add_widget(Block::new(false));

    // 4. Awkward static method just to set flex styles
    Block::init_flex(c, left)?;
    Block::init_flex(c, right)?;

    // 5. Complex dance to set children and sync layout
    c.with_widget(root_block, |block: &mut Block, ctx| {
        let children = [left, right];
        block.sync_layout(ctx, &children)
    })?;

    // 6. Separate build() calls for styling - verbose
    c.build(c.node_id()).flex_col();
    c.build(root_block).flex_item(1.0, 1.0, Dimension::Auto);

    Ok(())
}
```

---

## 1. Remove Redundant `on_mount` Guard

**Problem:** The `if Self::root_block_id(c).is_some()` check is unnecessary.

**Analysis:** The framework already guarantees `on_mount` is called exactly once per widget.
From `world.rs`:

```rust
fn mount_node(&mut self, node_id: NodeId) -> Result<()> {
    let should_mount = self.nodes.get(node_id).map(|node| !node.mounted).unwrap_or(false);
    if !should_mount {
        return Ok(());
    }
    // ... calls on_mount ...
    node.mounted = true;
}
```

The test `on_mount_runs_once_with_bound_context` confirms this behavior.

**Before:**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    if Self::root_block_id(c).is_some() {
        return Ok(());
    }
    // ... rest of mount logic
}
```

**After:**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    // No guard needed - framework guarantees single invocation
    // ... rest of mount logic
}
```

**Recommendation:** Remove the guard from focusgym. Document the single-invocation
guarantee in the `Widget::on_mount` trait method docs.

---

## 2. Context API Refactoring

**Problem:** Almost every Context method requires passing `c.node_id()` when operating on the
current node. This is the common case, making the API unnecessarily verbose:

```rust
c.children(c.node_id())           // ~30 occurrences in codebase
c.set_children(c.node_id(), vec)
c.add_child(c.node_id(), widget)
c.build(c.node_id()).flex_col()
c.with_style(c.node_id(), f)
```

**Solution:** Make methods operate on the current node by default. Add `context_for(descendant)`
for child operations.

### Node-bound Operations (remove redundant arg)

| Before | After |
|--------|-------|
| `c.children(c.node_id())` | `c.children()` |
| `c.set_children(c.node_id(), vec)` | `c.set_children(vec)` |
| `c.add_child(c.node_id(), widget)` | `c.add_child(widget)` |
| `c.build(c.node_id()).flex_col()` | `c.build().flex_col()` |
| `c.with_style(c.node_id(), f)` | `c.with_style(f)` |
| `c.mount_child(c.node_id(), child)` | `c.mount_child(child)` |

For explicit targets, add `_of` / `_to` variants:
- `children_of(node)`, `set_children_of(node, vec)`, `add_child_to(parent, widget)`

### Focus Operations (local vs global)

Focus operations search within a subtree. The common cases are:
- Search within current node's subtree: `focus_next(c.node_id())`
- Search from root (whole tree): `focus_next(c.root_id())`

**Proposed:**
```rust
c.focus_next()           // within current subtree
c.focus_next_global()    // from root
c.focus_next_in(subtree) // explicit subtree (renamed from current)
```

### `context_for(descendant)` for Child Operations

When you need to operate on a child or descendant, get a scoped context:

```rust
// Before
c.with_style(child_id, f);
c.build(child_id).flex_row();
c.focus_first(self.app);

// After
c.context_for(child_id)?.with_style(f);
c.context_for(child_id)?.build().flex_row();
c.context_for(self.app)?.focus_first();
```

**Constraint:** `context_for(node)` only allows descendants of the current node, not ancestors
or siblings. This enforces widget encapsulation - a widget can only control its own subtree.

### Complete Transformation Example

```rust
// BEFORE
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    let root_block = c.add_child(c.node_id(), Block::new(true))?;
    c.build(c.node_id()).flex_col();
    c.build(root_block).flex_item(1.0, 1.0, Dimension::Auto);
    c.focus_next(c.node_id());
    Ok(())
}

// AFTER
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    let root_block = c.add_child(Block::new(true))?;
    c.build().flex_col();
    c.context_for(root_block)?.build().flex_item(1.0, 1.0, Dimension::Auto);
    c.focus_next();
    Ok(())
}
```

---

## 3. Eliminate `Block::init_flex` Pattern (moved to Stage 5)

**Problem:** The `init_flex` static method is awkward:
```rust
fn init_flex(c: &mut dyn Context, node_id: NodeId) -> Result<()> {
    c.build(node_id).flex_item(1.0, 1.0, Dimension::Auto);
    Ok(())
}
```

This is called immediately after `add_widget`, creating a two-step pattern that should
be one step.

**Option A: Builder chaining from add_widget**

Proposed API:
```rust
fn add_widget<W>(&mut self, widget: W) -> WidgetBuilder<'_>;
```

**Before:**
```rust
let left = c.add_widget(Block::new(false));
Block::init_flex(c, left)?;
```

**After:**
```rust
let left = c.add_widget(Block::new(false))
    .flex_item(1.0, 1.0, Dimension::Auto)
    .id();
```

**Option B: Widget default styles via configure_style**

Widgets already have `configure_style(&self, style: &mut Style)`. If `Block` sets its
default flex properties there, no explicit `init_flex` is needed:

```rust
impl Widget for Block {
    fn configure_style(&self, style: &mut Style) {
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.flex_basis = Dimension::Auto;
        style.min_size.width = Dimension::Points(1.0);
        style.min_size.height = Dimension::Points(1.0);
    }
}
```

**Recommendation:** Option B is cleaner - widgets should configure their own defaults.
Reserve the builder pattern for overrides.

---

## 4. Clarify Orphan Widget Creation (Stage 3)

**Problem:** There are two ways to add widgets:
- `add_widget(w)` - adds to arena but NOT to tree
- `add_child(parent, w)` - adds to arena AND attaches to parent

This creates confusion. Users must remember which to use and when.

**Current confusion in focusgym:**
```rust
let root_block = c.add_child(c.node_id(), Block::new(true))?;  // attached
let left = c.add_widget(Block::new(false));                     // NOT attached
let right = c.add_widget(Block::new(false));                    // NOT attached
// ... later, manually attach via set_children
```

**Proposed simplification:**

Rename for clarity:
```rust
// Always use add_child to add AND attach (common case)
fn add_child<W>(&mut self, widget: W) -> Result<NodeId>;

// Rename add_widget to make its orphan nature explicit
fn add_orphan<W>(&mut self, widget: W) -> NodeId;  // for rare cases
```

**Before:**
```rust
let root_block = c.add_child(c.node_id(), Block::new(true))?;
let left = c.add_widget(Block::new(false));
let right = c.add_widget(Block::new(false));
```

**After:**
```rust
let root_block = c.add_child(Block::new(true))?;
let left = c.add_orphan(Block::new(false));  // clearly an orphan
let right = c.add_orphan(Block::new(false));
```

---

## 5. Add `add_widget` to NodeBuilder (Stage 4)

**Problem:** Setting up children requires multiple verbose steps:
```rust
c.with_widget(root_block, |block: &mut Block, ctx| {
    let children = [left, right];
    block.sync_layout(ctx, &children)
})?;
```

The `sync_layout` pattern appears in multiple widgets (Block, Panes, Root).

**Proposed API enhancement:**

Add `add_children` that returns a builder:

```rust
// Current (verbose)
let left = c.add_widget(Block::new(false));
let right = c.add_widget(Block::new(false));
c.set_children(root_block, vec![left, right])?;
c.build(root_block).flex_row();

// Proposed (fluent)
c.build(root_block)
    .flex_row()
    .add_child(c.add_orphan(Block::new(false)))
    .add_child(c.add_orphan(Block::new(false)));
```

**Alternative:** The `build()` API already has `add_child()` on `NodeBuilder`. The issue
is that you need the child IDs before calling it. Could extend to:

```rust
impl NodeBuilder {
    fn add_widget<W: Widget>(&mut self, widget: W) -> NodeId {
        let child = self.ctx.add_orphan(widget);
        self.add_child(child);
        child
    }
}
```

**Before:**
```rust
let left = c.add_widget(Block::new(false));
let right = c.add_widget(Block::new(false));
c.set_children(root_block, vec![left, right])?;
c.build(root_block).flex_row();
```

**After:**
```rust
let mut b = c.build(root_block).flex_row();
let left = b.add_widget(Block::new(false));
let right = b.add_widget(Block::new(false));
```

---

## 6. Complete Refactored Example

Applying all recommendations, the `on_mount` becomes:

**Before (current):**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    if Self::root_block_id(c).is_some() {
        return Ok(());
    }

    let root_block = c.add_child(c.node_id(), Block::new(true))?;
    let left = c.add_widget(Block::new(false));
    let right = c.add_widget(Block::new(false));
    Block::init_flex(c, left)?;
    Block::init_flex(c, right)?;

    c.with_widget(root_block, |block: &mut Block, ctx| {
        let children = [left, right];
        block.sync_layout(ctx, &children)
    })?;

    c.build(c.node_id()).flex_col();
    c.build(root_block).flex_item(1.0, 1.0, Dimension::Auto);

    Ok(())
}
```

**After (proposed):**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    c.build_self().flex_col();

    let root_block = c.add_child(Block::new(true))?;
    c.build(root_block)
        .flex_item(1.0, 1.0, Dimension::Auto)
        .flex_row()
        .add_widget(Block::new(false))
        .add_widget(Block::new(false));

    Ok(())
}
```

Key improvements:
- 18 lines -> 9 lines
- No unnecessary guard
- No redundant `c.node_id()` arguments
- No awkward static `init_flex` method
- Fluent builder pattern for children
- Widget default styles via `configure_style`

---

## 7. Clarify `measure` vs `canvas_size`

**Problem:** The Widget trait has two sizing methods with confusing, overlapping semantics:

```rust
fn measure(&self, known_dimensions, available_space) -> Size<f32> { ... }
fn canvas_size(&self, known_dimensions, available_space) -> Size<f32> {
    self.measure(known_dimensions, available_space)  // default just calls measure!
}
```

**Analysis:**
- `measure` is called by Taffy during layout to determine intrinsic size
- `canvas_size` is called after layout to determine the virtual scrollable canvas
- For non-scrolling widgets (the common case), these are identical
- The default `canvas_size` delegates to `measure`, obscuring their distinct purposes

**The Distinction:**

| Method | Purpose | When Different |
|--------|---------|----------------|
| `measure` | How much space to allocate in layout | Always used |
| `canvas_size` | Virtual canvas for scrolling | Only for scrollable widgets |

**Example:** A list with 100 items in a 10-row viewport:
- `measure` → 10 rows (fits the layout allocation)
- `canvas_size` → 100 rows (total scrollable content)

**Current Block implementation (unnecessary?):**
```rust
fn measure(&self, ...) -> Size<f32> {
    Size { width: 0.0, height: 0.0 }  // No intrinsic size, relies on flex
}

fn canvas_size(&self, ...) -> Size<f32> {
    // This is identical to the default measure implementation!
    let width = known_dimensions.width.or_else(...).unwrap_or(0.0);
    let height = known_dimensions.height.or_else(...).unwrap_or(0.0);
    Size { width, height }
}
```

Block doesn't scroll, so it likely doesn't need to override either method. The flex layout
handles sizing. These overrides may be cargo-culted boilerplate.

**Recommendation:**
1. Improve doc comments to clearly explain when each is needed
2. Remove unnecessary overrides from Block (and other non-scrolling widgets)
3. Consider renaming for clarity: `layout_size` and `content_size`?

---

## Implementation Checklist

See `ergo-exec.md` for the detailed staged execution plan. Summary:

1. [x] Document `on_mount` single-invocation guarantee in trait docs
2. [ ] Context API Refactoring (Stage 2):
   - Remove `node_id` arg from methods that operate on current node
   - Add `context_for(descendant)` for child operations
   - Add local/global focus method variants
3. [ ] Rename `add_widget` to `add_orphan` for clarity (Stage 3)
4. [ ] Add `add_widget` method to `NodeBuilder` (Stage 4)
5. [ ] Move Block's flex defaults to `configure_style` (Stage 5)
6. [ ] Simplify `sync_layout` pattern (Stage 6)
7. [ ] Clarify `measure` vs `canvas_size` docs (Stage 7)
8. [ ] Update focusgym and other examples (Stage 8)
