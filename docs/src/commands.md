# trait CommandNode

Commands are at the driver for much of Canopy's flexibility, and are the core mechanism for interacting with nodes. To
support commands, nodes implement the deceptively simple [CommandNode](doc/canopy/commands/trait.CommandNode.html)
trait. There are only two methods that need to be implemented to satisfy it:

```rust
fn commands() -> Vec<CommandSpec>
    where Self: Sized;

fn dispatch(
    &mut self,
    c: &mut dyn Core,
    cmd: &CommandInvocation
) -> Result<ReturnValue>;
```

The `commands` function returns a list of command specifications - that is commands that are supported by this node.
Each `CommandSpec` has a name, a description, and a type command signature. The converse of this is the `dispatch`
function, which takes a `CommandInvocation` specifcation, and runs the command on the node. Although these functions are
simple, implementing them by hand would be tedious, so Canopy has derive helpers to do this for you. We use the
`#[derive_commands]` attribute on the node impl block, then annotate each individual command with the `#[command]`
attribute. For example:

```rust
#[derive(StatefulNode)]
struct MyNode {
    state: NodeState,
    value: u64,
}

#[derive_commands]
impl MyNode {
    /// Increment our value.
    #[command]
    fn inc(&mut self) -> Result<()> {
        self.value.saturating_add(1);
        Ok(())
    }

    /// Decrement our value.
    #[command]
    fn dec(&mut self) -> Result<()> {
        self.value.saturating_sub(1);
        Ok(())
    }
}
```

# Arguments

Command functions can take an arbitrary number of arguments. The following types are supported:

- `&mut dyn Core`. This type is handled specially - if the first argument is of type `&mut dyn Core`, the `Core` object
  is automatically passed in to each invocation. That is, it does not have to be (and cannot be) specified in a script
  when a command is invoked.
- `isize`

The list of supported types are extended as needed. If you need a type that isn't supported, please open an issue.


# Return values

The following return value variants are supported:

- No return value.
- `String`

We also support `Result` variants of the above:

- `Result<()>`
- `Result<T>` where `T` is any supported type.

Sometimes we want to write functions that can be used from Rust or from a script, with slightly different semantics. It's useful to be able to ignore the return value for script commands in some cases, where those values are not representable in the script language. For example, consider the following function:

```rust
/// Delete the currently selected item.
#[command(ignore_result)]
pub fn delete_selected(&mut self, core: &mut dyn Core) -> Option<N> {
    self.delete_item(core, self.offset)
}
```

Here, we have a node that manages a list. The Rust variant of the item delete function returns the item if it exists - N
here is a generic type variable on the node. In scripts we would still like `delte_selected` to be available, but we
would like to discard the un-representalbe return value. This is what the `ignore_result` argument to the `command`
attribute does.


