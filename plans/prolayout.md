# TUI-Native Layout System (Integer Cell Layout)

This proposal replaces the current layout engine with a purpose-built integer layout system designed for terminal UIs. It provides a small set of primitives (stacking + measured sizing + weighted flex) and makes scrolling a first-class concern.

## Goals

* **All geometry is integer cells** (`u32` sizes and positions).
* **Predictable, deterministic layout**: no rounding, no float drift, stable remainder distribution.
* **Single source of truth for geometry**: positions and sizes are computed during layout, not render.
* **Box model clarity**: outer rect vs content rect; padding is structural.
* **Scrolling is explicit** via `canvas()` and a per-node viewport offset in content space.
* **Fail-soft in tiny terminals**: constraints can be zero; saturating math avoids panics.

## Non-goals

* Full CSS flexbox fidelity (shrink, wrap, baseline, margin collapsing, etc.).
* Absolute positioning, z-index (render order is tree order).
* Implicit cross-axis stretching or implicit alignment properties (alignment can be composed via helper widgets).

---

# Core Types

## Direction

```rust
enum Direction {
    /// Stack children vertically (column).
    Column,
    /// Stack children horizontally (row).
    Row,
}
```

## Display

Optional extension (high ergonomics; low complexity):

```rust
enum Display {
    /// Node participates in layout and rendering.
    Block,
    /// Node is removed from layout and not rendered.
    None,
}
```

`Display::None` is treated as if the node does not exist for:

* gap counting
* flex weight sums
* measuring and layout
* CanvasContext children lists

## Sizing

```rust
enum Sizing {
    /// Size derives from `measure()` / wrapping children.
    Measure,
    /// Weighted share of remaining space along the axis. Weight is clamped to at least 1.
    Flex(u32),
}
```

Notes:

* There is intentionally no `Fixed(n)` sizing. Fixed behavior is expressed via `min_* == max_* == n` (see builders).

## Layout

```rust
struct Layout {
    /// Whether this node participates in layout/render.
    display: Display,

    /// Stack direction for children.
    direction: Direction,

    /// Width sizing strategy (outer size).
    width: Sizing,
    /// Height sizing strategy (outer size).
    height: Sizing,

    /// Minimum outer width constraint (cells).
    min_width: Option<u32>,
    /// Maximum outer width constraint (cells).
    max_width: Option<u32>,

    /// Minimum outer height constraint (cells).
    min_height: Option<u32>,
    /// Maximum outer height constraint (cells).
    max_height: Option<u32>,

    /// Structural padding inside the widget (cells).
    /// Children are laid out inside the content box (outer minus padding).
    padding: Edges<u32>,

    /// Gap between children along the main axis (cells).
    gap: u32,
}
```

### Builders

```rust
impl Layout {
    fn column() -> Self; // display: Block; direction: Column; Measure both axes
    fn row() -> Self;    // display: Block; direction: Row;    Measure both axes
    fn fill() -> Self;   // display: Block; direction: Column; Flex(1) both axes

    fn none(self) -> Self;                    // display: None
    fn flex_horizontal(self, weight: u32) -> Self; // width = Flex(max(weight,1))
    fn flex_vertical(self, weight: u32) -> Self;   // height = Flex(max(weight,1))

    fn min_width(self, n: u32) -> Self;
    fn max_width(self, n: u32) -> Self;
    fn min_height(self, n: u32) -> Self;
    fn max_height(self, n: u32) -> Self;

    /// Convenience: fixed outer dimension without a `Fixed` enum.
    fn fixed_width(self, n: u32) -> Self { self.min_width(n).max_width(n) }
    fn fixed_height(self, n: u32) -> Self { self.min_height(n).max_height(n) }

    fn padding(self, edges: Edges<u32>) -> Self;
    fn gap(self, n: u32) -> Self;
}
```

---

# Box Model and Coordinate Spaces

Every node has two rectangles:

* **Outer (view) rect**: the node’s allocated rectangle in its parent’s *content coordinate space*. This is the area the widget may paint into (subject to ancestor clipping).
* **Content rect**: the inset area inside the outer rect after subtracting `padding` (saturating at 0). Children are laid out and clipped to this rect.

Terminology:

* `outer_size`: size of the node’s outer rect (cells).
* `content_size`: size of the node’s content rect (cells), computed as `outer_size.saturating_sub(padding)`.

### Padding semantics

* Padding affects **child layout** and **child clipping**.
* The widget itself may still render into its padding area (e.g., backgrounds, borders).
* In tiny terminals, `content_size` may be `0` even if padding is non-zero. This is not an error.

---

# Measurement Types

## Constraint (content-box units)

Constraints are expressed in **content-box cells** (excluding padding).

```rust
enum Constraint {
    /// No constraint on this axis.
    Unbounded,
    /// The engine guarantees at most n cells on this axis.
    AtMost(u32),
    /// The engine guarantees exactly n cells on this axis.
    Exact(u32),
}

struct MeasureConstraints {
    width: Constraint,
    height: Constraint,
}

/// Result of measuring a widget's content box.
enum Measurement {
    /// Fixed content size for leaf widgets.
    Fixed(Size<u32>),
    /// Wrap children: engine computes content size from children.
    Wrap,
}
```

### Clamp helpers

```rust
impl MeasureConstraints {
    /// Leaf widgets: clamp a content size to these constraints and return Fixed.
    fn clamp(&self, content: Size<u32>) -> Measurement {
        Measurement::Fixed(self.clamp_size(content))
    }

    /// Containers: request wrapping.
    fn wrap(&self) -> Measurement {
        Measurement::Wrap
    }

    fn clamp_size(&self, content: Size<u32>) -> Size<u32> { ... }
}
```

Clamp rules:

* `Unbounded`: unchanged.
* `AtMost(n)`: `min(value, n)`.
* `Exact(n)`: forced to `n`.
* `0` is valid and must be handled.

---

# Widget Trait

```rust
trait Widget {
    /// Layout configuration. Called on node creation and whenever layout is tainted.
    fn layout(&self) -> Layout { Layout::column() }

    /// Measure intrinsic content size (content box, excludes Layout padding).
    /// Only called if either axis is Sizing::Measure OR if the engine needs wrap sizing.
    fn measure(&self, c: MeasureConstraints) -> Measurement { c.wrap() }

    /// Canvas size in content coordinates (for scrolling).
    /// Called after children are laid out; child layout and child canvas are available.
    ///
    /// `view` here is this node's content_size (outer minus padding).
    /// Return at least view (engine enforces canvas >= view).
    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> { view }

    /// Render this widget's own content. Does not render children.
    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> { Ok(()) }
}
```

---

# CanvasContext

Provides access to children’s layout results during `canvas()` so composite scrollers can compute a scrollable extent without remeasuring.

```rust
struct CanvasContext<'a> {
    /// Child layout results in this node's *content* coordinate space.
    children: &'a [CanvasChild],
}

struct CanvasChild {
    /// Child outer rect relative to this node's content origin.
    rect: Rect<u32>,
    /// Child canvas size in the child's content coordinates.
    canvas: Size<u32>,
}

impl CanvasContext<'_> {
    fn children(&self) -> &[CanvasChild] { self.children }

    /// Extent of children outer rects: max(rect.origin + rect.size).
    fn children_extent(&self) -> Size<u32> { ... }
}
```

Design rule:

* `children_extent()` uses `rect`, not `canvas`. A child’s internal scrollability is the child’s responsibility.
* If you need “scroll through child canvases” as a higher-level behavior, add a dedicated widget that explicitly composes that (not implicit in the base engine).

Ordering guarantee:

* The engine lays out children and computes their canvas before invoking the parent’s `canvas()`.

---

# View and ViewContext

The render phase needs both outer and content information, plus scroll offset.

