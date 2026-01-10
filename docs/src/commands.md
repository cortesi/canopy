# Commands

Commands provide a uniform way to invoke widget behavior from Rust and from scripts. Each widget
exposes a static list of `CommandSpec` values, and the runtime routes invocations to the correct
node based on command metadata and focus context.

## Defining commands

Widgets implement the `CommandNode` trait. You almost never write it by hand; instead, use the
`#[derive_commands]` macro on an impl block and tag command methods with `#[command]`.

```rust
use canopy::{Context, ReadContext, Widget, derive_commands, command, error::Result, render::Render, state::NodeName};

struct Counter {
    value: i64,
}

#[derive_commands]
impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    #[command]
    fn inc(&mut self, _ctx: &mut dyn Context) {
        self.value = self.value.saturating_add(1);
    }

    #[command]
    fn add(&mut self, _ctx: &mut dyn Context, delta: i64) {
        self.value = self.value.saturating_add(delta);
    }
}

impl Widget for Counter {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("counter")
    }
}
```

The derive also generates `cmd_*()` helpers for building typed calls:

```rust
let call = Counter::cmd_add().call_with((5,));
ctx.dispatch_command(&call.invocation())?;
```

## Command registration

Commands must be registered on the `Canopy` instance to be available for dispatch or scripting.
Register widget command sets once during initialization:

```rust
canopy.add_commands::<Counter>()?;
```

## Arguments and conversions

Command parameters use `ArgValue` conversions. Built-in conversions include:

- `bool`, `String`, `&str`
- integers and floats
- `Option<T>`, `Vec<T>`, maps (`BTreeMap`/`HashMap`) where `T: ToArgValue`

For custom types, derive `CommandArg` (serde-backed):

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, canopy::CommandArg)]
struct MyArgs {
    name: String,
    count: u32,
}
```

## Injection

Some parameter types are injected by the runtime rather than supplied by the caller:

- `Event`
- `MouseEvent`
- `ListRowContext`
- `Option<T>` for optional injection
- `Injected<T>` to explicitly request injection
- `Arg<T>` to force a caller-supplied argument even if `T` is injectable

Injected values are sourced from the current command scope (event, mouse, list row).

## Return values

Commands can return:

- `()` or `Result<()>`
- any type that implements `FromArgValue`
- `Result<T>` where `T: FromArgValue`

Use `#[command(ignore_result)]` when the result should be available in Rust but not surfaced in
scripting.

## Dispatch and invocation

`CommandCall` and `CommandInvocation` are the portable representations of a command call:

- `CommandSpec::call()` builds a call with no args
- `CommandSpec::call_with(...)` builds with positional or named args
- `CommandCall::invocation()` produces the concrete `CommandInvocation`

Bindings (key/mouse) can target either scripts or typed command invocations.
