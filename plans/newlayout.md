# TUI-Native Layout System

This proposal replaces Taffy with a purpose-built layout system designed for terminal UIs.

## Motivation

The current Taffy-based layout has accumulated complexity from trying to fit a general-purpose CSS
flexbox engine to TUI needs:

- **Float conversions everywhere.** Taffy uses `f32`; terminals use integer cells. We convert back
  and forth constantly.
- **Overlapping concepts.** `Layout::width()` vs `measure()` returning `Extent::Fixed` — two ways
  to say the same thing, unclear which to use.
- **Bolted-on scrolling.** Taffy has `content_size` but we can't easily use it; we added our own
  `canvas_size` that duplicates the concept.
- **Unused complexity.** We don't need flex-wrap, baseline alignment, margin collapsing, or most
  CSS edge cases.

A TUI layout system can be much simpler because:

- All dimensions are integers (character cells)
- Layout modes are limited: stack vertically or horizontally; size is measured or flexed
- No sub-pixel rendering or complex alignment
- Scrolling is a first-class concern, not an afterthought

## Core Types

### Direction

```rust
enum Direction {
    /// Stack children vertically (column).
    Column,
    /// Stack children horizontally (row).
    Row,
}
```

### Sizing

```rust
enum Sizing {
    /// Size from measurement; the widget decides.
    Measure,
    /// Proportional share of remaining space. Weight is clamped to at least 1.
    Flex(u32),
}
```

### Layout

```rust
struct Layout {
    /// Stack direction for children.
    direction: Direction,
    /// Width sizing strategy.
    width: Sizing,
    /// Height sizing strategy.
    height: Sizing,
    /// Minimum width constraint.
    min_width: Option<u32>,
    /// Maximum width constraint.
    max_width: Option<u32>,
    /// Minimum height constraint.
    min_height: Option<u32>,
    /// Maximum height constraint.
    max_height: Option<u32>,
    /// Padding inside the widget.
    padding: Edges<u32>,
    /// Gap between children.
    gap: u32,
}
```

Builder API for ergonomics:

```rust
impl Layout {
    // Constructors
    fn column() -> Self { /* direction: Column, Measure both axes */ }
    fn row() -> Self { /* direction: Row, Measure both axes */ }
    fn fill() -> Self { /* direction: Column, Flex(1) both axes */ }

    // Builders
    fn flex_horizontal(self, weight: u32) -> Self { ... }
    fn flex_vertical(self, weight: u32) -> Self { ... }
    fn min_width(self, n: u32) -> Self { ... }
    fn padding(self, edges: Edges<u32>) -> Self { ... }
    fn gap(self, n: u32) -> Self { ... }
    // etc.
}
```

Use `fill()` for single-child or childless containers where direction is irrelevant and you just
want to fill the parent. For containers with multiple children, use `row()` or `column()` with
explicit flex settings since direction determines how children stack.

### Measurement Types

```rust
enum Constraint {
    /// No constraint on this axis.
    Unbounded,
    /// The layout engine guarantees at most n cells on this axis.
    AtMost(u32),
    /// The layout engine guarantees exactly n cells on this axis.
    Exact(u32),
}

struct MeasureConstraints {
    width: Constraint,
    height: Constraint,
}

/// Result of measuring a widget.
enum Measurement {
    /// Fixed size (for leaf widgets that know their intrinsic size).
    Fixed(Size<u32>),
    /// Wrap children (for containers; engine computes size from children).
    Wrap,
}

impl MeasureConstraints {
    /// Leaf widgets: return a fixed size, clamped to constraints.
    /// Always use this rather than constructing Measurement::Fixed directly.
    fn clamp(&self, size: Size<u32>) -> Measurement {
        Measurement::Fixed(self.clamp_size(size))
    }

    /// Containers: wrap children. Engine computes size and adds padding from Layout.
    fn wrap(&self) -> Measurement {
        Measurement::Wrap
    }

    /// Internal: clamp a size to constraints without wrapping in Measurement.
    fn clamp_size(&self, size: Size<u32>) -> Size<u32> { ... }
}
```

**Constraint handling:** Widgets compute their natural size, then `clamp()` enforces constraints.
The layout algorithm also clamps any measurement result (including `Wrap`) to the passed
constraints as a safety net. This is silent and intentional—not an error condition.