A minimal `View` shape (exact fields can differ):

```rust
struct View {
    /// Outer rect in screen coordinates (signed for scroll translations).
    outer: Rect<i32>,
    /// Content rect in screen coordinates (outer inset by padding).
    content: Rect<i32>,
    /// Viewport offset in content coordinates (scroll position).
    /// Always clamped so that (offset + view) is within canvas.
    tl: Point<u32>,
    /// Canvas size in content coordinates.
    canvas: Size<u32>,
}
```

```rust
trait ViewContext {
    fn view(&self) -> &View;

    /// Mark this node dirty; next frame will re-run layout and render for the tainted subtree.
    fn taint(&self);
}
```

Why signed screen rects?

* Child screen position is computed as `parent.content.origin + child.pos - parent.scroll`.
* That subtraction can legitimately go negative; clipping will discard offscreen cells.
* Keeping this signed avoids underflow footguns.

---

# Layout Lifecycle

* `Layout` is cached on the node and only refreshed when the node is tainted.
* `measure()` results are cached **per-pass** and keyed by `MeasureConstraints` (exact values included).
* A single layout pass resolves geometry for a subtree; a render pass consumes the cached geometry.

---

# Layout Algorithm

## High-level

```
layout_node(node, available_outer: Size<u32>) -> Size<u32>:
    cfg = node.layout_cached()
    if cfg.display == None:
        node.set_hidden_layout_zero()
        return Size::ZERO

    outer = resolve_outer_size(node, cfg, available_outer)
    node.set_outer_size(outer)

    content = outer.saturating_sub(cfg.padding)
    node.set_content_size(content)

    layout_children(node.children, cfg.direction, content, cfg.gap)

    canvas_ctx = CanvasContext::new(node)          // children populated (rect + canvas)
    canvas = node.widget.canvas(content, &canvas_ctx)
    canvas = canvas.max(content)                   // enforce invariant
    node.set_viewport(content, canvas)             // clamps stored scroll offset

    return outer
```

## Resolve outer size

### Design invariants

* `measure()` returns **content size**.
* `min_*` / `max_*` apply to **outer size**.
* Constraints passed to `measure()` are in **content-box units** and already incorporate:

  * parent available space
  * this node’s max constraints
  * this node’s padding (by subtraction)

### Safe clamp for min/max

Min/max may be contradictory. The engine must never panic.

Define:

* If `min > max`, treat `max` as the effective bound for clamping (and optionally debug-log). The system must remain total-order stable; never call a panicking clamp.

### Pseudocode

