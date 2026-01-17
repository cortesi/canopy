# Font Rendering Plan

This plan sketches a path to add ASCII font rendering into a bounded screen region, with
foreground/background gradients and app-bundled font assets. It focuses on API shape and data
flow; implementation details remain open until format and rendering choices are confirmed.

## Goals and constraints
- Render large-font text into a `Render`-provided region with clipping.
- Primary use case is banner/header text for app identity; support dynamic updates.
- Support multi-line input by splitting on `\n` (single-line is the common case).
- Font rendering accepts a fixed target canvas size supplied by layout.
- Apply gradients to foreground and background via the style system (solids remain default).
- Assume a modern RGB terminal; optionally use a Nerd Font glyph ramp.
- Let apps bundle fonts directly in the binary (e.g., `include_bytes!`).

## Research notes (repo)
- `Render::text` and `Render::fill` resolve styles by name and clip to the view
  (`crates/canopy/src/core/render.rs`).
- `TermBuf` supports grapheme-aware writes (`put_grapheme`, `text`) and can fill rectangles with a
  provided `Style` (`crates/canopy/src/core/termbuf.rs`).
- `Color` already exposes `blend`, `scale_brightness`, and related helpers suitable for gradients
  (`crates/canopy/src/core/style/color.rs`).
- The `Box` widget fills interiors with a style name, suggesting a pattern to extend for gradient
  fills (`crates/canopy-widgets/src/boxed.rs`).

## Design decisions

### Font data model
Introduce a `canopy-widgets::font` module that stores glyphs as rows of ASCII characters plus
metrics. Start with fixed-width placement and leave room for later kerning/smushing rules. Missing
glyphs render as a fallback glyph (default `?`), with a future option to skip instead.

- `Font`: owns glyph map, height, baseline, default spacing, and optional kerning rules.
- `Glyph`: vector of rows, width, and optional left/right bearings for overlap.

### Font loading and rasterization
V1 targets TTF fonts. Provide constructors that accept in-memory font sources so apps can bundle
fonts:

- `Font::from_bytes(...)` for TTF via `include_bytes!`.
- `Font::from_str(...)` reserved for future ASCII-art formats (FIGlet/BDF).
- Optional helpers for file-based loading (`Font::from_reader`).

Rasterize glyphs using the selected crate, map luminance to an ASCII ramp, and cache per-glyph
results for fast layout. Start with the ramp " .:-=+*#%@" and allow an optional Nerd Font ramp
for richer glyph sets.

### TTF rasterization crate survey
- `fontdue`: parser + rasterizer + layout tool; straightforward bitmap access.
- `ab_glyph`: glyph rasterization via outline drawing; lightweight but lower-level layout.
- `rusttype`: ASCII art example exists; older parser and limited OTF coverage.
- `swash`: full text shaping/rasterization; heavier and lower-level than needed for v1.

Decision: use `fontdue` for v1 (layout + rasterization in one crate). Keep `ab_glyph` as a backup
if we need a slimmer dependency surface later.

### Rendering pipeline and fit policy
The renderer receives text plus a fixed target canvas (`Expanse`) from layout. It rasterizes to
that height, aligns inside the target rect, and clips any overflow.

- Scale policy: derive font pixel height from target canvas height.
- Overflow policy: clip by default; future option for ellipsis or width-fit.
- Alignment: use `layout::Align` for horizontal and vertical positioning.

### Gradient support via `Paint`
Introduce a `Paint` model for foreground/background styling, backed by gradients.

- `Paint::Solid(Color)` for constant colors.
- `Paint::Gradient(GradientSpec)` for gradients on either channel.
- `GradientSpec` stores angle + stops; v1 implements axis-based two-stop interpolation.

Style system changes:
- Replace `Style` `fg/bg: Color` with `fg/bg: Paint` and introduce `ResolvedStyle` for per-cell
  colors stored in `TermBuf`.
