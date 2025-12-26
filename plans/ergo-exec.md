# API Ergonomics Execution Plan

This plan improves the Canopy widget API based on analysis of `focusgym.rs`. Each stage is
self-contained and leaves tests passing. See `./ergo.md` for full rationale.

**Goal:** Transform verbose widget creation patterns into fluent, intuitive APIs.

**Before (current):**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    let root_block = c.add_child(c.node_id(), Block::new(true))?;
    let left = c.add_widget(Block::new(false));
    let right = c.add_widget(Block::new(false));
    Block::init_flex(c, left)?;
    Block::init_flex(c, right)?;
    c.with_widget(root_block, |block: &mut Block, ctx| {
        block.sync_layout(ctx, &[left, right])
    })?;
    c.build(c.node_id()).flex_col();
    c.build(root_block).flex_item(1.0, 1.0, Dimension::Auto);
    Ok(())
}
```

**After (proposed):**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    c.build().flex_col();
    let root_block = c.add_child(Block::new(true))?;
    c.context_for(root_block)?
        .build()
        .flex_item(1.0, 1.0, Dimension::Auto)
        .flex_row()
        .add_widget(Block::new(false))
        .add_widget(Block::new(false));
    Ok(())
}
```

---

# Stage 1: Documentation and Cleanup

Quick wins that require no API changes. Clarify existing guarantees and remove dead code.

1. [x] Update `Widget::on_mount` doc comment in `crates/canopy/src/widget/mod.rs` to explicitly
       state: "Called exactly once when the widget is first mounted. The framework guarantees
       single invocation via an internal `mounted` flag."

2. [x] Remove the unnecessary guard from `FocusGym::on_mount` in
       `crates/examples/src/focusgym.rs:275-278`. The check `if Self::root_block_id(c).is_some()`
       is redundant since `on_mount` is guaranteed to run once.

3. [x] Search codebase for any other `on_mount` implementations with similar redundant guards
       and remove them. (None found - only FocusGym had the guard)

4. [x] Run tests: `cargo nextest run --all --all-features` (172 passed)

---

# Stage 2: Context API Refactoring

This is the core ergonomic improvement. Remove redundant `node_id()` arguments from Context
methods by making them operate on the current node by default. Add `context_for(descendant)`
to get a child-scoped context when needed.

## 2A: ViewContext - Remove NodeId from Current-Node Methods

The following methods almost always use `c.node_id()`. Change them to operate on the current
node by default:

5. [ ] `children(node)` → `children()` (current node) + keep `children_of(node)` for queries
       - Update trait in `crates/canopy/src/core/context.rs`
       - Update `CoreContext` and `CoreViewContext` implementations
       - Update `DummyContext` in testing

6. [ ] Update all call sites:
       - `c.children(c.node_id())` → `c.children()`
       - `c.children(other_node)` → `c.children_of(other_node)` (if querying non-current)

7. [ ] Run tests to verify no regressions

## 2B: Context - Remove NodeId from Mutation Methods

8. [ ] `set_children(parent, children)` → `set_children(children)` for current node
       - Add `set_children_of(parent, children)` for explicit parent cases

9. [ ] `with_style(node, f)` → `with_style(f)` for current node
       - Add `with_style_of(node, f)` for explicit node cases

10. [ ] `mount_child(parent, child)` → `mount_child(child)` mounts to current node
        - Add `mount_child_to(parent, child)` for explicit parent

11. [ ] `detach_child(parent, child)` → `detach_child(child)` detaches from current node
        - Add `detach_child_from(parent, child)` for explicit parent

12. [ ] `set_hidden(node, hidden)` → `set_hidden(hidden)` for current node
        - `hide()` and `show()` already exist and operate on current node

13. [ ] `add_child(parent, widget)` → `add_child(widget)` adds to current node
        - Add `add_child_to(parent, widget)` for explicit parent

14. [ ] Update all call sites for the above methods

15. [ ] Run tests

## 2C: Focus Operations - Local vs Global

Focus operations need special handling since they search within a subtree:

16. [ ] Add `focus_next()` / `focus_prev()` / `focus_first()` that search within current subtree
        (equivalent to `focus_next(c.node_id())`)

17. [ ] Add `focus_next_global()` / `focus_prev_global()` / `focus_first_global()` that search
        from root (equivalent to `focus_next(c.root_id())`)

18. [ ] Keep `focus_next_in(subtree)` / etc. for explicit subtree specification (rename from
        current `focus_next(root)`)

19. [ ] Similarly for directional focus: `focus_right()` / `focus_right_global()` /
        `focus_right_in(subtree)`

20. [ ] Update all call sites:
        - `c.focus_next(c.node_id())` → `c.focus_next()`
        - `c.focus_next(c.root_id())` → `c.focus_next_global()`
        - `c.focus_next(self.app)` → `c.focus_next_in(self.app)`

21. [ ] Run tests

## 2D: Add `context_for(descendant)` Method

22. [ ] Create `ContextFor<'a, C>` wrapper struct in `crates/canopy/src/core/context.rs`:
        ```rust
        pub struct ContextFor<'a, C: Context + ?Sized> {
            inner: &'a mut C,
            node_id: NodeId,
        }
        ```

23. [ ] Implement `ViewContext` for `ContextFor` - delegate all calls but override `node_id()`

24. [ ] Implement `Context` for `ContextFor` - delegate all calls using the overridden node_id

