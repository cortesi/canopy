Static review plus validation against the current repo state on 2026-01-11 (code inspection only;
no test runs yet). The big picture: the core ideas are solid (tree + context + layout + commands +
bindings), but a few hotspots are costing you ergonomics and simplicity: **effect stacking**,
**widget borrowing**, **Context surface area**, and **API duplication (bindings / command
plumbing)**. Below are concrete, “LLM-implementable” changes that should reduce line count and
indirection while improving power.

---

## 1) Fix a real correctness bug: unsigned `ToArgValue` is encoding as `Int`

### Rationale

In `crates/canopy/src/core/commands.rs`, unsigned primitives `u8/u16/u32` currently encode as `ArgValue::Int(i64)` instead of `ArgValue::UInt(u64)`. It “works” because `FromArgValue` accepts `Int|UInt` for unsigned, but it’s semantically wrong and will surprise anyone inspecting args / serializing / round-tripping through other systems.

### Validation (2026-01-11)

Confirmed in `crates/canopy/src/core/commands.rs`: `impl_uint_to_arg_value!` maps `u8/u16/u32`
to `ArgValue::Int(i64::from(self))`, while `u64` and `usize` already encode as
`ArgValue::UInt`. There is no existing test under `crates/canopy/tests` that asserts unsigned
encoding or round-tripping of `ArgValue::UInt` back into `u32`.

### Before

```rust
macro_rules! impl_uint_to_arg_value {
    ($($t:ty),*) => {
        $(
        impl ToArgValue for $t {
            fn to_arg_value(self) -> ArgValue {
                ArgValue::Int(i64::from(self))
            }
        }
        )*
    };
}

impl_uint_to_arg_value!(u8, u16, u32);
```

### After

```rust
macro_rules! impl_uint_to_arg_value {
    ($($t:ty),*) => {
        $(
        impl ToArgValue for $t {
            fn to_arg_value(self) -> ArgValue {
                ArgValue::UInt(u64::from(self))
            }
        }
        )*
    };
}

impl_uint_to_arg_value!(u8, u16, u32);
```

### Add a regression test (cheap + high value)

Create `crates/canopy/tests/test_arg_value_uint.rs`:

```rust
use canopy::commands::{ArgValue, ToArgValue, FromArgValue};

#[test]
fn u32_encodes_as_uint() {
    let v = (123u32).to_arg_value();
    assert!(matches!(v, ArgValue::UInt(_)));
    let back = u32::from_arg_value(v).unwrap();
    assert_eq!(back, 123);
}
```

---

## 2) Make `ReadContext` truly read-only: move `invalidate_layout` onto `Context` and delete `Cell<bool>` indirection

### Rationale

`ReadContext::invalidate_layout(&self)` is a mutation disguised as a read. You had to use `Cell<bool>` (`layout_dirty: Cell<bool>`) to make it work. This increases “spooky action at a distance”, complicates reasoning, and forces extra interior mutability.

I searched usage: widgets call `invalidate_layout()` from event/command handlers (which already have `&mut dyn Context`). I didn’t find a compelling need to invalidate layout from render-only contexts.

### Validation (2026-01-11)

Confirmed: `ReadContext` declares `invalidate_layout(&self)` and both `CoreContext` and
`CoreViewContext` implement it by setting `layout_dirty: Cell<bool>` in
`crates/canopy/src/core/context.rs`. `Node` stores `layout_dirty: Cell<bool>` in
`crates/canopy/src/core/node.rs`, and `refresh_layouts` uses `get`/`set` in
`crates/canopy/src/core/world.rs`. A repo-wide search (`rg "invalidate_layout("`) shows call sites
only in `crates/canopy-widgets/src/dropdown.rs` and `crates/examples/src/intervals.rs`, both on
`&mut dyn Context` in command/event handlers; no render-only uses found.

This change:

* **simplifies mental model**
* removes `Cell` from node state
* shrinks `CoreViewContext` and dummy contexts
* reduces “how is this allowed?” moments

### What to change

1. In `core/context.rs`:

