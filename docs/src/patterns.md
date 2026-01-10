# Patterns

This page collects practical patterns for building composable widgets with the current APIs.

## Typed IDs

Prefer `TypedId<T>` for widget IDs when you have a concrete type. It removes repetitive casts and
works with helpers like `Context::with_typed`.

```rust
let label_id = ctx.add_child(Text::new("Hello"))?;
ctx.with_typed(label_id, |text, _| {
    text.set_raw("Updated");
    Ok(())
})?;
```

## Keyed children

Use typed child keys to manage stable internal children. Define a key with the `key!` macro, then
add or query the child by key.

```rust
key!(LabelSlot: Text);

fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
    ctx.add_keyed::<LabelSlot>(Text::new("Label"))?;
    Ok(())
}

fn update_label(&self, ctx: &mut dyn Context) -> Result<()> {
    ctx.with_child::<LabelSlot, _>(|text, _| {
        text.set_raw("Updated");
        Ok(())
    })
}
```

## Slot pattern

For repeated access, cache a typed ID in your widget. This avoids searching the tree on every
update.

```rust
struct LabelSlot {
    id: Option<TypedId<Text>>,
}

impl LabelSlot {
    fn get_or_create(&mut self, ctx: &mut dyn Context) -> Result<TypedId<Text>> {
        if let Some(id) = self.id {
            return Ok(id);
        }
        let id = ctx.add_child(Text::new("Label"))?;
        self.id = Some(id);
        Ok(id)
    }
}
```

## Reconciling dynamic children

When a container's children are driven by data, keep a stable map of keys to node IDs and
synchronize with `set_children`:

- create missing nodes
- remove nodes that no longer exist (`remove_subtree`)
- update ordering with `set_children`

This keeps the tree stable while letting you update only what changed.
