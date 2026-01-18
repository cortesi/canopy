# Font Rendering Improvement Plan

This plan targets the TTF rasterization pipeline in `crates/canopy-widgets/src/font.rs`
and aims to improve outline strength and legibility at terminal sizes. It stages
experiments based on known rendering techniques (stem darkening, gamma-correct
coverage, subpixel sampling, hinting, and SDF/MSDF) so we can pick clear wins.

1. Stage One: Baseline and instrumentation

Capture a stable baseline and add small diagnostics so changes are measurable.

1. [ ] Add a fontgym debug toggle to render a fixed grid of sizes (8/12/16/20)
   and capture screenshots for comparison (fontgym example).
2. [ ] Add a lightweight coverage histogram or average-coverage report per glyph
   to compare stroke weight before/after changes (font renderer).
3. [ ] Add a small snapshot test fixture for a known string with a fixed font,
   height, and ramp to detect regressions.

2. Stage Two: Coverage + outline strengthening

Improve perceived weight and edge clarity in the current coverage pipeline.

1. [ ] Prototype gamma-correct coverage (linearize, blend, re-encode) and compare
   against current coverage mapping in fontgym.
2. [ ] Prototype size-dependent embolden/stem darkening by dilating the coverage
   buffer or applying a small outline expansion before downsampling.
3. [ ] Test "edge boost" (slight coverage curve) to increase contrast on thin
   strokes without filling counters.

3. Stage Three: Subpixel and aspect-aware sampling

Explore sharper horizontal detail and better sampling for terminal aspect ratios.

1. [ ] Add an aspect-aware supersampling option that biases more samples in X
   than Y for typical terminal cell geometry.
2. [ ] Prototype a subpixel-style horizontal sampling mode that uses per-channel
   coverage internally and collapses to grayscale (no color fringing) to see if
   edges appear sharper.

4. Stage Four: Hinting and layout improvements

Evaluate whether better grid fitting and layout improves small-size legibility.

1. [ ] Investigate hinting support in available Rust font stacks (fontdue vs
   freetype/harfbuzz) and assess integration cost.
2. [ ] Prototype kerning-aware layout for small strings to reduce uneven spacing.

5. Stage Five: Alternative representations (SDF/MSDF)

Check if distance-field rendering produces clearer outlines at low resolution.

1. [ ] Prototype MSDF generation for a small glyph set (offline or on-the-fly)
   and map signed distance to the current ramp.
2. [ ] Compare SDF/MSDF rendering against supersampled coverage for a few sizes
   to decide if it is a straight win.

6. Stage Six: Selection + integration

Decide on the best improvements and wire them into the renderer.

1. [ ] Pick the top 1-2 techniques based on clarity and cost, and integrate them
   behind a `FontRenderer` option (no env vars).
2. [ ] Add tests, update docs, and clean up experiments.

## References

- https://freetype.org/freetype2/docs/reference/ft2-properties.html
- https://freetype.org/freetype2/docs/reference/ft2-outline_processing.html
- https://freetype.org/freetype2/docs/reference/ft2-lcd_rendering.html
- https://www.microsoft.com/en-us/research/project/cleartype/
- https://learn.microsoft.com/en-us/typography/truetype/hinting
- https://github.com/Chlumsky/msdfgen