* Remove from `ReadContext`:

  ```rust
  fn invalidate_layout(&self);
  ```
* Add to `Context`:

  ```rust
  fn invalidate_layout(&mut self);
  ```

2. In `core/node.rs` / node struct in `core/world.rs`:

* Change:

  ```rust
  layout_dirty: Cell<bool>,
  ```

  to:

  ```rust
  layout_dirty: bool,
  ```

3. Update layout refresh code in `core/world.rs`:

### Before

```rust
fn refresh_layouts(core: &mut Core) {
    for (_id, node) in core.nodes.iter_mut() {
        if !node.layout_dirty.get() { continue; }
        if let Some(widget) = node.widget.as_ref() {
            node.layout = widget.layout();
        }
        node.layout_dirty.set(false);
    }
}
```

### After

```rust
fn refresh_layouts(core: &mut Core) {
    for (_id, node) in core.nodes.iter_mut() {
        if !node.layout_dirty { continue; }
        if let Some(widget) = node.widget.as_ref() {
            node.layout = widget.layout();
        }
        node.layout_dirty = false;
    }
}
```

4. Update `CoreContext::invalidate_layout` to set the bool:

```rust
fn invalidate_layout(&mut self) {
    if let Some(node) = self.core.nodes.get_mut(self.node_id) {
        node.layout_dirty = true;
    }
}
```

5. Delete `CoreViewContext::invalidate_layout` entirely.

This is a net **line-count reduction** and removes a conceptual “trap door”.

---

## 3) Remove per-frame allocations and simplify effects: replace `Box + box_clone()` with `Arc`

### Rationale

Rendering currently clones effects via `box_clone()` at every node during traversal:

* `Node.effects: Option<Vec<Box<dyn StyleEffect>>>`
* traversal pushes `effect.box_clone()` into a stack each render

That is **an allocation per pushed effect per frame** (even though effects like `Dim` are tiny `Copy` types). Also `StyleEffect::box_clone` is boilerplate and leaks an implementation detail into the trait.

Switching to `Arc<dyn StyleEffect>`:

* removes `box_clone()` method and the `Clone for Box<dyn StyleEffect>` impl
* makes per-frame stacking clones cheap (atomic refcount bump)
* simplifies the trait and call sites

### Validation (2026-01-11)

Confirmed: `StyleEffect` defines `box_clone` and `Clone for Box<dyn StyleEffect>` in
`crates/canopy/src/core/style/effects.rs`, and helpers like `dim` return `Box<dyn StyleEffect>`.
`Node.effects` stores `Option<Vec<Box<dyn StyleEffect>>>` in `crates/canopy/src/core/node.rs`.
`core/canopy.rs` pushes `effect.box_clone()` into a per-frame `Vec<Box<dyn StyleEffect>>`, and
`core/render.rs` consumes `&[Box<dyn StyleEffect>]`.

### Before (`core/style/effects.rs`)

```rust
pub trait StyleEffect: Send + std::fmt::Debug {
    fn apply(&self, style: Style) -> Style;
    fn box_clone(&self) -> Box<dyn StyleEffect>;
}

impl Clone for Box<dyn StyleEffect> {
    fn clone(&self) -> Self { self.box_clone() }
}
```

### After

```rust
use std::sync::Arc;

pub type Effect = Arc<dyn StyleEffect>;

pub trait StyleEffect: Send + Sync + std::fmt::Debug {
    fn apply(&self, style: Style) -> Style;
}
```

### Node storage change

In node struct:

```rust
pub(crate) effects: Option<Vec<Effect>>,
```

### Render stack change (`core/canopy.rs`)

#### Before

```rust
let mut effect_stack: Vec<Box<dyn StyleEffect>> = Vec::new();
// ...
if let Some(local) = self.core.nodes[node_id].effects.as_ref() {
    for effect in local {
        traversal.effect_stack.push(effect.box_clone());
    }
}
```

#### After

```rust
let mut effect_stack: Vec<style::effects::Effect> = Vec::new();
// ...
if let Some(local) = self.core.nodes[node_id].effects.as_ref() {
    traversal.effect_stack.extend(local.iter().cloned());
}
```

