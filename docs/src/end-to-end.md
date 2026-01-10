# End-to-end example

This page shows a minimal widget with a command, a key binding that invokes it, and a style
override.

## Widget + command

```rust
use canopy::{Context, ReadContext, Widget, derive_commands, command, error::Result, render::Render, state::NodeName};
use canopy_widgets::Text;

key!(LabelSlot: Text);

pub struct Counter {
    value: i64,
}

#[derive_commands]
impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    #[command]
    pub fn inc(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.value = self.value.saturating_add(1);
        self.sync_label(ctx)
    }

    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let label = format!("count: {}", self.value);
        ctx.with_child::<LabelSlot, _>(|text, _| {
            text.set_raw(label);
            Ok(())
        })
    }
}

impl Widget for Counter {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        ctx.add_keyed::<LabelSlot>(Text::new("count: 0"))?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("counter")
    }
}
```

## Install and bind

```rust
use canopy::Canopy;
use canopy_widgets::Root;

let mut canopy = Canopy::new();
canopy.add_commands::<Counter>()?;

// Replace the root widget for a minimal app.
canopy.core.replace_subtree(canopy.core.root_id(), Counter::new())?;

// Bind a key to the typed command.
canopy.bind_key_command('j', "", Counter::cmd_inc().call())?;
```

## Style override

```rust
use canopy::style::{Attr, StyleMap, solarized};

let mut style = StyleMap::new();
style
    .rules()
    .fg("counter", solarized::BASE3)
    .bg("counter", solarized::BASE02)
    .attr("counter", Attr::Bold)
    .apply();

canopy.style = style;
```

The key pieces are:

- `#[derive_commands]` + `#[command]` for command metadata
- `cmd_*().call()` for a typed invocation
- path-based styles to keep appearance separate from behavior
