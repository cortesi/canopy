# API Ergonomics Execution Plan

This plan improves the Canopy widget API based on analysis of `focusgym.rs`. See `./ergo.md` for
full rationale.

## Completed Work

The following major improvements have been implemented:

1. **Context API Refactoring** - Methods now operate on current node by default:
   - `children()` / `children_of(node)`
   - `set_children()` / `set_children_of()`
   - `with_style()` / `with_style_of()`
   - `add_child()` / `add_child_to()`
   - `build()` / `build_node()`
   - Focus: `focus_next()` / `focus_next_global()` / `focus_next_in(subtree)`

2. **Widget::accept_focus Signature** - Now takes `ViewContext` parameter, enabling widgets to
   query tree state (e.g., `ctx.children().is_empty()`) when deciding focus.

3. **Block Widget Simplification** - Removed `has_children`, `init_flex`, `sync_layout`. Block now
   uses `configure_style` for defaults and queries children dynamically.

**Current FocusGym::on_mount:**
```rust
fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
    c.build().flex_col();
    let root_block = c.add_child(Block::new(true))?;
    c.add_child_to(root_block, Block::new(false))?;
    c.add_child_to(root_block, Block::new(false))?;
    Ok(())
}
```

---

# Remaining Work

## Stage 1: Clarify view_size vs canvas_size ✓

The Widget trait has two sizing methods. Renamed `measure` → `view_size` for clarity.

| Method | Purpose | Example (100-item list in 10-row view) |
|--------|---------|----------------------------------------|
| `view_size` | Size requested for visible area | Returns 10 (viewport allocation) |
| `canvas_size` | Total scrollable content size | Returns 100 (total items) |

1. [x] Renamed `measure` → `view_size` and improved doc comments in `crates/canopy/src/widget/mod.rs`:
       - `view_size`: "Returns the size this widget requests for its view (visible area)."
       - `canvas_size`: "Returns the total canvas size (scrollable content area)."

2. [x] Review `Block` in focusgym.rs - it overrides both `view_size` and `canvas_size`. Block doesn't
       scroll, so analyze whether these overrides can be removed or need clarifying comments.
       **Result**: Both overrides are intentional and now documented:
       - `view_size` returns (0, 0) because Block relies entirely on flex layout
       - `canvas_size` returns available space because it's needed for viewport calculations

3. [x] Search for other widgets that override both methods and verify the distinction is
       intentional and documented.
       **Result**: Only `List` overrides `canvas_size` (for scrolling). Other widgets like
       `Editor`, `Input`, `Text` only override `view_size`. The distinction is now documented.

4. [x] Run tests: `cargo nextest run --all --all-features`

---

## Stage 2: Builder add_widget Method (Optional)

Enable fluent widget creation+attachment via the builder pattern:
```rust
c.build().flex_row().add_widget(Child::new()).add_widget(Child::new());
```

**Assessment:** With `add_child()` and `add_child_to()` now available, this is less critical. The
current pattern is already quite clean. Consider whether the added API surface is worth the
marginal ergonomic benefit.

5. [ ] Evaluate whether this is still needed given current APIs. If yes:
       - Extend `BuildContext` trait with `add_orphan`
       - Add `add_widget` method to `NodeBuilder`
       - Update focusgym to demonstrate the pattern

6. [ ] Run tests if changes made

---

## Stage 3: Final Validation

7. [ ] Manually test focusgym example: `cargo run --example focusgym`
       - Verify splits work correctly
       - Verify focus navigation works
       - Verify flex grow/shrink adjustments work
       - Verify delete works

8. [ ] Update ergo.md to mark as complete and summarize the final API patterns.