**Unbounded constraints:** When a constraint is `Unbounded`, the widget returns its intrinsic
size—the natural dimensions of its content without external limits. This occurs at the root of the
tree or inside containers using `Sizing::Measure`. Widgets with content always know their natural
size; just return it.

For `clamp_size()`: `Unbounded` leaves the axis unchanged. `AtMost(n)` reduces values greater than
`n`. `Exact(n)` forces the axis to `n`. Zero is valid and preserved unless `Exact` overrides it.

### CanvasContext

`CanvasContext` provides access to child layout results during `canvas()` so composite scrollers
can compute a scrollable extent without re-measuring children.

```rust
struct CanvasContext<'a> {
    /// Child layout results in this node's content coordinate space.
    children: &'a [CanvasChild],
}

struct CanvasChild {
    /// Child view rect relative to this node's content origin.
    rect: Rect<u32>,
    /// Child canvas size (may exceed rect.size()).
    canvas: Size<u32>,
}

impl CanvasContext<'_> {
    fn new(node: &Node) -> CanvasContext<'_> { ... }
    fn children(&self) -> &[CanvasChild] { self.children }
    fn children_extent(&self) -> Size<u32> { ... }
}
```

`children_extent()` computes `max(rect.origin + rect.size())` across all children—the furthest
bottom-right corner of any child's view rect. This uses `rect`, not `canvas`: a child's internal
scrollable content is its own concern. For leaf widgets, ignore the context.

**Ordering guarantee:** The layout algorithm calls `canvas()` on children before calling it on the
parent, so `CanvasChild.canvas` is always populated when the parent's `canvas()` runs.

## Widget Trait

```rust
trait Widget {
    /// Layout configuration. Called when the node is created and when layout is explicitly
    /// invalidated. The layout pass uses the cached Layout; it does not call layout() each frame.
    fn layout(&self) -> Layout {
        Layout::column()
    }

    /// Measurement result under the provided constraints. Only called if any axis is
    /// Sizing::Measure. Leaf widgets return Fixed size; containers return Wrap.
    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.wrap()  // Default: container behavior
    }

    /// Canvas size for scrolling. Called after layout with final view dimensions.
    /// Return view unchanged for non-scrolling widgets.
    /// CanvasContext provides child layout results for composite scrollers.
    ///
    /// The view size passed here usually matches what `measure()` saw (when it ran):
    /// - Flex axes are passed to `measure()` as `Exact(available)`.
    /// - Measure axes use the measured size unless later clamped by min/max.
    ///
    /// If min/max clamping changed the size, view may differ from what measure() saw.
    /// This is correct—recompute for the actual view size. Widgets can cache expensive
    /// computations (e.g., text wrapping) if needed.
    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        view
    }

    /// Render this widget's content. Does NOT render children—canopy handles that.
    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }
}
```

### Relationship Between Methods

| Method | When Called | Purpose |
|--------|-------------|---------|
| `layout()` | Node creation, explicit update | Configure sizing strategy |
| `measure()` | Layout: any axis `Sizing::Measure` | Report size under constraints |
| `canvas()` | After layout complete | Report scrollable content size (child layout available) |
| `render()` | During render pass | Draw this widget only (not children) |

The key insight: `layout()` says *who* sizes each axis. If the widget owns sizing on an axis, the
layout pass calls `measure()` to obtain the actual size.

### Layout Configuration Lifecycle

`Layout` is cached on the node. The layout pass reads the cached value rather than calling
`layout()` each frame. When a widget's layout configuration changes, it should explicitly mark
its layout as dirty so canopy re-calls `layout()` before the next pass.

### Measurement Contract

`measure()` returns a `Measurement` indicating the widget's sizing intent:

- **`Measurement::Fixed(size)`** — Leaf widgets that know their intrinsic size. The size is
  *without padding*; the layout engine adds padding from `Layout` when computing final dimensions.
  Use `constraints.clamp(size)` to produce this. Also use `Fixed(Size::ZERO)` for intentionally
  empty containers.

- **`Measurement::Wrap`** — Containers whose size derives from children. The layout engine
  computes size by measuring children, summing main-axis sizes (plus gaps), taking max cross-axis
  size, and adding padding from `Layout`. Use `constraints.wrap()` to produce this. If the widget
  has no children, it resolves to the size of the padding.