### Render API change (`core/render.rs`)

#### Before

```rust
effects: &'a [Box<dyn StyleEffect>],
pub fn with_effects(mut self, effects: &'a [Box<dyn StyleEffect>]) -> Self
```

#### After

```rust
effects: &'a [style::effects::Effect],
pub fn with_effects(mut self, effects: &'a [style::effects::Effect]) -> Self
```

### Context API: keep ergonomics

Change:

```rust
fn push_effect(&mut self, node: NodeId, effect: Box<dyn StyleEffect>) -> Result<()>;
```

to:

```rust
fn push_effect(&mut self, node: NodeId, effect: style::effects::Effect) -> Result<()>;
```

Then update helpers in `effects.rs`:

```rust
pub fn dim(amount: f64) -> Effect {
    Arc::new(Dim { amount })
}
```

This change is a **big win** for both performance and simplification.

---

## 4) Kill the custom widget “slot take/restore” guard: store widgets in `RefCell` and use `try_borrow_mut` for reentrancy-safe errors

### Rationale

`WidgetSlotGuard` in `core/world.rs` temporarily `take()`s the widget out of the node so you can mutably borrow both widget and core.

This creates a nasty failure mode: **re-entrant calls on the same node panic** with “Widget missing from node”. That’s easy to trigger accidentally if a widget calls `ctx.with_widget_mut(self_id, ...)` during its own event handling (or indirectly via helper code).

A simpler, more standard approach:

* store widget as `RefCell<Box<dyn Widget>>`
* borrow with `try_borrow_mut()` and turn borrow failures into a structured error (`Error::ReentrantWidgetBorrow(node_id)`)

This removes:

* `WidgetSlotGuard` struct
* `NonNull` pointer juggling
* the “widget temporarily disappears” state

### Validation (2026-01-11)

Confirmed: `WidgetSlotGuard` in `crates/canopy/src/core/world.rs` calls
`core.nodes[node_id].widget.take()` and `widget_mut()` panics with
`expect("Widget missing from node")`.
`Core::with_widget_mut` and `Core::with_widget_view` both depend on the guard, so any re-entrant
attempt to borrow the same widget will hit the missing-widget panic rather than returning a
structured error.

### Concrete change

In `core/node.rs` / node struct:

#### Before

```rust
pub(crate) widget: Option<Box<dyn Widget>>,
```

#### After

```rust
use std::cell::RefCell;

pub(crate) widget: RefCell<Box<dyn Widget>>,
```

Node initialization:

```rust
widget: RefCell::new(Box::new(root_widget)),
```

Then rewrite `Core::with_widget_mut`:

#### Before (current shape)

```rust
pub(crate) fn with_widget_mut<R>(&mut self, node_id: NodeId, f: impl FnOnce(&mut dyn Widget, &mut Self) -> R) -> R {
    let mut guard = WidgetSlotGuard::new(self, node_id);
    let core_ptr: *mut Self = self;
    let core: &mut Self = unsafe { &mut *core_ptr };
    f(guard.widget_mut(), core)
}
```

#### After (no take/restore)

```rust
pub(crate) fn with_widget_mut<R>(
    &mut self,
    node_id: NodeId,
    f: impl FnOnce(&mut dyn Widget, &mut Self) -> R,
) -> Result<R> {
    // Take a raw pointer to self to work around borrow splitting.
    let core_ptr: *mut Self = self;

    // Borrow the widget via RefCell with runtime checking.
    let widget_ref = self.nodes.get(node_id)
        .ok_or(Error::NodeNotFound(node_id))?
        .widget
        .try_borrow_mut()
        .map_err(|_| Error::Internal(format!("re-entrant widget borrow: {node_id:?}")))?;

    // SAFETY: core_ptr is self; widget borrow is tracked by RefCell.
    let core: &mut Self = unsafe { &mut *core_ptr };
    Ok(f(&mut **widget_ref, core))
}
```

You’ll also update callers to handle `Result<R>`. In practice, most of your public APIs already return `Result`, so this tends to *reduce panics* without making user code noisier.

