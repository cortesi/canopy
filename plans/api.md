# API Ergonomics: Tree-Wide Type Search and Focus-Aware Lookup

This document proposes two API additions to improve ergonomics when working with widget trees. These
patterns emerged from pain points in `crates/examples/src/listgym.rs`.

## Problem 1: Finding Widgets Anywhere in the Tree

### Current Pain Point

The `StatusBar::panes_id` function manually implements a DFS to find a `Panes` widget from the
root:

```rust
fn panes_id(ctx: &dyn ViewContext) -> Option<NodeId> {
    let panes_type = TypeId::of::<Panes>();
    let mut stack = vec![ctx.root_id()];
    while let Some(id) = stack.pop() {
        if ctx.node_type_id(id) == Some(panes_type) {
            return Some(id);
        }
        for child in ctx.children_of(id).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}
```

### Why Existing API Doesn't Help

- `first_descendant<W>()` exists on `dyn Context` but not on `dyn ViewContext`
- It only searches descendants of the *current node*, not from an arbitrary root
- `StatusBar` is a sibling of `Panes`, not an ancestor, so it needs tree-wide search

### Proposal A: Add Type-Search to ViewContext

Add extension methods to `dyn ViewContext + '_`:

```rust
impl dyn ViewContext + '_ {
    /// Return the first node of type `W` within `root` and its descendants.
    pub fn first_from<W: Widget + 'static>(&self, root: NodeId) -> Option<TypedId<W>> {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self.node_type_id(id) == Some(TypeId::of::<W>()) {
                return Some(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        None
    }

    /// Return all nodes of type `W` within `root` and its descendants.
    pub fn all_from<W: Widget + 'static>(&self, root: NodeId) -> Vec<TypedId<W>> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self.node_type_id(id) == Some(TypeId::of::<W>()) {
                out.push(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    /// Return the first widget of type `W` anywhere in the tree, including the root.
    pub fn first_in_tree<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.first_from::<W>(self.root_id())
    }

    /// Return all widgets of type `W` anywhere in the tree, including the root.
    pub fn all_in_tree<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.all_from::<W>(self.root_id())
    }
}
```

### Result

The `panes_id` function becomes:

```rust
fn panes_id(ctx: &dyn ViewContext) -> Option<NodeId> {
    ctx.first_in_tree::<Panes>().map(Into::into)
}
```

---

## Problem 2: Focus-Aware Descendant Lookup

### Current Pain Point

The `ListGym::list_id` function finds a list, preferring one on the focus path:

```rust
fn list_id(&self, c: &dyn Context) -> Result<NodeId> {
    let lists = c.descendants_of_type::<List<ListEntry>>();
    if lists.is_empty() {
        return Err(Error::Invalid("list not initialized".into()));
    }
    if let Some(id) = lists
        .iter()
        .copied()
        .find(|id| c.node_is_on_focus_path((*id).into()))
    {
        return Ok(id.into());
    }
    Ok(lists[0].into())
}
```

### Why This Pattern Is Common

Multi-pane UIs frequently need to target the "active" instance of a widget type. The pattern
"prefer focused, else first" is a sensible default that appears repeatedly.

### Proposal B: Add Focus-Aware Descendant Helpers

Add to `dyn Context + '_`, scoped to the current node's subtree:

```rust
impl dyn Context + '_ {
    /// Return the descendant of type `W` that is on the focus path, if any.
    pub fn focused_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.descendants_of_type::<W>()
            .into_iter()
            .find(|id| self.node_is_on_focus_path((*id).into()))
    }

    /// Return the descendant of type `W` on the focus path, or the first if none focused.
    ///
    /// This searches only within the current node's subtree. Use the tree-wide helpers on
    /// `ViewContext` if you need to search from an arbitrary root.
    pub fn focused_or_first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        let mut descendants = self.descendants_of_type::<W>();
        let focused = descendants
            .iter()
            .copied()
            .find(|id| self.node_is_on_focus_path((*id).into()));
        focused.or_else(|| descendants.into_iter().next())
    }

    /// Execute a closure with the focused descendant of type `W`, or the first if none focused.
    pub fn with_focused_or_first_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = self
            .focused_or_first_descendant::<W>()
            .ok_or_else(|| Error::NotFound(type_name::<W>().to_string()))?;
        self.with_typed(node, f)
    }
}
```

### Result

The `list_id` function becomes:

```rust
fn list_id(&self, c: &dyn Context) -> Result<NodeId> {
    c.focused_or_first_descendant::<List<ListEntry>>()
        .map(Into::into)
        .ok_or_else(|| Error::Invalid("list not initialized".into()))
}
```

Or we can eliminate the helper entirely and inline:

```rust
fn with_list<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
where
    F: FnMut(&mut List<ListEntry>, &mut dyn Context) -> Result<R>,
{
    c.with_focused_or_first_descendant::<List<ListEntry>, _>(f)
}
```

---

## Summary

| Proposal | Adds to | Methods |
|----------|---------|---------|
| A | `dyn ViewContext` | `first_descendant_from`, `descendants_from`, `first_in_tree`, `all_in_tree` |
| B | `dyn Context` | `focused_descendant`, `focused_or_first_descendant`, `with_focused_or_first_descendant` |

Both proposals are independent and address distinct use cases:
- **A**: Tree-wide search from read-only context
- **B**: Focus-aware lookup for multi-instance widgets