```
resolve_outer_size(node, cfg, available_outer) -> Size<u32>:
    pad_x = cfg.padding.left + cfg.padding.right (saturating)
    pad_y = cfg.padding.top + cfg.padding.bottom (saturating)

    available_content_w = available_outer.w.saturating_sub(pad_x)
    available_content_h = available_outer.h.saturating_sub(pad_y)

    // Build initial content constraints
    c0 = MeasureConstraints {
        width:  constraint_for_axis(cfg.width,  available_content_w, cfg.min_width,  cfg.max_width,  pad_x),
        height: constraint_for_axis(cfg.height, available_content_h, cfg.min_height, cfg.max_height, pad_y),
    }

    measured_content = Size::ZERO
    did_measure = (cfg.width == Measure) || (cfg.height == Measure)

    if did_measure:
        m0 = node.measure_cached(c0)
        raw0 = match m0:
            Fixed(content) => content
            Wrap => measure_wrap_content(node, cfg, c0)
        measured_content = c0.clamp_size(raw0) // safety net

    // Compute preliminary outer size
    outer_w0 = match cfg.width:
        Flex(_)   => available_outer.w
        Measure   => measured_content.w.saturating_add(pad_x)
    outer_h0 = match cfg.height:
        Flex(_)   => available_outer.h
        Measure   => measured_content.h.saturating_add(pad_y)

    outer = Size { w: outer_w0, h: outer_h0 }
    outer = clamp_outer(outer, cfg.min_width, cfg.max_width, cfg.min_height, cfg.max_height)

    content = outer.saturating_sub(cfg.padding)

    // Reflow pass (at most one width-driven remeasure) when measure() ran:
    //
    // If the content width the widget effectively saw differs from final content width,
    // remeasure with width Exact(final) so height (and wrap sizing) can reflow correctly.
    if did_measure:
        width_seen = match c0.width:
            Exact(n) => n,
            AtMost(_) | Unbounded => measured_content.w

        if content.w != width_seen:
            c1 = MeasureConstraints { width: Exact(content.w), height: c0.height }
            m1 = node.measure_cached(c1)
            raw1 = match m1:
                Fixed(content) => content
                Wrap => measure_wrap_content(node, cfg, c1)
            content1 = c1.clamp_size(raw1)

            // Only update height if height is Measure; otherwise height is owned by parent.
            if cfg.height == Measure:
                outer_h1 = content1.h.saturating_add(pad_y)
                outer.h = outer_h1
                outer = clamp_outer(outer, cfg.min_width, cfg.max_width, cfg.min_height, cfg.max_height)
                content = outer.saturating_sub(cfg.padding)

    // Final "exact-size" measure call (optional but recommended) if height got clamped or width changed,
    // so widgets can cache layout for the actual content box they will render into.
    // This call does not alter geometry (constraints are exact); it exists for reflow/caching correctness.
    if did_measure:
        c_final = MeasureConstraints { width: Exact(content.w), height: Exact(content.h) }
        node.measure_cached(c_final)

    return outer
```

### `constraint_for_axis`

Constraints are for **content size**, but min/max are stored in **outer size** terms.

```
constraint_for_axis(sizing, available_content, min_outer, max_outer, pad_axis) -> Constraint:
    match sizing:
        Flex(_) => Exact(available_content)

        Measure => {
            // available space is the implicit upper bound (content box)
            // max_outer further reduces that bound
            effective_max_outer = match max_outer:
                Some(m) => min(m, available_content.saturating_add(pad_axis)),
                None => available_content.saturating_add(pad_axis)

            effective_max_content = effective_max_outer.saturating_sub(pad_axis)

            // If min == max (outer), we can treat it as exact sizing (in content units).
            if let (Some(min_o), Some(max_o)) = (min_outer, max_outer) {
                if min_o == max_o {
                    return Exact(max_o.saturating_sub(pad_axis))
                }
            }

            AtMost(effective_max_content)
        }
```

Notes:

* We intentionally do not encode min into measure constraints; min is enforced after measurement and may overflow the available space by design.

---

# Measuring wrapped containers

When a container returns `Measurement::Wrap`, the engine computes the container’s **content size** from its children’s **outer sizes** and gaps. Padding is handled separately by `resolve_outer_size`.

Key rule:

* **Flex only has meaning once the parent’s size on that axis is fixed.**

  * During a parent’s Wrap measurement, if the parent’s constraint on an axis is `Exact(n)`, that axis is fixed and flex behaves normally.
  * If the parent’s constraint is `AtMost(n)` or `Unbounded`, the parent’s size is not fixed; on that axis, children with `Flex` are treated as if they were `Measure` for intrinsic sizing purposes.

This implements the intent you described in prose but that the original pseudocode did not correctly encode.

