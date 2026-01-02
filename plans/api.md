# Canopy API Review

Analysis of `canopy`, `canopy-geom`, and `canopy-widgets` crates for opportunities to improve
consistency, conceptual soundness, ergonomics, and power.

---

## 1. Context API Inconsistency: `&mut dyn Context` vs `&dyn ViewContext`

**Issue:** The API splits context into two traits with different capabilities, but the naming
doesn't clearly communicate the distinction. `ViewContext` is read-only; `Context` is mutable.
However:
- Many methods on `Context` don't actually need mutation
- The naming `ViewContext` suggests it's about rendering, but it's used in `accept_focus` and
  other non-render scenarios

**Recommendation:** Consider unifying into a single `Context` with clear sections, or rename
`ViewContext` to `ReadContext` to clarify the actual distinction.

**Before (focusgym.rs:137):**
```rust
fn accept_focus(&self, ctx: &dyn ViewContext) -> bool {
    ctx.children().is_empty()
}
```

**After:**
```rust
fn accept_focus(&self, ctx: &dyn ReadContext) -> bool {
    ctx.children().is_empty()
}
```

---

## 2. Inconsistent Focus Navigation Methods

**Issue:** The `Context` trait has a proliferation of focus methods with inconsistent naming
patterns:

```rust
fn focus_dir(&mut self, dir: Direction) {}
fn focus_dir_in(&mut self, root: NodeId, dir: Direction);
fn focus_dir_global(&mut self, dir: Direction) {}
fn focus_first(&mut self) {}
fn focus_first_in(&mut self, root: NodeId);
fn focus_first_global(&mut self) {}
// ... and so on for next, prev, right, left, up, down
```

This creates 24+ methods where a more composable approach would suffice.

**Recommendation:** Consolidate into fewer methods with explicit scope parameters:

**Before (focusgym.rs:99):**
```rust
c.focus_next();
```

**After:**
```rust
// Option A: Enum for scope
c.focus(FocusMove::Next, FocusScope::Subtree);

// Option B: Builder pattern
c.focus().next().in_subtree();
```

However, if you want to preserve discoverability, consider keeping the explicit method names but
organizing them into a sub-object:

```rust
c.focus.next();           // current subtree (default)
c.focus.next_in(root);    // specific subtree
c.focus.next_global();    // from tree root
```

---

## 3. Inconsistent `with_*` Pattern Returns

**Issue:** The `with_*` methods have inconsistent return patterns:

```rust
// Returns the closure's return value
fn with_widget<W, R>(&mut self, node: NodeId, f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>) -> Result<R>

// Returns Result<()> even when closure returns something
fn with_layout(&mut self, f: &mut dyn FnMut(&mut Layout)) -> Result<()>

// Some use closures, some use references
fn with_unique_descendant<W, R>(&mut self, f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>) -> Result<R>
```

**Recommendation:** Standardize the `with_*` pattern:

**Before (todo/src/lib.rs:164-170):**
```rust
c.with_layout(&mut |layout| {
    *layout = Layout::fill().direction(Direction::Stack);
})?;
```

**After:**
```rust
c.update_layout(|layout| {
    *layout = Layout::fill().direction(Direction::Stack);
})?;

// Or even simpler:
c.set_layout(Layout::fill().direction(Direction::Stack))?;
```

---

## 4. Keyed Children API Ergonomics

**Issue:** The keyed children API requires separate methods and string keys:

```rust
c.add_child_keyed("main", MainContent)?;
c.child_keyed("main");
c.with_keyed::<Modal, _>("modal", |_, ctx| { ... })?;
```

**Recommendation:** Use typed keys for compile-time safety and better ergonomics:

**Before (todo/src/lib.rs:152-158):**
```rust
if c.child_keyed("main").is_some() {
    return Ok(());
}
let main_content_id = c.add_child_keyed("main", MainContent)?;
```

**After (with typed key struct):**
```rust
struct MainKey;
impl ChildKey for MainKey {
    type Widget = MainContent;
}

// Usage becomes type-safe and auto-completing:
if c.has_child::<MainKey>() {
    return Ok(());
}
let main_content_id = c.add_child::<MainKey>(MainContent::new())?;
```

---

## 5. Layout Builder Lacks Symmetry

**Issue:** The `Layout` builder has asymmetric methods:

```rust
Layout::column()
    .flex_horizontal(1)      // sets width to Flex
    .flex_vertical(1)        // sets height to Flex
    .fixed_width(10)         // convenience for width = Fixed
    .fixed_height(10)        // convenience for height = Fixed
    .min_width(1)            // sets min_width
    .min_height(1)           // sets min_height
```

But there's no `width()` or `height()` that accepts a `Sizing` directly, and `Sizing::Measure` is
completely missing from the builder.

**Recommendation:** Add symmetric sizing methods:

**Before (listgym.rs:177-179):**
```rust
c.with_layout_of(list_id, &mut |layout| {
    *layout = Layout::fill();
})?;
```