`Constraint` values are explicit and force widgets to acknowledge whether each axis is bounded,
unbounded, or exact.

Constraints passed to `measure()` are derived from available space and `min_*` / `max_*` bounds.
**Available space is an implicit upper bound on resolved size**—measurements may compute larger
intrinsic sizes (especially for `Wrap`), but the layout pass clamps results to constraints before
applying min/max. This ensures ContentFrames and dialogs naturally clip to the viewport without
explicit max constraints. Widgets needing to exceed available space (for scrolling) use `canvas()`.

The layout engine caches measurement results per pass and will call `measure()` at most three times:
once initially, optionally again if width is clamped (to reflow content like text wrapping), and
optionally a third time if height is then clamped. This re-measurement ensures widgets always see
their actual final size—if `min_width` expands a widget beyond its initial measurement, it gets
re-measured with `Exact(final_width)` so it can reflow content correctly.

## Unified Positioning and Rendering

A fundamental design principle: **position and size are determined in ONE place—during layout.**
Render only renders the widget itself; it never computes positions or manages children.

### What Layout Determines

After layout completes, each node stores:

- **Position** relative to parent's content area
- **Size** (width and height in cells)
- **Canvas size** for scrolling (if different from view size)
- **Viewport offset** for scroll position

These values are computed once during layout and cached in the node. Render reads them; it never
computes them.

### Scrolling Ownership and Clamping

Scroll offset is state, not layout. Input or widget state changes it; layout only clamps it to the
current canvas bounds when calling `node.set_viewport(size, canvas)`. Widgets read the offset via
`ViewContext` (for example `view.tl`) and should never mutate scroll state during `render()`.

### Layout Invalidation

Layout invalidation is explicit and deferred. Widgets may mark themselves dirty from any phase,
including `render()`, but invalidation only schedules the *next* layout pass.

```rust
trait ViewContext {
    /// Mark this node dirty. Next frame will re-run layout and render.
    fn taint(&self);
}
```

One method. If anything changes—content, sizing strategy, visual state—call `taint()`. The
system clears caches and recalculates layout for the tainted subtree on the next frame.

For TUI apps, layout and rendering are both cheap (simple integer math, character buffer writes).
The complexity of distinguishing "visual only" vs "needs layout" isn't worth the API overhead.

If `taint()` is called during a render pass, canopy completes the current frame, then schedules a
new layout and render pass.

### Render Semantics

The `render` method has simple semantics:

1. **Render only yourself.** Draw borders, backgrounds, text, or whatever this widget displays.
   Don't call render on children—canopy does that.

2. **Use the provided context.** The `ViewContext` gives you your allocated area. Draw within it.

3. **No child management.** You don't position children, clip them, or decide render order. That's
   canopy's job.

Container widgets like `Frame` just draw their border. They don't recurse into children.

### Canopy's Render Loop

Canopy walks the tree and handles everything:

```
render_tree(node):
    // Skip invisible nodes
    if not node.is_visible():
        return

    // Set up clipping for this node's area
    push_clip(node.rect())

    // Render the widget itself
    node.widget.render(render_context, view_context)

    // Recurse into children in order
    for child in node.children():
        render_tree(child)

    pop_clip()
```

Key guarantees:

- **Clipping is automatic.** Children cannot draw outside their parent's content area. Canopy
  enforces this via clip regions.
- **Render order = child order.** Children are rendered in the order returned by the parent. No
  separate z-index or post_render phase needed.
- **Visibility is pre-computed.** Layout marks nodes that are entirely off-screen; render skips
  them.

### Why This Matters

The current system has render methods that:

- Compute child positions during render
- Manually clip children
- Decide whether to render children based on visibility
- Handle scroll offsets during render

This is duplicated work (layout already computed positions) and error-prone (easy to get clipping
wrong). The unified model means:

- Layout is the single source of truth for geometry
- Render is purely about drawing
- Container widgets become trivial—just draw your chrome
- Scrolling is handled uniformly by canopy

## Layout Algorithm

The pseudocode below assumes `layout_cached()` returns the cached `Layout` and
`measure_cached()` returns a per-pass cached measurement result for the given constraints.

