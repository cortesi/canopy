# Render buffer allocation proposal

This proposal addresses feedback point D: the renderer allocates a per-node buffer per
frame. I reviewed the current render path in `crates/canopy/src/core/canopy.rs` and
`crates/canopy/src/core/render.rs`, the buffer implementation in
`crates/canopy/src/core/termbuf.rs`, and the view projection logic in
`crates/canopy/src/core/viewstack.rs`.

## Current behavior and cost

- Each visible node allocates a `TermBuf` in `Render::new` sized to `vp.view()`.
- Widgets draw in canvas coordinates into that per-node buffer.
- After rendering, the node buffer is copied into the destination buffer via
  `view_stack.projection()` and `TermBuf::copy_rect_to_rect`.
- This happens every frame for every visible node.

This has two main costs: (1) repeated allocations proportional to tree depth and
view sizes and (2) an extra full-buffer copy per node per frame.

## Is this worth addressing?

Yes, if the project is aiming for smooth scrolling, deep trees, or large views.
The current model scales as O(sum(view area per node)) allocations and copies per
frame, which becomes a real cost once the tree grows or rendering is frequent.
For very small trees or low refresh rates it is acceptable, but this is a core
hot-path and the current behavior will become a bottleneck for larger UIs. It is
worth addressing for performance and headroom, even if the immediate gains are
only visible under stress.

## Recommended solution assessment

### 1) Single composed buffer + clip stack

This is appropriate and is the cleanest long-term fix. The existing `ViewStack`
projection already provides the mapping between a node's canvas coordinates and
screen coordinates. Replacing per-node buffers with a shared destination buffer
reduces allocations to one per frame and eliminates the copy step. It does add
complexity to `Render` because it must clip and translate into the shared buffer,
but this is contained and testable.

### 2) Buffer pool reuse

This is a lower-risk incremental improvement, but it keeps the per-node copy
cost and retains per-node buffer sizes at peak. It can reduce allocation churn
but not overall memory or rendering work. It is useful only as a transitional
step if the shared-buffer refactor is too large to do now.

### Alternative approach

A hybrid approach can be a good compromise: render directly into the shared
buffer by default, and only allocate an offscreen `TermBuf` for special cases
(eg. widgets that explicitly request an offscreen buffer for post-processing).
This keeps the fast path simple while preserving flexibility. It also opens the
path to add a small render cache for widgets that are known to be static.

## Proposed direction

Proceed with the single composed buffer and clip translation model. Preserve the
current widget-facing `Render` API so widgets continue to draw in canvas
coordinates, but change the internal `Render` implementation to optionally draw
into a shared `TermBuf` with a clip rect and translation offset. Keep a fallback
constructor for offscreen buffers so tests and any future offscreen rendering
needs are supported.

## Design (complete)

### Rendering model

- Keep the render traversal order (parent first, then children). This preserves
  the current composition semantics where child draws can overwrite parent
  content and untouched areas remain visible.
- Use the existing `ViewStack::projection()` for each node to compute:
  - `canvas_rect`: visible region in the node's canvas coordinates.
  - `screen_rect`: destination region on the screen.
- Derive a translation offset:
  - `screen_origin = screen_rect.tl - canvas_rect.tl`.
  - This maps any canvas point `p` to `p + screen_origin` in screen space.
- Skip rendering for nodes that are hidden or whose projection is `None`.

### Render implementation changes

- Replace `Render`'s internal buffer with a render target abstraction:
  - `RenderTarget::Owned(TermBuf)` for offscreen buffers.
  - `RenderTarget::Shared(&mut TermBuf)` for the shared destination buffer.
- `Render` stores:
  - `clip: Rect` in canvas coordinates (use `canvas_rect` from projection).
  - `origin: Point` for translation to destination coordinates.
  - `style` and `stylemap` unchanged.
- `Render::new` continues to create an owned buffer (for tests and any explicit
  offscreen use cases). Add `Render::new_shared(stylemap, style, target, clip,
  origin)` to render into the shared buffer.
- `Render::fill` / `Render::text` / `Render::solid_frame`:
  - Intersect the draw rect with `clip` in canvas coordinates.
  - Translate to destination coordinates by `origin`.
  - Write directly into the target buffer.
- `Render::get_buffer`:
  - Remains available for owned buffers (tests use it).
  - Returns `None` (or becomes a `Result`) for shared renderers. If we want to
    avoid API breakage, add `Render::buffer()` returning `Option<&TermBuf>` and
    keep `get_buffer` for owned-only use inside tests.

### Canopy render traversal

- In `Canopy::render_traversal`:
  - Remove `Render::new` + per-node `TermBuf` allocation.
  - If `view_stack.projection()` yields `(canvas_rect, screen_rect)`, construct
    `Render::new_shared` with the destination buffer, `clip = canvas_rect`, and
    `origin = screen_rect.tl - canvas_rect.tl`.
  - Call `widget.render` with the new renderer.
  - Remove the `copy_rect_to_rect` step entirely.
- Keep `TermBuf::new` for the per-frame destination buffer in `Canopy::render`.

### Semantics and correctness

- Clipping behavior is preserved by `clip` (only visible canvas region can draw).
- Transparency semantics remain: since traversal is top-down and there is no
  explicit per-node clear, unchanged cells leave parent drawings intact.
- Text padding still overwrites with spaces, matching current behavior when
  compositing the per-node buffer.
- Cursor overlay logic is unchanged (still applied in `post_render`).

### Tests and benchmarks

- Update `crates/canopy/tests/test_render.rs` and `crates/canopy/src/core/render.rs`
  tests to use the owned-buffer constructor explicitly.
- Add a new test that renders a child with a clipped view into a shared buffer
  and verifies that only the visible region changes.
- Consider extending `crates/canopy/benches/rendering.rs` to compare old vs new
  render paths, if we keep a temporary feature flag during the transition.

### Risks and mitigations

- **API churn**: introducing `Render::new_shared` requires minor API updates in
  internal code and tests. Mitigation: keep `Render::new` intact for external
  users and tests.
- **Clipping bugs**: wrong translation or clipping can shift output. Mitigation:
  add focused tests for projection and clipping in the shared render path.

## Staged execution checklist

1. Stage One: Rendering surface API

Clarify the render surface abstraction without changing behavior.

1. [x] Introduce a `RenderTarget` abstraction and update `Render` to support
       owned buffers while keeping `Render::new` unchanged.
2. [x] Add `Render::new_shared` and internal buffer access for shared renders.
3. [x] Adjust `crates/canopy/src/core/render.rs` tests to explicitly use owned
       buffers and ensure they still pass.

2. Stage Two: Shared-buffer rendering path

Switch the render traversal to render directly into the destination buffer.

1. [x] Update `Canopy::render_traversal` to use `Render::new_shared` with
       `clip = canvas_rect` and `origin = screen_rect.tl - canvas_rect.tl`.
2. [x] Remove per-node buffer allocation and the `copy_rect_to_rect` step.
3. [x] Add a unit test for shared-buffer clipping in
       `crates/canopy/src/core/render.rs`.

3. Stage Three: Cleanup and validation

Verify behavior and remove any now-unused helpers.

1. [x] Review any unused `TermBuf` helpers or render utilities and remove
       dead code introduced by the change.
2. [x] Run formatting, clippy, and tests; fix any warnings or failures.
3. [x] Confirm no docs or comments describe per-node buffer composition.
