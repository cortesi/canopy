use std::{
    cmp::{max, min},
    collections::HashMap,
    sync::Arc,
};

use canopy::{
    error::{Error, Result},
    geom::Size,
    layout::Align,
};
use canopy_mosaic::{RasterView, SymbolSet, Tier, coverage_into, fixed_into};
use fontdue::{Font as FontdueFont, FontSettings, LineMetrics, Metrics};

/// Supersampling scale factor used to rasterize glyphs before downsampling.
const COVERAGE_SCALE: u32 = 8;

/// What the height of a font layout fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// The line box of the font, its ascent and descent, so lines of any
    /// text stand alike.
    #[default]
    Line,
    /// The ink of the text, from its highest mark to its lowest, so a short
    /// word fills the height.
    Ink,
}

/// Layout configuration for font layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutOptions {
    /// Horizontal alignment within the target canvas.
    pub h_align: Align,
    /// Vertical alignment within the target canvas.
    pub v_align: Align,
    /// What the height fits.
    pub fit: Fit,
    /// Draw each quadrant of a cell fully or not at all, instead of blending
    /// partial coverage into the colors of the cell. Crisp text has hard
    /// edges and no shades between the text and the ground.
    pub crisp: bool,
    /// Treat a cell as twice as tall as it is wide, as terminal fonts draw
    /// it, so glyphs keep their proportions. Otherwise a cell is square, and
    /// a glyph draws at twice its height.
    pub tall_cells: bool,
}

/// Rendering effects applied to font output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FontEffects {
    /// Thicken strokes by adding extra coverage.
    pub bold: bool,
    /// Slant glyphs to the right.
    pub italic: bool,
    /// Draw an underline through the glyphs.
    pub underline: bool,
    /// Reduce contrast by dimming coverage.
    pub dim: bool,
    /// Draw an overline through the glyphs.
    pub overline: bool,
    /// Draw a strike-through line through the glyphs.
    pub strike: bool,
}

/// Glyph raster data rendered to pixel coverage.
#[derive(Debug, Clone)]
struct Glyph {
    /// Rasterized coverage mask, row-major, 0-255 per pixel.
    pub bitmap: Vec<u8>,
    /// Glyph width in pixels.
    pub width: u32,
    /// Glyph height in pixels.
    pub height: u32,
    /// Horizontal bearing to the left of the glyph origin, in pixels.
    pub bearing_left: i32,
    /// Vertical bearing to the bottom of the glyph relative to the baseline, in
    /// pixels.
    pub bearing_bottom: i32,
    /// Horizontal advance width in pixels.
    pub advance: f32,
}

/// Rasterized font data for terminal rendering.
#[derive(Clone)]
pub struct Font {
    /// Parsed font data.
    font: FontdueFont,
}

impl Font {
    /// Load a font from in-memory bytes.
    pub fn from_bytes(data: impl AsRef<[u8]>) -> Result<Self> {
        let font = FontdueFont::from_bytes(data.as_ref(), FontSettings::default())
            .map_err(|e| Error::Invalid(format!("font loading failed: {e}")))?;
        Ok(Self { font })
    }

    /// Return the font name, if provided in metadata.
    pub fn name(&self) -> Option<&str> {
        self.font.name()
    }
}

/// Placement information for a glyph on a line.
#[derive(Debug, Clone)]
struct GlyphPlacement {
    /// X offset in pixels.
    x: i32,
    /// Y offset in pixels.
    y: i32,
    /// Cached glyph raster.
    glyph: Arc<Glyph>,
}

/// Raster layout data for a single line.
#[derive(Debug, Clone)]
struct LineLayout {
    /// Glyph placements for this line, in pixel coordinates.
    placements: Vec<GlyphPlacement>,
    /// Line width in pixels.
    width: u32,
    /// Line height in pixels.
    height: u32,
    /// Baseline offset from the top of the line, in pixels.
    baseline_offset: i32,
}