```
layout_node(node, available: Size<u32>) -> Size<u32>:
    config = node.layout_cached()

    // 1. Compute this node's size (measurements are cached per pass)
    size = resolve_size(config, available, node)

    // 2. Compute content area (subtract padding, saturating at 0)
    content_area = size.saturating_sub(config.padding)

    // 3. Layout children within content area
    layout_children(node.children, config.direction, content_area, config.gap)

    // 4. Compute canvas for scrolling and clamp scroll offset
    canvas_ctx = CanvasContext::new(node)
    canvas = node.canvas(size, canvas_ctx)
    node.set_viewport(size, canvas)

    return size

resolve_size(config, available, node) -> Size<u32>:
    constraints = MeasureConstraints {
        width: constraint_for_axis(config.width, available.width,
                                   config.min_width, config.max_width),
        height: constraint_for_axis(config.height, available.height,
                                    config.min_height, config.max_height),
    }

    should_measure = config.width == Measure or config.height == Measure
    measured = if should_measure:
        measurement = node.measure_cached(constraints)
        raw = match measurement:
            Measurement::Fixed(size) => size
            Measurement::Wrap => {
                measure_children_extent(node, config, constraints)
            }
        constraints.clamp_size(raw)
    else:
        Size { width: 0, height: 0 }

    width = match config.width:
        Measure => measured.width
        Flex(_) => available.width  // Parent allocates flex space

    height = match config.height:
        Measure => measured.height
        Flex(_) => available.height

    // Apply min/max constraints
    clamped_width = clamp(width, config.min_width, config.max_width)
    clamped_height = clamp(height, config.min_height, config.max_height)

    // If clamped on a Measure axis, re-measure once with the clamped size.
    // This allows content to reflow (e.g., text wrapping to a narrower width).
    if config.width == Measure and clamped_width != width:
        new_constraints = MeasureConstraints {
            width: Exact(clamped_width),
            height: constraints.height,
        }
        measurement = node.measure_cached(new_constraints)
        remeasured = match measurement:
            Measurement::Fixed(size) => size
            Measurement::Wrap => measure_children_extent(node, config, new_constraints)
        remeasured = new_constraints.clamp_size(remeasured)
        clamped_height = clamp(remeasured.height, config.min_height, config.max_height)

    if config.height == Measure and clamped_height != height:
        new_constraints = MeasureConstraints {
            width: Exact(clamped_width),
            height: Exact(clamped_height),
        }
        // Re-measure for height reflow
        node.measure_cached(new_constraints)

    return Size { width: clamped_width, height: clamped_height }

measure_children_extent(node, config, constraints) -> Size<u32>:
    direction = config.direction
    gap = config.gap
    children = node.children

    main_total = 0
    cross_max = 0

    for child in children:
        child_config = child.layout_cached()
        child_main_sizing = if direction == Column {
            child_config.height
        } else {
            child_config.width
        }
        child_cross_sizing = if direction == Column {
            child_config.width
        } else {
            child_config.height
        }

        // Determine if we need to measure this child.
        // - Measure on main axis: contributes to main_total
        // - Measure on cross axis: contributes to cross_max
        // - Flex on both axes: no measurement needed (fills available space)
        needs_measure = child_main_sizing == Measure or child_cross_sizing == Measure

        if not needs_measure:
            continue  // Flex on both axes; child fills available space during layout

        // Pass constraint on cross axis, unbounded on main (we're measuring)
        child_constraints = if direction == Column:
            MeasureConstraints { width: constraints.width, height: Unbounded }
        else:
            MeasureConstraints { width: Unbounded, height: constraints.height }

        measurement = child.measure_cached(child_constraints)
        child_size = match measurement:
            Measurement::Fixed(size) => size
            Measurement::Wrap => measure_children_extent(child, child_config, child_constraints)
        child_size = child_constraints.clamp_size(child_size)

        // Only Measure children contribute to main axis total; Flex children get remaining space
        if child_main_sizing == Measure:
            main_total += child_size.main(direction)

        // All measured children contribute to cross axis max
        cross_max = max(cross_max, child_size.cross(direction))

    // Add gaps between children
    if children.len() > 1:
        main_total += gap * (children.len() - 1)

    // Add padding from Layout
    main_total += config.padding.main(direction)
    cross_max += config.padding.cross(direction)

    // Return as Size, respecting direction
    Size::from_main_cross(direction, main_total, cross_max)

constraint_for_axis(sizing, available, min, max) -> Constraint:
    match sizing:
        Flex(_) => Exact(available)
        Measure => {
            // Available space is an implicit upper bound for this node's measurement. Wrap may
            // compute a larger intrinsic size, but resolve_size clamps it.
            effective_max = match max:
                Some(m) => min(m, available)
                None => available
            match (min, effective_max):
                _ if min == Some(effective_max) => Exact(effective_max)
                _ => AtMost(effective_max)
        }

layout_children(children, direction, available, gap):
    if children.is_empty():
        return

    // First pass: resolve Measure children and sum their main sizes
    measure_total = 0
    flex_total_weight = 0
    pre_sizes = Map<Child, Size<u32>>()

    for child in children:
        config = child.layout_cached()
        main_sizing = if direction == Column { config.height } else { config.width }

        // Cross-axis space is always the parent's cross dimension; flex fills it, measure may
        // choose a smaller size.
        cross_available = available.cross(direction)

        match main_sizing:
            Flex(weight) => flex_total_weight += max(weight, 1)
            _ => {
                child_available = Size::from_main_cross(direction,
                    available.main(direction), cross_available)
                size = resolve_size(config, child_available, child)
                pre_sizes[child] = size
                measure_total += size.main(direction)
            }

    // Account for gaps
    total_gap = gap * children.len().saturating_sub(1)
    remaining = available.main(direction).saturating_sub(measure_total + total_gap)

    // Second pass: allocate space and position
    position = 0
    remainder = if flex_total_weight > 0 { remaining % flex_total_weight } else { 0 }

    for child in children:
        config = child.layout_cached()
        main_sizing = if direction == Column { config.height } else { config.width }

        child_main = match main_sizing:
            Flex(weight) => {
                w = max(weight, 1)
                share = (remaining as u64 * w as u64 / flex_total_weight as u64) as u32
                if remainder > 0 { share += 1; remainder -= 1 }
                share
            }
            _ => pre_sizes[child].main(direction)
        }

        // Cross-axis: Flex fills available, Measure uses measured size
        cross_available = available.cross(direction)
        child_available = Size::from_main_cross(direction, child_main, cross_available)

        child.set_position(position)
        // Advance by the actual size so min constraints don't cause overlaps.
        actual = layout_node(child, child_available)

        position += actual.main(direction) + gap
```