Do the same for `with_widget_view`.

If you want a “nicer” error, add:

```rust
Error::ReentrantWidgetBorrow(NodeId)
```

This is a high-impact correctness + ergonomics fix.

---

## 5) Shrink `Context` impls: make scrolling/page/line helpers default methods, implement only `scroll_to` + `scroll_by`

### Rationale

`CoreContext` currently implements `page_up`, `page_down`, `scroll_up`, `scroll_left`, etc. These are pure wrappers around `scroll_by` and view sizing. They add ~80 lines of duplication and expand the trait surface area.

Make them default methods on `Context` (like you already do for focus direction wrappers), so the only required primitives are:

* `scroll_to`
* `scroll_by`

### Validation (2026-01-11)

Confirmed: `Context` currently requires `page_up`, `page_down`, `scroll_up`, `scroll_down`,
`scroll_left`, and `scroll_right` in `crates/canopy/src/core/context.rs`, and `CoreContext`
implements each wrapper explicitly (see the scroll methods around the `scroll_to`/`scroll_by`
implementation).

### Before (CoreContext has lots of wrappers)

```rust
fn page_up(&mut self) -> bool { ... }
fn page_down(&mut self) -> bool { ... }
fn scroll_up(&mut self) -> bool { ... }
fn scroll_down(&mut self) -> bool { ... }
// etc
```

### After (in the `Context` trait)

```rust
fn scroll_to(&mut self, x: i32, y: i32) -> bool;
fn scroll_by(&mut self, x: i32, y: i32) -> bool;

fn scroll_up(&mut self) -> bool { self.scroll_by(0, -1) }
fn scroll_down(&mut self) -> bool { self.scroll_by(0, 1) }
fn scroll_left(&mut self) -> bool { self.scroll_by(-1, 0) }
fn scroll_right(&mut self) -> bool { self.scroll_by(1, 0) }

fn page_up(&mut self) -> bool {
    let h = self.view().content.h as i32;
    self.scroll_by(0, -h)
}
fn page_down(&mut self) -> bool {
    let h = self.view().content.h as i32;
    self.scroll_by(0, h)
}
```

Then delete the wrapper implementations from `CoreContext`.

Net effect: **less code, less API surface, fewer places for scroll semantics to diverge**.

---

## 6) Add “configured add” helpers to collapse the most common boilerplate: `add_* + set_layout_of` (and optionally `set_children_of`)

### Rationale

In real widget code (example: `examples/todo`), the dominant pattern is:

1. add child/keyed child
2. set layout
3. maybe set children ordering

This is repetitive and makes the library feel “ceremony heavy”. You can keep the primitives but add a couple of *surgical* helpers that drastically reduce user line count.

### Validation (2026-01-11)

Confirmed: there are no existing `add_child_to_with_layout` or `add_keyed_to_with_layout` helpers
in `crates/canopy/src/core/context.rs` (repo-wide `rg` finds none). `examples/todo/src/lib.rs`
shows the repeated add-then-`set_layout_of` pattern in `ensure_tree` and `ensure_modal`.

### Add these methods (in `impl dyn Context` in `core/context.rs`)

```rust
pub fn add_child_to_with_layout<W: Widget + 'static>(
    &mut self,
    parent: impl Into<NodeId>,
    widget: W,
    layout: Layout,
) -> Result<TypedId<W>> {
    let id = self.add_child_to(parent, widget)?;
    self.set_layout_of(id, layout)?;
    Ok(id)
}

pub fn add_keyed_to_with_layout<K: ChildKey, W: Widget + 'static>(
    &mut self,
    parent: impl Into<NodeId>,
    widget: W,
    layout: Layout,
) -> Result<TypedId<W>> {
    let id = self.add_keyed_to::<K>(parent, widget)?;
    self.set_layout_of(id, layout)?;
    Ok(id)
}
```

### Before (from `examples/todo/src/lib.rs`)

