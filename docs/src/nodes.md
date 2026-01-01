
# Nodes

Each node in a Canopy application tree is a data container managed by the
[`Core`](doc/canopy/struct.Core.html). Nodes are identified by `NodeId` handles
and store a [`Widget`](doc/canopy/trait.Widget.html) implementation that
provides behavior.

Widgets are also [`CommandNode`](doc/canopy/commands/trait.CommandNode.html)s,
so they can expose commands and handle dispatch. Finally, every node has a
name used for paths and bindings; see [Node names](./state.md).

## Tree structure

Nodes live in the core arena and are either attached to the root or detached. A node is attached
when it is reachable from `Core.root` by following `children`. Detached nodes are in the arena but
not in the active tree.

## Creating and attaching nodes

Use the create-and-attach APIs when possible. `Context::add_child` and
`Context::add_child_to` create a widget and attach it in one step, with
transactional behavior: on error, the tree and arena are unchanged.

`Context::create_detached` allocates a node without attaching it; pair it with `attach` when you
need to reparent or build a subtree off-screen. `detach` removes a node from its parent but leaves
it in the arena. `remove_subtree` deletes a node and its descendants; see
[Lifecycle](./lifecycle.md) for ordering and teardown.

## Keyed children

Keyed children provide stable names for direct child roles such as `"label"` or `"status"`. Keys
are unique per parent, and the association is removed when a child is detached. Use
`add_child_keyed`/`add_child_to_keyed` to create keyed children, `child_keyed` to look them up, and
`with_keyed`/`try_with_keyed` for typed access.

## Typed and path lookups

Type-based queries traverse in pre-order. Use `unique_child`/`unique_descendant` (and their
`with_*` variants) when you expect exactly one match; they surface ambiguity instead of silently
choosing the first match. Path helpers `find_one` and `try_find_one` enforce the same uniqueness
guarantee for path queries.

## Best practices

- Prefer keyed children for stable internal roles.
- Use `unique_*` lookups when the structure guarantees a single match; avoid `first_*` in complex
  widgets.
- Store `NodeId`s or keys for hot paths; avoid repeated deep traversal in per-frame loops.
- Use `create_detached` + `attach` only when you need to reparent or stage a subtree.
- Use `remove_subtree` for deletion so lifecycle hooks run and invariants are enforced.