```
measure_wrap_content(node, cfg, constraints) -> Size<u32>:
    direction = cfg.direction
    gap = cfg.gap

    children = node.children.filter(display != None)
    if children.is_empty(): return Size::ZERO

    main_fixed = is_exact(constraints.main(direction))
    cross_fixed = is_exact(constraints.cross(direction))

    // Compute an available bound for child measurement/layout simulation.
    // For AtMost(n) we use n. For Unbounded we use u32::MAX (with saturating sums).
    avail_main = max_bound(constraints.main(direction))
    avail_cross = max_bound(constraints.cross(direction))
    avail = Size::from_main_cross(direction, avail_main, avail_cross)

    // Pass 1: resolve non-flex (or flex treated as measure) children and sum main
    fixed_main_total = 0
    flex_children = [] // (index, weight)

    child_sizes = vec![Size::ZERO; children.len]

    for (i, child) in children:
        child_cfg = child.layout_cached()

        eff_main = effective_sizing(child_cfg.main(direction), main_fixed)
        eff_cross = effective_sizing(child_cfg.cross(direction), cross_fixed)

        if eff_main is Flex(w):
            flex_children.push((i, max(w,1)))
            continue

        // Measure/layout-resolve this child with the full available bound.
        // This resolves the child's *outer* size.
        child_available = avail
        size = resolve_outer_size_for_measure(child, child_cfg, child_available)
        child_sizes[i] = size
        fixed_main_total += size.main(direction)

    gap_total = gap * (children.len - 1)
    remaining = avail_main.saturating_sub(fixed_main_total.saturating_add(gap_total))

    // Pass 2: allocate flex shares if the parent main axis is fixed; otherwise flex was treated as measure
    if main_fixed && !flex_children.is_empty():
        shares = allocate_flex_shares(remaining, flex_children.weights_in_order())

        for (k, (i, w)) in enumerate(flex_children):
            share_main = shares[k]
            child_cfg = children[i].layout_cached()
            child_available = Size::from_main_cross(direction, share_main, avail_cross)
            size = resolve_outer_size_for_measure(children[i], child_cfg, child_available)
            child_sizes[i] = size

    // Now compute intrinsic content extent from resolved child sizes.
    main_total = sum(child_sizes[i].main(direction)) + gap_total
    cross_max = max(child_sizes[i].cross(direction))

    content = Size::from_main_cross(direction, main_total, cross_max)
    return constraints.clamp_size(content)
```

`resolve_outer_size_for_measure` is the measurement-only version of `resolve_outer_size` (same logic, but does not set node geometry or call canvas/layout children). In practice you can implement this by factoring `resolve_outer_size` into a pure helper.

---

# Flex allocation (correct and deterministic)

The engine must guarantee:

* Sum of allocated shares equals `remaining`.
* Allocation is proportional to weights (integer).
* Tie-breaking is deterministic and stable.

Use the **largest remainder method**:

```
allocate_flex_shares(remaining: u32, weights: &[u32]) -> Vec<u32>:
    total = sum(weights)
    if remaining == 0 || total == 0:
        return vec![0; weights.len]

    // base shares and fractional remainders
    base[i] = floor(remaining * weights[i] / total)
    rem[i]  = (remaining * weights[i]) % total

    used = sum(base)
    extra = remaining - used         // invariant: extra < weights.len

    // distribute `extra` cells to highest rem[i], tie-break by lower index
    winners = indices sorted by (rem desc, index asc)
    for j in 0..extra:
        base[winners[j]] += 1

    return base
```

Properties:

* No over-allocation (fixes the remainder bug in the original pseudocode).
* Stable across runs.

---

# Child layout (positions)

Once a node’s `content_size` is known, laying out children is straightforward and uses the same flex allocation algorithm.