/// Cache key for rasterized glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphCacheKey {
    /// Source character for the glyph.
    ch: char,
    /// Pixel size for rasterization.
    px_bits: u32,
}

/// A rendered font cell with coverage weights for foreground and background.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FontCell {
    /// Rendered character for this cell.
    pub ch: char,
    /// Foreground coverage weight (0-255).
    pub fg_coverage: u8,
    /// Background coverage weight (0-255).
    pub bg_coverage: u8,
}

/// Cached layout for rasterized font text.
#[derive(Debug, Clone)]
pub(crate) struct FontLayout {
    /// Target canvas size.
    pub size: Size,
    /// Size of the rendered content before clipping.
    pub content_size: Size,
    /// Rendered cell data for each row.
    pub cells: Vec<Vec<FontCell>>,
}

/// Renderer that converts fonts into terminal text.
pub struct FontRenderer {
    /// Font used for rasterization.
    font: Font,
    /// Fallback glyph for missing characters.
    fallback: char,
    /// Cached glyph rasters.
    cache: HashMap<GlyphCacheKey, Arc<Glyph>>,
}

impl FontRenderer {
    /// Create a renderer for the provided font.
    pub fn new(font: Font) -> Self {
        Self {
            font,
            fallback: '?',
            cache: HashMap::new(),
        }
    }

    /// Render text into a layout that fits within the target canvas, in the
    /// symbols of `tier`.
    pub(crate) fn layout(
        &mut self,
        text: &str,
        size: Size,
        options: LayoutOptions,
        effects: FontEffects,
        tier: Tier,
    ) -> FontLayout {
        if size.w == 0 || size.h == 0 {
            return FontLayout {
                size,
                content_size: Size::new(0, 0),
                cells: Vec::new(),
            };
        }

        let (scale_x, scale_y) = cell_scale(options);
        let lines: Vec<&str> = text.split('\n').collect();
        let Placed {
            ascent,
            descent,
            line_advance,
            lines: layout_lines,
            width_px: content_width_px,
            height_px: content_height_px,
        } = self.place(&lines, size, options, effects);

        let content_width = content_width_px.div_ceil(scale_x);
        let content_height = content_height_px.div_ceil(scale_y);
        let offset_x = align_offset(content_width, size.w, options.h_align) as i32;
        let offset_y = align_offset(content_height, size.h, options.v_align) as i32;

        let buffer_width = size.w.saturating_mul(scale_x);
        let buffer_height = size.h.saturating_mul(scale_y);
        let mut buffer = vec![0u8; (buffer_width * buffer_height) as usize];
        let offset_x_px = offset_x * scale_x as i32;
        let offset_y_px = offset_y * scale_y as i32;

        for (line_idx, line) in layout_lines.iter().enumerate() {
            let line_y = offset_y_px + line_idx as i32 * line_advance;
            for placement in &line.placements {
                let glyph = &placement.glyph;
                if glyph.width == 0 || glyph.height == 0 {
                    continue;
                }
                let base_x = offset_x_px + placement.x;
                let base_y = line_y + placement.y;
                let glyph_width = glyph.width as i32;
                let glyph_height = glyph.height as i32;
                for row in 0..glyph_height {
                    let y = base_y + row;
                    if y < 0 || y >= buffer_height as i32 {
                        continue;
                    }
                    let row_start = row as usize * glyph.width as usize;
                    for col in 0..glyph_width {
                        let coverage = glyph.bitmap[row_start + col as usize];
                        if coverage == 0 {
                            continue;
                        }
                        let x = base_x + col;
                        if x < 0 || x >= buffer_width as i32 {
                            continue;
                        }
                        let idx = y as usize * buffer_width as usize + x as usize;
                        buffer[idx] = buffer[idx].max(coverage);
                    }
                }
            }
        }

        if effects.italic {
            buffer = apply_italic(
                &buffer,
                buffer_width,
                buffer_height,
                &layout_lines,
                line_advance,
                offset_y_px,
            );
        }

        if effects.bold {
            apply_bold(&mut buffer, buffer_width, buffer_height);
        }

        if effects.underline || effects.overline || effects.strike {
            let mut line_ctx = EffectLineContext {
                buffer: &mut buffer,
                width: buffer_width,
                height: buffer_height,
                lines: &layout_lines,
                line_advance,
                offset_x_px,
                offset_y_px,
                ascent,
                descent,
                effects,
                thickness: max((scale_y / 4) as i32, 1),
            };
            draw_effect_lines(&mut line_ctx);
        }

        if effects.dim {
            apply_dim(&mut buffer, 0.6);
        }

        let cells = match_cells(&buffer, size, scale_y, options.crisp, tier);

        FontLayout {
            size,
            content_size: Size::new(content_width, content_height),
            cells,
        }
    }

