# Widget

Widgets provide the behavior attached to nodes in the Core arena. Every widget
implements the `Widget` trait, which includes:

- `render` to draw the widget into a buffer
- `measure` to report intrinsic size (used by layout)
- `canvas_size` to report content size for scrolling/clipping (defaults to `measure`)
- `on_event` to handle input events
- `poll` for scheduled updates
- `accept_focus` and `cursor` for focus handling

Widgets are also `CommandNode`s, so they can expose commands used by the
scripting and binding system. By default, a widget's node name is derived from
its type name; see [Node names](./state.md).
