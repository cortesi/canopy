# Unified Architecture: Arena Tree & Flexbox Layout

## Executive Summary
This design proposes a fundamental architectural shift for Canopy: moving from a recursive, pointer-based tree to a flat, arena-based memory model. This change is synergistic with the integration of the Taffy layout engine, allowing for a persistent, high-performance layout tree that mirrors the application state 1:1.

## Core Problems Solved
1.  **Borrow Checker Friction:** The current `children(&mut closure)` pattern exists solely to appease the borrow checker. It makes cross-node communication (e.g., a child triggering a parent update) difficult.
2.  **Layout Performance:** The current immediate-mode layout requires rebuilding the geometry tree frequently.
3.  **Layout Expressiveness:** Manual integer math is brittle. Flexbox (via Taffy) provides a robust declarative standard.

## 1. The Arena Architecture

### Data Structure
Instead of `struct Node { children: Vec<Box<dyn Node>> }`, the entire application tree will be stored in a flat structure (Arena).

We will use a library like `slotmap` (already in dependencies) or a `Vec` with generation indexing for `NodeId`.

```rust
pub struct Core {
    /// The flattening of the tree.
    pub nodes: SlotMap<NodeId, NodeContainer>,
    /// The root of the UI tree.
    pub root: NodeId,
    /// The persistent Taffy layout tree.
    pub taffy: TaffyTree<NodeId>, // Context is NodeId
    // ... focus, event_queue, etc.
}

pub struct NodeContainer {
    /// The actual widget logic.
    pub widget: Box<dyn Node>,
    /// Tree linkage.
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Link to the parallel Taffy node.
    pub taffy_id: taffy::NodeId,
    /// Layout configuration (The source of truth for Taffy).
    pub style: taffy::Style,
}
```

### The `Node` Trait Evolution
The `Node` trait methods currently take `&mut self`. In an Arena model, methods often need access to the "World" (the Arena) to query children or parents.

**Current:**
```rust
fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()>;
```

**Proposed:**
The `children` method is no longer needed on the trait. The *Container* manages structure. The *Widget* manages logic.

```rust
trait Node {
    /// Render self. Access to children is done via the Context if needed, 
    /// but usually render is just "draw self".
    fn render(&mut self, c: &mut Context, r: &mut Render) -> Result<()>;

    /// Handle events. Context provides access to the Tree to trigger updates on other nodes.
    fn handle_key(&mut self, c: &mut Context, k: Key) -> Result<EventOutcome>;
}
```

## 2. Persistent Taffy Integration

Because the Canopy tree is now a persistent Arena, we can maintain a *parallel* persistent `TaffyTree`.

### Synchronization
*   **Creation:** When a node is added to `Core.nodes`, we immediately call `Core.taffy.new_leaf()` (or `new_with_children`) and store the `taffy::NodeId` in `NodeContainer`.
*   **Structure Change:** When `parent.add_child(child)` is called on the Core, we update the `children` Vec in `NodeContainer` AND call `taffy.set_children(...)`.
*   **Style Change:** When a user modifies `node.style`, we mark the node as "layout dirty".

### The Layout Cycle
Unlike the "Immediate Mode Bridge" (which rebuilds the tree every frame), this architecture allows O(1) updates.

1.  **Update Phase:** User input modifies state or style. `core.mark_dirty(node_id)` is called.
2.  **Layout Phase:**
    *   Canopy calls `taffy.compute_layout(root_taffy_id, viewport_size)`.
    *   Taffy efficiently re-calculates geometry, caching unaffected branches.
3.  **Sync Phase:**
    *   Canopy iterates over the `SlotMap` (or just the dirty/visible nodes).
    *   It reads the computed layout from Taffy (`taffy.layout(node.taffy_id)`).
    *   It updates `node.state.viewport`.

## 3. User Experience

### Creating Widgets
Widgets no longer own their children. They *register* their children.

```rust
// In a hypothetical `init` or `mount` method
fn build_ui(c: &mut Core) -> NodeId {
    let root = c.add(Panel::new());
    
    let child1 = c.add(Label::new("Hello"));
    c.style(child1).flex_grow = 1.0;
    
    let child2 = c.add(Button::new("Click Me"));
    
    c.set_children(root, &[child1, child2]);
    
    // Set layout on parent
    let style = c.style(root);
    style.display = Display::Flex;
    style.flex_direction = FlexDirection::Row;
    
    root
}
```

### Event Bubbling
Bubbling becomes trivial iteration.

```rust
// Core logic
fn trigger_event(&mut self, target: NodeId, event: Event) {
    let mut current = Some(target);
    while let Some(id) = current {
        let node = &mut self.nodes[id];
        match node.widget.handle(event) {
            Handle => break,
            Ignore => current = node.parent, // Walk up the tree easily!
        }
    }
}
```

## 4. Migration Strategy
This is a major rewrite.
1.  **Refactor Core:** Implement the `Core` struct with `SlotMap` and `TaffyTree`.
2.  **Shim the Trait:** Create a temporary adapter that allows existing recursive `Node` implementations to sit inside the Arena (leafs first).
3.  **Port Widgets:** One by one, convert `Panel`, `Label`, etc., to use the new `NodeId` based API.
4.  **Switch Layout:** Enable `compute_layout` and remove the old `Layout::place` manual logic.
