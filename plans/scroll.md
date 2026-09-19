# Smoother scrollbar thumbs

## Description

Scrollbar thumbs are placed and sized in whole terminal cells.
`LineSegment::split_active` maps the scrolled window onto the track, and
`View::vactive` turns that into the rect a `Scrollbar` paints and
hit-tests. The mapping used to floor the thumb start and round the thumb
length up.

Flooring the start means the thumb sits still while the true position
advances through a cell, then jumps a whole cell at once. A 30-row track
over a 1000-line document moves the thumb one cell every ~33 lines.
Rounding the length up also overstates the visible share of a short
viewport: six rows of thirty produce a two-cell thumb where 1.2 cells is
exact.

This plan covers three steps: nearest-cell rounding (implemented),
sub-cell thumb edges using block glyphs (proposed, vertical only), and a
dedicated scrollbar gutter (alternative, not proposed now). The last two
exist because the remaining error is quantization, and terminal cells are
the smallest unit of normal text. Block Elements can represent part of a
cell through a glyph's fill plus the cell background, which doubles or
octuples vertical resolution, at the cost described under Risks.

## Changes

### C1: Nearest-cell thumb placement (implemented)

`split_active` now rounds both ends of the active extent to the nearest
cell instead of flooring the start and rounding the length up. The active
extent keeps a minimum of one cell and is capped so the track is never
overrun. `Scrollbar::offset_for_thumb_start` inverts the same rounding, so
a dragged thumb stays under the pointer, and the last thumb position
still reaches the final offset.

The maximum positional error drops from one cell to half a cell, and a
short viewport no longer inflates the thumb. One visible consequence: in
configurations where `ceil` added a cell, the thumb is now narrower and
its last position is a cell further down the track. Dragging to the end
needs the pointer on the final thumb position rather than near it; that is
the standard mapping, and Ratatui rounds the same way.

Tests: the geom `split_active` cases were already exact.
`scrolling_tests::owners_draw_and_expose_their_configured_scrollbar_glyphs`
expected the inflated four-cell horizontal thumb and now expects three.
`render_tests::frame_scrollbar_keeps_the_thumb_under_the_pointer` expects
the exact one-row thumb and its centered press offset.

Files: `crates/canopy-geom/src/linesegment.rs`,
`crates/canopy-widgets/src/scrollbar.rs`,
`crates/canopy-widgets/src/scrolling_tests.rs`,
`crates/canopy-widgets/src/render_tests.rs`.

### C2: Sub-cell thumb edges (proposed, vertical only)

Keep `View::vactive` and the integer rect for hit testing, and derive the
painted extent separately from the view. Compute the thumb boundaries in
eighths, for example `b0 = round(8*T*s/V)` and
`b1 = round(8*T*(s+W)/V)` in u64 arithmetic, where `T` is the track
length, `s` the scroll offset, `W` the viewport, and `V` the canvas. Cells
fully inside the thumb paint the full block. The first and last partial
cells paint a lower block (`▄`, `▅`…`▇`) whose foreground is the
remainder fraction and whose background is the thumb, or the mirror of
that pairing.

Because a cell background fills the whole cell, the remainder colour must
come from a style rule; there is no transparency. The thumb role needs a
companion for the remainder, or must lean on a surface rule the theme
already sets (the help overlay's `help/frame` background is the
precedent). `Scrollbar::render_marks` must carry the partial glyph into
mark rendering, so a marked cell under a partial thumb recolours the thumb
fraction and keeps its shape.

Proposed shape:

- Vertical only. Horizontal is capped at half-cell resolution, because
  only quadrants can sit in the bottom half of a border row, left and
  right eighth blocks are full height, and legacy-computing sextants are
  not available in monospace fonts.
- Start with half blocks rather than eighths. Font coverage for
  `▀▄█` is far better than for `▁…▇` and quadrants, and the eighth blocks
  buy less than they appear to for typical track lengths.