```
layout_children(children, direction, available_content, gap):
    visible_children = children.filter(display != None)
    if visible_children.is_empty(): return

    // Pass 1: resolve non-flex children (outer sizes)
    fixed_main_total = 0
    flex = []  // (index, weight)
    pre = vec![Size::ZERO; visible_children.len]

    for (i, child) in visible_children:
        cfg = child.layout_cached()
        main_sizing = cfg.main(direction)

        if main_sizing is Flex(w):
            flex.push((i, max(w,1)))
            continue

        // Non-flex children can be resolved with the full available main bound.
        child_available = Size::from_main_cross(direction,
            available_content.main(direction),
            available_content.cross(direction)
        )
        size = resolve_outer_size(child, cfg, child_available)
        pre[i] = size
        fixed_main_total += size.main(direction)

    gap_total = gap * (visible_children.len - 1)
    remaining = available_content.main(direction)
        .saturating_sub(fixed_main_total.saturating_add(gap_total))

    shares = allocate_flex_shares(remaining, flex.weights_in_order())

    // Pass 2: position + final layout
    pos_main = 0
    for (i, child) in visible_children in order:
        cfg = child.layout_cached()
        main = match cfg.main(direction):
            Flex(_) => shares[next_flex_index()]
            Measure => pre[i].main(direction)

        child_available = Size::from_main_cross(direction, main, available_content.cross(direction))

        child.set_position(Point::from_main_cross(direction, pos_main, 0))

        actual = layout_node(child, child_available)

        // Advance by actual size so min constraints never cause overlaps.
        pos_main = pos_main.saturating_add(actual.main(direction)).saturating_add(gap)
```

---

# Scrolling and viewport clamping

* Each node stores:

  * `outer_rect` (size + position)
  * `content_rect` (outer minus padding)
  * `canvas_size` (content coords)
  * `viewport_offset` (content coords; scroll state)

The engine is responsible for clamping scroll state whenever view/canvas changes:

```
set_viewport(view: Size<u32>, canvas: Size<u32>):
    canvas = canvas.max(view)

    max_x = canvas.w.saturating_sub(view.w)
    max_y = canvas.h.saturating_sub(view.h)

    offset.x = min(offset.x, max_x)
    offset.y = min(offset.y, max_y)
```

This ensures the invariant:

* `0 <= offset <= canvas - view` (per axis)

---

# Rendering semantics (correct clipping)

Render is strictly separated from layout. The render traversal must clip children to the parent’s **content rect**, not the outer rect.

A correct traversal is:

```
render_tree(node, parent_clip):
    if node.display == None: return

    // Intersect with this node's outer rect: a widget cannot draw outside its own allocation.
    clip_outer = intersect(parent_clip, node.view.outer)

    // Render this widget itself (border/background may paint in padding).
    push_clip(clip_outer)
    node.widget.render(render, view_context_for(node))
    pop_clip()

    // Children are clipped to this node's content rect (structural padding exclusion).
    clip_children = intersect(parent_clip, node.view.content)

    // Apply scroll offset when mapping child content coords to screen coords.
    for child in node.children_in_order():
        child_screen_origin =
            node.view.content.origin
            + child.rect.origin
            - node.view.tl  // signed math

        // Child outer rect in screen coords = origin + child.rect.size
        child_outer_rect = Rect { origin: child_screen_origin, size: child.rect.size }

        render_tree(child, clip_children)
```

Notes:

* Scroll translation applies to **children positioning**, not to the parent’s own rendering.
* Widgets can read the current scroll offset from `ViewContext` to draw scrollbars, etc.
* Containers that draw borders should do so in `outer`, while children are clipped to `content`.

---

# Design decisions (clarified)

## Overflow and min constraints

* If children (after min/max) plus gaps exceed available space, overflow is clipped.
* Min constraints are hard requirements and may force a node to exceed available space (the parent clips).

No shrink/priority model is included. This keeps the system simple and predictable.

## Cross-axis positioning

Children are positioned at the start of the cross axis. Alignment is achieved via composition (spacers, Center/Align helper widgets). This remains the default model.

(If you later decide you want alignment primitives, they can be added as a small extension to `Layout` without invalidating the core design; but they are intentionally omitted here.)

## Responsive layouts

Because `layout()` is cached and has no inputs, responsive behavior is state-driven:

* A widget may observe `content_size` in `canvas()` or `render()`, update internal state, and call `taint()` to trigger a new layout pass.
* Use hysteresis to avoid oscillation at thresholds.

---

# Examples (updated for content/outer semantics)

## Fixed 5×5 leaf (no padding)

```rust
fn layout(&self) -> Layout {
    Layout::column()
}

fn measure(&self, c: MeasureConstraints) -> Measurement {
    c.clamp(Size { width: 5, height: 5 }) // content size == outer size (no padding)
}
```