    /// Return the size of `text` laid out in `size`, in cells, without
    /// drawing it.
    pub(crate) fn content_size(
        &mut self,
        text: &str,
        size: Size,
        options: LayoutOptions,
        effects: FontEffects,
    ) -> Size {
        let (scale_x, scale_y) = cell_scale(options);
        let lines: Vec<&str> = text.split('\n').collect();
        let placed = self.place(&lines, size, options, effects);
        Size::new(
            placed.width_px.div_ceil(scale_x),
            placed.height_px.div_ceil(scale_y),
        )
    }

    /// Place `lines` at the scale that fits the height of `size`, shrunk
    /// until the text fits its width too, so wide text shrinks rather than
    /// clips.
    fn place(
        &mut self,
        lines: &[&str],
        size: Size,
        options: LayoutOptions,
        effects: FontEffects,
    ) -> Placed {
        /// Most shrinks before the text settles; each one aims at the room
        /// left, so rounding is all that remains after the first.
        const SHRINKS: usize = 4;
        let (scale_x, scale_y) = cell_scale(options);
        let mut px = match options.fit {
            Fit::Line => self.scale_for_height(size.h, lines.len(), scale_y),
            Fit::Ink => self.scale_for_ink(lines, size.h, scale_y),
        };
        let room = size.w.saturating_mul(scale_x);
        let mut placed = self.place_at(lines, px, options, effects);
        for _ in 0..SHRINKS {
            if placed.width_px <= room || placed.width_px == 0 {
                break;
            }
            px = (px * room as f32 / placed.width_px as f32 * 0.99).max(1.0);
            placed = self.place_at(lines, px, options, effects);
        }
        placed
    }

    /// Place `lines` at a scale of `px`.
    fn place_at(
        &mut self,
        lines: &[&str],
        px: f32,
        options: LayoutOptions,
        effects: FontEffects,
    ) -> Placed {
        let metrics = self.line_metrics(px);
        let ascent = metrics.ascent.round() as i32;
        let descent = metrics.descent.abs().round() as i32;
        let baseline = ascent;
        // A line that fits its ink is as tall as its ink, so the line box
        // of the font adds nothing to it.
        let (line_ascent, line_descent, mut line_advance) = match options.fit {
            Fit::Line => (
                ascent,
                descent,
                metrics.new_line_size.max(1.0).round() as i32,
            ),
            Fit::Ink => (0, 0, 0),
        };

        let mut layout_lines = Vec::with_capacity(lines.len());
        let mut width_px = 0u32;
        let mut max_line_height_px = 0u32;
        for line in lines {
            let layout = self.layout_line(line, px, baseline, line_ascent, line_descent);
            width_px = width_px.max(layout.width);
            max_line_height_px = max_line_height_px.max(layout.height);
            layout_lines.push(layout);
        }
        if max_line_height_px > 0 {
            line_advance = line_advance.max(max_line_height_px as i32);
        }

        let line_advance_px = max(line_advance, 0) as u32;
        let height_px = line_advance_px.saturating_mul(layout_lines.len() as u32);
        if width_px > 0 {
            if effects.italic {
                let slant = italic_slant(line_advance).max(0) as u32;
                width_px = width_px.saturating_add(slant);
            }
            if effects.bold {
                width_px = width_px.saturating_add(1);
            }
        }
        Placed {
            ascent,
            descent,
            line_advance,
            lines: layout_lines,
            width_px,
            height_px,
        }
    }

