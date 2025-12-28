# Widget-Based List Items

This document explores converting `List` from a render-delegate pattern to a widget-based pattern
where list items are actual widgets in the tree.


## Current Design

### The `ListItem` Trait

```rust
pub trait ListItem {
    fn set_selected(&mut self, _state: bool) {}
    fn measure(&self, available_width: u32) -> Expanse;
    fn render(
        &mut self,
        rndr: &mut Render,
        area: Rect,
        selected: bool,
        offset: Point,
        full_size: Expanse,
    ) -> Result<()>;
}
```

Items are **data objects** that implement rendering logic directly. They:
- Receive a `Render` buffer and geometry, not a `Context`
- Cannot create child widgets
- Cannot participate in focus management
- Must handle partial visibility (scrolling) manually via `offset`/`area`

### Current Usage Examples

**listgym - Block items with wrapped text:**
```rust
pub struct Block {
    color: String,
    width: u32,
    lines: Vec<String>,
}

impl ListItem for Block {
    fn measure(&self, _available_width: u32) -> Expanse {
        Expanse::new(self.width.saturating_add(2), self.lines.len() as u32)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool,
              offset: Point, _full_size: Expanse) -> Result<()> {
        // Manual text slicing for horizontal scroll
        // Manual line skipping for vertical scroll
        // Manual selection indicator drawing
    }
}
```

**termgym - Boxed terminal labels:**
```rust
struct TermItem {
    index: usize,
}

impl ListItem for TermItem {
    fn measure(&self, available_width: u32) -> Expanse {
        Expanse::new(available_width.max(ENTRY_MIN_WIDTH), ENTRY_HEIGHT)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool,
              offset: Point, full_size: Expanse) -> Result<()> {
        // Manual box-drawing with entry_lines()
        // Manual partial visibility handling
        // Could use Box + Center + Text widgets instead
    }
}
```

**intervals - Simple counter display:**
```rust
impl ListItem for IntervalItem {
    fn measure(&self, available_width: u32) -> Expanse {
        Expanse::new(available_width.max(1), 1)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool,
              _offset: Point, _full_size: Expanse) -> Result<()> {
        let style = if selected { "blue/text" } else { "text" };
        rndr.text(style, area.line(0), &self.value.to_string())?;
        Ok(())
    }
}
```


## Proposed Design: Widget-Based Items

### API Design: Typed vs Heterogeneous

There are two approaches for storing items:

**Option 1: Typed `List<W: Widget>`**

```rust
pub struct List<W: Widget> {
    items: Vec<NodeId>,
    selected: Option<usize>,
    _marker: PhantomData<W>,
}

impl<W: Widget> List<W> {
    pub fn append(&mut self, ctx: &mut dyn Context, widget: W) -> Result<NodeId>;
    pub fn item(&self, ctx: &dyn Context, index: usize) -> Option<&W>;
    pub fn item_mut(&mut self, ctx: &mut dyn Context, index: usize) -> Option<&mut W>;
}
```

Usage:
```rust
// Type-safe construction
let list: List<Button> = List::new();
list.append(ctx, Button::new("One"))?;
list.append(ctx, Button::new("Two"))?;

// Direct typed access - no downcasting
if let Some(btn) = list.item_mut(ctx, 0) {
    btn.set_label(ctx, "Updated")?;
}

// Compile-time error for wrong types
list.append(ctx, Text::new("oops"))?;  // ERROR: expected Button, found Text
```

**Option 2: Heterogeneous `List` (NodeId-based)**

```rust
pub struct List {
    items: Vec<NodeId>,
    selected: Option<usize>,
}

impl List {
    pub fn append<W: Widget>(&mut self, ctx: &mut dyn Context, widget: W) -> Result<NodeId>;
    pub fn item_id(&self, index: usize) -> Option<NodeId>;
}
```

Usage:
```rust
// Mixed types allowed
let list = List::new();
list.append(ctx, Button::new("Action"))?;
list.append(ctx, Text::new("Label"))?;
list.append(ctx, CustomWidget::new())?;

// Access requires Context and downcasting
let id = list.item_id(0).unwrap();
ctx.with_widget(id, |btn: &mut Button, _| {
    btn.set_label(ctx, "Updated")?;
    Ok(())
})?;

// Runtime error if type doesn't match
ctx.with_widget(id, |txt: &mut Text, _| { ... })?;  // Panics or returns Err
```

**Tradeoff Summary:**

| Aspect | Typed `List<W>` | Heterogeneous `List` |
|--------|-----------------|----------------------|
| Type safety | Compile-time | Runtime |
| Mixed item types | No | Yes |
| Direct item access | `list.item(i)` | `ctx.with_widget(id, \|w\| ...)` |
| Ergonomics | Better for uniform lists | Better for mixed content |
| Common in practice | File lists, menu items | Complex UIs with varied items |

