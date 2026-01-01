# Lifecycle

Widgets move through a simple lifecycle driven by attachment and removal.

## Mounting

`Widget::on_mount` runs exactly once, the first time a node becomes attached to the root. Nodes are
mounted in pre-order (parent before children). Detaching and re-attaching a mounted node does not
re-run `on_mount`.

## Detaching

`detach` removes a node from its parent without deleting it. The node remains in the arena and
keeps its mounted state.

## Removal

`remove_subtree` deletes a node and its descendants. It runs in three phases:

- `pre_remove` top-down to validate or veto removal.
- `on_unmount` bottom-up while the nodes are still attached.
- Detach and delete the subtree, then enforce focus and mouse capture invariants.

`pre_remove` should be side-effect free or safe to repeat. `on_unmount` is best-effort and must
not fail.
