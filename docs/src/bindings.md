# Binding system

Bindings are declared from Luau with `canopy.bind_with()` and `canopy.bind_mouse_with()`.
Bindings are resolved against the focused node's path and can be scoped by mode.

## Quick start

Key binding:

```luau
canopy.bind_with("q", { desc = "Quit" }, function()
    root.quit()
end)
```

Path-scoped binding:

```luau
canopy.bind_with("j", { path = "list", desc = "Next item" }, function()
    list.select_next()
end)
```

## Modes

Bindings live in named modes. Use the default mode or set a mode explicitly in the options table:

```luau
canopy.bind_with("j", { mode = "nav", desc = "Cursor down" }, function()
    editor.down()
end)
```

Applications switch modes through the runtime's `InputMap`; if the current mode does not match a
binding, Canopy falls back to the default mode.

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

```luau
canopy.bind_mouse_with("LeftDown", { path = "list", desc = "Activate item" }, function()
    list.activate()
end)
```

## Default binding scripts

Widgets can register optional default bindings that are callable from Luau after `finalize_api()`:

```luau
root.default_bindings()
help.default_bindings()
```

Applications typically run a short startup script that composes widget defaults with
app-specific bindings.
