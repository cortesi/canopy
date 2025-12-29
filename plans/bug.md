# Bug Report: Horizontal Scrolling Does Not Reveal Off-Screen Content

## Summary

When scrolling horizontally in the List widget using `h`/`l` or arrow keys, text content that extends
beyond the visible area is never revealed. The text is simply cut off at the view boundary, and
scrolling right does not show the truncated content.

## Reproduction

1. Run `cargo run --example listgym`
2. Observe that TextBlock items have text that extends beyond the visible width (text is cut off)
3. Press `l` or Right arrow to scroll right
4. **Expected**: Text that was cut off should scroll into view from the right
5. **Actual**: Nothing changes - the text remains cut off at the same position

## Technical Investigation

### Architecture Overview

The rendering system uses these coordinate spaces:

1. **Screen coordinates**: Absolute position on terminal (signed `RectI32`)
2. **Canvas coordinates**: Virtual content space for a widget
3. **Outer-local coordinates**: Relative to widget's outer rect origin (0,0)
4. **View**: Window into canvas, with scroll offset (`view.tl`)

### Key Files

- `/crates/canopy/src/widgets/list.rs` - List widget with scroll commands
- `/crates/canopy/src/core/canopy.rs` - `render_traversal()` handles clipping
- `/crates/canopy/src/core/world.rs` - Layout computation, child positioning
- `/crates/canopy/src/core/view.rs` - View struct and coordinate helpers
- `/crates/canopy/src/layout.rs` - Measurement and constraints
- `/crates/examples/src/listgym.rs` - TextBlock widget for testing

### How Layout Works

Child outer rect position is computed in `world.rs:770`:

```rust
let outer_x = parent_view.content.tl.x as i64 + rect.tl.x as i64 - parent_view.tl.x as i64;
```

When parent scrolls right (`parent_view.tl.x` increases), children shift left in screen coordinates.

### How Rendering Works

In `render_traversal()` (`canopy.rs:305-383`):

1. `screen_clip = view.outer.intersect_rect(parent_clip)` - visible portion of node
2. `local_clip = outer_clip_to_local(view.outer, screen_clip)` - convert to local coords
3. Render object created with `local_clip` - widgets can only draw within this clip
4. Children rendered with `children_clip = view.content.intersect_rect(parent_clip)`

### The Problem

When the List widget scrolls:

1. List's `view.tl.x` increases (e.g., scroll right by 5)
2. Child outer rects shift left: `outer_x = content.x + 0 - 5 = content.x - 5`
3. Child's screen_clip is intersection of child.outer with parent_clip
4. If child.outer is at x=-5 and parent_clip starts at x=0, screen_clip starts at x=0
5. local_clip for child becomes `(5, 0, width-5, height)` - starts at local x=5

The child widget renders at local coordinates (0, 1, 2, ...) but the clip starts at x=5,
so the first 5 columns are clipped. The content that SHOULD scroll into view from the right
is never rendered because:

- **The child's outer rect is only as wide as the parent's view**, not as wide as its canvas
- Even when child measures to intrinsic width, **the layout system may not honor it**
- Child content beyond the outer rect width is never drawn

### Attempted Fixes

#### Attempt 1: Scroll Sync

Synced horizontal scroll from List to children:
```rust
fn sync_horizontal_scroll(&self, c: &mut dyn Context) {
    let list_scroll_x = c.view().tl.x;
    for id in &self.items {
        c.with_widget((*id).into(), |_w: &mut W, ctx| {
            ctx.scroll_to(list_scroll_x, child_scroll_y);
            Ok(())
        }).ok();
    }
}
```

**Result**: Failed. Child scroll is clamped by `clamp_scroll_offset()` based on canvas vs content_size.
Also, mixing parent scroll (shifts outer rect) with child scroll (shifts content within outer)
causes double-shifting - content gets clipped AND skipped.

#### Attempt 2: Intrinsic Width Measurement

Changed TextBlock to return intrinsic width, bypassing constraints:
```rust
fn measure(&self, c: MeasureConstraints) -> Measurement {
    let width = self.max_line_width.saturating_add(2);
    Measurement::Fixed(Size::new(width, height))
}
```

**Result**: Failed. Even though measure returns intrinsic width, the layout system may still
assign a smaller outer rect based on available space.

### Root Cause: CONFIRMED

The fundamental issue is that **the layout system clamps children's measured sizes to available space**.

In `world.rs`, `resolve_outer_size_with_layout()`:

1. **Line 1284-1286** - When no explicit `max_width` is set, constraint uses available width:
   ```rust
   let effective_max_outer = match max_outer {
       Some(m) => m.min(available_content.saturating_add(pad_axis)),
       None => available_content.saturating_add(pad_axis),  // <-- Uses parent width!
   };
   ```

2. **Line 849** - The measurement is clamped to this constraint:
   ```rust
   measured_content = c0.clamp_size(raw0);  // Clamps 100 to 40
   ```

3. **Line 854** - The outer width uses the clamped measurement:
   ```rust
   Sizing::Measure => measured_content.width.saturating_add(pad_x),
   ```

**Concrete Example**:
- Parent content width: 40
- Child intrinsic width: 100
- `constraint_for_axis` returns `Constraint::AtMost(40)`
- Child's `measure()` returns `Size::new(100, height)`
- `c0.clamp_size()` clamps width to 40
- Child's outer rect gets width=40
- During render, clip is based on outer rect (40 wide)
- Content beyond column 40 is never rendered