## Examples

### Fixed 5x5 Widget

```rust
fn layout(&self) -> Layout {
    Layout::column()
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    c.clamp(Size { width: 5, height: 5 })
}
```

### Vertical Divider

```rust
fn layout(&self) -> Layout {
    Layout::column()
        .flex_vertical(1)
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    c.clamp(Size { width: 1, height: 1 })
}
```

### Horizontal Divider

```rust
fn layout(&self) -> Layout {
    Layout::row()
        .flex_horizontal(1)
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    c.clamp(Size { width: 1, height: 1 })
}
```

### Text Widget with Wrapping

```rust
fn layout(&self) -> Layout {
    Layout::column()
        .flex_horizontal(1)
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    let width = match c.width {
        Constraint::Exact(n) => n,
        Constraint::AtMost(n) => std::cmp::min(n, self.text.len() as u32),
        Constraint::Unbounded => self.text.len() as u32,
    };
    let lines = self.wrap(width).len() as u32;
    c.clamp(Size { width, height: lines })
}
```

### Two-Pane Split (Equal)

```rust
// Parent
fn layout(&self) -> Layout {
    Layout::row().gap(1)
}

// Each child pane
fn layout(&self) -> Layout {
    Layout::fill()
}
```

### Two-Pane Split (30/70)

```rust
// Left pane: weight 3
fn layout(&self) -> Layout {
    Layout::column().flex_horizontal(3)
}

// Right pane: weight 7
fn layout(&self) -> Layout {
    Layout::column().flex_horizontal(7)
}
```

### Image Viewer with Scrolling

```rust
fn layout(&self) -> Layout {
    Layout::fill()
}

fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
    Size {
        width: self.image.width(),
        height: self.image.height(),
    }
}
```

### Text Field with Max Height

```rust
fn layout(&self) -> Layout {
    Layout::column()
        .flex_horizontal(1)
        .max_height(5)
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    let width = match c.width {
        Constraint::Exact(n) => n,
        Constraint::AtMost(n) => std::cmp::min(n, self.text.len() as u32),
        Constraint::Unbounded => self.text.len() as u32,
    };
    let lines = self.wrap(width).len() as u32;
    c.clamp(Size { width, height: lines })  // max_height clamps this
}

fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
    let lines = self.wrap(view.width).len() as u32;
    Size { width: view.width, height: lines }
}
```