```rust
let main_content_node = c.add_keyed::<MainSlot>(MainContent)?;
c.set_layout_of(
    main_content_node,
    Layout::fill().direction(Direction::Stack),
)?;

let list_node = c.add_child_to(main_content_node, Frame::new())?;
c.set_layout_of(list_node, Layout::fill().padding(1))?;

let todo_node = c.add_keyed_to::<TodoSlot>(
    list_node,
    List::new(...)
)?;
c.set_layout_of(todo_node, Layout::fill())?;
```

### After

```rust
let main_content_node = c.add_keyed_to_with_layout::<MainSlot>(
    c.node_id(),
    MainContent,
    Layout::fill().direction(Direction::Stack),
)?;

let list_node = c.add_child_to_with_layout(
    main_content_node,
    Frame::new(),
    Layout::fill().padding(1),
)?;

let todo_node = c.add_keyed_to_with_layout::<TodoSlot>(
    list_node,
    List::new(...),
    Layout::fill(),
)?;
```

This is pure ergonomic sugar, but it’s exactly the kind of sugar that makes a UI tree library feel *pleasant*.

---

## 7) Reduce command-binding pain: push users toward typed `CommandCall` and update examples/docs to stop escaping strings

### Rationale

Your docs already support typed command binding (`Root::cmd_quit().call()` etc), but `examples/todo` still uses scripts like:

```rust
.todo("page(\\\"down\\\")")
```

That’s brittle and makes the library look worse than it is. Low-hanging fruit: update the example to use typed calls everywhere possible.

### Validation (2026-01-11)

Confirmed: `examples/todo/src/lib.rs` binds keys and mouse actions using string scripts like
`"todo::page(\\\"down\\\")"` (see the `Binder` usage near the default bindings). Typed binding
helpers (`key_command`, `mouse_command`) already exist in `crates/canopy/src/core/binder.rs`, and
typed calls are available via `CommandSpec::call`/`call_with` in
`crates/canopy/src/core/commands.rs`.

### Before (example)

```rust
Binder::new()
  .with_path("todo/")
  .try_key('j', "todo::page(\"down\")")?
  .try_key('k', "todo::page(\"up\")")?
  .try_key(' ', "todo::toggle_item()")?;
```

### After (typed, no string escaping)

```rust
Binder::new()
  .with_path("todo/")
  .key_command('j', Todo::cmd_page().call_with((VerticalDirection::Down,)))
  .key_command('k', Todo::cmd_page().call_with((VerticalDirection::Up,)))
  .key_command(' ', Todo::cmd_toggle_item().call());
```

### Extra ergonomic win: add a tiny macro for call_with tuples (optional)

If you want to reduce tuple noise, add:

```rust
macro_rules! call {
    ($spec:expr $(, $arg:expr )* $(,)?) => {
        $spec.call_with(($($arg,)*))
    };
}
```

Then:

```rust
.key_command('j', call!(Todo::cmd_page(), VerticalDirection::Down))
```

---

## 8) Unify and simplify binding APIs: one `bind()` with `BindingAction`, deprecate the 8 near-duplicates

### Rationale

`Canopy` currently has a matrix of methods:

* `bind_key`, `bind_mode_key`, `bind_mouse`, `bind_mode_mouse`
* `*_command` variants

It’s not *that* much code, but it fragments the mental model and adds surface area. You can preserve old APIs as thin wrappers but internally collapse to one path.

### Validation (2026-01-11)

Confirmed: `crates/canopy/src/core/canopy.rs` exposes `bind_key`, `bind_mode_key`, `bind_mouse`,
`bind_mode_mouse`, plus `*_command` variants for keys and mouse. `inputmap` already has `bind` and
`bind_command`, but there is no unified `BindingAction`-style entry point.

### Proposed design

```rust
pub enum BindingAction<'a> {
    Script(&'a str),
    ScriptId(ScriptId),
    Command(CommandInvocation),
}

pub fn bind(
    &mut self,
    mode: &str,
    input: InputSpec,
    path_filter: &str,
    action: BindingAction<'_>,
) -> Result<inputmap::BindingId> {
    match action {
        BindingAction::Script(src) => {
            let sid = self.compile_script(src)?;
            Ok(self.keymap.bind(mode, input, path_filter, sid))
        }
        BindingAction::ScriptId(sid) => Ok(self.keymap.bind(mode, input, path_filter, sid)),
        BindingAction::Command(cmd) => Ok(self.keymap.bind_command(mode, input, path_filter, cmd)),
    }
}
```