### Analysis: Typed Containers Across Canopy

Canopy already has `TypedId<T>` in `core/id.rs` - a type-safe wrapper around `NodeId`. Let's examine
how existing containers work and whether typed containers make sense more broadly.

**Current Container Patterns:**

| Widget | Child Storage | Type Knowledge |
|--------|--------------|----------------|
| `Button` | `box_id: Option<NodeId>`, `text_id: Option<NodeId>` | Always `Box` and `Text` |
| `Frame` | None (uses `ctx.children()`) | Any single child |
| `Panes` | `columns: Vec<Vec<NodeId>>` | Any widget per pane |
| `Root` | `app: NodeId`, `inspector: NodeId` | App varies, Inspector fixed |

**Three Categories of Containers:**

1. **Known child types** (Button): Always creates specific widget types internally. Could use
   `TypedId<Box>` and `TypedId<Text>` for compile-time safety.

2. **Inherently heterogeneous** (Panes, Frame): Accept any widget as children. Must stay
   `NodeId`-based because the child type varies.

3. **Homogeneous collections** (List): User adds items of a single type. Can benefit from
   `List<W>` for type safety.

**Could Button use TypedId?**

```rust
// Current
struct Button {
    box_id: Option<NodeId>,
    text_id: Option<NodeId>,
}

// With TypedId
struct Button {
    box_id: Option<TypedId<Box>>,
    text_id: Option<TypedId<Text>>,
}
```

This provides documentation value but limited practical benefit - Button never exposes these IDs
publicly and always knows what types they are internally.

**Could Panes be typed?**

```rust
// This doesn't work - each pane can be a different widget type
struct Panes<T: Widget> {  // What is T? Can't be one type.
    columns: Vec<Vec<TypedId<T>>>,
}
```

Panes is fundamentally heterogeneous. A user might have `Frame<Editor>` in one pane and
`Frame<Terminal>` in another. No single `T` works.

**List is the sweet spot for typed containers**

Unlike Panes, List items are typically uniform:
- File browser: all items are `FileEntry` widgets
- Menu: all items are `MenuItem` widgets
- Terminal list: all items are `Button` widgets

This is why `List<W: Widget>` makes sense while `Panes<W>` doesn't.

**Recommendation: Typed List, Heterogeneous Panes/Frame**

```rust
// List: typed, uniform items
pub struct List<W: Widget> {
    items: Vec<TypedId<W>>,
    selected: Option<usize>,
}

impl<W: Widget> List<W> {
    pub fn append(&mut self, ctx: &mut dyn Context, widget: W) -> Result<TypedId<W>>;
    pub fn item(&self, index: usize) -> Option<TypedId<W>>;
}

// Usage
let list: List<Button> = List::new();
list.append(ctx, Button::new("One"))?;
let id: TypedId<Button> = list.item(0).unwrap();

// Access still requires Context, but type is known
ctx.with_widget(id.into(), |btn: &mut Button, _| { ... })?;  // Type guaranteed
```

**Impact on existing code:**

1. `List<T: ListItem>` becomes `List<W: Widget>` - similar generic signature
2. Examples update their type annotations: `List<Block>` → `List<TextBlock>`
3. The `with_widget` calls become safer - type mismatch is caught at compile time

**TypedId improvements we could make:**

```rust
// Add a convenience method to Context
impl Context {
    fn with_typed_widget<W: Widget>(
        &mut self,
        id: TypedId<W>,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<()>
    ) -> Result<()>;
}

// Then usage becomes:
ctx.with_typed_widget(list.item(0).unwrap(), |btn, ctx| {
    btn.set_label(ctx, "Updated")?;
    Ok(())
})?;
```

**Decision: Use typed `List<W: Widget>`**

The typed approach fits List well because:
1. List items are typically uniform in practice
2. Compile-time type safety catches errors early
3. Matches the existing `List<T: ListItem>` generic pattern
4. TypedId infrastructure already exists

For genuinely heterogeneous needs, users can use `List<Box<dyn Widget>>` or we could provide a
separate `MixedList` type.


### Example: termgym After

```rust
// Before: 60 lines of TermItem + ListItem impl + entry_lines helper

// After: Use existing Button widget directly with typed List
fn add_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
    let index = self.terminals.len() + 1;
    let button = Button::new(index.to_string())
        .with_glyphs(boxed::SINGLE);

    self.with_list(c, |list: &mut List<Button>, ctx| {
        let id: TypedId<Button> = list.append(ctx, button)?;
        Ok(())
    })?;

    // ... rest of terminal creation
}
```

### Example: listgym After