### Frame (Constraining Container)

A frame that fills available space and constrains children to fit within it. Typical use: a panel
or window where content must fit within the allocated area.

```rust
fn layout(&self) -> Layout {
    Layout::fill().padding(Edges::all(1))  // Border takes 1 cell on each side
}

// Uses default measure() -> c.wrap(), but since both axes are Flex,
// measure() is never called. Size comes from parent.

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.border("frame", ctx.view().rect(), BorderStyle::Single)?;
    Ok(())
}
```

Children receive the frame's content area (after padding) as their available space.

### ContentFrame (Wrapping Container)

A frame that takes its size from children, but won't exceed available space. Typical use: a dialog,
tooltip, or inline bordered region that shrinks to fit its content.

```rust
fn layout(&self) -> Layout {
    Layout::column().padding(Edges::all(1)).gap(1)  // Measure on both axes
}

// Uses default measure() -> c.wrap()
// Engine computes size from children, adds padding for border
// Size is clamped to available space (parent's content area)

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.border("content-frame", ctx.view().rect(), BorderStyle::Single)?;
    Ok(())
}
```

The frame's size = min(children extent + gaps + padding, available space).

If content exceeds available space, the frame clips to available and content can scroll via
`canvas()`. To center a ContentFrame in the viewport, wrap it in a centering container.

### Key Difference

| Type | Sizing | Size Source | Use Case |
|------|--------|-------------|----------|
| Frame | `Flex` on both axes | Parent allocation | Panels, windows, fixed regions |
| ContentFrame | `Measure` on both axes | Children + padding | Dialogs, tooltips, inline boxes |

Both draw a border and let canopy handle children—the only difference is sizing strategy.

## Migration

This is a flag-day replacement with no backward compatibility. All widgets must be updated to the
new API in one pass. Given the small codebase (~6 widget implementations plus examples), this is
tractable.

### Changes Required

1. Remove Taffy dependency
2. Implement new layout algorithm (~200 lines)
3. Replace `Layout` struct with new version
4. Rename `view_size` → `measure`, update signature
5. Rename `canvas_size` → `canvas`, update signature (add `CanvasContext`)
6. Simplify `render` signature—remove `area` parameter (use `ctx.view()` instead)
7. Remove child rendering from container widgets—canopy handles it
8. Update all Widget implementations
9. Implement canopy's render loop with automatic clipping
10. Update viewport/scrolling to use new layout output

### What Stays the Same

- Widget trait structure (renamed methods, simplified signatures)
- Event handling
- Viewport/scrolling concepts (canvas vs view)
- Node tree structure

### What Gets Simpler

- Container widgets no longer manage child rendering
- No manual clipping in widget code
- No position computation during render
- Render methods are shorter and more focused

## Design Decisions

### Truncation on Overflow

When measured children (after min/max) plus gaps exceed available space, layout does not shrink
them; overflow is clipped to the parent's content area. If content doesn't fit, scrolling
(canvas > view) handles it.

**Min constraints are guarantees.** If you set `min_width: Some(20)` on a widget, it will be at
least 20 cells wide, even if that causes the parent to overflow. This is intentional—min
constraints express hard requirements, not preferences. If sibling min sizes sum to more than
available space, the parent clips. There is no shrink/priority model; such a system would add
complexity and undermine the "simple flex" goal.

**Design rule:** If overflow is unacceptable, ensure your min constraints are achievable. If they
conflict, fix the constraints or restructure the layout.

### No Absolute Positioning

Absolute positioning (for modals, tooltips, etc.) is not included. TUI modals can be handled by
restructuring the tree—a modal becomes a sibling that renders on top of content. This avoids the
complexity of a separate positioning mode.

### No Z-Ordering

Render order equals child order. There's no z-index or post_render phase. If ordering matters,
structure the tree accordingly. This keeps the mental model simple: what you see in the tree is
what you get on screen.

### Integer Flex Weights

Flex weights use `u32`, not floats. Ratios like 1:2:1 or 3:7 cover all practical TUI cases.
Fractional weights would add complexity without clear benefit.

