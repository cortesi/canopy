# Binding system

Bindings map input events (keys or mouse actions) to either scripts or typed command invocations.
Bindings are resolved against the focused node's path and can be scoped by mode.

## Quick start

Script binding:

```rust
canopy.bind_key('q', "", "root::quit()")?;
```

Typed command binding:

```rust
canopy.bind_key_command('q', "", Root::cmd_quit().call())?;
```

The empty path filter (`""`) matches any focus path.

## Modes

Bindings live in named modes. Use the default mode (`""`) or set a mode explicitly:

```rust
canopy.bind_mode_key('j', "nav", "", "editor::down()")?;
canopy.keymap.set_mode("nav")?;
```

If the current mode does not match a binding, Canopy falls back to the default mode.

## Path filters

Bindings include a path filter string. The filter is matched against the focus path, and the most
specific match wins. Path filters are slash-separated components; `*` matches any component span.

Examples:

- `""` matches all paths
- `"editor"` matches any path containing `editor`
- `"/root/editor"` anchors to the root
- `"editor/"` anchors to the end
- `"editor/*/line"` matches a line widget anywhere under editor

## Mouse bindings

Mouse bindings use the same path filters:

```rust
use canopy::event::mouse;

canopy.bind_mouse(mouse::MouseEvent::left_click(), "list", "list::activate()")?;
```

## Binder helper

The `Binder` helper offers a fluent API for building sets of bindings and scoping them by path.
It is especially useful in examples and apps with many bindings.