25. [ ] Add `context_for(descendant: NodeId) -> Result<ContextFor<'_, Self>>` to Context trait:
        - Validate that `descendant` is actually a descendant of `self.node_id()`
        - Return error if not (enforces encapsulation - can't reach up or sideways)

26. [ ] Update call sites that operate on child nodes:
        - `c.with_style(child, f)` → `c.context_for(child)?.with_style(f)`
        - `c.build(child).flex_row()` → `c.context_for(child)?.build().flex_row()`

27. [ ] Run tests

## 2E: Simplify `build()` Method

28. [ ] Change `build(node)` to `build()` operating on current node
        - The `context_for(child)?.build()` pattern handles child building

29. [ ] Update all call sites:
        - `c.build(c.node_id())` → `c.build()`
        - `c.build(child)` → `c.context_for(child)?.build()`

30. [ ] Run tests: `cargo nextest run --all --all-features`

---

# Stage 3: Clarify Orphan Widget Creation

Rename `add_widget` to `add_orphan` to make explicit that created widgets are not attached to
the tree. This prevents confusion about when to use `add_child` vs `add_widget`.

31. [ ] In `crates/canopy/src/core/context.rs`, rename `add_widget` to `add_orphan`:
        ```rust
        pub fn add_orphan<W: Widget + 'static>(&mut self, widget: W) -> NodeId {
            self.add(widget.into())
        }
        ```

32. [ ] Similarly rename `add_typed` to `add_orphan_typed` for consistency.

33. [ ] Update all call sites from `add_widget` to `add_orphan`:
        - `crates/examples/src/focusgym.rs` - Block::add, Block::split
        - `crates/canopy/tests/test_on_mount.rs` - MountProbe::on_mount

34. [ ] Run tests: `cargo nextest run --all --all-features`

---

# Stage 4: Add `add_widget` to NodeBuilder

Enable fluent widget creation+attachment via the builder pattern. This allows:
```rust
c.build().flex_row().add_widget(Child::new()).add_widget(Child::new());
```

35. [ ] In `crates/canopy/src/core/builder.rs`, extend `BuildContext` trait:
        ```rust
        pub trait BuildContext {
            fn with_style(...) -> Result<()>;
            fn mount_child(...) -> Result<()>;
            fn add_orphan(&mut self, widget: Box<dyn Widget>) -> NodeId;
        }
        ```

36. [ ] Implement `add_orphan` for `dyn Context + '_` and `Core` in builder.rs.

37. [ ] Add `add_widget` method to `NodeBuilder`:
        ```rust
        pub fn add_widget<W: Widget + 'static>(self, widget: W) -> Self {
            let child = self.ctx.add_orphan(Box::new(widget));
            self.add_child(child)
        }
        ```

38. [ ] Update focusgym to use the new fluent pattern where beneficial.

39. [ ] Run tests: `cargo nextest run --all --all-features`

---

# Stage 5: Move Block Flex Defaults to configure_style

Eliminate the awkward `Block::init_flex` static method by moving default flex styles into
the widget's `configure_style` implementation.

40. [ ] In `crates/examples/src/focusgym.rs`, update `Block::configure_style` to include flex
        defaults:
        ```rust
        fn configure_style(&self, style: &mut Style) {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
            style.min_size.width = Dimension::Points(1.0);
            style.min_size.height = Dimension::Points(1.0);
        }
        ```

41. [ ] Remove all calls to `Block::init_flex(c, node_id)?` from focusgym.rs

42. [ ] Delete the `Block::init_flex` method entirely.

43. [ ] Run tests: `cargo nextest run --all --all-features`

---

# Stage 6: Simplify sync_layout Pattern

The `sync_layout` pattern in Block requires `with_widget` to access mutably. Consider whether
Block needs to track `has_children` or if this can be derived.

44. [ ] Analyze whether `Block::has_children` field can be removed by deriving it from
        `c.children().is_empty()` at render time. If so:
        - Remove `has_children` field from Block struct
        - Update `accept_focus` to query children dynamically
        - Simplify or remove `sync_layout`

45. [ ] If Block still needs sync_layout, simplify the calling pattern using the new APIs.

46. [ ] Run tests: `cargo nextest run --all --all-features`

---

# Stage 7: Clarify measure vs canvas_size

The Widget trait has two sizing methods with confusing semantics:
- `measure` - Called by Taffy layout engine for intrinsic size
- `canvas_size` - Called after layout for scrollable canvas dimensions

**Distinction:**
| Method | Purpose | Example (100-item list in 10-row view) |
|--------|---------|----------------------------------------|
| `measure` | Layout allocation | Returns 10 (fits viewport) |
| `canvas_size` | Scrollable content | Returns 100 (total items) |

47. [ ] Improve doc comments in `crates/canopy/src/widget/mod.rs`:
        - `measure`: "Returns intrinsic size for Taffy layout..."
        - `canvas_size`: "Returns the virtual canvas size for scrolling..."

48. [ ] Review `Block` in focusgym.rs - it overrides both. If Block doesn't scroll, it likely
        only needs the default implementations. Analyze whether these overrides can be removed.

49. [ ] If Block overrides are unnecessary, remove them. If they're needed, add comments.

50. [ ] Search for other widgets that override both methods and verify the distinction is
        intentional and documented.

51. [ ] Run tests: `cargo nextest run --all --all-features`

---

# Stage 8: Final Cleanup and Validation

52. [ ] Review the final `FocusGym::on_mount` implementation - it should now match the target.

53. [ ] Run full test suite: `cargo nextest run --all --all-features`

54. [ ] Run clippy and fix any warnings: `cargo clippy --all --all-targets --all-features`

55. [ ] Format code: `cargo +nightly fmt --all`

56. [ ] Manually test focusgym example: `cargo run --example focusgym`
        - Verify splits work correctly
        - Verify focus navigation works
        - Verify flex grow/shrink adjustments work

57. [ ] Update ergo.md to mark as complete and document any deviations from the original plan.
