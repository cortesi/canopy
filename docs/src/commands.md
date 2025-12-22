# trait CommandNode

Commands drive much of Canopy's flexibility and are the core mechanism for
interacting with widgets. Widgets implement the
[CommandNode](doc/canopy/commands/trait.CommandNode.html) trait, which has two
methods:

```rust
fn commands() -> Vec<CommandSpec>
    where Self: Sized;

fn dispatch(
    &mut self,
    c: &mut dyn Context,
    cmd: &CommandInvocation
) -> Result<ReturnValue>;
```

The `commands` function returns a list of command specifications supported by
this widget. Each `CommandSpec` includes a name, description, and signature.
The `dispatch` function takes a `CommandInvocation` and runs the command.

Implementing these functions by hand is tedious, so Canopy provides derive
helpers. Use `#[derive_commands]` on the widget's impl block and annotate each
command with `#[command]`. For example:

```rust
struct MyWidget {
    value: u64,
}

#[derive_commands]
impl MyWidget {
    /// Increment our value.
    #[command]
    fn inc(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.value = self.value.saturating_add(1);
        Ok(())
    }

    /// Decrement our value.
    #[command]
    fn dec(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.value = self.value.saturating_sub(1);
        Ok(())
    }
}
```

# Arguments

Command functions can take an arbitrary number of arguments. The following
types are supported:

- `&mut dyn Context`. This is handled specially: if the first argument is of
  type `&mut dyn Context`, it is automatically supplied during dispatch and
  cannot be specified in a script.
- `isize`

The list of supported types can be extended as needed.

# Return values

The following return value variants are supported:

- No return value.
- `String`

We also support `Result` variants of the above:

- `Result<()>`
- `Result<T>` where `T` is any supported type.

Sometimes we want to expose a command that returns a value in Rust but has no
equivalent in the scripting layer. In those cases use
`#[command(ignore_result)]`. For example:

```rust
/// Delete the currently selected item.
#[command(ignore_result)]
pub fn delete_selected(&mut self, ctx: &mut dyn Context) -> Option<N> {
    self.delete_item(ctx, self.offset)
}
```