    /// Rasterize a single line into glyph placements.
    fn layout_line(
        &mut self,
        text: &str,
        px: f32,
        baseline: i32,
        ascent: i32,
        descent: i32,
    ) -> LineLayout {
        let mut placements = Vec::new();
        let mut cursor_x: f32 = 0.0;

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for ch in text.chars() {
            let glyph = self.glyph_for(ch, px);
            let advance = glyph.advance;
            let x = cursor_x.round() as i32 + glyph.bearing_left;
            let y = baseline - (glyph.bearing_bottom + glyph.height as i32);

            if glyph.width > 0 && glyph.height > 0 {
                min_x = min(min_x, x);
                min_y = min(min_y, y);
                max_x = max(max_x, x + glyph.width as i32);
                max_y = max(max_y, y + glyph.height as i32);
            }

            placements.push(GlyphPlacement {
                x,
                y,
                glyph: Arc::clone(&glyph),
            });
            cursor_x += advance;
        }

        let cursor_end = cursor_x.round() as i32;
        if min_x == i32::MAX {
            min_x = 0;
            min_y = 0;
            max_x = cursor_end;
            max_y = 0;
        }

        let line_top = min(min_y, baseline - ascent);
        let line_bottom = max(max_y, baseline + descent);

        let width = max(max_x, cursor_end).saturating_sub(min_x);
        let height = max(line_bottom - line_top, 0);
        let x_shift = -min_x;
        let y_shift = -line_top;
        let baseline_offset = baseline + y_shift;

        for placement in &mut placements {
            placement.x += x_shift;
            placement.y += y_shift;
        }

        LineLayout {
            placements,
            width: max(width, 0) as u32,
            height: max(height, 0) as u32,
            baseline_offset,
        }
    }

    /// Return a cached glyph raster for a character and pixel size.
    fn glyph_for(&mut self, ch: char, px: f32) -> Arc<Glyph> {
        let ch = if self.font.font.has_glyph(ch) {
            ch
        } else {
            self.fallback
        };
        let key = GlyphCacheKey {
            ch,
            px_bits: px.to_bits(),
        };
        if let Some(cached) = self.cache.get(&key) {
            return Arc::clone(cached);
        }
        let (metrics, bitmap) = self.font.font.rasterize(ch, px);
        let glyph = Arc::new(rasterize_glyph(metrics, bitmap));
        self.cache.insert(key, Arc::clone(&glyph));
        glyph
    }

    /// Compute a scale in pixels that fits the target height.
    fn scale_for_height(&self, height: u32, lines: usize, sample_scale: u32) -> f32 {
        let line_count = max(lines, 1) as f32;
        let target_height = max(height, 1) as f32 * sample_scale as f32;
        let base = self.line_metrics(1.0);
        let per_px = base.new_line_size.max(1.0);
        let px = target_height / (per_px * line_count);
        px.max(1.0)
    }

    /// Compute a scale in pixels that fits the ink of `lines` to the target
    /// height, each line as tall as the tallest ink of any line. Text with
    /// no ink fits the line box instead.
    fn scale_for_ink(&self, lines: &[&str], height: u32, sample_scale: u32) -> f32 {
        /// Scale at which the ink is measured. Ink scales with the font, so
        /// any scale gives the same proportion.
        const PROBE: f32 = 64.0;
        let ink = lines
            .iter()
            .map(|line| {
                let (top, bottom) = line
                    .chars()
                    .map(|ch| self.font.font.metrics(ch, PROBE))
                    .filter(|metrics| metrics.height > 0)
                    .fold((f32::MIN, f32::MAX), |(top, bottom), metrics| {
                        let low = metrics.ymin as f32;
                        (top.max(low + metrics.height as f32), bottom.min(low))
                    });
                (top - bottom).max(0.0)
            })
            .fold(0.0, f32::max);
        if ink <= 0.0 {
            return self.scale_for_height(height, lines.len(), sample_scale);
        }
        let target_height = max(height, 1) as f32 * sample_scale as f32;
        let px = target_height / (ink / PROBE * max(lines.len(), 1) as f32);
        px.max(1.0)
    }