**After:**
```rust
// Direct sizing methods
Layout::column()
    .width(Sizing::Flex(1))
    .height(Sizing::Measure)

// Or shorthand
Layout::column()
    .measured()          // both axes use Measure
    .flex_width(1)       // override width only
```

---

## 6. Event Outcome Semantics

**Issue:** `EventOutcome::Handle` vs `EventOutcome::Consume` distinction is subtle and potentially
confusing:

```rust
pub enum EventOutcome {
    Handle,  // "processed and propagation stops"
    Consume, // "processed without state change and propagation stops"
    Ignore,  // "bubble up"
}
```

Both `Handle` and `Consume` stop propagation - the difference is whether a re-render is triggered.
This is non-obvious.

**Recommendation:** Rename for clarity:

```rust
pub enum EventOutcome {
    Handled,           // State changed, stop propagation, re-render
    ConsumedSilently,  // No state change, stop propagation, no re-render
    Ignored,           // Bubble up to parent
}
```

Or consider a struct-based approach:

```rust
pub struct EventOutcome {
    pub propagate: bool,
    pub needs_render: bool,
}

impl EventOutcome {
    pub fn handled() -> Self { Self { propagate: false, needs_render: true } }
    pub fn consumed() -> Self { Self { propagate: false, needs_render: false } }
    pub fn ignored() -> Self { Self { propagate: true, needs_render: false } }
}
```

---

## 7. Missing Expressive Power: No Batch Child Operations

**Issue:** Adding multiple children requires separate calls:

**Before (focusgym.rs:203-206):**
```rust
let root_block = c.add_child(Block::new(true))?;
c.add_child_to(root_block, Block::new(false))?;
c.add_child_to(root_block, Block::new(false))?;
```

**Recommendation:** Add batch operations:

```rust
let root_block = c.add_child(Block::new(true))?;
c.add_children_to(root_block, [
    Block::new(false),
    Block::new(false),
])?;

// Or builder pattern:
c.add_child(Block::new(true))?
    .with_children([
        Block::new(false),
        Block::new(false),
    ])?;
```

---

## 8. VStack/Panes Widget Inconsistency

**Issue:** `VStack` uses a builder pattern with `push_flex`/`push_fixed`, while `Panes` uses method
calls:

**Before (listgym.rs:224-229):**
```rust
// VStack: builder pattern with detached nodes
c.add_child(
    VStack::new()
        .push_flex(panes_id, 1)
        .push_fixed(status_id, 1),
)?;

// Panes: method calls after construction
c.with_widget(panes_id, |panes: &mut Panes, ctx| {
    panes.insert_col(ctx, frame_id)
})?;
```

**Recommendation:** Unify the pattern - either both use builders or both use mutation:

```rust
// Consistent builder approach:
c.add_child(
    VStack::new()
        .child(panes_id, Sizing::Flex(1))
        .child(status_id, Sizing::Fixed(1)),
)?;

c.add_child(
    Panes::new()
        .column(frame_id),
)?;
```

---

## 9. Script vs Command Binding Inconsistency

**Issue:** Key bindings can use either scripts (string) or typed commands, with different methods:

```rust
// Script-based (string, runtime errors)
.key('j', "list::select_next()")

// Command-based (typed, compile-time safe)
.key_command('j', List::<TodoEntry>::cmd_select_next())
```

**Recommendation:** Favor typed commands and improve ergonomics:

**Before (listgym.rs:274):**
```rust
.key('j', "list::select_next()")
```

**After:**
```rust
// More discoverable with better IDE support
.key('j', list::select_next)

// Or with explicit dispatch target
.key('j', List::cmd_select_next())
```

---

## 10. Geometry Types Could Be More Ergonomic

**Issue:** Point and Rect construction requires explicit struct building:

```rust
Point { x: 10, y: 20 }
Rect::new(0, 0, 100, 50)
```

**Recommendation:** Add tuple conversions and named constructors:

**Before:**
```rust
let p = Point { x: 10, y: 20 };
let r = Rect::new(0, 0, 100, 50);
```

**After (already partially supported, extend further):**
```rust
let p: Point = (10, 20).into();
let r: Rect = (0, 0, 100, 50).into();  // Already exists

// Add named constructors
let r = Rect::from_size(100, 50);        // at origin
let r = Rect::from_center((50, 25), 100, 50);
```

---

## Summary of Highest-Impact Changes

| Priority | Change | Impact |
|----------|--------|--------|
| High | Rename `ViewContext` -> `ReadContext` | Clarity |
| High | Consolidate focus navigation methods | API surface reduction |
| High | Add `c.set_layout()` convenience method | Ergonomics |
| Medium | Typed child keys | Type safety |
| Medium | Rename `EventOutcome` variants | Clarity |
| Medium | Batch child operations | Ergonomics |
| Low | Unify VStack/Panes patterns | Consistency |
