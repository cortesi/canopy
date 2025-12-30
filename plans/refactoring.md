# Structural refactoring proposals

This document expands the larger refactors into concrete design discussions. These are API-shaping
changes and should be treated as separate design work from the fixes plan.

## 1. Split dirty flags into layout/view/render

### Problem
Today, `layout_dirty` is used as a catch-all and the render path always runs layout. That makes it
hard to optimize scrolling, focus changes, and small style updates without re-measuring the world.
It also obscures intent: a scroll change is not the same as a layout change.

### Proposal
Introduce distinct dirty flags and a small propagation model:

- `layout_dirty`: requires measure/layout for the subtree.
- `view_dirty`: requires recomputing views (e.g., scroll offsets, clip rects).
- `render_dirty`: requires redraw/diff only.

Propagate upward or downward as needed:

- Layout invalidation implies view + render invalidation.
- View invalidation implies render invalidation.
- Render invalidation does not imply layout or view invalidation.

### Implementation sketch
Add a small flag struct on nodes (or a per-tree accumulator) and route invalidations through
`Context` helpers instead of a single `taint()` path.

```rust
#[derive(Default)]
struct DirtyFlags {
    layout: bool,
    view: bool,
    render: bool,
}

impl DirtyFlags {
    fn invalidate_layout(&mut self) {
        self.layout = true;
        self.view = true;
        self.render = true;
    }

    fn invalidate_view(&mut self) {
        self.view = true;
        self.render = true;
    }

    fn invalidate_render(&mut self) {
        self.render = true;
    }
}
```

Pipeline sketch:

```rust
if core.any_layout_dirty() {
    core.update_layout(root_size)?;
} else if core.any_view_dirty() {
    core.update_views_only(root_size)?;
}

if core.any_render_dirty() {
    canopy.render_pass()?;
}
```

### Notes and trade-offs
- You need a consistent rule for which actions trigger which invalidations. For example, scroll
  should mark view/render but not layout; style changes may be render-only unless layout depends on
  style.
- The split lets you keep always-render semantics but avoid full layout for many events.

## 2. Unmount semantics and node removal

### Problem
Widgets have `on_mount`, but there is no counterpart for unmounting or removal. Without explicit
lifecycle hooks, background work, poll scheduling, and resource cleanup must be manual and can
leak when nodes are detached.

### Proposal
Add an `on_unmount` hook and a formal removal API that visits the subtree and performs cleanup.
Ensure focus, pollers, and event routing are updated accordingly.

### Implementation sketch
Extend the widget trait and add core removal helpers:

```rust
pub trait Widget: Any + Send + CommandNode {
    fn on_unmount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Ok(())
    }
}

impl Core {
    pub fn remove_subtree(&mut self, root: NodeId) -> Result<()> {
        self.detach_from_parent(root)?;
        for id in self.subtree_postorder(root) {
            self.with_widget_mut(id, |w, core| {
                let mut ctx = CoreContext::new(core, id);
                w.on_unmount(&mut ctx)
            })?;
            self.clear_node_state(id);
        }
        self.ensure_focus_visible();
        Ok(())
    }
}
```

### Notes and trade-offs
- Decide whether removal is permanent (node IDs never reused) or allows reuse.
- Ensure poller entries are removed or ignored for unmounted nodes.
- Consider a “deactivate” state if you want to keep IDs but stop rendering.

## 3. Instance tags/IDs for path matching

### Problem
Paths are derived from widget type names, so multiple instances of the same widget are
indistinguishable. This limits ergonomic binding and script targeting in complex trees.

### Proposal
Add an optional per-node tag (instance identifier). Use the tag in paths when present, while
keeping the widget type name for command modules and default paths.

### Implementation sketch
Add a tag to `Node` and a helper to set it:

```rust
pub struct Node {
    pub name: NodeName,
    pub tag: Option<NodeName>,
    // ...
}

impl Core {
    pub fn set_tag(&mut self, node: NodeId, tag: impl Into<NodeName>) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.tag = Some(tag.into());
        }
    }
}
```

Define a path representation such as `editor@left` when a tag is present:

```rust
fn path_component(node: &Node) -> String {
    if let Some(tag) = &node.tag {
        format!("{}@{}", node.name, tag)
    } else {
        node.name.to_string()
    }
}
```

### Notes and trade-offs
- Keep tags optional to avoid breaking existing paths.
- Decide whether tags are part of `NodeName` or a separate type to avoid conflating intent.
- Update path matching and documentation to cover the tag syntax.

## 4. Command registration ergonomics

### Problem
Commands must be registered manually via `add_commands::<T>()`. Scripts compiled before a widget
is registered fail with confusing errors, and command availability is tightly coupled to call
ordering.

### Proposal
Make command registration automatic or lazy, and improve error reporting when a script references
an unknown module.

### Implementation sketch
Option A: auto-register when a widget type is first added:

```rust
pub trait CommandSource {
    fn command_specs() -> &'static [CommandSpec];
}

impl Canopy {
    fn ensure_commands<T: CommandSource>(&mut self) {
        if !self.commands.has::<T>() {
            self.commands.register::<T>(T::command_specs());
            self.script_host.load_commands(self.commands.specs());
        }
    }
}

impl Core {
    pub fn add<T: Widget + CommandSource>(&mut self, w: T) -> NodeId {
        self.canopy.ensure_commands::<T>();
        self.add_boxed(Box::new(w))
    }
}
```

Option B: keep explicit registration, but upgrade errors:

```text
script compile error: module "foo" not found
hint: did you forget add_commands::<Foo>()? known modules: [bar, baz]
```

### Notes and trade-offs
- Auto-registration requires a stable registry and may be awkward for dynamic plugin loading.
- Lazy registration can hide errors until first use; decide whether that is acceptable.
- Improving error messages is a low-risk immediate win even if auto-registration is deferred.

## 5. Panic-safe widget access (with_widget_mut / with_widget_view)

### Problem
`with_widget_mut` and `with_widget_view` take the widget out of the node. If a panic occurs, the
widget is lost and the node becomes unusable. Re-entrancy can also cause subtle bugs.

### Proposal
Wrap widget extraction in a guard that guarantees reinsertion on drop. This reduces the blast
radius of panics and simplifies reasoning about node integrity.

### Implementation sketch

```rust
struct WidgetGuard<'a> {
    node_id: NodeId,
    core: &'a mut Core,
    widget: Option<Box<dyn Widget>>,
}

impl<'a> WidgetGuard<'a> {
    fn new(core: &'a mut Core, node_id: NodeId) -> Self {
        let widget = core.nodes[node_id].widget.take();
        Self { node_id, core, widget }
    }

    fn widget_mut(&mut self) -> &mut dyn Widget {
        self.widget.as_deref_mut().expect("widget missing")
    }
}

impl Drop for WidgetGuard<'_> {
    fn drop(&mut self) {
        if let Some(widget) = self.widget.take() {
            self.core.nodes[self.node_id].widget = Some(widget);
        }
    }
}

fn with_widget_mut<R>(
    core: &mut Core,
    node_id: NodeId,
    f: impl FnOnce(&mut dyn Widget, &mut Core) -> R,
) -> R {
    let mut guard = WidgetGuard::new(core, node_id);
    f(guard.widget_mut(), guard.core)
}
```

### Notes and trade-offs
- This does not fix re-entrancy by itself; you may still want explicit guards or refactoring.
- Alternative: store widgets in a separate arena or behind `RefCell` to avoid taking ownership.
- RAII guard is minimal change and can be introduced incrementally.
