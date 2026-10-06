# Mosaic Rendering

## Description

Canopy draws pictures and large text from block characters, and each consumer does it its own way:

- `ImageView` draws every cell as `▀`, with the top half of the cell in the foreground and the
  bottom half in the background (`canopy-widgets/src/image_view.rs`, `render_cells`).
- `FontRenderer`, under `FontBanner`, rasterizes text with fontdue at 8 pixels per cell column. It
  averages each quadrant and picks one of the 16 quadrant glyphs by least squares
  (`canopy-widgets/src/font.rs`, `GlyphRamp`).
- `BigText` maps bitmap-face pixels to half blocks at integer scales (`big_text/mod.rs`,
  `Placement::cell`).
- The charts use eighth blocks and a braille canvas (`chart.rs`).

[Chafa](https://hpjansson.org/chafa/) gets far better results from the same cell grid. It treats
each cell as an 8×8 matrix of pixels. It searches a dictionary of symbol shapes for the shape and
the two colors that reproduce the matrix with the least squared error. A cell can then place an
edge at a quarter of a cell in many orientations, where a half block places only one horizontal
split.

This plan builds that facility for Canopy as a pure Rust crate, `canopy-mosaic`, and moves the
font banners, the images, and big text onto it. It does not bind libchafa, and it copies no chafa
code or tables (Decisions). The first version covers truecolor symbol output. Palette modes,
dithering, and pixel graphics protocols are later work.

The plan has three parts:

- **Background**: how chafa works, read from its source (`d7d52e8`, October 2026) and its
  documentation.
- **Evidence**: what Canopy has today, and a spike. The spike measured the speed of exact search,
  and the gain of each symbol family on Canopy's own gym image and fonts.
- **Changes, Testing, and the Execution Plan**: the crate, the integration, how we prove it, and
  the order of work.

An external review by `gpt-5.6-sol` on 2026-10-06 shaped this revision. It led to integer scoring,
glyph support in layout, the image effects rule, the raster contract, and the stage order.

## Background: how chafa works

### The pipeline

1. **Size.** Chafa computes the output size in cells from the terminal's cell aspect, the font
   ratio, which is 1:2 by default.
2. **Scale.** Smolscale resamples the image to `cols × 8` by `rows × 8` matrix pixels, in linear
   light, composited over the background color. The scale is anisotropic: on a 1:2 cell, a matrix
   pixel is a 1:2 rectangle. Symbol outlines are drawn in the same squashed space, so the shapes
   still agree. Placement edges are rounded to cell boundaries.
3. **Preprocess.** With 16 colors or fewer only, chafa stretches the contrast from an intensity
   histogram. It crops 5, 10, or 20 percent at each end for 16, 8, or 2 colors. It also doubles the
   saturation for 16 and 8 colors.
4. **Dither.** In palette modes only, it can dither with a 16×16 Bayer matrix, a 64×64 blue noise
   texture, or serpentine Floyd–Steinberg. The dither works on grains of 1, 2, 4, or 8 matrix
   pixels, 4×4 by default. The base intensity is 0.1 for 256 colors, 0.25 for 16, 0.5 for 8, and
   1.0 for two.
5. **Color space.** Palette modes can match in DIN99d. Truecolor forces RGB, because chafa cannot
   convert DIN99d back to RGB.
6. **Match.** Each cell is matched on its own. Batches of rows run on threads.
7. **Wide symbols.** A ring buffer of four cells lets chafa try a double-width symbol over each
   pair of neighbors. It keeps the wide symbol if the sum of the two errors falls.
8. **Fill.** A featureless cell, such as a space or a cell whose pens are equal, can take a fill
   symbol. Chafa picks it by coverage alone, so that two palette colors mix toward the mean.
9. **Print.** The printer emits SGR, reuses attributes, and can use REP.

### Matching one cell

A symbol is a 64-bit mask over the 8×8 matrix (bit 63 is the top-left pixel). A cell shows a
symbol with two pens: the foreground on set bits and the background on clear bits.

For a fixed mask, the pens with the least squared error are the means of the pixels under each
part. Chafa's default extractor, `average`, uses these means. A `median` extractor exists, and
the man page says it gives crisper edges on clean images. A comment in its code calls one of its
steps "extremely slow" for "almost no difference". The error is the sum of squared RGB distances
between each pixel and its pen, with alpha ignored. The symbol with the least error wins.

The CLI work factor `-w 1` to `-w 9` maps to a library value from 0 to 1, and then to an integer
from 1 to 10. Chafa has two searches:

- **Exhaustive**, at `-w 7` to `-w 9`: compute the means and the error of every symbol, in two
  passes of 64 pixels each.
- **Fast**, at `-w 1` to `-w 6`:
  1. Take a contrasting pair: the darkest and brightest pixels on the channel with the widest
     range.
  2. Assign each pixel to the nearer of the pair. This gives a 64-bit silhouette.
  3. Compute the Hamming distance from the silhouette to every symbol mask with XOR and popcount.
     The inverse distance, `64 − d`, also counts, because swapped pens invert a mask.
  4. Keep the `k` nearest masks and evaluate them exactly. From `-w 1` to `-w 6`, `k` is 1, 1, 3,
     4, 5, and 6. The default, `-w 5`, keeps 5.

In the 16/8 mode, chafa evaluates the error with pens quantized to the palette. The other modes
quantize after the match. A uniform cell becomes a blank, and its unused foreground pen copies its
neighbor's, to save SGR.

### Symbols

- The built-in table holds hand-drawn 8×8 outlines for ASCII, Latin, box drawing, block elements,
  wedges, and Japanese kana (16×8, wide). Braille, sextants, and octants are generated in code.
- The 8×8 matrix forces approximations. Sextant rows split 3/2/3, because 8 is not a multiple of 3.
  A braille dot is 2×1 pixels.
- Tags select symbols with an ordered `+`/`-` syntax, for example `block+border+space-wide`. The
  default is `block+border+space-wide-inverted`. Since 1.8 the default stays with these families,
  because they render alike in most fonts. Ambiguous and ugly symbols are excluded unless a
  selector names them, and so are right-to-left scripts.
- `--glyph-file` imports glyphs through FreeType. Chafa probes pixel sizes until the 7th octile of
  the advances, ascents, and descents measures 8×8. It renders monochrome, scales to 8×8, sharpens
  with a 3×3 kernel (center 6, cross −1), and thresholds at half coverage. The import helps only
  if the terminal uses the same font.

### Where the quality comes from

1. **Shape search.** The symbol set is a dictionary of sub-cell edges. With about 200 distinct
   masks, a cell can put a horizontal edge at any quarter of its height, or a vertical edge at its
   middle. It can also put a corner at any octant, or a diagonal at several slopes.
2. **Least-squares pens.** In truecolor, each shape gets its exact best colors, so nothing is lost
   to a palette.
3. **Independent cells.** No cell depends on another, so the search is simple and parallel. The
   cost is that neighbors can swap pens, and seams can show. The author lists "spatial
   hysteresis" as an idea he has not built.
4. **Conservative defaults.** Legacy computing symbols depend on the font and the terminal, so
   chafa offers them only when asked.

The author tried 16×16 masks and found that the gain did not justify the complexity. He also
notes that quality metrics do not always agree with subjective quality.

## Evidence

### Canopy today

| Consumer | Pixels per cell | Glyphs | Colors |
| --- | --- | --- | --- |
| `ImageView` | 1×2 box samples from an `IntegralImage` | `▀` only | top and bottom samples, alpha over the `background` ground |
| `FontRenderer` | 8×8 or 8×16 coverage, averaged to 2×2 | 16 quadrants | `Coverage { fg, bg }` through `Render::put_covered` |
| `BigText` | 1×2 face pixels per cell | `▀ ▄ █` | the run's style, gradients at the cell center |
| charts | eighths, or 2×4 braille dots | eighths, braille | two tones per cell |

Other facts that shape the design:

- There is no shared sub-cell raster. Each consumer has its own.
- Canopy has no color depth detection and no downgrade. Every built-in theme is RGB, and crossterm
  emits colors as they are.
- Nothing in Canopy uses sextants (U+1FB00) or octants (U+1CD00). `unicode-width` gives both
  ranges width 1, so `TermBuf` accepts them.
- The screenshot rasterizer draws box drawing, block elements, braille, and triangles from
  geometry (`screenshot/glyphs.rs`, `draw`). Everything else comes from JetBrains Mono, which has
  no sextants or octants, so the gallery would show them as missing boxes.
- fh decodes previews to at most 256×256 (`IMAGE_VIEW_BOUNDS`, `view_pane.rs:49`). An 8×8 matrix
  over a 100×50 pane wants about 800×800 source pixels.
- `IntegralImage` keeps four `u64` tables, 32 bytes for each source pixel.
- In dev builds, fh optimizes `canopy` and `canopy-widgets` at level 2 and `image` at level 3.
  Canopy's own workspace optimizes nothing.

### Spike: the cost of exact search

The spike is in `tmp/mosaic-spike`: `cargo run --release`, and the `tiers`, `text`, and
`fontcheck` binaries. It matched 12,000 cells, a 200×60 view, against 334 masks: octants,
sextants, eighths, and diagonals. The run was single-threaded on an Apple M2 Max.

| Method | Time | Total error, chafa's metric |
| --- | --- | --- |
| Exhaustive, chafa style: truncated means, then an error pass | 755–865 ms | the reference |
| Exhaustive, closed form, masked sums | 210–224 ms | +0.0007% |
| Exhaustive, closed form, row subset-sum tables | 51–57 ms | +0.0007% |
| **Exhaustive, closed form, nibble subset-sum tables** | **35 ms** | **+0.0007%** |
| Fast, `k = 1` | 16–20 ms | +1.82% |
| Fast, `k = 5` (chafa's default) | 26 ms | +0.12% |
| Fast, `k = 8` | 31–35 ms | +0.05% |

The error column scores every method with chafa's metric: means truncated to integers. The
closed forms optimize real-valued means, so they score 0.0007% worse under that metric. The three
closed forms picked the same mask in every cell.

Two facts make exact search cheap:

- For a mask with `n₁` set pixels, let `S` be the sum of the cell's pixels and `Q` the sum of
  their squares. If `S₁` is the sum under the mask, and `S₀ = S − S₁`, the least squared error is
  `Q − |S₁|²/n₁ − |S₀|²/n₀`. Only `S₁` depends on the mask.
- Each run of 4 pixels has 16 subsets. A table of their sums takes 15 additions per run, and the
  three channels pack into one `u32` (4 × 255 < 1024). `S₁` for any mask is then 16 lookups.

So exact search costs about as much as the fast path at `k = 8`, and finds the optimum of its
objective. Chafa's default, `k = 5`, is a quarter cheaper and 0.12% worse. A debug build runs the
matcher 27 to 30 times slower than a release build.

The timing is evidence of feasibility, not a production budget. The raster was synthetic. The
spike scored in `f32`, which C4 replaces with integers. Its fast path counted a mask and its
inverse as two candidates and kept duplicates, so its `k` overstates the distinct masks scored.

### Spike: the gain of each symbol family

The `tiers` binary matched `.assets/shyness.jpg` at 120×40 cells. The `text` binary matched
"Canopy" in Tangerine at 6 rows and in Bungee at 4 rows. Each family adds to the one above it. The
reconstruction assumes an ideal terminal that draws each mask exactly.

The metric is PSNR after a Gaussian blur in display space (σ in pixels, at 8 pixels per column).
The blur stands in for viewing distance. A box downsample on the cell grid was tried first and
rejected: it favors quadrants, whose means it preserves exactly.

| Family | Distinct masks | Photo σ=1 | Photo σ=2 | Tangerine σ=1 | Bungee σ=1 |
| --- | --- | --- | --- | --- | --- |
| Half blocks (today's `ImageView`) | 2 | 21.42 | 25.88 | — | — |
| + quadrants (today's `FontRenderer`) | 8 | 22.67 | 27.55 | 21.06 | 17.85 |
| + eighths: all of U+2580–U+259F | 20 | 23.36 | 28.45 | 21.32 | 20.39 |
| + sextants | 48 | 23.92 | 29.24 | 22.09 | 20.97 |
| + octants | 166 | 24.51 | 29.93 | 22.63 | 21.50 |
| + diagonals | 196 | 24.70 | 30.09 | 23.64 | 22.41 |

The diagonals in the spike are half planes through the sextant lattice. They approximate the
Unicode smooth mosaics. In the crops (`tmp/mosaic-spike/out/crop_*.png`), half blocks blur the
branches away and the richest family keeps them. Block elements alone gain 1.9 dB on the photo
over today's half blocks. They gain 2.5 dB on Bungee over today's quadrants.

The spike has known limits:

- It used one photo and two fonts, with margins around each word.
- It resized in sRGB with a triangle filter, not the linear-light area filter of C6.
- Its diagonals approximate the smooth mosaics.
- It scored real-valued means in `f32`, and reconstructed ideal masks, not terminal glyphs.

It shows that the method works and the direction of the gains. It does not establish the
production budgets or the exact gain of `Full`. Stage 3 replaces it with a corpus and a
harness.

### Spike: what fonts draw

Chafa can take the terminal's font, so we asked whether Canopy should too. The `fontcheck` binary
rasterizes each mosaic character from a font, in the font's own cell: the advance of `M` by the
line height. It averages the ink into the 8×8 matrix, and `tools/compare.py` compares the result
with the shape that the Unicode name states.

| Font | Block elements | Sextants | Octants | Difference from the Unicode shape |
| --- | --- | --- | --- | --- |
| Iosevka Nerd Font Mono | 29/29 | 60/60 | 230/230 | 0.001 mean coverage, no matrix pixel flips |
| JetBrains Mono | 29/29 | 0/60 | 0/230 | 0.007 mean coverage, no flips |
| SF Mono | 29/29 | 0/60 | 0/230 | 0.12 mean coverage, up to 16 of 64 pixels flip, 13% of ink outside the cell |
| Menlo | 29/29 | 0/60 | 0/230 | 0.10 mean coverage, up to 16 of 64 pixels flip, 15% of ink outside the cell |

Fonts that draw these characters well draw the Unicode shapes exactly, so a font model adds
nothing for them. Where a font differs, the result depends on the cell box that the terminal
chooses: its line spacing, padding, and pixel snapping. The font file cannot tell us that box.
Many terminals also draw block elements from their own geometry and ignore the font. A font that
lacks sextants or octants gets them from a fallback font that Canopy cannot see.

## Changes

### Part A: The mosaic crate

#### C1: Crate and boundaries

Add `crates/canopy-mosaic`. It has no dependency on `canopy`, takes plain buffers, and returns
plain cells:

- It works with no terminal or application state, so it is easy to test, benchmark, and fuzz.
- It can be optimized at level 3 in the dev profiles of both workspaces, as fh does for `image`,
  without optimizing all of `canopy-widgets`. Unoptimized, the matcher is about 30 times slower.
- `canopy-widgets` depends on it for the font banner, images, big text, and the screenshot
  rasterizer.

The crate has three modules:

- `geometry`: symbol shapes in cell coordinates (C2).
- `symbols`: the families, tiers, and compiled symbol sets (C3).
- `matcher` and `raster`: matching (C4, C5) and raster preparation (C6).

#### C2: Symbol geometry

One table describes each supported character as a shape in unit cell coordinates, from 0 to 1 on
each axis. A shape is a union of rectangles and convex polygons:

- Block elements, U+2580–U+259F, except the three shades: halves, eighths, quadrants, and the
  upper and right eighths.
- Sextants, U+1FB00–U+1FB3B: thirds of the height, halves of the width.
- Octants, U+1CD00–U+1CDE5: quarters of the height, halves of the width.
- The 26 octant patterns that Unicode assigns to other characters. These are space, some block
  elements, and eight quarter blocks from Symbols for Legacy Computing and its Supplement. The
  quarter blocks are U+1FB82, U+1FB85, U+1FBE6, U+1FBE7, U+1CEA0, U+1CEA3, U+1CEA8, and U+1CEAB.
- Smooth mosaics, U+1FB3C–U+1FB6F: 44 diagonals and 8 triangles on the sextant lattice.
- One-eighth bars and quarter blocks, U+1FB70–U+1FB8B.

Each shape yields two rasterizations:

- `Shape::mask() -> Mask`: the 8×8 mask, sampled at pixel centers. Sextants sample at exact
  thirds, which gives chafa's 3/2/3 rows.
- `Shape::coverage_into(width, height, out)`: area coverage at any size, antialiased, written into
  a buffer that the caller owns. The screenshot rasterizer (C8) caches the result by character and
  cell size. The evaluation harness (T4) uses it too.

The geometry is the single source of truth for matching, screenshots, and evaluation. A display
that disagrees with the matcher then shows up as an error in the harness, not as a surprise in a
terminal.

#### C3: Symbol tiers

A family is a group of characters that renders alike, and a tier is a cumulative set of families:

| Tier | Families | Characters | Distinct masks |
| --- | --- | --- | --- |
| `Halves` | space, `█`, `▀`, `▄` | 4 | 2 |
| `Blocks` | space, and all of U+2580–U+259F except the shades | 30 | 20 |
| `Full` | `Blocks`, plus sextants, octants, smooth mosaics, and the U+1FB70 bars | about 400 | about 210 |

A compiled `SymbolSet` holds one entry for each pair of a mask and its inverse, because the two are
one symbol with the pens swapped:

- Each entry stores the mask, and the character for each orientation that has one.
- The entries are in a total order: family priority first, then a fixed order within the family.
  `Blocks` comes before `Full`, and within `Full` octants come before sextants, and sextants
  before diagonals. The glyph that more terminals draw well therefore wins a tie.
- The packed masks also serve the Hamming prefilter, if C5 lands.

`SymbolSet::tier(tier) -> &'static SymbolSet` compiles each built-in tier once, on first use, and
widgets share it. `SymbolSet::from_families` builds other combinations for tests and experiments.
Selector strings in chafa's style are not part of v1.

#### C4: The matcher

The matcher has one engine and three objectives. Each reads a raster view (C6) of `cols × 8` by
`rows × 8` matrix pixels, and writes one cell for each 8×8 block.

- **RGB** (images). Pixels are opaque sRGB `[u8; 3]`. The pens are the means of the two parts,
  rounded to `u8`. The output is `Cell { ch, fg: [u8; 3], bg: [u8; 3] }`.
- **Coverage** (antialiased text). Pixels are ink coverage from 0 to 255, and the objective is the
  RGB one on a single channel. The output pens are coverages, `Cell { ch, fg: u8, bg: u8 }`,
  which map to Canopy's `Coverage`.
- **Fixed pens** (crisp text). Each pen is 255 or 0. With ink on the set pixels of a mask, the
  error is `n₁ · 255² − 510 · S₁` plus a constant. With ink on the clear pixels, the same
  expression holds for the inverse. The output pens are `(255, 0)` or `(0, 255)`, so a shape whose
  character exists only in the inverse orientation still draws.

Coverage matching is exact only in an unquantized model. Canopy resolves gradients at the cell
center, so the style colors are constant within a cell, and `Coverage::apply` mixes them linearly.
Color error is then `|fg − bg|²` times coverage error. But `Color::mix` truncates each channel to
`u8`, so with low-contrast colors several coverages show the same color. The matcher stays
independent of style anyway: `FontLayout` stays cacheable, and gradients and motion still resolve
at emission. T2 measures the gap on emitted colors.

The rest of the engine:

- **Integer scores.** For each candidate, the matcher rounds the means to the `u8` pens that it
  would emit. It scores those pens with `Q − 2·p₁·S₁ + n₁·p₁² − 2·p₀·S₀ + n₀·p₀²` per channel.
  The scores are exact in `i64`. A table of reciprocals for `n` from 1 to 64 replaces the
  division, and an exhaustive test proves that it rounds as the division does. An empty or full
  mask scores the single pen that it uses.
- **Why not `f32`.** The spike scored the closed form in `f32`. A cell's `Q` reaches about 12.5
  million, where one `f32` step is about 1. Candidates whose errors differ by less than that tie
  falsely.
- **Nibble tables.** Each entry packs three 10-bit lanes. The matcher unpacks the lanes before it
  adds the 16 entries of a mask, because a packed sum would carry between lanes.
- **Orientation.** The output records which orientation of the entry won, and its character.
  Where both characters exist, the matcher picks the one whose background covers at least half
  the cell. A uniform cell becomes a space with `fg = bg`.
- **Determinism.** Ties go to the set order of C3, and then to the orientation with the larger
  background. The output depends only on the input and the options, in debug and release builds.
- **Threads.** Cells are independent. Stage 2 starts single-threaded. It adds scoped threads over
  row bands only if a benchmark misses its budget (T6). Results must not depend on the thread
  count.
- **API.** A `Matcher` owns its scratch buffers, so a widget can reuse it from frame to frame. The
  output methods clear their buffer and write exactly `cols × rows` cells. `RenderLimits` bounds
  that size in Canopy.

```rust
let set = SymbolSet::tier(Tier::Full);
let raster = RasterView::new(&pixels, cols * 8, rows * 8, stride)?;
let mut matcher = Matcher::new(Options { effort: Effort::Exact });
matcher.rgb_into(&raster, set, &mut cells)?; // cols × rows cells
```

#### C5: Fast search, only on evidence

`Effort::Exact` is the default. `Effort::Fast(k)` lands only if T6 shows that exact search misses
its budget after the threads of C4. It is chafa's fast path, with three rules:

- **Distinct candidates.** A candidate is a set entry, not a mask and its inverse counted twice.
  Its distance is the smaller of the Hamming distances to the mask and to the inverse.
- **Nested order.** Candidates sort by a total key: distance, then the set order. The first `k`
  candidates are a prefix of the first `k + 1`, so a larger `k` never does worse.
- **Exact scores.** Candidates take the integer scores of C4, from masked sums. With few
  candidates, building the tables costs more than it saves.

Try refining chafa's contrasting pair with one Lloyd step of 2-means, and keep it if T4 shows a
gain at small `k`.

#### C6: Raster preparation

`RasterView<'a, P>` is the matcher's input: a borrowed slice, a width and a height that are
multiples of 8, and a stride in pixels. Its constructor checks the slice length and returns an
error on a mismatch.

`raster::resample` builds the raster of an image for a view:

- **Input.** Unassociated sRGB RGBA8 and its size, a view of `cols × rows` cells, the zoom, and
  the pan offset. The zoom keeps the meaning that `ImageView` gives it: display subpixels per image
  pixel. A display subpixel is today's half-cell sample, one column by half a row.
- **Coordinates.** A matrix pixel is an eighth of a subpixel wide and a quarter of one tall. Its
  footprint is the rectangle that it covers in image coordinates. A footprint can span many source
  pixels, or part of one.
- **Filter.** Each matrix pixel is the area-weighted mean of the source pixels under its
  footprint, with fractional weights at the edges. It averages in linear light, with premultiplied
  alpha, in `f32`. A 256-entry table converts sRGB to linear. Transparent source pixels add
  nothing, whatever their RGB.
- **Edges.** A footprint that leaves the image covers transparency there. The image never extends
  past its own edge.
- **Output.** Premultiplied linear RGBA, stored as `u16` for each channel, `cols × 8` by
  `rows × 8`.

`raster::composite` lays each cell's pixels over that cell's ground color, in linear light. It
writes opaque sRGB `[u8; 3]` through a 4096-entry table back from linear. Compositing happens
before matching, because a mask must see the image edge against the ground that it will show.
Edges inside a cell then need no special care, so chafa's rounding to cell boundaries is not
needed. Today's `ImageView` mixes in sRGB, so partly transparent edges change slightly.

Fonts need no resampling. The font renderer already rasterizes into a coverage buffer (C9).

### Part B: Canopy integration

#### C7: Glyph support

Which symbols are safe depends on the terminal and its font, and Canopy cannot detect it reliably.
Add a setting to Canopy core:

- `canopy::render::Glyphs` is `Full`, the default, or `Blocks`.
- **Layout reads it.** `MeasureConstraints` gains a `glyphs` field, which the layout driver fills
  from the setting. The driver keys its measure cache on the constraints, so the cache keys on
  the setting too. `BigText` then measures and renders with the same ladder.
- **Rendering reads it.** `Render::glyphs()` returns the same value.
- **Changes.** `Canopy::set_glyphs` and a Luau `canopy.set_glyphs("blocks")` change it. A change
  invalidates layout and renders a new frame. Automation can then test both tiers headless.
- `canopy-widgets` maps `Glyphs` to a mosaic tier.

Canopy reads no environment variables for it. An application such as fh exposes it in its
configuration. Detection by `TERM_PROGRAM` is later work, for terminals known to draw the
mosaics from geometry.

A person can judge a terminal better than a font file can. The `imgview` gym gains a glyph chart:
each family in seamless patterns, such as runs of octants that should join into smooth bars and
diagonals. Each row has border characters, and an ASCII sentinel follows each `Full` character.
A fallback glyph of the wrong width then shows as a broken border. A user looks at the chart in
their own terminal and picks the tier that draws cleanly.

#### C8: Screenshot glyphs

Extend `screenshot::glyphs::draw` so that it draws every character of every tier from
`Shape::coverage_into`, cached by character and cell size. This covers sextants, octants, the
quarter blocks, smooth mosaics, and the U+1FB70 bars. Route U+2580–U+259F through the geometry
too, so the matcher and the screenshots share one definition. Box drawing and braille keep their
own code. The gallery's `missing_glyphs` list must stay empty.

#### C9: Font banners

Replace `GlyphRamp` in `FontRenderer` with the matcher:

- **Raster.** The coverage buffer is 8×8 per cell, or 8×16 with `tall_cells`. With `tall_cells`,
  each vertical pair averages into one matrix pixel.
- **Objectives.** Soft text uses the coverage objective. `crisp` uses fixed pens, which replaces
  the threshold at 128. A crisp cell has the coverages `(255, 0)` or `(0, 255)`.
- **Output.** `FontCell { ch, fg_coverage, bg_coverage }` keeps its shape, and `FontBanner` still
  draws with `put_covered`. Gradients, themes, and motion at emission therefore keep working.
- **Cache.** The `FontLayout` cache key gains the tier.
- **Effects.** Bold, italic, underline, overline, strike, and dim still act on the coverage buffer
  before matching.

#### C10: Images

Rebuild `ImageView::render_cells` on the mosaic:

- **Memory.** Keep the decoded RGBA thumbnail, 4 bytes for each pixel, and remove
  `IntegralImage`, 32 bytes for each pixel. The decode limits stay as they are:
  `PREVIEW_MAX_EDGE`, `PREVIEW_MAX_PIXELS`, and `PREVIEW_DECODE_BYTES`.
- **Ground.** Resolve each cell's ground from the `background` style without the effect stack.
  `Render::resolve_style` applies effects, so add `Render::resolve_base_style`, which does not.
- **Match.** Resample, composite over the grounds, and match.
- **Cache.** Keep the last cells with their key. The key is the view size, zoom, and pan, the
  tier and the effort, the image generation, and the per-cell grounds. Compare the stored ground
  colors exactly, not through a hash, so a collision cannot return wrong cells. The effect stack
  is not part of the key.
- **Effects.** After the cache, apply the effect stack to each pen alone. Each pen is the
  foreground and the background of a style, through `Render::apply_effects`, resolved at the cell.
  The result's foreground becomes the pen.

The effects rule treats the whole image alike, whatever the orientation of each mask. A modal dim
then costs no new match. For effects that act linearly on color, such as a dim, the result equals
today's, where the effect acts before compositing.

One behavior changes on purpose. An effect that replaces only the foreground used to change only
the top half of each cell. It now changes the whole image, and
`image_effects_can_replace_solid_paint_with_a_gradient` changes to match.

#### C11: Big text

Bitmap faces gain a second pixel grid. On the octant grid, a face pixel is half a column by a
quarter row. It is square on a 1:2 cell, and half the size of a half-block pixel. The octant
grid needs the `Full` tier.

- **Ladder.** The ladder becomes face, scale, and grid, and `Resolved` gains the grid. Octant
  scale `s` matches half-block scale `s / 2`, so the ladder gains the half steps. `Fit` keeps its
  rule: the largest cap height that fits, and then the existing tie breaks.
- **Measure.** `BigText::measure` reads `glyphs` from its constraints (C7), so measurement and
  rendering resolve the same ladder.
- **Cells.** `Placement::cell` reads a 2×4 pattern from the `Column` bitmasks. It maps the pattern
  to its character through `canopy_mosaic::symbols::octant`, a direct lookup. This needs no
  matcher, because every 2×4 pattern has a character.
- **Runs.** Where two runs meet in one cell, the run with more ink pixels takes the cell. On equal
  ink, the run that comes first in the text takes it.
- **Motion.** The roll animation moves in quarter rows.
- **Blocks tier.** Under `Blocks`, `BigText` keeps the half grid only. An octant pattern drawn
  with quadrants would deform the glyphs.

#### C12: fh

- Raise `IMAGE_VIEW_BOUNDS` to 1024×1024, which is `PREVIEW_MAX_EDGE`. The retained thumbnail is
  then at most 4 MiB. Today a 256×256 thumbnail with its integral tables takes about 2.4 MB. The
  matrix raster of a 200×60 view adds 3 MB while it renders.
- Add `[profile.dev.package.canopy-mosaic] opt-level = 3` to fh and to Canopy.
- Expose the glyph setting in fh's configuration.
- Update `docs/architecture.md`, which states the 256×256 bound.

#### C13: Docs and API

- `docs/styles.md`: rewrite Images, Big text, and Font banners for the tiers, and state the new
  effects rule for images.
- `docs/architecture.md`: add a section on mosaics. It covers the matcher, the tiers, glyph
  support in layout and rendering, and that screenshots draw tier symbols from the same geometry.
- Refresh the API captures with `ncode api` and review the diff, `canopy-mosaic` included.

## Testing

### T1: Geometry against Unicode

Unicode names state the geometry of most mosaic characters: `BLOCK SEXTANT-235`,
`BLOCK OCTANT-2345678`, `LOWER THREE EIGHTHS BLOCK`, `LEFT HALF UPPER ONE QUARTER BLOCK`. The
smooth mosaics name the ends of their diagonals, for example `LOWER LEFT BLOCK DIAGONAL LOWER
MIDDLE LEFT TO LOWER CENTRE`.

- **Fixture.** An xtask extracts the names of the covered ranges from UnicodeData.txt into
  `crates/canopy-mosaic/testdata/unicode.txt`. It needs version 16.0 or later, which added
  octants. The fixture is checked in, and `--check` reports drift, as `cargo xtask big-text` does.
- **Provenance.** The fixture's header records the Unicode version, the source URL, the xtask
  command, and the Unicode data license notice.
- **Names.** A test parses each name into cells of the sextant, octant, or eighth lattice, or into
  the ends of a diagonal. It then compares the result with `Shape::mask` and `Shape::coverage_into`.
- **Families.**
  - Each family has the expected number of characters.
  - Each tier's masks are distinct after the inverse fold.
  - Each character has width 1 and passes `validate_cell_character`.
  - The 256 octant patterns map to 256 distinct characters.

### T2: Matcher and raster properties

Proptest, with a direct reference that computes the rounded pens and their integer errors by
brute force:

- **Scores.** The integer score equals the reference for random cells and every mask. Cells of
  all 255, the largest lane values, are among the cases.
- **Rounding.** The reciprocal table rounds as the division does, for every `n` and every sum.
- **Search.** Exact search returns the reference's least error, with the tie order of C4.
- **Round trip.** Paint any mask of a tier in any two colors, and match it. The error is zero,
  and the reconstruction equals the input.
- **Order.** A superset tier never has a higher exact error. If C5 lands, `Fast(k)` error is at
  least the exact error, and does not rise as `k` grows.
- **Uniform cells.** A uniform cell, including a cell that compositing leaves all ground, gives a
  space with `fg = bg`.
- **Coverage.** Matching `c` and `255 − c` gives the same mask with the pens swapped. Fixed pens
  equal the reference in both orientations.
- **Emitted color.** Over low-contrast, reversed, and equal color pairs, compare each coverage
  match with the best displayed color after `Coverage::apply`. Record the gap, and fail if it
  exceeds a threshold set when the test lands.
- **Builds and threads.** Debug and release builds give the same cells, and so does every thread
  count.
- **Raster.** Known vectors check linear-light averaging, premultiplied edges, transparent pixels
  with color, fractional pan, magnification, minification, and footprints at the image edge.

### T3: Golden cells

Small rasters drawn in code: a gradient, a disc edge, lines at 15° steps, a checkerboard, and "Ag"
in two fonts. Each is matched in each tier and objective, and dumped as rows of characters plus a
table of pens. The goldens use the `UPDATE_GOLDEN` convention of `themes.golden`. They make each
change visible in review.

### T4: Quality harness

Speed and correctness are not enough. The harness proves that the output looks better, and that
later changes do not undo it. It runs on the production matcher and resampler, so it lands after
C4 and C6.

- **Corpus.** Generated images: a zone plate, gradients, thin lines at many angles, discs, and text
  in Bungee, Fira Mono, and Tangerine. Real images: `.assets/shyness.jpg`, plus two or three more
  under permissive licenses, kept small.
- **Ideal display.** Reconstruct each cell from its mask and pens, and compare with the raster.
  Use PSNR after a display-space Gaussian blur at σ = 1 and σ = 2. A box downsample is biased
  (Evidence).
- **Real display.** Rasterize the cells with the screenshot module (C8) at its real cell size.
  Compare with the source scaled to the same pixels. This checks that the model and the display
  agree.
- **Baselines.** Commit the metrics, rounded to 0.01 dB, for each image, tier, and objective, and
  for `Fast(k)` if it lands. Include today's renderers, the half-block `ImageView` and the quadrant
  `FontRenderer`, so the gain stays on record. `UPDATE_GOLDEN` rewrites the baselines after an
  improvement.
- **Rules.** Exact search in each tier must equal or beat today's renderer on every image. No
  metric may drop more than 0.05 dB below its baseline. `Fast(k)` is judged against exact search,
  not against today's renderers.
- **Contact sheets.** `cargo xtask mosaic` writes the source, each tier, and the real display side
  by side to `tmp/mosaic/`, for a person to judge. Metrics and eyes can disagree.

### T5: Widgets, gyms, smoke, and gallery

- **Glyph setting.** After `set_glyphs`, layout runs again, and `BigText` measurement and
  rendering agree. Check unbounded, `MaxRows`, and `Exact` sizes in both tiers.
- **`ImageView`.**
  - Tier families appear, alpha shows the ground, and zoom and pan keep their mapping.
  - A dim and the gradient effect act on the whole image, in every mask orientation.
  - A second render with no change matches nothing, and neither does a modal dim. A change of
    ground does. A test hook counts matches.
- **`FontBanner`.** The rows of a known word in each tier, soft and crisp.
- **`BigText`.** Digits on the octant grid, as the existing `DIGITS` test does on the half grid.
  Two runs with equal ink in one cell.
- **Gyms.** `imgview` gains the glyph chart and tier toggles. `fontgym` and `biggym` gain a tier
  toggle. Their Luau scripts switch tiers with `canopy.set_glyphs` and check for octant characters
  in the capture.
- **Gallery.** Gallery shots of each gym in both tiers.
- **Every tier draws.** A unit test checks that every character of every tier draws from geometry
  in the screenshot module.
- **fh.** fh's smoke suite runs as it does today. The `Blocks` default keeps its captures stable
  until fh opts in.

### T6: Benchmarks and budgets

Criterion benchmarks, built by `cargo xtask bench-check`, land with the code that they measure:

| Stage | Benchmark | Budget |
| --- | --- | --- |
| 2 | matching 200×60 cells, `Blocks`, exact, RGB and coverage | 10 ms |
| 2 | matching 200×60 cells, `Full`, exact, RGB and coverage | 40 ms |
| 2 | resampling a 1024×683 source to a 200×60 view | 10 ms |
| 5 | an `ImageView` render with a cold cache | the sum of the two above |
| 5 | an `ImageView` render with a warm cache | 2× a `TermBuf` fill of the same area |
| 5 | an `ImageView` render after a ground change | the cold budget |

- The budgets are for one release-build thread on an Apple M2 Max. If `Full` exact search misses
  its budget, add the threads of C4 in Stage 2, and consider C5 in Stage 3.
- In the dev profiles of both workspaces, with the opt level of C12, matching stays within 2× of
  release.
- The benchmarks also count allocations. A warm render allocates nothing beyond its output.
- The fh path is measured too: a 1024-pixel thumbnail in a full-screen pane.

### T7: Cross-check against chafa (optional)

A manual xtask mode runs only when `chafa` is installed. Install it with `brew install chafa`.

- **Run.** It runs `chafa --format symbols --symbols <tier> --work 9 --colors full --color-space rgb
  --preprocess off` at the same cell size.
- **Compare.** It parses the characters and SGR colors into cells and reconstructs them with our
  geometry. It then reports both metrics against our raster.
- **Expectation.** Chafa truncates its means, maps tiers to its own selectors, and resamples
  differently. The two searches therefore minimize similar errors, not the same one. Expect close
  metrics, and look into a gap of more than 0.5 dB.
- **Scope.** This is a sanity check, not a CI gate. Do not install chafa or its LGPL code in the
  build.

## Decisions

- **Pure Rust, written from the algorithm.** Chafa is LGPL-3.0-or-later and Canopy is MIT. We read
  chafa to understand it. We copy no code and no outline tables. The geometry comes from Unicode
  names, and T1 checks it against them. Later dithering can use the blue noise texture chafa
  uses, by Christoph Peters under CC0.
- **8×8 matrix in a `u64`.** Popcount, packing, and the nibble tables depend on it. The matrix
  resolves the block elements, octants, and bars exactly, and samples the thirds of sextants and
  the diagonals. Chafa's author found 16×16 not worth its cost. Fonts still rasterize finer and
  average down (C9).
- **Mean pens, rounded and scored as emitted.** Means give the least squared error, and they are
  chafa's default. The matcher scores the rounded `u8` pens in integers, so it optimizes what the
  terminal shows, and floating point cannot tie candidates falsely.
- **Exact search by default.** Integer scores make it exact, and the tables make it about as cheap
  as a fast search with 8 candidates. Exactness also lets T2's properties hold without
  tolerances. Fast search lands only if the benchmarks need it.
- **Coverage out for text, colors out for images.** Text must stay in the style system, so that
  themes, gradients, and motion at emission still apply. Images have no style, so the matcher
  gives their colors directly. The cost for text is a small, measured error from `u8` rounding at
  low contrast.
- **Glyph support is read by layout.** `BigText` sizes depend on it, and `measure` sees only its
  constraints. A field there keys the measure cache with no new context. The alternative was a
  grid setting on each widget. It would put a terminal capability into every call site.
- **Image effects act after matching, on each pen alike.** Mosaic pens are not the halves of a
  cell, so an effect cannot act by position. Acting after the cache keeps a modal dim free.
- **`Full` by default, `Blocks` by opt-out.** Visual tests on real terminals favored `Full`
  everywhere, so it became the default. A font that lacks sextants and octants falls back to a
  font that Canopy cannot see, so an application lets the user set `Blocks` instead. The `Blocks`
  tier alone gains 1.9 dB on the photo.
- **Unicode geometry, not the user's font.** The font spike shows that fonts with these
  characters draw the Unicode shapes exactly. Where a font differs, the terminal's cell box
  decides the result, and the font file does not state it. A font setting would also go stale
  over SSH and after a font change. The glyph chart (C7) checks the terminal itself.
- **One geometry for matching, screenshots, and evaluation.** One definition removes the chance of
  silent disagreement, and makes the harness's real-display check meaningful.
- **Truecolor only in v1.** Canopy has no color depth detection, and its themes are RGB. Palette
  matching and dithering belong with a future depth capability.
- **A separate crate.** No Canopy dependency, its own opt level in dev builds, and benchmarks and
  property tests that run alone.
- **No braille, box drawing, letters, or shades in image tiers.** Their strokes depend on the font,
  which breaks the model. The harness can test them later.
- **No wide symbols and no fill pass.** In truecolor, two narrow cells with their own pens beat one
  wide symbol, as chafa's author observes. Fill matters only with a palette.
- **Edges need no cell alignment.** Masks split an image edge from its ground inside the cell, so
  a zoomed or panned image needs no rounding.

## Later Work

- **Palette output.** A color depth capability in Canopy, then 256, 240, and 16 color matching with
  palette-aware error. Add ordered, blue noise, and serpentine diffusion dithers by grain, and the
  preprocessing that chafa applies at 16 colors and fewer.
- **Font-derived symbols.** Letters, box drawing, and braille look different in each font. A
  `SymbolSet::from_font` could build their masks from the font that the terminal uses, as chafa's
  `--glyph-file` does. Adopt it only if the harness shows a gain. Chafa's author notes that it
  helps only when the terminal uses that exact font.
- **Pixel graphics.** Kitty, sixel, and iTerm2 output. This needs its own design, because image
  placements sit outside the cell diff.
- **Perceptual color.** Test matching in OKLab, and coverage mixing in linear light rather than in
  sRGB (`Coverage::apply`), with the harness.
- **Style-aware text.** If T2's emitted-color gap matters in practice, match text against resolved
  style colors, at the cost of `FontLayout` caching.
- **Fractional masks.** Weight boundary pixels in sextants and diagonals by their real coverage.
  The pens then solve a 2×2 least-squares system per channel.
- **Seams.** Penalize pen swaps between neighbors, chafa's unbuilt "spatial hysteresis".
- **Animation.** GIF and WebP frames, with a cache per frame.
- **More families.** Test braille, box drawing, and shades in the harness, and keep any that win.
- **Vector big text.** Let `BigText` lay out vector faces through the matcher. This could merge
  `FontBanner` into it.

## Execution Plan

### Stage 1: Geometry and tiers

- [x] C1: add `crates/canopy-mosaic` to the workspace, with workspace lints, `[lints] workspace =
      true`, and an API capture. Add `[profile.dev.package.canopy-mosaic] opt-level = 3` to the
      root `Cargo.toml` of Canopy and of fh.
- [x] C2: add `geometry` with `Shape`, `Shape::mask`, and `Shape::coverage_into`, for block
      elements, sextants, octants, the eight quarter blocks, smooth mosaics, and the U+1FB70 bars.
- [x] C3: add `symbols` with the families, `Tier`, `SymbolSet` and its total order, the inverse
      fold, `SymbolSet::tier` behind a `OnceLock`, `SymbolSet::from_families`, and
      `symbols::octant`.
- [x] T1: add the xtask that writes `crates/canopy-mosaic/testdata/unicode.txt` with its
      provenance header and `--check`, the name parser, and the family tests.
- [x] C8: draw every tier character from geometry in `canopy-widgets/src/screenshot/glyphs.rs`,
      cached by character and cell size, and route U+2580–U+259F through the geometry.
- [x] T5: add the test that every character of every tier draws from geometry in the screenshot
      module.

Validation: all 406 characters match their Unicode names (`tests/unicode.rs`), and the gallery
lists no missing glyphs.

### Stage 2: Matcher and raster preparation

- [x] C4: add `RasterView` with its checked constructor, and `Cell`. The matcher is three free
      functions, `rgb_into`, `coverage_into`, and `fixed_into`: each cell's tables live on the
      stack, so there is no `Matcher` to hold scratch.
- [x] C4: add the RGB, coverage, and fixed-pen objectives, with both crisp orientations.
- [x] C4: add the nibble tables, the reciprocal table, integer scoring of the rounded pens,
      orientation, and the tie order. The table packs three 16-bit lanes in a `u64`, wide enough
      that 16 entries cannot carry, so no unpacking happens before the sum.
- [x] C6: add `raster::resample`, with linear light, premultiplied alpha, fractional footprints,
      and transparent edges, and `raster::composite`.
- [x] T2: add the matcher properties against the brute-force reference, the exhaustive rounding
      test, and the raster known vectors.
- [ ] T3: golden cells. The quality baselines and the property tests cover the same ground, so
      this was not built.
- [x] T6: add `canopy-mosaic/benches` for matching each tier and for resampling. Threads were not
      needed.

Validation: release, one thread, 200×60 cells: `Blocks` exact matching 4.2 ms, `Full` 21 ms,
resampling and compositing a 1024×683 source 10 ms. A debug build runs about 30 times slower
without the dev opt level.

### Stage 3: Quality harness

- [x] T4: add the corpus, the ideal display metrics, and the committed baselines in
      `tests/quality.golden`, with the rule that every tier equals or beats today's renderers.
      The real display check is a screenshot test in `canopy-widgets`.
- [x] T4: add `cargo xtask mosaic`, which runs the `sheets` example of `canopy-mosaic` and writes
      contact sheets to `tmp/mosaic/`. The xtask stays a leaf, so it runs the example rather
      than linking the crate.
- [ ] T7: the optional chafa cross-check. Not built: chafa is not installed here.
- [x] C5: decided against fast search. Exact search meets its budgets in every tier.

Validation: `Full` beats today's renderers on every item at σ = 1. It gains 3.0 dB on the
photo, 5.0 on the zone plate, 5.8 on the lines, and 8.7 on the disc. It gains 2.1 to 4.4
on text. The real
display agrees with the model above 30 dB, and keeps the order of the tiers. A person reviewed
the contact sheets.

### Stage 4: Glyph support and font banners

- [x] C7: add `canopy::render::Glyphs`, the `glyphs` field of `MeasureConstraints` filled by the
      layout driver, `Render::glyphs`, `Canopy::set_glyphs` with layout invalidation, and the Luau
      `canopy.glyphs` and `canopy.set_glyphs`.
- [x] C7: add the glyph chart, with borders and sentinels, to the `imgview` gym.
- [x] C9: move `FontRenderer` in `canopy-widgets/src/font.rs` onto the matcher, add the tier to
      the `FontLayout` cache key, and remove `GlyphRamp` and `coverage_weights`.
- [ ] T2: the emitted-color test for coverage matching, with its threshold. Not built.
- [x] T5: add the layout tests for the setting, the `fontgym` tier toggle with its Luau script,
      and the gallery shots.

Validation: the font tests and the font gym's script pass in every tier, and the harness shows
the gain for text.

### Stage 5: Images

- [x] C10: add `Render::resolve_base_style`.
- [x] C10: rebuild `ImageView::render_cells` on resample, composite, and match, with the cache, an
      exact comparison of grounds, and the effects rule. Remove `IntegralImage`. The view reuses
      its scratch buffers, so a render that hits the cache allocates no new buffers.
- [x] C10: update the effect tests for the effects rule.
- [x] C12: raise `IMAGE_VIEW_BOUNDS` in `fh/crates/canopy-fileselect/src/view_pane.rs` to
      1024×1024.
- [x] T5: add the `ImageView` tests for tiers, alpha, effects in every orientation, and match
      counts. Add the tier toggle to the `imgview` gym and its Luau script.
- [x] T6: add the `ImageView` render benchmarks, cold and warm, beside a buffer fill of the same
      area.
- [ ] T6: count allocations in the benchmarks. Not built.

Validation: a cold render of 200×60 cells takes 26–32 ms. A warm render takes 1–2 ms for the
whole frame. A bare buffer fill takes 0.26 ms, so the guessed budget of 2× missed. The
render stays well inside a frame. The matrix rasters of a 200×60 view take about 8 MB.

### Stage 6: Big text, fh, and docs

- [x] C11: add the octant grid to `big_text/layout.rs`, the grid in `Resolved`, measurement from
      the constraints, `Placement::cell` through `symbols::octant`, and the run rule for shared
      cells.
- [x] T5: add the big text tests for the octant grid and run majority, and the `biggym` glyphs
      setting with its Luau script.
- [x] C12: set the glyphs in fh's default configuration with `canopy.set_glyphs`, and update fh's
      `docs/architecture.md`.
- [x] C13: update `docs/styles.md`, `docs/architecture.md`, and `docs/scripting.md`, refresh the
      API captures with `ncode api`, and review the diff.

Validation: Canopy's 1484 tests, fh's 445, `cargo xtask smoke`, the gallery, and both tidy runs
pass.

## Implementation notes

- A fourth tier, `Quadrants`, matches today's `FontRenderer`, so the gyms compare the old and
  new looks. Each gym's toggle steps from the terminal's glyphs through every tier and back.
- `Shape::mask` sets a pixel when the shape covers at least half of it. For rectangles this
  equals sampling at pixel centers, and for polygons it settles the pixels on an edge.
- Screenshots snap the rectangles of a shape to whole pixels, as terminals draw blocks, and
  antialias its polygons.
- On an even octant scale, `BigText` draws exactly as half blocks draw, so a fit takes octants
  only where an odd scale gives larger capitals.