```rust
// Before: Block struct with manual text wrapping and scroll handling

// After: Use a simple text widget with typed List
fn create_column(c: &mut dyn Context) -> Result<(NodeId, NodeId)> {
    let list_id = c.add_orphan(List::<TextBlock>::new());

    for i in 0..10 {
        let text_block = TextBlock::new(TEXT)
            .with_wrap_width(rand::random_range(10..150))
            .with_style(COLORS[i % 2]);

        c.with_widget(list_id, |list: &mut List<TextBlock>, ctx| {
            list.append(ctx, text_block)
        })?;
    }

    let frame_id = c.add_orphan(frame::Frame::new());
    c.mount_child_to(frame_id, list_id)?;
    Ok((frame_id, list_id))
}
```


## Focus and Selection

### Chosen Approach: Focus-Based Selection

Selection follows focus. Item widgets implement `accept_focus() -> true`, and the focused item is
the selected item. This integrates naturally with canopy's focus system:

- Tab navigation between items works automatically
- Items can receive keyboard events directly
- `is_focused()` and `is_on_focus_path()` provide styling hooks

```rust
// Item widgets accept focus
impl Widget for MyListItem {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let style = if ctx.is_focused() {
            "item/focused"
        } else if ctx.is_on_focus_path() {
            "item/active"
        } else {
            "item"
        };
        // ...
    }
}
```

### Retaining Selection When List Loses Focus

When the user tabs away from the list entirely, the list must visually retain a "last selected"
indicator. Since `is_on_focus_path()` will be false for all items when focus is elsewhere, we need
a separate mechanism.

**Solution: List tracks selection independently and uses style layers**

The List widget maintains its own `selected: Option<usize>` that follows focus when the list is
active, but persists when focus leaves:

```rust
pub struct List<W: Widget> {
    items: Vec<TypedId<W>>,
    selected: Option<usize>,
    _marker: PhantomData<W>,
}

impl<W: Widget> List<W> {
    /// Called when focus changes within the list's subtree.
    fn on_child_focus_changed(&mut self, ctx: &dyn ViewContext) {
        // Update selected to match the newly focused child
        for (i, id) in self.items.iter().enumerate() {
            if ctx.node_is_on_focus_path((*id).into()) {
                self.selected = Some(i);
                return;
            }
        }
    }
}

impl<W: Widget + Send + 'static> Widget for List<W> {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("list");

        // Push "selected" layer for the selected item regardless of focus state
        // Items inherit this layer and can style accordingly
        for (i, id) in self.items.iter().enumerate() {
            if Some(i) == self.selected {
                rndr.push_layer("selected");
            }
            // Child rendering happens here via layout system
            if Some(i) == self.selected {
                rndr.pop_layer();
            }
        }
        Ok(())
    }
}
```