## Frame (structural border via padding)

Frame fills parent and reserves 1-cell border so children cannot draw under it.

```rust
fn layout(&self) -> Layout {
    Layout::fill().padding(Edges::all(1))
}

// measure() not called (both axes Flex)

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.border("frame", ctx.view().outer, BorderStyle::Single)?;
    Ok(())
}
```

Children are laid out into `ctx.view().content`.

## ContentFrame (wraps children; border padding included)

```rust
fn layout(&self) -> Layout {
    Layout::column().padding(Edges::all(1)).gap(1)
}

// measure() default wrap()

fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
    r.border("dialog", ctx.view().outer, BorderStyle::Double)?;
    Ok(())
}
```

If you want scrollability, wrap the dialog content in a dedicated `Scroll` widget or override `canvas()`.

## Scroll container (recommended standard widget)

A generic scroller that computes canvas from children extent:

```rust
fn layout(&self) -> Layout {
    Layout::fill()
}

fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
    let ext = ctx.children_extent();
    Size {
        width: view.width.max(ext.width),
        height: view.height.max(ext.height),
    }
}
```

This works for both:

* long vertical lists (extent height grows)
* wide rows of widgets (extent width grows)

---

# Migration (still flag-day)

* Remove old engine
* Implement:

  * size/rect/edges utilities (saturating math)
  * layout pass with measurement caching per pass
  * render traversal with correct child clipping to content rect and signed scroll translation
  * viewport clamping
* Update widgets to the content-box measurement contract (`measure()` excludes padding)

---

# Addendum: Unit Test Matrix

Below is a practical, high-signal matrix of unit tests. Most of these can be pure functions (no terminal buffer needed). Where geometry requires tree traversal, build a minimal node harness with deterministic child lists.

## A) Primitive math + constraints

1. **ClampSize.Unbounded**

   * Input: Unbounded × Unbounded, size (w,h)
   * Expect: unchanged

2. **ClampSize.AtMost**

   * Input: AtMost(0), AtMost(n), size > n
   * Expect: axis clamped

3. **ClampSize.Exact**

   * Input: Exact(0), Exact(n), arbitrary size
   * Expect: forced exactly n

4. **Edges.SaturatingAdd**

   * Large left+right overflow scenario
   * Expect: saturating_add, no wrap

5. **Size.SaturatingSubPadding**

   * outer smaller than padding total
   * Expect: content axis = 0

## B) Min/Max clamping (no panics)

6. **ClampOuter.NoBounds**

   * outer unchanged

7. **ClampOuter.MinOnly**

   * min > available
   * Expect: expands beyond available

8. **ClampOuter.MaxOnly**

   * max < computed
   * Expect: reduced

9. **ClampOuter.MinGreaterThanMax**

   * min=10, max=5
   * Expect: deterministic (documented) behavior, no panic

## C) Resolve outer size (padding + measurement contract)

10. **Leaf.MeasureAddsPadding**

    * Leaf returns content 5×5, padding 1 all sides
    * Expect: outer 7×7, content 5×5

11. **Leaf.PaddingConsumesAll**

    * available 1×1, padding 1 all sides
    * Expect: outer at most 1×1 (if flex) or clamped; content 0×0

12. **FlexAxisConstraintsAreExact**

    * width flex, height measure
    * Verify measure called with width Exact(available_content_w)

13. **WidthClampTriggersReflowRemeasure**

    * Text-like widget where height depends on width
    * Scenario: initial width seen 20; final content width becomes 10 (via max_width or parent alloc)
    * Expect: second measure call with width Exact(10); height updated (if height Measure)

14. **MinWidthExpandsFlexWidthTriggersReflow**

    * width flex, height measure
    * available_content_w=10, min_width forces outer to 30 -> content_w=28 (with padding)
    * Expect: remeasure with width Exact(28) and updated height

