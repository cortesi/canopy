# Node creation ergonomics: context-aware conversion to NodeId

This document explores a higher-level alternative to the current
`add_orphan` / `add_child` split. The idea is to define a trait that can
turn a widget, `NodeId`, or `TypedId` into a `NodeId` using a `Context`,
and then update APIs to accept that more general input.

## Problem statement

- `add_child` is ergonomic but only accepts a widget and always mounts to
  the current node.
- Many APIs accept `NodeId`, which nudges callers toward `add_orphan` even
  when they would prefer to work at the `add_child` level.
- For typed IDs, we only have `add_orphan_typed`. There is no
  `add_child_typed` or `add_child_to_typed`, so `add_orphan` becomes the
  default in typed code.

The result is that examples (including listgym) contain a lot of
`add_orphan` even when the conceptual intent is simply “create a child.”

## Proposed direction: a context-aware conversion trait

Introduce a trait, name TBD, that can produce a `NodeId` given a context.
Candidate names: `IntoNodeId`, `NodeInput`, or `NodeSpec`.

```rust
pub trait IntoNodeId {
    fn into_node_id(self, ctx: &mut dyn Context) -> NodeId;
}

impl IntoNodeId for NodeId {
    fn into_node_id(self, _: &mut dyn Context) -> NodeId { self }
}

impl<W: Widget + 'static> IntoNodeId for W {
    fn into_node_id(self, ctx: &mut dyn Context) -> NodeId {
        ctx.add_orphan(self)
    }
}

impl<W: Widget + 'static> IntoNodeId for TypedId<W> {
    fn into_node_id(self, _: &mut dyn Context) -> NodeId { self.into() }
}

impl IntoNodeId for Box<dyn Widget> {
    fn into_node_id(self, ctx: &mut dyn Context) -> NodeId {
        ctx.add_orphan(self)
    }
}
```

## API changes that would leverage the trait

The goal is to keep behavior the same while making call sites more
uniform. The following methods could accept `impl IntoNodeId`:

- `Context::add_child`
- `Context::add_child_to`
- `Context::add_children`
- `Context::add_children_to`
- `Panes::insert_row`
- `Panes::insert_col`

For command methods, generics are not viable, so command-facing APIs
would continue to accept `NodeId`. Internal helpers can be generic.

## Before/after examples

### 1) Use `add_child` with either a widget or an existing node

Before:
```rust
let list_id = c.add_orphan(List::<Text>::new());
c.mount_child(list_id)?;

let frame_id = c.add_child(frame::Frame::new())?;
c.mount_child_to(frame_id, list_id)?;
```

After:
```rust
let list_id = c.add_child(List::<Text>::new())?;

let frame_id = c.add_child(frame::Frame::new())?;
c.add_child_to(frame_id, list_id)?;
```

### 2) Pass a widget directly to panes without manual allocation

Before:
```rust
let frame_id = c.add_orphan(frame::Frame::new());
panes.insert_col(ctx, frame_id)?;
```

After:
```rust
panes.insert_col(ctx, frame::Frame::new())?;
```

### 3) Batch child mounting with mixed inputs

Before:
```rust
let a = c.add_orphan(Text::new("A"));
let b = c.add_orphan(Text::new("B"));
let ids = c.add_children(vec![Box::new(Text::new("C"))])?;
// Manually mount a and b, and handle ids from add_children
```

After:
```rust
let ids = c.add_children([
    Text::new("A"),
    Text::new("B"),
    Box::new(Text::new("C")),
])?;
```

## Benefits

- Keeps `add_child` as the default entry point in most user code.
- Reduces the pressure to reach for `add_orphan` when an API expects a
  `NodeId`.
- Allows paned layouts and container widgets to accept concrete widgets
  directly, improving example readability.
- Works naturally with typed IDs without forcing `add_orphan_typed`.

## Constraints and risks

- Generic methods cannot be exposed as commands via `#[derive_commands]`.
  Command-facing APIs remain `NodeId`-based.
- Accepting `NodeId` in `add_child` or `add_child_to` makes it easier to
  reparent nodes. This is already possible via `mount_child`, but the
  broader signature changes the feel of the API. If this is a concern,
  we can introduce new helper methods (e.g. `attach_child_to`) and leave
  `add_child_to` widget-only.
- There is still no validation that a `NodeId` belongs to the same core
  instance. That is an existing issue and should be addressed separately
  if needed.

## Relationship to `add_orphan`

This proposal does not remove `add_orphan`. It remains the explicit,
low-level tool for detached node creation. We could consider renaming it
(`add_detached` / `add_unmounted`) to make its role clearer, but the main
shift is to make it less necessary in typical call sites.

## Open questions

- Name of the trait: `IntoNodeId` vs `NodeInput` vs `NodeSpec`.
- Should `add_child_to` accept `NodeId`, or should we introduce a
  separate helper to avoid semantic surprise?
- Do we want a fallible conversion trait (`Result<NodeId>`) for future
  validation, or keep it infallible for simplicity?
- Which core widgets should be updated to accept `IntoNodeId` first
  (Panes, Root helpers, Frame wrappers, etc.)?
