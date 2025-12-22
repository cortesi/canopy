## 1. Executive Summary

This document specifies a foundational refactor of the `canopy` library. We are moving from a **recursive pointer tree** to a **flat Arena architecture** (`SlotMap`) integrated with the **Taffy** Flexbox engine.

**Objective:** To enable robust, web-standard layouts (Flexbox) and eliminate `RefCell` borrow conflicts, while making the API more terse and expressive.

## 2. Implementation Directives (Strict)

**To the Implementing Agent/Engineer:**
You are performing "open-heart surgery" on the library. Adhere to these constraints:

1. **Zero Feature Regression:** Do not remove capabilities. Mouse handling, focus, and rendering must persist.
2. **Green Tests:** Existing tests in `tests/` will break because they rely on `Node::new()`. You must refactor them to use the new `Core` system. **All tests must pass.**
3. **Example Parity:** The `examples/` directory is the source of truth. Every example must be ported to the new Builder API and behave identically.
4. **Borrow Safety:** You must use `NodeId` handles. Do not use `Rc<RefCell<Node>>`.
5. **Split Borrow Pattern:** When implementing the layout sync, you must follow the "Split Borrow" pattern (see Section 5) to avoid `&mut self` conflicts.

---

## 3. Core Data Structures

### 3.1 Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
taffy = { version = \"0.3\", features = [\"flexbox\"] }
slotmap = \"1.0\"

```

### 3.2 The Node ID

Use `slotmap` keys to safely reference nodes without pointers.

```rust
// src/core/id.rs
use slotmap::new_key_type;

new_key_type! {
    pub struct NodeId;
}

```

### 3.3 The Node Container

The `Node` struct is now a data container. It connects the **Logic** (Widget) to the **Layout** (Taffy) and **Tree** (Arena).

```rust
// src/core/node.rs
use taffy::prelude::*;
use crate::core::id::NodeId;
use crate::geom::Rect;

pub struct Node {
    /// The user behavior (Button, Label, etc.)
    pub widget: Box<dyn Widget>,
    
    /// Hierarchy (Flat Adjacency List)
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    
    /// Layout Engine Link
    pub taffy_id: taffy::node::NodeId,
    /// Cache style here for easy modification
    pub style: Style, 
    
    /// Computed Geometry (Absolute Screen Coordinates)
    /// Updated by Core::update_layout()
    pub viewport: Rect,
    
    /// State flags
    pub hidden: bool,
}

```

### 3.4 The Core (The World)

This replaces the old root node. It holds the entire application state.

```rust
// src/core/core.rs
use slotmap::SlotMap;
use taffy::TaffyTree;

pub struct Core {
    pub nodes: SlotMap<NodeId, Node>,
    pub taffy: TaffyTree,
    pub root: NodeId,
    pub focus: Option<NodeId>,
}

```

---

## 4. The Widget Trait Refactor

The `Widget` trait is stripped of layout responsibilities.

**Key Changes:**

* **Remove:** `layout()`, `add_child()`.
* **Add:** `measure()` (for auto-sizing leaf nodes like Text).
* **Update:** `render()` receives a pre-calculated `Rect`.

```rust
// src/widget/mod.rs

pub trait Widget: Any + Send {
    /// Render the widget into the buffer.
    /// `area` is the absolute screen coordinates (calculated by Core).
    fn render(&self, frame: &mut Frame, area: Rect, ctx: &Context);

    /// (Optional) Calculate intrinsic size.
    /// Used by Taffy for leaf nodes (e.g., Text) to determine size based on content.
    /// If not implemented, size is controlled purely by Style.
    fn measure(&self, ctx: &Context) -> Size<Option<f32>> {
        Size { width: None, height: None } 
    }

    /// Handle events.
    fn on_event(&mut self, event: &Event, ctx: &mut Context) -> EventResult {
        EventResult::Ignored
    }
}

```

---

## 5. Critical Algorithms

### 5.1 Layout Synchronization (The "Split Borrow" Pattern)

**Problem:** You cannot recurse on `&mut Core` because you need to mutate `nodes` while reading structure.
**Solution:** Separate `nodes` and `taffy` references.

```rust
// src/core/layout.rs

impl Core {
    pub fn update_layout(&mut self, screen_size: Size) {
        // 1. Run Solver (Taffy owns its own data, safe to borrow)
        let root_t_id = self.nodes[self.root].taffy_id;
        self.taffy.compute_layout(
            root_t_id,
            taffy::geometry::Size {
                width: available(screen_size.width),
                height: available(screen_size.height),
            }
        ).expect(\"Layout solver failed\");

        // 2. Sync Viewports (Call helper with split borrows)
        let root = self.root;
        sync_viewports(&mut self.nodes, &self.taffy, root, Point::new(0, 0));
    }
}

/// Helper: Recursive sync that does not borrow Core
fn sync_viewports(
    nodes: &mut SlotMap<NodeId, Node>,
    taffy: &TaffyTree,
    node_id: NodeId,
    parent_offset: Point
) {
    let node = &mut nodes[node_id];
    let layout = taffy.layout(node.taffy_id).unwrap();

    // Calculate Absolute Position
    let abs_pos = parent_offset + Point::new(layout.location.x as u16, layout.location.y as u16);
    let size = Size::new(layout.size.width as u16, layout.size.height as u16);
    
    node.viewport = Rect::new(abs_pos, size);

    // Clone children IDs to release borrow on 'node' before recursion
    let children = node.children.clone();

    for child in children {
        sync_viewports(nodes, taffy, child, abs_pos);
    }
}

```

### 5.2 Event Bubbling (The Parent Loop)

In a flat arena, bubbling is an explicit loop, not a call stack return.

```rust
// src/core/events.rs

impl Core {
    pub fn dispatch_event(&mut self, event: Event) {
        // Start at focused node or root
        let mut target = self.focus.or(Some(self.root));
        
        while let Some(id) = target {
            let node = &mut self.nodes[id];
            
            // 1. Dispatch to widget
            let result = node.widget.on_event(&event);
            
            // 2. Handle propagation
            if result == EventResult::StopPropagation {
                break;
            }
            
            // 3. Bubble up
            target = node.parent;
        }
    }
}

```

---

## 6. Developer Experience: Fluent Builder

To satisfy the "terse and expressive" requirement, implement a Builder.

```rust
// src/builder.rs

pub struct NodeBuilder<'a> {
    core: &'a mut Core,
    id: NodeId,
}

impl<'a> NodeBuilder<'a> {
    /// Modify Style
    pub fn style(self, f: impl FnOnce(&mut Style)) -> Self {
        let t_id = self.core.nodes[self.id].taffy_id;
        // Load, Modify, Save
        let mut style = self.core.taffy.style(t_id).cloned().unwrap_or_default();
        f(&mut style);
        self.core.taffy.set_style(t_id, style).unwrap();
        // Update local cache
        self.core.nodes[self.id].style = style;
        self
    }

    // --- Layout Shorthands ---
    pub fn flex_row(self) -> Self {
        self.style(|s| {
            s.display = Display::Flex;
            s.flex_direction = FlexDirection::Row;
        })
    }
    
    pub fn flex_col(self) -> Self {
        self.style(|s| {
            s.display = Display::Flex;
            s.flex_direction = FlexDirection::Column;
        })
    }
    
    pub fn w_full(self) -> Self {
        self.style(|s| s.size.width = Dimension::Percent(1.0))
    }

    // --- Hierarchy ---
    /// Add a child and return the Parent builder (chainable)
    pub fn add_child(self, child_id: NodeId) -> Self {
        self.core.mount_child(self.id, child_id);
        self
    }
}

```

---

## 7. Migration Guide: Examples

The implementing model must rewrite `examples/` to use this new syntax.

### Example: `hello_world.rs`

**Old Syntax:**

```rust
let mut root = Node::new(Panel::new());
root.split_horiz(
    Node::new(Label::new(\"Left\")),
    Node::new(Label::new(\"Right\"))
);

```

**New Syntax:**

```rust
fn main() {
    let mut app = Canopy::new();
    let core = &mut app.core;

    // 1. Create Leaves
    let left = core.add(Label::new(\"Left\"));
    let right = core.add(Label::new(\"Right\"));

    // 2. Compose
    core.build(core.root)
        .flex_row()
        .w_full()
        .add_child(left)
        .add_child(right);

    app.run();
}

```

## 8. Implementation Checklist

1. [x] **Core:** Implement `Core`, `Node`, `NodeId`.
2. [x] **Taffy Bridge:** Implement `Core::add()` (registers to Arena AND Taffy) and
   `Core::mount_child()` (updates Arena parent/child AND Taffy parent/child).
3. [x] **Algorithms:** Copy-paste `update_layout` (Section 5.1) and `dispatch_event` (Section 5.2).
4. [x] **Widgets:** Update `Label` to implement `measure`. Remove `layout` from all widgets.
5. [x] **Tests:** Fix `tests/` compilation errors.
6. [x] **Examples:** Port all files in `examples/`.

## 9. Follow-up Checklist (Post-port Fixes)

1. [x] **Focusgym Render:** Ensure the initial render draws the focus blocks without input.
2. [x] **Focusgym Layout:** Align child panes so edges meet consistently across splits.
3. [x] **Focusgym Controls:** Add flex grow/shrink commands and bindings with unit tests.
4. [x] **Focus Navigation:** Prevent panics when focus traverses nodes mid-command dispatch.

## 10. Zero-Size Focus & Render Guardrails

1. [x] **Render Boundaries:** Allow zero-size children to sit on parent edges without crashing.
2. [x] **Focus Filtering:** Skip zero-size nodes during focus traversal while preserving fallbacks.
3. [x] **Focus Recovery:** Move focus off nodes that collapse to zero after layout updates.
4. [x] **Core Tests:** Add core tests reproducing the zero-size render and focus scenarios.

## 11. Focusgym Flex Controls & Resize Stability

1. [x] **Explicit Commands:** Rename flex grow/shrink commands to explicit coefficient names.
2. [x] **Min-Size Guard:** Prevent flex adjustments that would collapse a block below 1×1.
3. [x] **Resize Crash Fix:** Clamp layout positions to parent bounds during layout sync.
4. [x] **Core Tests:** Add a deep-tree resize regression test.

## 12. Focusgym Deletion & Edge Frames

1. [x] **Flush Edges:** Avoid drawing borders on screen edges so blocks are flush to the screen.
2. [x] **Delete Command:** Add a focused-node delete command with an `x` binding.
3. [x] **Example Tests:** Cover edge flush behavior and focused deletion in focusgym tests.

## 13. Focusgym Constant Separators

1. [x] **Single-Side Borders:** Draw separators only on right/bottom edges to avoid doubles.
2. [x] **Example Tests:** Add a test that asserts a single separator column between root children.

## 14. Focusgym Separator & Focus Deletion Follow-up

1. [x] **Layout Rounding:** Align layout rounding so sibling boundaries remain contiguous.
2. [x] **Separator Drawing:** Draw left/top separators on all non-edge leaves to ensure a single
   constant border, even across nested splits.
3. [x] **Delete Focus Order:** Keep focus on the next focusable node after deletion with tests.