Then all existing methods become 1–3 lines each and can be marked `#[deprecated(note = "use bind()")]` later.

This improves flexibility (action is now first-class), reduces duplication, and makes future features (e.g., “binding chains”, “conditional bindings”) easier.

---

## 9) Decouple `Widget` from `CommandNode` so widgets without commands don’t need derive/macros/boilerplate

### Rationale

Right now every widget must satisfy:

```rust
pub trait Widget: Any + Send + CommandNode { ... }
```

That forces a bunch of empty `impl CommandNode for X` or `#[derive_commands] impl X { ... }` even when the widget defines *zero* commands (see `Frame`, `StatusBar`, etc.). It also makes the command system feel more invasive than necessary.

### Validation (2026-01-11)

Confirmed: `Widget` is defined as `pub trait Widget: Any + Send + CommandNode` in
`crates/canopy/src/widget/mod.rs`, and simple widgets implement `CommandNode` even when they have
no commands (e.g., `RootContainer` in `crates/canopy/src/core/world.rs`).

### Proposed change

1. Change the widget trait:

#### Before

```rust
pub trait Widget: Any + Send + CommandNode { ... }
```

#### After

```rust
pub trait Widget: Any + Send { ... }
```

2. Keep `CommandNode` as a separate optional capability:

```rust
pub trait CommandNode {
    fn commands() -> &'static [&'static CommandSpec];
}
```

3. `Canopy::add_commands<T: CommandNode>` stays exactly as-is.

This is a big ergonomic win:

* Most widgets stop caring about commands entirely.
* Command-heavy widgets still derive/implement `CommandNode`.
* Your examples and tests get simpler immediately.

(Yes, it’s an API break; it’s worth it.)

---

## 10) Add a first-class traversal helper to delete repeated DFS stacks sprinkled everywhere

### Rationale

You implement pre-order traversal by hand in multiple places:

* `ReadContext` helpers (`unique_descendant`, `descendants_of_type`, `first_leaf`, …)
* `Canopy::build_owner_target_index`
* focus/search code

A small iterator eliminates repetition and makes it harder for traversal semantics to drift.

### Validation (2026-01-11)

Confirmed: `ReadContext` traversal helpers (`first_from`, `all_from`, `descendants_of_type`,
`first_leaf`) each roll their own DFS stacks in `crates/canopy/src/core/context.rs`. A separate
pre-order stack exists in `Canopy::build_owner_target_index` in
`crates/canopy/src/core/canopy.rs`, and focus traversal helpers in
`crates/canopy/src/core/focus.rs` implement their own traversal logic. There is no shared iterator
or traversal helper today.

### Concrete addition (in `impl dyn ReadContext`)

```rust
pub fn preorder(&self, root: impl Into<NodeId>) -> Preorder<'_> {
    Preorder { ctx: self, stack: vec![root.into()] }
}

pub struct Preorder<'a> {
    ctx: &'a dyn ReadContext,
    stack: Vec<NodeId>,
}

impl<'a> Iterator for Preorder<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.stack.pop()?;
        let children = ReadContext::children_of(self.ctx, id);
        for ch in children.into_iter().rev() {
            self.stack.push(ch);
        }
        Some(id)
    }
}
```

Then rewrite helpers:

```rust
pub fn first_leaf(&self, root: impl Into<NodeId>) -> Option<NodeId> {
    self.preorder(root).find(|&id| ReadContext::children_of(self, id).is_empty())
}
```

This is one of those refactors that **reduces code and increases clarity**.

---

## 11) Make `NodeName` reject empty strings (small correctness + debugging win)

### Rationale

`valid_nodename("")` currently returns `true`. That allows empty node names, which can create weird paths, ambiguous matching, and ugly debugging output.

### Validation (2026-01-11)