    /// Resolve line metrics at the requested scale.
    fn line_metrics(&self, px: f32) -> LineMetrics {
        self.font
            .font
            .horizontal_line_metrics(px)
            .unwrap_or_else(|| self.fallback_line_metrics(px))
    }

    /// Fallback line metrics when the font lacks line data.
    fn fallback_line_metrics(&self, px: f32) -> LineMetrics {
        let metrics = self.font.font.metrics(self.fallback, px);
        LineMetrics {
            ascent: metrics.height as f32,
            descent: 0.0,
            line_gap: 0.0,
            new_line_size: metrics.height as f32,
        }
    }
}

/// Return how many pixels wide and tall each cell rasterizes.
fn cell_scale(options: LayoutOptions) -> (u32, u32) {
    let scale_y = if options.tall_cells {
        COVERAGE_SCALE * 2
    } else {
        COVERAGE_SCALE
    };
    (COVERAGE_SCALE, scale_y)
}

/// Lines of text placed at one scale, before they are drawn.
struct Placed {
    /// Ascent of the font in pixels.
    ascent: i32,
    /// Descent of the font in pixels.
    descent: i32,
    /// Advance between lines in pixels.
    line_advance: i32,
    /// Each line, placed.
    lines: Vec<LineLayout>,
    /// Width of the text in pixels, effects included.
    width_px: u32,
    /// Height of the text in pixels.
    height_px: u32,
}

/// Compute an offset for aligning content inside a span.
pub(crate) fn align_offset(content: u32, available: u32, align: Align) -> u32 {
    if available <= content {
        return 0;
    }
    match align {
        Align::Start => 0,
        Align::Center => (available - content) / 2,
        Align::End => available - content,
    }
}

/// Convert a font raster into a glyph.
fn rasterize_glyph(metrics: Metrics, bitmap: Vec<u8>) -> Glyph {
    Glyph {
        bitmap,
        width: metrics.width as u32,
        height: metrics.height as u32,
        bearing_left: metrics.xmin,
        bearing_bottom: metrics.ymin,
        advance: metrics.advance_width,
    }
}

/// Match the coverage buffer of a layout, `scale_y` pixel rows to each cell
/// row, to the symbols of `tier`. Crisp text draws each pixel at full ink or
/// bare ground.
fn match_cells(
    buffer: &[u8],
    size: Size,
    scale_y: u32,
    crisp: bool,
    tier: Tier,
) -> Vec<Vec<FontCell>> {
    let width = size.w as usize * COVERAGE_SCALE as usize;
    let height = size.h as usize * COVERAGE_SCALE as usize;
    // Each matrix pixel averages the buffer rows it spans.
    let rows_per_pixel = (scale_y / COVERAGE_SCALE).max(1) as usize;
    let mut matrix = vec![0u8; width * height];
    for (y, row) in matrix.chunks_mut(width).enumerate() {
        for (x, pixel) in row.iter_mut().enumerate() {
            let sum: u32 = (0..rows_per_pixel)
                .map(|dy| u32::from(buffer[(y * rows_per_pixel + dy) * width + x]))
                .sum();
            *pixel = ((sum + rows_per_pixel as u32 / 2) / rows_per_pixel as u32) as u8;
        }
    }
    let Ok(raster) = RasterView::new(&matrix, width, height, width) else {
        return Vec::new();
    };
    let set = SymbolSet::tier(tier);
    let mut matched = Vec::new();
    if crisp {
        fixed_into(&raster, set, &mut matched);
    } else {
        coverage_into(&raster, set, &mut matched);
    }
    matched
        .chunks(size.w as usize)
        .map(|row| {
            row.iter()
                .map(|cell| FontCell {
                    ch: cell.ch,
                    fg_coverage: cell.fg,
                    bg_coverage: cell.bg,
                })
                .collect()
        })
        .collect()
}