**No core extensions required.** The List widget:
1. Tracks `selected` index that follows focus when active
2. Pushes a "selected" style layer during render for the selected item
3. Items use both `is_on_focus_path()` (for focused styling) AND check for inherited "selected"
   layer (for selection styling when list isn't focused)

Items can then distinguish three states:
- **Focused**: `is_focused()` is true (keyboard input goes here)
- **Selected but not focused**: In "selected" style layer, but `is_focused()` is false
- **Neither**: Normal state

Note: The List needs to detect when focus changes within its subtree. This could be done via:
- Polling in `render()` to check if focus path has changed
- A focus change event/callback system (if we add one)
- Checking `focus_path()` changes between renders


## Multi-Select

Multi-select (checkbox-style selection where multiple items can be "checked") is handled by item
widgets themselves, not by the List. An item widget can track its own `checked: bool` state and
render a checkbox indicator. The List's single-selection concept is separate from this - it's about
which item has focus/is active, not which items are "checked".


## Horizontal Layout

Horizontal layout is not supported initially. The List widget arranges items vertically. If
horizontal layout is needed in the future, it can be added as a configuration option or a separate
`HorizontalList` widget.


## Performance Analysis

### The Concern

The current `ListItem` design was presumably chosen for performance - avoiding widget tree overhead
for potentially thousands of items. Let's examine if this is actually a problem.

### Widget Overhead

Each widget in the tree has:
- A `NodeId` (8 bytes, arena index)
- A `WidgetNode` struct containing:
  - `Box<dyn Widget>` (pointer + vtable)
  - Layout info (`Layout`, `View`)
  - Children vec
  - Parent pointer
- Style resolution during render

For 1000 items, this is roughly:
- 1000 arena slots (~100KB with typical node size)
- 1000 heap allocations for `Box<dyn Widget>`

### Current ListItem Overhead

With `ListItem`:
- Items stored in a `Vec<N>` inside the `List` widget
- Each item is whatever size `N` is
- No arena overhead per item

### Real-World Considerations

1. **Terminal UI scale**: Lists rarely exceed a few hundred visible items. Even file browsers with
   10,000 files typically virtualize.

2. **Measure/layout cost**: The current design already measures every item in `List::measure()` and
   `List::canvas()`. Widget-based items would have the same measurement cost.

3. **Render cost**: Only visible items render. With either design, we iterate items and render
   visible ones. The overhead is checking visibility, not widget machinery.

4. **Virtualization potential**: A widget-based list could implement virtualization - only mounting
   widgets for visible items, recycling widgets as the user scrolls. This is actually *easier* with
   widgets because we can unmount/remount.

### Verdict: Performance is NOT a real concern

For typical TUI use cases (lists of 10-1000 items), the overhead is negligible. The benefits of
widget composition, focus integration, and code reuse far outweigh theoretical performance costs.

If we ever need lists with 100,000+ items, we can implement virtualization at the `List` level,
mounting/unmounting item widgets as needed.


## Migration Strategy

Complete clean break. The old `ListItem` trait and `List<T: ListItem>` implementation will be
deleted and replaced with the new widget-based `List`. All examples (termgym, listgym, intervals)
will be updated in the same change.


## Recommendation

Proceed with widget-based list items using the typed `List<W: Widget>` approach. The benefits are
compelling:

1. **Composition**: Items can use `Box`, `Center`, `Text`, `Button`, etc.
2. **Focus integration**: Natural keyboard navigation and focus indication
3. **Code reduction**: termgym's `TermItem` becomes trivial or unnecessary
4. **Consistency**: Items are widgets like everything else in canopy
5. **Type safety**: Compile-time guarantees for item types via `TypedId<W>`

The "performance" argument for the current design doesn't hold up under scrutiny for TUI scale.


## Future Considerations

**Improving TypedId usage in other widgets**: While not part of this change, we could later update
widgets like `Button` to use `TypedId<Box>` and `TypedId<Text>` internally for documentation value.
This is low priority since these are internal implementation details.

**Adding `with_typed_widget` to Context**: A convenience method that takes `TypedId<W>` and
guarantees the closure receives `&mut W` would improve ergonomics. This could be added as part of
this work or separately.


---

## Staged Execution Plan

This plan uses the typed `List<W: Widget>` approach with `TypedId<W>` for compile-time type safety.

### Stage 1: Core List Widget

Implement the new typed List widget with basic functionality. Tests should pass after this stage.

1. [ ] Delete the existing `ListItem` trait and `List<T: ListItem>` from `widgets/list.rs`
2. [ ] Define new `List<W: Widget>` struct with `items: Vec<TypedId<W>>` and `selected: Option<usize>`
3. [ ] Implement `append()` returning `TypedId<W>` and `insert()` that mount widgets as children
4. [ ] Implement `remove()` that unmounts and removes from the items vec
5. [ ] Implement `item(index) -> Option<TypedId<W>>` for typed item access
6. [ ] Implement `Widget` trait with column layout for children
7. [ ] Implement `measure()` and `canvas()` that sum child heights
8. [ ] Add basic scrolling support (the List acts as a scrollable viewport)
9. [ ] Write unit tests for add/remove/measure with typed access


### Stage 2: Selection and Focus

Add selection tracking and focus integration.

1. [ ] Track `selected: Option<usize>` that follows focus
2. [ ] Detect focus changes within subtree (check focus path in render or poll)
3. [ ] Push "selected" style layer for the selected item during render
4. [ ] Implement `select()` method to programmatically select an item
5. [ ] Implement `ensure_selected_visible()` to scroll selected item into view
6. [ ] Add `select_next()`, `select_prev()`, `select_first()`, `select_last()` commands
7. [ ] Add `page_up()`, `page_down()` commands
8. [ ] Register commands via `Loader` and `derive_commands`


### Stage 3: Migrate Examples

Update all examples to use the new typed List. Each example should work after its migration.

1. [ ] **termgym**: Remove `TermItem` and `entry_lines()`. Use `List<Button>` directly.
       Update `setup_bindings()` styles.
2. [ ] **listgym**: Remove `Block` struct. Create a simple `TextBlock` widget. Use `List<TextBlock>`.
       Update `create_column()` and related functions.
3. [ ] **intervals**: Remove `IntervalItem`. Create a simple `CounterWidget`. Use `List<CounterWidget>`.
       Update `with_list()` calls.
4. [ ] Verify all three examples run correctly with `cargo run --example <name>`


### Stage 4: Cleanup and Polish

Final cleanup and documentation.

1. [ ] Remove any dead code from the old ListItem implementation
2. [ ] Update module documentation in `widgets/list.rs`
3. [ ] Ensure all public items have doc comments
4. [ ] Run clippy and fix any warnings
5. [ ] Run `cargo fmt`
6. [ ] Final test run: `cargo nextest run --all --all-features`