For horizontal scrolling to work, either:

1. **Children's outer rects must be as wide as their canvas** (intrinsic width), OR
2. **The render clip must consider the child's canvas size**, not just outer rect, OR
3. **Children must render their content shifted** based on parent's scroll, using the parent's
   scroll offset to determine which portion of their canvas to draw

### Comparison: Why Vertical Scrolling Works

Vertical scrolling works because:
- Children are at different Y positions in the parent's canvas (0, 10, 25, ...)
- Parent's canvas height = sum of children's heights
- When parent scrolls down, children at higher Y become visible
- Each child renders fully within its outer rect; no content is beyond the outer rect

Horizontal scrolling differs:
- All children are at X=0 in parent's canvas
- Children's content extends horizontally beyond their outer rect
- Need to reveal content that's beyond the child's outer rect width

### Debugging Suggestions

1. **Add debug logging** to `render_traversal()` to print:
   - `parent_clip`
   - `view.outer` for each child
   - `screen_clip` computed
   - `local_clip` computed

2. **Add debug logging** to layout to print:
   - Child measure result
   - Actual outer rect assigned to child

3. **Check if child outer width matches intrinsic width** after layout completes

### Potential Solutions

#### Solution A: Expand Child Outer Rects

Modify layout to give children their full measured width, even if it exceeds parent's content area.
This would require changes to the layout algorithm in `world.rs`.

#### Solution B: Canvas-Based Clipping

Modify `render_traversal()` to compute clip based on child's canvas intersected with visible area,
not just child's outer rect. This would allow rendering content beyond the outer rect.

#### Solution C: Parent-Scroll-Aware Child Rendering

Children could read the parent's scroll offset and shift their render accordingly:
- Parent scroll right by 10
- Child's outer is at x=-10 (shifted left)
- Child's clip starts at local x=10
- Child should render content starting at canvas x=10 at local position x=10

This requires children to know parent's scroll, which breaks encapsulation.

#### Solution D: Virtual Child Positioning

Instead of positioning all children at canvas x=0, position children based on their content.
This doesn't quite make sense for a column layout where all items span the full width.

### Concrete Fix Proposals

#### Fix 1: Skip Measurement Clamping for Scrollable Containers

In `world.rs`, modify `resolve_outer_size_with_layout()` to not clamp when the parent
is a scrollable container:

```rust
// Line 849: Only clamp if parent doesn't scroll horizontally
if parent_scrolls_horizontally {
    measured_content = raw0;  // Use unclamped measurement
} else {
    measured_content = c0.clamp_size(raw0);
}
```

**Pros**: Minimal change, targeted fix
**Cons**: Requires passing parent scroll capability down, could break other layouts

#### Fix 2: Add Unbounded Constraint for Cross-Axis in Scrollable Layouts

In `layout_children_sequential()`, when the parent allows horizontal scrolling,
provide `Unbounded` constraint for width:

```rust
// Line 1121-1122: Use Unbounded for cross-axis if parent scrolls
let child_available = if parent_scrolls_cross_axis {
    Size::from_main_cross(layout.direction, main, u32::MAX)
} else {
    Size::from_main_cross(layout.direction, main, content.cross(layout.direction))
};
```

**Pros**: Clean separation, children measure to intrinsic width
**Cons**: Requires layout to know about scrollability

#### Fix 3: Canvas-Based Clipping in Render

In `render_traversal()`, use the child's canvas size for clipping instead of outer rect:

```rust
// Current:
let Some(screen_clip) = view.outer.intersect_rect(parent_clip) else { ... };

// Proposed: Extend clip to canvas bounds
let canvas_outer = RectI32::new(
    view.outer.tl.x,
    view.outer.tl.y,
    view.canvas.w.max(view.outer.w),
    view.canvas.h.max(view.outer.h),
);
let Some(screen_clip) = canvas_outer.intersect_rect(parent_clip) else { ... };
```

**Pros**: Works without layout changes
**Cons**: Could cause rendering artifacts if widgets don't expect wider clip

#### Fix 4: Widget-Level Scroll Offset Awareness

Widgets in scrollable containers read the parent's scroll offset and render their
content shifted accordingly. The parent passes scroll offset to children's render context.

**Pros**: Full control at widget level
**Cons**: Breaks encapsulation, requires API changes

### Recommended Approach

**Fix 2** seems cleanest: modify `layout_children_sequential()` to provide unbounded
width constraint when the parent's layout allows horizontal scrolling. This would
require adding a flag to `Layout` like `allow_overflow_x: bool`.

When `allow_overflow_x` is true:
1. Children get `Constraint::Unbounded` for width
2. Children measure to intrinsic width
3. Children's outer rects are intrinsic width (not clamped)
4. Parent's canvas is max of children's widths (already works in `List::canvas()`)
5. Render clips children to visible area of their outer rect (already works)

### Current State

After attempted fixes:
- List widget scrolls (view.tl.x changes)
- Children shift left in screen coordinates
- But children's content beyond their outer rect is never visible
- The layout clamps children's width to parent's available width
- The render clip constrains drawing to the (clamped) outer rect bounds