/// Compute the horizontal slant applied per line for italic rendering.
fn italic_slant(line_height: i32) -> i32 {
    if line_height <= 1 {
        return 0;
    }
    let slant = (line_height as f32 * 0.2).round() as i32;
    slant.max(1)
}

/// Shear the buffer to approximate italic glyphs.
fn apply_italic(
    buffer: &[u8],
    width: u32,
    height: u32,
    lines: &[LineLayout],
    line_advance: i32,
    offset_y_px: i32,
) -> Vec<u8> {
    let mut output = vec![0u8; buffer.len()];
    if width == 0 || height == 0 {
        return output;
    }
    let line_height = line_advance.max(1);
    let slant = italic_slant(line_height);
    let span = line_height.saturating_sub(1);

    for (line_idx, _line) in lines.iter().enumerate() {
        let line_top = offset_y_px + line_idx as i32 * line_advance;
        for row in 0..line_height {
            let y = line_top + row;
            if y < 0 || y >= height as i32 {
                continue;
            }
            let shift = if span == 0 {
                0
            } else {
                (span - row) * slant / span
            };
            let row_start = y as usize * width as usize;
            for x in 0..width {
                let value = buffer[row_start + x as usize];
                if value == 0 {
                    continue;
                }
                let dest_x = x as i32 + shift;
                if dest_x < 0 || dest_x >= width as i32 {
                    continue;
                }
                let dest_idx = row_start + dest_x as usize;
                output[dest_idx] = output[dest_idx].max(value);
            }
        }
    }

    output
}

/// Thicken strokes by copying coverage into the next column.
fn apply_bold(buffer: &mut [u8], width: u32, height: u32) {
    if width < 2 || height == 0 {
        return;
    }
    for y in 0..height {
        let row_start = y as usize * width as usize;
        for x in (0..(width - 1)).rev() {
            let idx = row_start + x as usize;
            let value = buffer[idx];
            if value == 0 {
                continue;
            }
            let next_idx = row_start + x as usize + 1;
            buffer[next_idx] = buffer[next_idx].max(value);
        }
    }
}

/// Scale coverage values to simulate dim text.
fn apply_dim(buffer: &mut [u8], factor: f32) {
    if !(0.0..1.0).contains(&factor) {
        return;
    }
    for value in buffer.iter_mut() {
        let scaled = (*value as f32 * factor).round() as u32;
        *value = scaled.min(u8::MAX as u32) as u8;
    }
}

/// Shared line metrics for drawing underline/overline/strike effects.
struct EffectLineContext<'a> {
    /// Target coverage buffer.
    buffer: &'a mut [u8],
    /// Buffer width in pixels.
    width: u32,
    /// Buffer height in pixels.
    height: u32,
    /// Line layout data.
    lines: &'a [LineLayout],
    /// Line advance in pixels.
    line_advance: i32,
    /// Horizontal offset in pixels.
    offset_x_px: i32,
    /// Vertical offset in pixels.
    offset_y_px: i32,
    /// Ascent in pixels.
    ascent: i32,
    /// Descent in pixels.
    descent: i32,
    /// Active font effects.
    effects: FontEffects,
    /// Thickness of each line in pixels.
    thickness: i32,
}

