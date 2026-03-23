# Proposals for Future Refactoring

## 2. Eliminate ChildKey trait and macro

Currently, keyed children use a `ChildKey` trait and a `key!` macro to provide type-safe child resolution:
```rust
key!(ModalSlot: Modal);
// ...
let id = ctx.get_child_in::<ModalSlot>(parent);
```
While this ensures the widget type is correct, it requires defining a zero-sized struct for every key.

**Proposal:**
We can simplify `Slot` to just hold the string key and the generic widget type `W`, removing the need for the `key!` macro entirely.

**Before:**
```rust
pub trait ChildKey {
    type Widget: Widget + 'static;
    const KEY: &'static str;
}

pub struct Slot<K: ChildKey> { ... }
```

**After:**
```rust
pub struct Slot<W: Widget + 'static> {
    key: &'static str,
    id: Option<TypedId<W>>,
}

impl<W: Widget + 'static> Slot<W> {
    pub const fn new(key: &'static str) -> Self { ... }
}

// Usage:
let slot: Slot<Modal> = Slot::new("ModalSlot");
```
This requires updating all usages across the widgets and examples.

## 3. Replace ArgValue with serde_json::Value

Currently, Canopy maintains its own `ArgValue` enum (with variants like `Null`, `Bool`, `Int`, `UInt`, `String`, `Array`, `Map`) and custom traits (`ToArgValue`, `FromArgValue`, `TryToArgValue`). This duplicates the functionality already provided by `serde_json::Value`, which is an ecosystem standard.

**Before:**
```rust
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    Null,
    Bool(bool),
    Int(i64),
    // ...
}

pub trait ToArgValue {
    fn to_arg_value(self) -> ArgValue;
}
pub trait FromArgValue: Sized {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError>;
}
```

**After:**
We would remove `ArgValue` entirely and just use `serde_json::Value`. The traits can also be removed because `serde_json::from_value` and `serde_json::to_value` cover the exact same use cases for any type implementing `Serialize` or `DeserializeOwned`.
```rust
// In command signatures and dispatch logic:
pub type ArgValue = serde_json::Value;

// When reading arguments:
let my_arg: u32 = serde_json::from_value(inv.args[0].clone())?;

// When producing arguments:
let val = serde_json::to_value(my_data)?;
```
This simplifies the macro generation in `canopy-derive` as well, since it no longer needs to generate `ToArgValue` implementations.

## 4. Consolidate ReadContext and Context

Currently, widgets receive either a `&dyn ReadContext` (for `render`, `measure`, `accept_focus`) or a `&mut dyn Context` (for `on_event`, `poll`, `on_mount`). `Context` is a trait that extends `ReadContext`.
The large surface area of these traits (30+ methods combined) makes them difficult to mock.

**Before:**
```rust
pub trait ReadContext {
    fn node_id(&self) -> NodeId;
    fn view(&self) -> &View;
    // ... 20 other methods
}

pub trait Context: ReadContext {
    fn set_focus(&mut self, node: NodeId) -> bool;
    // ... 15 other methods
}
```

**After:**
Instead of passing a "God trait", we could pass a struct that borrows the necessary internals (e.g. `Core`, `Canopy`, or a subset of them). If trait abstraction is needed for mocking, we should break them down into smaller capabilities, e.g. `FocusQuery`, `FocusMut`, `ViewQuery`, `CommandContext`. 
Alternatively, pass `&mut Core` directly where applicable, since most methods just delegate to `Core`.
```rust
pub struct WidgetCtx<'a> {
    pub core: &'a mut Core,
    pub current_node: NodeId,
    // ...
}

pub trait Widget {
    fn render(&mut self, frame: &mut Render, ctx: &WidgetReadCtx) -> Result<()>;
    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) -> Result<EventOutcome>;
}
```
This reduces trait boilerplate and makes the API boundaries clearer.
