# Node names

Every node in the tree has a **node name**, which is used for command dispatch
and path matching in input bindings. Names are stored as a `NodeName`, which is
validated to contain only lowercase ASCII letters, digits, and underscores.

## Default naming

Widgets can override `Widget::name` to control the node name, but if they
don't, the default implementation converts the Rust type name to snake case and
removes invalid characters. This means a widget named `FocusGym` becomes the
node name `focus_gym`.

## Manual conversion

If you need to construct a node name yourself, use:

```rust
use canopy::state::NodeName;

let name = NodeName::convert("MyWidget");
```