/// Draw underline, overline, and strike-through lines into the buffer.
fn draw_effect_lines(ctx: &mut EffectLineContext<'_>) {
    if ctx.width == 0 || ctx.height == 0 {
        return;
    }
    let line_height = ctx.line_advance.max(1);
    let thickness = ctx.thickness;
    let slant = if ctx.effects.italic {
        italic_slant(line_height)
    } else {
        0
    };
    let underline_offset = max(ctx.descent / 2, 1);
    let strike_offset = ((ctx.ascent as f32) * 0.4).round() as i32;

    for (line_idx, line) in ctx.lines.iter().enumerate() {
        if line.width == 0 {
            continue;
        }
        let line_top = ctx.offset_y_px + line_idx as i32 * ctx.line_advance;
        let baseline_y = line_top + line.baseline_offset;
        let mut x_start = ctx.offset_x_px;
        let mut x_end = ctx.offset_x_px + line.width as i32 + slant;
        if ctx.effects.bold {
            x_end = x_end.saturating_add(1);
        }
        if x_end <= x_start {
            continue;
        }
        if x_start < 0 {
            x_start = 0;
        }
        if x_end > ctx.width as i32 {
            x_end = ctx.width as i32;
        }

        if ctx.effects.overline {
            let y = baseline_y - ctx.ascent;
            draw_centered_line(
                ctx.buffer, ctx.width, ctx.height, y, thickness, x_start, x_end,
            );
        }
        if ctx.effects.underline {
            let y = baseline_y + underline_offset;
            draw_centered_line(
                ctx.buffer, ctx.width, ctx.height, y, thickness, x_start, x_end,
            );
        }
        if ctx.effects.strike {
            let y = baseline_y - strike_offset;
            draw_centered_line(
                ctx.buffer, ctx.width, ctx.height, y, thickness, x_start, x_end,
            );
        }
    }
}

/// Draw a horizontal line centered on the specified row.
fn draw_centered_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    center_y: i32,
    thickness: i32,
    x_start: i32,
    x_end: i32,
) {
    if thickness <= 0 || x_end <= x_start {
        return;
    }
    let half = thickness / 2;
    let start_y = center_y - half;
    let end_y = start_y + thickness;
    for y in start_y..end_y {
        if y < 0 || y >= height as i32 {
            continue;
        }
        let row_start = y as usize * width as usize;
        for x in x_start..x_end {
            if x < 0 || x >= width as i32 {
                continue;
            }
            let idx = row_start + x as usize;
            buffer[idx] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use canopy::geom::Size;

    use super::*;

    const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/Bungee-Regular.ttf");

    fn test_font() -> Font {
        Font::from_bytes(TEST_FONT).expect("font loads")
    }

    #[test]
    fn layout_sizes_match_target() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Size::new(24, 8);
        let layout = renderer.layout(
            "Hello",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
            Tier::Blocks,
        );
        assert_eq!(layout.size, size);
        assert_eq!(layout.cells.len(), size.h as usize);
        assert!(layout.content_size.w > 0);
    }

    #[test]
    fn layout_clips_overflow() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Size::new(8, 4);
        let layout = renderer.layout(
            "Overflow",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
            Tier::Blocks,
        );
        assert_eq!(layout.cells.len(), size.h as usize);
        assert_eq!(layout.cells[0].len(), size.w as usize);
        assert!(layout.content_size.w >= size.w);
    }

    #[test]
    fn layout_handles_multiline() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Size::new(24, 8);
        let layout = renderer.layout(
            "Hi\nThere",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
            Tier::Blocks,
        );
        let top_half = layout.cells[..4].iter().any(|row| {
            row.iter()
                .any(|cell| cell.fg_coverage > 0 || cell.bg_coverage > 0)
        });
        let bottom_half = layout.cells[4..].iter().any(|row| {
            row.iter()
                .any(|cell| cell.fg_coverage > 0 || cell.bg_coverage > 0)
        });
        assert!(top_half);
        assert!(bottom_half);
    }

    #[test]
    fn missing_glyphs_use_fallback() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Size::new(24, 8);
        let missing = renderer.layout(
            "\u{10ffff}",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
            Tier::Blocks,
        );
        let fallback = renderer.layout(
            "?",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
            Tier::Blocks,
        );
        assert_eq!(missing.cells, fallback.cells);
    }
}
