# State

Canopy tracks housekeeping data for each node - this includes whether the node has focus, the size and location of the
node, whether the node has been tainted, and so on. This data is tracked in an opaque structure called `NodeState`, and
each node is responsible for keeping its own state and returning it back to Canopy on request. The mechanism for doing
this is the [StatefulNode](doc/canopy/trait.StatefulNode.html) trait. There are three functions that need to be implemented to support this trait:

```rust
/// The name of this node, used for debugging and command dispatch.
fn name(&self) -> NodeName;

/// Get a reference to the node's state object.
fn state(&self) -> &NodeState;

/// Get a mutable reference to the node's state object.
fn state_mut(&mut self) -> &mut NodeState;
```

These are simple enough to implement by hand, but it's such common boilerplate that Canopy provides a macro to do this
for you. All you need to do is make sure that the struct for your node has an attribute called `state` of type
`NodeState`.

```rust
#[derive(StatefulNode)]
struct MyNode {
    state: NodeState,
    // ...
}
```

The derive macro takes the name of the struct as the node name - in this case it would be `MyNode`.