Confirmed: `valid_nodename` in `crates/canopy/src/core/state.rs` checks only
`name.chars().all(valid_nodename_char)` so an empty string passes. `NodeName::convert` can return
an empty `name` when input has no valid characters.

### Before (`core/state.rs`)

```rust
pub fn valid_nodename(name: &str) -> bool {
    name.chars().all(valid_nodename_char)
}
```

### After

```rust
pub fn valid_nodename(name: &str) -> bool {
    !name.is_empty() && name.chars().all(valid_nodename_char)
}
```

Then ensure `NodeName::convert` never produces empty:

```rust
pub fn convert(s: impl AsRef<str>) -> Self {
    let raw = s.as_ref().to_case(Case::Snake);
    let filtered: String = raw.chars().filter(|c| valid_nodename_char(*c)).collect();
    let name = if filtered.is_empty() { "node".to_string() } else { filtered };
    Self { name }
}
```

---

## 12) Low-hanging “make it feel good”: add `canopy::prelude` and update examples

### Rationale

The library is currently import-heavy. A prelude is a pure ergonomics feature that also makes docs/examples shorter and clearer. This is cheap and high-value.

### Validation (2026-01-11)

Confirmed: there is no `prelude` module under `crates/canopy/src` (repo-wide search only finds
`proptest::prelude` usage). Examples and tests import from `canopy` directly rather than via a
prelude.

### Proposed `src/prelude.rs`

```rust
pub use crate::{
    Context, ReadContext, Widget,
    error::Result,
    layout::{Layout, Direction, Align, Constraint, Display},
    geom::{Point, Rect, Expanse},
    render::Render,
    event::{Event, Key, mouse},
    state::NodeName,
    key,
};
```

Then examples become:

```rust
use canopy::prelude::*;
```

This doesn’t reduce *library* LOC much, but it makes *user* LOC drop immediately (and improves adoption).

---

# Suggested implementation order (minimize pain, maximize payoff)

1. **Fix unsigned `ToArgValue`** + add tests (safe, surgical).
2. **Make `NodeName` reject empty strings** (small correctness + debugging win).
3. **Switch effects to `Arc`** (big win, relatively contained).
4. **Default scroll wrappers** (pure deletion of duplicated code).
5. **Add “configured add” helpers** (immediate ergonomics win; no breaking change).
6. **Update `examples/todo` to typed command calls** (marketing + correctness).
7. **Add `canopy::prelude` + update examples** (ergonomics, no behavior change).
8. **Make `ReadContext` truly read-only** (mild breaking change, simplifies internals).
9. **Replace widget slot guard with `RefCell` + `try_borrow_mut`** (big correctness win).
10. **Decouple `Widget` from `CommandNode`** (API break but huge ergonomic improvement).
11. **Unify binding APIs** (surface area reduction; behavior preserved via wrappers).
12. **Traversal iterator cleanup** (nice-to-have; pays down duplication).

If you do only three things: **(2) Arc effects, (4) configured add helpers, (7) RefCell
widgets** — the library will feel dramatically simpler and more robust without changing the
conceptual model.

---

If you want, I can also sketch a “post-refactor” rewrite of the `examples/todo::ensure_tree`
function using the new helper APIs to show exactly how much line count you can shave at call
sites.

## Staged execution checklist

1. Stage One: ArgValue + NodeName correctness
1. [x] Update `impl_uint_to_arg_value!` in `crates/canopy/src/core/commands.rs` to emit
    `ArgValue::UInt` for `u8/u16/u32` and add `crates/canopy/tests/test_arg_value_uint.rs`.
2. [x] Tighten node name validation in `crates/canopy/src/core/state.rs` to reject empty names and
    ensure `NodeName::convert` falls back to `"node"` when filtering removes all chars.
3. [x] Run the standard lint command (see Standard validation commands).
4. [x] Run the standard test command (see Standard validation commands).
5. [x] Run the standard format command (see Standard validation commands).

2. Stage Two: Arc-based style effects
1. [x] Replace `Box<dyn StyleEffect>` with `Effect = Arc<dyn StyleEffect>` in
    `crates/canopy/src/core/style/effects.rs`, update constructors, and remove `box_clone`.
