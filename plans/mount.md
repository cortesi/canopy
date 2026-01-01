# Node Mounting, Child Management, and Structural Invariants

## Scope and goals

This specification defines a unified refactor of node lifecycle operations (creation, attachment, detachment, deletion), child identity/access APIs (by key, by type, by path), widget lifecycle hooks for teardown, and core invariants for focus and mouse capture. All widgets and internal call sites must be migrated to this API set in a single change; no compatibility shims are provided.

Goals:

* Make “create + attach” the default and safe (no orphan leakage on errors).
* Provide stable, ergonomic ways for widgets to re-find and mutate their children without storing IDs in the common case.
* Introduce real subtree deletion and a teardown lifecycle hook.
* Centralize enforcement of focus and mouse capture invariants after all structural mutations.
* Provide “exactly one” path helpers to eliminate silent ambiguity.

Non-goals:

* Introducing a reconciler/diff engine or declarative UI framework.
* Maintaining incremental global indices for arbitrary queries (beyond minimal type identity storage).
* Automatic elimination of all child ID storage (it remains valid and recommended for hot paths).

---

## Terminology and lifecycle states

A node transitions among these states:

1. **Detached**: present in the arena, not reachable from `root`.
2. **Attached**: reachable from `root` via parent/children links.
3. **Removed**: deleted from the arena; its `NodeId` is invalid.

Definitions:

* **Attached-to-root**: a node is attached if it is reachable from `Core.root` by following `children`.
* **Mounted**: a node is mounted if it has had `Widget::on_mount` called at least once.

Lifecycle semantics:

* `on_mount` runs **exactly once** for each node, the first time it becomes attached-to-root.
* `detach` / `attach` **do not** re-run `on_mount` for nodes that are already mounted.
* `on_unmount` runs **exactly once** for each node, immediately before it is removed from the arena.
* `pre_remove` runs as a veto/validation phase prior to removal.

---

## API surface

### Object-safe `Context` core primitives

The `Context` trait remains object-safe and continues to expose non-generic primitives (existing or equivalent), such as:

* `fn node_id(&self) -> NodeId`
* `fn with_widget<R>(&mut self, id: NodeId, f: impl FnOnce(&mut dyn Widget, &mut dyn Context) -> Result<R>) -> Result<R>` *(or existing typed variant)*
* `fn children_of(&self, id: NodeId) -> Vec<NodeId>`
* `fn parent_of(&self, id: NodeId) -> Option<NodeId>`
* `fn find_nodes(&self, path: &str) -> Vec<NodeId>` *(existing path system)*
* Structural primitives implemented in `Core` and surfaced through `Context` (see below)

All generic conveniences (typed lookups, typed `with_*`) are implemented as inherent methods on `dyn Context`.

---

## Node lifecycle operations

### 1) Creation and attachment

#### Primary creation APIs (always attach)

```rust
// Attach new child to the current context node
fn add_child<W: Widget + 'static>(&mut self, widget: W) -> Result<NodeId>;

// Attach new child to explicit parent
fn add_child_to<W: Widget + 'static>(&mut self, parent: NodeId, widget: W) -> Result<NodeId>;
```

Behavioral requirements:

* **Transactional:** If `add_child(_to)` returns `Err`, the UI tree and arena are unchanged (no leaked nodes).
* On success, the node is attached as the last child of `parent`.
* If `parent` is attached-to-root, the new node becomes attached-to-root and must be mounted (see mounting rules).

#### Detached creation for advanced reparenting

```rust
// Create node in arena but detached from the tree.
fn create_detached<W: Widget + 'static>(&mut self, widget: W) -> NodeId;
```

Behavioral requirements:

* `create_detached` never mounts the node.
* Nodes created detached may be attached later via `attach`.

#### Attaching a detached node

```rust
fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()>;
```

Behavioral requirements:

* **Preconditions / errors:**

  * `parent` and `child` must exist in the arena.
  * `child` must be detached (`child.parent == None`) or `attach` returns `Err(AlreadyAttached)`.
  * Attaching must not create cycles (`Err(WouldCreateCycle)`).