### Flex Allocation and Remainders

Flex weights are clamped to at least 1. Remaining cells are divided by weight, and any remainder
is distributed to earlier flex children so the sum of allocations equals the available space. If
measure children (plus gaps) exceed available space, the remaining space is 0; flex children
collapse to zero and overflow is handled by clipping. If a flex child expands beyond its share
due to min constraints, later siblings shift and the overflow is clipped.

### Children Control Both Axes

Children specify sizing for both axes by choosing whether to call `flex_horizontal()` or
`flex_vertical()`. A child in a Row can leave height as `Sizing::Measure` and return `height: 5`
from measurement instead of filling the row's height. The layout algorithm respects the child's
cross-axis sizing just like the main axis—`Measure` uses the widget's measured size, `Flex` fills
available space.

Flex only truly "fills" once the parent has a resolved size for that axis. When a parent is
`Sizing::Measure` on an axis, children with `Flex` on that axis are treated like `Measure` during
the parent's measurement pass; the parent resolves its size from measured children, then lays them
out using that resolved size.

Unlike CSS Flexbox (which defaults to `align-items: stretch`), there is no implicit stretching.
If a widget wants to fill the cross axis, it must explicitly use `Flex` on that axis. This keeps
the mental model simple: each widget controls its own size.

**Cross-axis positioning:** Children are positioned at the start of the cross axis (top for rows,
left for columns). For centering or other alignment, use spacer widgets.

### Centering and Alignment

There is no `align` or `justify` property. Alignment is achieved with spacer widgets:

```rust
// Center a dialog horizontally in a row
// [Spacer(Flex 1)] [Dialog(Measure)] [Spacer(Flex 1)]

// Spacer widget: invisible, fills available space
fn layout(&self) -> Layout {
    Layout::fill()
}
```

This is more verbose than CSS alignment properties, but explicit. The tree structure shows exactly
what's happening. For common patterns, create helper widgets (e.g., `Center`) that wrap content
with spacers. A standard `Center` widget is provided to handle this common pattern without boilerplate.

### Overlays and Z-Order

Overlays (modals, tooltips, dropdowns) are handled by tree structure, not layout properties:

1. **Render order = child order.** Later children render on top of earlier ones.
2. **Overlays are siblings.** A modal is a sibling of the main content, positioned later in the
   child list so it renders on top.
3. **Transient overlays** (tooltips, popups) can be children of a dedicated overlay container near
   the root, ensuring they render above all other content.

**Strict Clipping:** Because children are strictly clipped to their parent's content area, self-contained popups (like a dropdown menu opening *outside* its button) are not possible in-place. Such widgets must rely on a hosted overlay manager or "portal" pattern where the popup is rendered as a child of the root node.

There is no z-index. If something needs to render on top, structure the tree so it comes later.
This keeps the mental model simple: tree structure = render structure.

### Responsive Layouts

`layout()` has no inputs and is cached, so widgets cannot dynamically change direction or sizing
based on available space. This is intentional—it keeps the core simple and cacheable.

For responsive patterns (e.g., switching from Row to Column when width is narrow):

1. **State-driven reconfiguration.** A widget can track its size in `canvas()` or `render()`. When
   it crosses a threshold, update internal state and call `taint()` to trigger layout
   recalculation. The next `layout()` call returns the new configuration.

2. **Adaptive helper widgets.** Create containers like `AdaptiveStack` that encapsulate responsive
   logic. The container tracks available space and reconfigures its layout strategy accordingly.

This follows the same pattern as centering: keep the core minimal, use composition for advanced
features.

## Worked Examples

### Frame with Scrollable Content

A constraining frame containing content that may overflow. The frame fills available space; content
scrolls if it exceeds the frame's interior.

```rust
// Frame: fills parent, reserves border space
fn layout(&self) -> Layout {
    Layout::fill().padding(Edges::all(1))
}

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.border("frame", ctx.view().rect(), BorderStyle::Single)?;
    Ok(())
}

// Content: fills frame interior, scrolls if canvas exceeds view
fn layout(&self) -> Layout {
    Layout::fill()
}

fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
    // Content may be larger than view
    Size { width: view.width, height: self.content_height() }
}
```

### ContentFrame with Buttons

A wrapping frame that sizes to its content, clamped to available space. Used for dialogs or popups.

