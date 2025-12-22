
# Nodes

Each node in a Canopy application tree is a data container managed by the
[`Core`](doc/canopy/struct.Core.html). Nodes are identified by `NodeId` handles
and store a [`Widget`](doc/canopy/trait.Widget.html) implementation that
provides behavior.

Widgets are also [`CommandNode`](doc/canopy/commands/trait.CommandNode.html)s,
so they can expose commands and handle dispatch. Finally, every node has a
name used for paths and bindings; see [Node names](./state.md).