* Attaches `child` as the last child of `parent`.
* If `parent` is attached-to-root, `child` becomes attached-to-root; any nodes in that newly attached subtree that are not yet mounted must be mounted (pre-order; see “Mounting order” below).
* If `parent` is not attached-to-root, `child` remains detached-to-root and is not mounted.

#### Detaching a node (no deletion)

```rust
fn detach(&mut self, child: NodeId) -> Result<()>;
```

Behavioral requirements:

* Detaches `child` from its current parent (if attached), leaving it in the arena.
* Does not call `on_unmount` (node is not removed).
* Does not change `mounted` state.
* Must update focus/capture invariants (see “Structural invariants”).

### 2) Keyed children (stable identity without storing IDs)

Keyed children provide a stable, per-parent mapping from a short role key (e.g. `"label"`) to a direct child.

#### Keyed creation

```rust
fn add_child_keyed<W: Widget + 'static>(&mut self, key: &str, widget: W) -> Result<NodeId>;
fn add_child_to_keyed<W: Widget + 'static>(&mut self, parent: NodeId, key: &str, widget: W) -> Result<NodeId>;
```

Behavioral requirements:

* Keys are unique among direct children of a given parent.
* If `key` is already present under the parent, return `Err(DuplicateChildKey)` and do not mutate tree/arena.
* Keyed creation is **transactional** with the same “no leak on error” guarantee as `add_child_to`.

#### Keyed attachment (optional but supported)

Key association is defined by the parent-child relationship, not by the node itself. When a keyed child is detached, its key association is removed.

The API surface does not require a “keyed attach” operation, but if provided it must follow the same uniqueness constraints:

```rust
fn attach_keyed(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()>;
```

#### Keyed lookup

```rust
fn child_keyed(&self, key: &str) -> Option<NodeId>;
```

Typed keyed mutation:

```rust
fn with_keyed<W: Widget + 'static, R>(
    &mut self,
    key: &str,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<R>;

fn try_with_keyed<W: Widget + 'static, R>(
    &mut self,
    key: &str,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<Option<R>>;
```

Behavioral requirements:

* If `key` is missing:

  * `with_keyed` returns `Err(NotFound)`
  * `try_with_keyed` returns `Ok(None)`
* If `key` exists but the node is not of type `W`: return `Err(TypeMismatch)`.

### 3) Deletion (real subtree removal)

```rust
fn remove_subtree(&mut self, node: NodeId) -> Result<()>;
```

Behavioral requirements:

* Removes `node` and all descendants from the arena.
* If `node` is attached, it is implicitly detached as part of removal.
* `node == root` is invalid and must return `Err(InvalidOperation)`.

Removal phases and ordering:

1. **Gather** nodes in the subtree (stable traversal order).
2. **Veto / validation**: call `pre_remove` top-down (pre-order). If any `pre_remove` returns `Err`, abort removal and perform no detachment/unmount/deletion.
3. **Teardown**: call `on_unmount` bottom-up (post-order) while nodes are still structurally present (still attached during this phase).
4. **Detach and delete**: remove edges and delete nodes from the arena (children before parents).
5. **Enforce invariants**: run focus/capture invariant enforcement with knowledge of the removed subtree root.

---

## Widget lifecycle hooks

The widget trait gains explicit hooks for mount and unmount:

```rust
pub trait Widget: Any + Send + CommandNode {
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> { Ok(()) }

    // Validation/veto hook. Must be side-effect free or safely repeatable.
    fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> { Ok(()) }

    // Best-effort teardown hook. Must not fail.
    fn on_unmount(&mut self, _ctx: &mut dyn Context) { }
}
```

Mounting order (normative):

* When a subtree becomes attached-to-root for the first time, nodes that are not yet mounted are mounted in **pre-order** (parent before children), following the current `children` ordering.
* If `on_mount` for a node returns `Err`, the operation that triggered mounting (`add_child_to`, `attach`, etc.) must roll back such that:

  * the tree is unchanged, and
  * no nodes created by that operation remain leaked in the arena.

---

## Typed lookup by type

Type-based lookups are convenience APIs. Because type collisions are common (e.g. multiple `Text` nodes), the API distinguishes “first match” vs “unique match”.