- Update `PartialStyle`, `StyleBuilder`, and `StyleRules` to accept `Paint` (with `fg`/`bg`
  taking `impl Into<Paint>` plus convenience helpers for solids/gradients).
- `StyleManager` resolves `Style` (paints), while `Render` resolves to `ResolvedStyle` per cell
  using `Paint::resolve(rect, point)` in local render coordinates.
- `StyleEffect` implementations apply to `Paint` by mapping over solid colors and gradient stops.

### API sketch

```rust
pub enum Paint { Solid(Color), Gradient(GradientSpec) }

pub struct GradientStop { pub offset: f32, pub color: Color }

pub struct GradientSpec { pub angle_deg: f32, pub stops: Vec<GradientStop> }

pub struct Font { /* glyph map + metrics */ }

pub struct FontRenderer { font: Font, ramp: GlyphRamp, fallback: char }

pub struct FontLayout { pub lines: Vec<String>, pub size: Expanse }

pub struct LayoutOptions {
    pub h_align: Align,
    pub v_align: Align,
    pub overflow: OverflowPolicy,
}

pub enum OverflowPolicy { Clip }
```

### Widget integration
Provide a widget (e.g., `FontBanner` or `AsciiText`) that wraps the renderer:

- Holds text, font, alignment, style path for glyph paint, optional gradient config.
- Caches `FontLayout` keyed by text and available width to avoid recompute (patterned after the
  `Text` widget's wrap cache).
- `measure` returns the font layout size; `canvas` can mirror view for no scrolling.

### Testing and examples
- Unit tests for TTF rasterization mapping, layout sizing/overflow, and gradient interpolation.
- `fontgym` example app: scrollable frame of font blocks with different fonts/styles; a text
  edit box updates the string rendered in every block.
- Docs update outlining font loading, renderer API, and widget usage.

## Staged execution checklist

1. Stage One: Upfront design + format choice
1. [x] Confirm UI scope (banner/header, multiline split, alignment, fixed target canvas).
2. [x] Confirm v1 is TTF-only, modern RGB terminal assumption, optional Nerd Font ramp.
3. [x] Confirm missing glyph fallback and initial ASCII ramp.
4. [x] Confirm gradient scope (fg+bg via style system) and extensible angle+stop model.
5. [x] Survey TTF rasterization crates and select `fontdue` for v1.
6. [x] Define fit/overflow policy for rendering text into a fixed target canvas.
7. [x] Design `Paint` integration in `Style`, `StyleMap`, `StyleManager`, `Render`, `TermBuf`.
8. [x] Draft public API surface for `Font`, `Glyph`, `FontRenderer`, `FontLayout`, `Paint`,
    `GradientSpec`, and widget options.
9. [x] Specify bundling patterns for TTF (`include_bytes!`) and note future ASCII-art formats.

2. Stage Two: Rendering core
1. [x] Implement `FontRenderer` and `FontLayout` using a fixed target canvas, multiline split, and
    alignment options.
2. [x] Implement TTF rasterization + ASCII ramp mapping with caching using `fontdue`.
3. [x] Implement fallback glyph behavior and optional Nerd Font ramp.
4. [x] Add unit tests for layout sizing, overflow policy, multi-line behavior, and fallback glyphs.

3. Stage Three: Gradient utilities
1. [x] Implement `Paint` and `GradientSpec` per the Stage One design (fg/bg, angle+stops).
2. [x] Update style resolution and rendering (`StyleMap`, `StyleManager`, `Render`, `TermBuf`).
3. [x] Add gradient tests for foreground/background, orientation, and stop interpolation.

4. Stage Four: Widget + docs
1. [x] Create `FontBanner` widget with caching, alignment, and gradient fill.
2. [x] Build `fontgym` example app with a scrollable frame of font blocks and a shared text edit
    control that updates all blocks.
3. [x] Add bundled font/style variations for `fontgym` (multiple fonts + gradients).
4. [x] Update docs for font loading, renderer API, and widget usage.