- Opt in through the glyph set (`THIN_SMOOTH`, or a mode on
  `ScrollbarGlyphs`), not by default. Custom and ASCII sets (`T/t/|/-` in
  the tests) cannot be smooth, and `Frame` and `Columns` share the glyph
  type.
- Keep integer geometry for wheel, press, and drag. A rendered half cell
  outside the integer rect is then not grabbable; either accept that or
  widen the hit area by one cell at each partial edge.
- A thumb shorter than a cell is a band in the middle of a cell, and Block
  Elements has no middle-band glyph. Clamp the painted thumb to a minimum
  of one half or one quarter cell, anchored at the true start.

### C3: Dedicated scrollbar gutter (alternative)

If the thin frame line must survive intact, the only clean answer is a
cell of its own for the thumb: a frame edge becomes two cells, the border
line keeps its cell, and the scrollbar paints backgrounds and partial
blocks in the cell beside it. This changes frame layout and every widget
that composes a frame, so it is not proposed now.

## Risks

- **The line cannot survive a partial cell.** A cell has one glyph and one
  background. The glyph's fill can be a fraction, and the background fills
  the whole cell, so a cell can show a partial thumb but cannot also show
  the thin track line in the remainder. The choices are a line gap of at
  most one cell at each thumb edge, a thickened track segment, or a
  thin tinted edge. This is fundamental, not something the foreground and
  background trick fixes.
- **Font coverage and seams.** Eighth blocks and quadrants are missing
  from many monospace fonts; a terminal then falls back to another font,
  often with different glyph widths, which breaks alignment or shows
  tofu. Half blocks are much safer. Block glyphs are also drawn by the
  font, and some fonts draw them narrower or shorter than the cell, so a
  solid thumb can show seams. Painting the thumb body with the background
  colour avoids glyph seams for the body but not for the partial edges.
- **Minimum thumb and mid-cell bands.** A very long document rounds to a
  thumb shorter than a cell. Block Elements has no glyph for a band that
  touches neither the top nor the bottom of the cell, so a minimum size
  and an anchor policy are required, and the thumb position becomes
  approximate for the longest documents.
- **Styling without transparency.** The remainder fraction paints a solid
  colour, so it must match the surface behind the bar. A frame inside a
  panel needs a context rule (the same reason `help/frame` carries a
  background today). All built-in themes and the golden theme capture
  change, and the two-colour cell needs roles that exist for the body, the
  remainder, the active drag, and marks.
- **Interaction mismatch.** Hit testing and dragging use the integer rect.
  A rendered partial cell outside it is click-through, and the drag
  inverse maps to whole offsets. Keeping integer interaction is the simple
  choice and costs at most half a cell of grabbing accuracy.
- **Marks.** Scroll marks currently repaint every covered track cell with
  the mark colour and the thumb glyph. Partial cells multiply the
  combinations: a mark under a partial thumb needs the mark colour in the
  thumb fraction, the remainder colour elsewhere, and the partial glyph
  preserved.
- **Test and automation churn.** Rendering tests assert exact block rows
  (`rows_with(... '█')`), smoke scripts read `screen_text()`, and the
  custom glyph test shows the full-cell path must remain. Any change here
  touches many expectations at once.
- **Terminal background semantics.** A thumb drawn with background colour
  is affected by background transparency, minimum-contrast settings, and
  selection inversion differently from a foreground block. This varies by
  terminal and cannot be detected.
- **It is only the thumb that gets smoother.** Scroll offsets are whole
  lines and content redraws whole rows. Sub-cell thumbs improve the
  position indicator, not the motion of the text itself.

## Recommendation

Take C1 now; it is free of visual risk and halves the worst-case error.
Treat C2 as a prototype: vertical only, half blocks first, opt-in per
glyph set, integer input geometry, and an explicit remainder role. Judge
it in several fonts and terminals before considering a default, and
measure whether the notched line reads as smoother or as a defect. Keep
C3 as the fallback if the thin line turns out to matter more than the
added cell.