All type-based traversals use **pre-order** (depth-first, left-to-right), and `descendant` searches exclude the current node.

### Queries

```rust
// Direct children
fn first_child<W: Widget + 'static>(&self) -> Option<TypedId<W>>;
fn unique_child<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>>; // Err if >1
fn children_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>>;

// Descendants (excluding self)
fn first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>>;
fn unique_descendant<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>>; // Err if >1
fn descendants_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>>;
```

### Typed mutation helpers

```rust
fn with_first_descendant<W: Widget + 'static, R>(
    &mut self,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<R>; // Err if not found

fn try_with_first_descendant<W: Widget + 'static, R>(
    &mut self,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<Option<R>>;

fn with_unique_descendant<W: Widget + 'static, R>(
    &mut self,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<R>; // Err if none or >1

fn try_with_unique_descendant<W: Widget + 'static, R>(
    &mut self,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<Option<R>>; // Err if >1
```

Typed direct-child variants may also be provided (`with_unique_child`, etc.) with analogous semantics.

---

## Path lookup extensions

Path lookup uses the existing path matcher. This spec adds “exactly one” helpers to eliminate ambiguity.

```rust
fn find_one(&self, path: &str) -> Result<NodeId>;              // Err if 0 or >1
fn try_find_one(&self, path: &str) -> Result<Option<NodeId>>;  // Err if >1
```

Typed mutation via path:

```rust
fn with_node_at<W: Widget + 'static, R>(
    &mut self,
    path: &str,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<R>; // Err if 0 or >1 matches, or type mismatch

fn try_with_node_at<W: Widget + 'static, R>(
    &mut self,
    path: &str,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
) -> Result<Option<R>>; // Err if >1 matches, or type mismatch if matched
```

---

## Structural invariants: focus and mouse capture

The core maintains the following invariants:

### Focus invariant

If `Core.focus` is `Some(id)`, then:

* `id` exists in the arena
* `id` is attached-to-root
* `id` is visible:

  * `node.hidden == false`
  * `node.view` is non-zero (or equivalent “renderable” predicate)
* `id` is focusable (`node.focusable == true`, or widget-defined focusable predicate)

If any condition fails, focus must be moved to a valid replacement according to the recovery policy below, or cleared if no focusable nodes exist.

### Mouse capture invariant

If `Core.mouse_capture` is `Some(id)`, then:

* `id` exists in the arena
* `id` is attached-to-root

If not, capture must be cleared (`None`). (Visibility is not required; capture is released strictly on detachment/removal.)

### Invariant enforcement points (normative)

`Core::ensure_invariants(...)` must be called after every structural mutation, including at minimum:

* `add_child`, `add_child_to`, `add_child_keyed`, `add_child_to_keyed`
* `create_detached` does not require invariants (no structural change), but attaching does
* `attach`, `attach_keyed`
* `detach`
* `remove_subtree`
* `set_children` / child reorder APIs
* `set_hidden` and any APIs that can make a focused node invisible
* any future API that changes parent/children relationships or visibility

### Focus recovery policy

When focus becomes invalid, choose a replacement in this order:

1. If a subtree was removed (`removed_root: Some(NodeId)`), attempt a “nearby” focusable relative to the removed subtree:

   * **Next** focusable node after the removed subtree in a depth-first traversal of the tree
   * Else **previous** focusable node before the removed subtree
   * Else nearest focusable **ancestor** of the removed subtree’s former parent chain (if available)
2. Else (no specific removed root), attempt:

   * the next focusable in traversal order after the current focus (if current exists but is invisible), else
   * `first_focusable(Core.root)`
3. If no focusable node exists, clear focus (`None`).

All candidates must satisfy the focus invariant (exist, attached, visible, focusable).

### API shape

`Core` provides:

```rust
fn ensure_invariants(&mut self, removed_root: Option<NodeId>);

fn ensure_focus_valid(&mut self, removed_root: Option<NodeId>);
fn ensure_mouse_capture_valid(&mut self);
```

`ensure_invariants` is invoked by structural mutation methods as described above.

---

## Error model (normative)

The following error conditions must be distinguishable (exact enum names may vary, but the semantics must exist):