## D) Wrap measurement (flex semantics + gaps)

15. **Wrap.NoChildren**

    * Wrap container with padding: content size 0, outer size = padding (after resolve)
    * Expect: content 0, outer padding (bounded)

16. **Wrap.SumMainMaxCross**

    * Column with 3 fixed-size children (content sizes), gap 1
    * Expect: content.height = sum + gaps; content.width = max

17. **Wrap.IncludesChildPaddingViaOuterSizes**

    * Child leaf content 3×1 with padding 1 -> outer 5×3
    * Parent wraps: ensure it uses child outer sizes

18. **Wrap.FlexChildTreatedAsMeasureWhenParentAxisNotExact**

    * Parent column measuring height (AtMost), child height Flex(1) but has intrinsic measure height 4
    * Expect: wrap height includes 4 (flex treated as measure for intrinsic sizing)

19. **Wrap.FlexChildBehavesAsFlexWhenParentAxisExact**

    * Parent row with width constraint Exact(10), height measure, two flex children that wrap height based on allocated widths
    * Expect: allocation uses shares (e.g., 5/5), height computed from those shares (not full 10 each)

20. **Wrap.GapCountsOnlyVisibleChildren**

    * Mix Display::None and Block children
    * Expect: gaps computed with visible_count

## E) Flex share allocation correctness (critical)

21. **FlexShares.SumEqualsRemaining**

    * random weights, random remaining
    * Expect: sum(shares) == remaining

22. **FlexShares.ProportionalSanity**

    * remaining=5, weights [3,7]
    * Expect: [2,3] or [1,4] depending on remainder tie-break; specify expected under largest remainder

23. **FlexShares.StableTieBreak**

    * remaining=2, weights [1,1,1]
    * Expect: extras go to lowest indices deterministically

24. **FlexShares.WeightZeroClamped**

    * weights include 0
    * Expect: treated as 1

## F) Child positioning / overflow

25. **Positions.MonotonicMain**

    * verify child positions strictly non-decreasing by main axis

26. **NoOverlapsEvenWithMinExpansion**

    * flex child share small but min expands; ensure next child position uses actual size, not share

27. **OverflowClipsButGeometryStillConsistent**

    * measure_total + gaps > available
    * expect remaining=0, flex shares 0, but positions still monotonic

## G) Scroll invariants

28. **CanvasClampedAtLeastView**

    * canvas smaller than view returned by widget
    * expect engine promotes to view

29. **OffsetClampedWhenCanvasShrinks**

    * initial offset near end; then canvas shrinks
    * expect offset reduced to new max

30. **OffsetClampedWhenViewGrows**

    * resize increases view; expect offset clamped

31. **ZeroView**

    * view 0×0, arbitrary canvas
    * expect offsets clamp to 0 and no panics

## H) CanvasContext correctness

32. **ChildrenExtentEmpty**

    * no children => extent 0×0

33. **ChildrenExtentComputesMaxCorner**

    * multiple children with positions and sizes
    * verify max(x+w), max(y+h)

34. **ChildrenExtentIgnoresChildCanvas**

    * child canvas larger than rect
    * extent uses rect only

## I) Render geometry transforms (pure math)

35. **ChildScreenOriginSigned**

    * parent content origin (10,10), child pos (0,0), scroll tl (5,0)
    * expect child screen origin (5,10) (signed ok)

36. **ChildClipIsContentRect**

    * ensure computed clip for children is parent.content, not outer

## J) Property/fuzz tests (recommended)

37. **RandomTreeNoPanics**

    * Generate random trees with random min/max/padding/gaps and sizes
    * Run layout; assert invariants:

      * canvas >= view (content)
      * offset <= canvas-view
      * content == outer.saturating_sub(padding)
      * positions monotonic along main axis

38. **RandomFlexAllocationInvariants**

    * random weights/remaining -> sum == remaining; each share differs by at most 1 from ideal floor/ceil around proportional target (optional stronger check)