```rust
// ContentFrame: shrinks to children with border padding, clips to available
fn layout(&self) -> Layout {
    Layout::column().padding(Edges::all(1)).gap(1)
}

// Uses default measure() -> c.wrap()
// Constraints include available space as upper bound

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.border("dialog", ctx.view().rect(), BorderStyle::Double)?;
    Ok(())
}

// Button row inside: horizontal, wraps its buttons
fn layout(&self) -> Layout {
    Layout::row().gap(1)
}

// Each button: fixed size from label
fn measure(&self, c: MeasureConstraints) -> Measurement {
    c.clamp(Size { width: self.label.len() as u32 + 4, height: 1 })
}
```

Result: dialog sizes to fit title + button row + border, but never exceeds parent's available space.

### Text in Container

Container fills parent, text child wraps to container width with height determined by content.

```rust
// Container: fills parent, uses default measure() which returns Wrap
fn layout(&self) -> Layout {
    Layout::fill()
}

// Text: fills width, height from wrapped line count
fn layout(&self) -> Layout {
    Layout::column()
        .flex_horizontal(1)
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    let width = match c.width {
        Constraint::Exact(n) => n,
        Constraint::AtMost(n) => std::cmp::min(n, self.text.len() as u32),
        Constraint::Unbounded => self.text.len() as u32,
    };
    let lines = self.wrap(width).len() as u32;
    c.clamp(Size { width, height: lines })
}
```

### Scrollable List of Wrapped Text

A scrollable list that stacks text items vertically. Each text item wraps to the list width.

```rust
/*
List
  TextItem("first")
  TextItem("second")
  ...
*/

// List container: fills parent, stacks items vertically.
fn layout(&self) -> Layout {
    Layout::column()
        .flex_horizontal(1)
        .flex_vertical(1)
        .gap(1)
}

fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
    let extent = ctx.children_extent();
    Size {
        width: view.width,
        height: view.height.max(extent.height),
    }
}

// Text item: fills width, height from wrapped line count.
fn layout(&self) -> Layout {
    Layout::column()
        .flex_horizontal(1)
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    let width = match c.width {
        Constraint::Exact(n) => n,
        Constraint::AtMost(n) => std::cmp::min(n, self.text.len() as u32),
        Constraint::Unbounded => self.text.len() as u32,
    };
    let lines = self.wrap(width).len() as u32;
    c.clamp(Size { width, height: lines })
}
```

### Layout Padding vs Visual Chrome

Two ways to add space around content—use the right one:

- **Layout padding:** Space reserved by the layout engine. Children cannot render there. Use for
  structural spacing like borders where the chrome is drawn by the parent but children must not
  overlap it. The engine adds padding to measured size automatically.

- **Visual chrome:** Decorations rendered as part of the widget's content. The widget includes them
  in its measured size. Use for inline decorations like button brackets `[ OK ]` that are part of
  the widget's visual identity.

**Rule of thumb:** If children need to stay out of the space, use `Layout` padding. If it's just
how this widget draws itself, include it in the measured size.

### Button Sized to Label

Button sizes to fit its label plus visual chrome (brackets and spaces). This is *visual* chrome
rendered as part of the content, not `Layout` padding—the button measures its full visual extent.

```rust
fn layout(&self) -> Layout {
    Layout::column()
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    // +4 for "[ " and " ]" visual chrome, not Layout padding
    c.clamp(Size {
        width: self.label.len() as u32 + 4,
        height: 1,
    })
}

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.text("button", ctx.view().line(0), &format!("[ {} ]", self.label))?;
    Ok(())
}
```

### Scrollable Image Viewer

View is fixed size, canvas is the full image. Only visible portion is rendered.

```rust
fn layout(&self) -> Layout {
    Layout::column()
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    c.clamp(Size { width: 40, height: 20 })
}

fn canvas(&self, _view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
    Size { width: 1000, height: 1000 }
}

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    let view = ctx.view();  // 40x20 window into the 1000x1000 canvas

    // view.tl gives scroll offset—only render visible region
    for y in 0..view.h {
        for x in 0..view.w {
            let img_x = view.tl.x + x;
            let img_y = view.tl.y + y;
            let pixel = self.image.get(img_x, img_y);
            r.cell("image", x, y, pixel)?;
        }
    }
    Ok(())
}
```