* `NotFound`: referenced node or key not found
* `TypeMismatch`: node exists but is not of the requested widget type
* `MultipleMatches`: a query that requires uniqueness matched more than one node
* `DuplicateChildKey`: adding/attaching keyed child with an existing key under the same parent
* `AlreadyAttached`: attaching a node that already has a parent
* `WouldCreateCycle`: attaching would create a parent/child cycle
* `InvalidOperation`: e.g. removing root, attaching to non-existent parent, etc.

---

## Internal data model requirements

### Node metadata for fast type checks

Each node stores its widget’s `TypeId` at creation:

```rust
widget_type: std::any::TypeId
```

Type queries and typed access must use `TypeId` comparisons rather than repeated downcasts.

### Mounted state

Each node stores:

```rust
mounted: bool
```

This is used to ensure `on_mount` runs exactly once across detach/attach operations.

### Keyed children bookkeeping

Each node (as a parent) maintains a mapping:

* `child_keys: HashMap<String, NodeId>` (or equivalent)

Keyed association must be updated on attach, detach, remove, and set_children (if set_children can reorder children, it must not invalidate keys).

---

## Behavioral examples

### Button: keyed child access (stable)

```rust
pub struct Button {
    label: String,
}

impl Button {
    pub fn set_label(&mut self, ctx: &mut dyn Context, label: &str) -> Result<()> {
        self.label = label.to_string();
        ctx.with_keyed::<Text, _>("label", |t, _| { t.set_raw(label); Ok(()) })
    }
}

impl Widget for Button {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let root = ctx.add_child(Box::new())?;
        let center = ctx.add_child_to(root, Center::new())?;
        ctx.add_child_to_keyed(center, "label", Text::new(&self.label))?;
        Ok(())
    }
}
```

### List: remove vs take

```rust
pub fn remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<bool> {
    if index >= self.items.len() { return Ok(false); }
    let id = self.items.remove(index);
    ctx.remove_subtree(id.into())?;
    self.repair_selection_and_focus(ctx);
    Ok(true)
}

pub fn take(&mut self, ctx: &mut dyn Context, index: usize) -> Result<Option<TypedId<W>>> {
    if index >= self.items.len() { return Ok(None); }
    let id = self.items.remove(index);
    ctx.detach(id.into())?;
    self.repair_selection_and_focus(ctx);
    Ok(Some(id))
}
```

---

# Addendum: staged implementation plan (markdown checklist)

## Stage 0 — Core scaffolding and invariants

* [x] Add `widget_type: TypeId` and `mounted: bool` fields to `Node`.
* [x] Add per-parent keyed child map storage (`child_keys: HashMap<String, NodeId>` or equivalent).
* [x] Define/extend error types: `NotFound`, `TypeMismatch`, `MultipleMatches`, `DuplicateChildKey`, `AlreadyAttached`, `WouldCreateCycle`, `InvalidOperation`.
* [x] Implement `Core::is_attached_to_root(node: NodeId) -> bool`.
* [x] Implement focus invariant helpers:

  * [x] `Core::first_focusable(root) -> Option<NodeId>`
  * [x] `Core::next_focusable_after_subtree(removed_root) -> Option<NodeId>`
  * [x] `Core::prev_focusable_before_subtree(removed_root) -> Option<NodeId>`
  * [x] `Core::nearest_focusable_ancestor(start) -> Option<NodeId>`
* [x] Implement `Core::ensure_mouse_capture_valid()`.
* [x] Implement `Core::ensure_focus_valid(removed_root: Option<NodeId>)` with the specified recovery policy.
* [x] Implement `Core::ensure_invariants(removed_root: Option<NodeId>)`.

## Stage 1 — Transactional structural primitives in `Core`

* [x] Implement `Core::create_detached(widget) -> NodeId` (allocates node with `mounted=false`, no parent).
* [x] Implement `Core::attach(parent, child) -> Result<()>`:

  * [x] Validate existence, `child.parent == None`, cycle prevention.
  * [x] Update `parent.children`, set `child.parent`.
  * [x] If `parent` attached-to-root: mount newly attached nodes (pre-order) where `mounted=false`.
  * [x] On mount failure: roll back parent/child links; ensure no partial attachment remains.
  * [x] Call `ensure_invariants(None)`.