2. [x] Update effect storage and traversal (`crates/canopy/src/core/node.rs`,
    `crates/canopy/src/core/canopy.rs`, `crates/canopy/src/core/render.rs`,
    `crates/canopy/src/core/context.rs`, `crates/canopy/src/core/testing/dummyctx.rs`) to use
    `Effect` and `cloned()` stacking.
3. [x] Run the standard lint command (see Standard validation commands).
4. [x] Run the standard test command (see Standard validation commands).
5. [x] Run the standard format command (see Standard validation commands).

3. Stage Three: Context ergonomics + examples
1. [x] Make scroll wrappers default methods on `Context` and remove the duplicated implementations
    from `CoreContext` (update `crates/canopy/src/core/testing/dummyctx.rs` accordingly).
2. [x] Add `add_child_to_with_layout` and `add_keyed_to_with_layout` helpers in
    `crates/canopy/src/core/context.rs`.
3. [x] Update `examples/todo/src/lib.rs` bindings to typed `CommandCall` usage (decide whether to
    add the optional `call!` macro or keep `call_with` in the example).
4. [x] Add `crates/canopy/src/prelude.rs` and update examples to import from `canopy::prelude`.
5. [x] Run the standard lint command (see Standard validation commands).
6. [x] Run the standard test command (see Standard validation commands).
7. [x] Run the standard format command (see Standard validation commands).

4. Stage Four: ReadContext mutability cleanup
1. [x] Move `invalidate_layout` from `ReadContext` to `Context` and update `CoreContext`,
    `CoreViewContext`, and `crates/canopy/src/core/testing/dummyctx.rs`.
2. [x] Convert `layout_dirty` to a `bool` in `crates/canopy/src/core/node.rs` and update
    `crates/canopy/src/core/world.rs` refresh logic and node initialization.
3. [x] Run the standard lint command (see Standard validation commands).
4. [x] Run the standard test command (see Standard validation commands).
5. [x] Run the standard format command (see Standard validation commands).

5. Stage Five: Widget borrowing + CommandNode decoupling
1. [x] Replace `Node.widget` with `RefCell<Option<Box<dyn Widget>>>` and use a guard to temporarily
    take widgets without panics, returning `Error::ReentrantWidgetBorrow(NodeId)` on re-entrancy.
2. [x] Update `with_widget_mut`/`with_widget_view` to use `try_borrow_mut`, return `Result`, and add
    a structured re-entrancy error (e.g., `Error::ReentrantWidgetBorrow(NodeId)`).
3. [x] Update all call sites to handle the new `Result` (event dispatch, render, focus helpers,
    and tests) without panics.
4. [x] Remove `CommandNode` from the `Widget` supertrait in `crates/canopy/src/widget/mod.rs`,
    and adjust command registration/derives as needed.
5. [x] Run the standard lint command (see Standard validation commands).
6. [x] Run the standard test command (see Standard validation commands).
7. [x] Run the standard format command (see Standard validation commands).

6. Stage Six: Binding API unification + traversal helper
1. [x] Introduce `BindingAction` and a unified `bind` API in `crates/canopy/src/core/canopy.rs`,
    refactor existing `bind_*` wrappers to delegate (decide whether to mark deprecations).
2. [x] Add a `ReadContext::preorder` iterator in `crates/canopy/src/core/context.rs` and refactor
    traversal code in `crates/canopy/src/core/context.rs`, `crates/canopy/src/core/canopy.rs`, and
    `crates/canopy/src/core/focus.rs` to use it.
3. [x] Run the standard lint command (see Standard validation commands).
4. [x] Run the standard test command (see Standard validation commands).
5. [x] Run the standard format command (see Standard validation commands).

### Standard validation commands

```bash
cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests \
  --examples 2>&1
```

```bash
cargo nextest run --all --all-features
# If nextest is unavailable:
cargo test --all --all-features
```

```bash
cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml
# If the config file is unavailable:
cargo +nightly fmt --all
```
