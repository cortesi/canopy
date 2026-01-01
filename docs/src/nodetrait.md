# Widget

Widgets provide the behavior attached to nodes in the Core arena. Every widget
implements the `Widget` trait, which includes:

- `layout` to describe layout configuration for this widget
- `render` to draw the widget into a buffer
- `measure` to report intrinsic size (used by layout)
- `canvas` to report content size for scrolling/clipping (defaults to the view size)
- `on_event` to handle input events
- `poll` for scheduled updates
- `accept_focus` and `cursor` for focus handling
- `on_mount` for one-time initialization after first attachment
- `pre_remove` to validate or veto removal
- `on_unmount` for best-effort teardown before deletion

Widgets are also `CommandNode`s, so they can expose commands used by the
scripting and binding system. By default, a widget's node name is derived from
its type name; see [Node names](./state.md).

The lifecycle hooks are described in more detail in [Lifecycle](./lifecycle.md).