* [x] Implement `Core::detach(child) -> Result<()>`:

  * [x] If detached: `Ok(())`.
  * [x] Remove from parent’s `children`.
  * [x] Remove any keyed association under parent.
  * [x] Clear `child.parent`.
  * [x] Call `ensure_invariants(Some(child))` or `ensure_invariants(None)` per implementation convenience.
* [x] Implement keyed operations in `Core`:

  * [x] `Core::attach_keyed(parent, key, child) -> Result<()>` with key uniqueness and map maintenance.
  * [x] `Core::add_child_to_keyed(parent, key, widget) -> Result<NodeId>` transactional via create_detached + attach_keyed + rollback removal on failure.

## Stage 2 — Mounting and removal lifecycle

* [x] Extend `Widget` trait with `pre_remove` and `on_unmount` hooks per spec.
* [x] Implement mount traversal used by `attach`:

  * [x] Pre-order traversal; call `on_mount` once per node; set `mounted=true` on success.
* [x] Implement `Core::remove_subtree(node) -> Result<()>`:

  * [x] Reject `node == root`.
  * [x] Gather subtree ids (stable ordering).
  * [x] Run `pre_remove` top-down; abort on error with no structural changes.
  * [x] Run `on_unmount` bottom-up while still attached.
  * [x] Detach subtree root if attached.
  * [x] Delete nodes from arena (children before parents).
  * [x] Call `ensure_invariants(Some(node))`.

## Stage 3 — `Context` exposure and generic convenience methods

* [x] Expose `add_child`, `add_child_to`, `create_detached`, `attach`, `detach`, `remove_subtree`, keyed variants via `dyn Context` inherent methods.
* [x] Implement keyed accessors on `dyn Context`:

  * [x] `child_keyed`, `with_keyed`, `try_with_keyed`.
* [x] Implement type-based queries on `dyn Context`:

  * [x] `first_child`, `unique_child`, `children_of_type`
  * [x] `first_descendant`, `unique_descendant`, `descendants_of_type`
  * [x] Typed `with_*`/`try_with_*` helpers with correct error semantics.
* [x] Implement path helpers on `dyn Context`:

  * [x] `find_one`, `try_find_one`
  * [x] `with_node_at`, `try_with_node_at` with uniqueness + type enforcement.

## Stage 4 — Enforce invariants after *all* structural mutations

* [x] Audit all existing APIs that mutate structure or visibility and ensure they invoke `Core::ensure_invariants(...)`:

  * [x] `set_children` / reorder APIs
  * [x] `set_hidden` and any visibility toggles
  * [x] Any remaining attach/detach/mount helpers
* [x] Add targeted regression tests:

  * [x] Focus cleared or recovered when removing focused node
  * [x] Mouse capture released when captured node is detached/removed
  * [x] Focus recovered to nearby nodes per policy
  * [x] Key uniqueness enforced; keyed detach clears mapping
  * [x] Transactionality: failures do not leak nodes or partially mutate the tree

## Stage 5 — Widget and example migration (single sweeping change)

* [x] Convert widgets from `add_orphan`/`mount_child*`/`ensure_tree` patterns to `add_child*` and keyed/type/path access.
* [x] Replace common “single child by type” patterns with:

  * [x] keyed children for stable internal roles, or
  * [x] `with_unique_descendant` when uniqueness is structural and stable.
* [x] Update list-like widgets:

  * [x] Add `remove` (delete) and `take` (detach) with correct invariant repair.
* [x] Update any demo apps/examples to match new APIs.

## Stage 6 — Cleanup and API finalization

* [x] Remove old APIs (`add_orphan`, `mount_child`, `mount_child_to`, `detach_child`, etc.).
* [x] Update documentation and inline docs for all new APIs and semantics.
* [x] Add “best practices” guidance:

  * [x] Prefer keyed children for stable intra-widget roles.
  * [x] Use `unique_*` APIs for structural invariants; avoid `first_*` in complex widgets.
  * [x] Store IDs or use keys for hot paths; avoid repeated subtree traversal in per-frame loops.
