use std::{
    cmp::{max, min},
    collections::HashMap,
    io::Read,
    str::FromStr,
    sync::Arc,
};

use canopy::{geom::Expanse, layout::Align};
use fontdue::{Font as FontdueFont, FontSettings, LineMetrics, Metrics};

use crate::error::{Error, Result};

/// Supersampling scale factor used to rasterize glyphs before downsampling.
const COVERAGE_SCALE: u32 = 8;

/// Policy for handling content overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Clip glyphs that exceed the target bounds.
    Clip,
}

/// Alignment and overflow configuration for font layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutOptions {
    /// Horizontal alignment within the target canvas.
    pub h_align: Align,
    /// Vertical alignment within the target canvas.
    pub v_align: Align,
    /// Overflow handling policy.
    pub overflow: OverflowPolicy,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            h_align: Align::Start,
            v_align: Align::Start,
            overflow: OverflowPolicy::Clip,
        }
    }
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
pub struct Glyph {
    /// Rasterized coverage mask, row-major, 0-255 per pixel.
    pub bitmap: Vec<u8>,
    /// Glyph width in pixels.
    pub width: u32,
    /// Glyph height in pixels.
    pub height: u32,
    /// Horizontal bearing to the left of the glyph origin, in pixels.
    pub bearing_left: i32,
    /// Horizontal bearing to the right of the glyph advance, in pixels.
    pub bearing_right: i32,
    /// Vertical bearing to the bottom of the glyph relative to the baseline, in pixels.
    pub bearing_bottom: i32,
    /// Horizontal advance width in pixels.
    pub advance: f32,
}

/// Coverage mask for a glyph, ordered as top-left, top-right, bottom-left, bottom-right.
#[derive(Debug, Clone, Copy)]
struct GlyphSample {
    /// Glyph character.
    ch: char,
    /// Coverage mask for each quadrant, 0-255.
    mask: [u8; 4],
}

impl GlyphSample {
    /// Build a sample with uniform coverage across quadrants.
    fn uniform(ch: char, coverage: u8) -> Self {
        Self {
            ch,
            mask: [coverage; 4],
        }
    }

    /// Build a sample with an explicit mask.
    fn mask(ch: char, mask: [u8; 4]) -> Self {
        Self { ch, mask }
    }
}

/// Selected glyph choice with its mask.
#[derive(Debug, Clone, Copy)]
struct GlyphChoice {
    /// Chosen character.
    ch: char,
    /// Mask for the chosen glyph.
    mask: [u8; 4],
}

/// A glyph ramp used to convert coverage regions into terminal glyphs.
#[derive(Debug, Clone)]
pub struct GlyphRamp {
    /// Candidate glyph samples used for coverage matching.
    glyphs: Vec<GlyphSample>,
}

impl GlyphRamp {
    /// Default ASCII ramp.
    pub fn ascii() -> Self {
        Self::from_chars(" .:-=+*#%@").expect("ascii ramp is non-empty")
    }

    /// Nerd Font ramp using private-use glyphs.
    pub fn nerd_font() -> Self {
        let glyphs = [
            ' ', '\u{f10c}', '\u{f111}', '\u{f0c8}', '\u{f0c8}', '\u{f0c8}', '\u{f0c8}',
            '\u{f0c8}', '\u{f0c8}', '\u{f0c8}',
        ];
        Self::from_glyphs(glyphs).expect("nerd font ramp is non-empty")
    }

    /// Block-element ramp that matches 2x2 quadrant coverage.
    pub fn blocks() -> Self {
        let on = 255;
        let off = 0;
        let glyphs = vec![
            GlyphSample::mask(' ', [off, off, off, off]),
            GlyphSample::mask('▘', [on, off, off, off]),
            GlyphSample::mask('▝', [off, on, off, off]),
            GlyphSample::mask('▀', [on, on, off, off]),
            GlyphSample::mask('▖', [off, off, on, off]),
            GlyphSample::mask('▌', [on, off, on, off]),
            GlyphSample::mask('▞', [off, on, on, off]),
            GlyphSample::mask('▛', [on, on, on, off]),
            GlyphSample::mask('▗', [off, off, off, on]),
            GlyphSample::mask('▚', [on, off, off, on]),
            GlyphSample::mask('▐', [off, on, off, on]),
            GlyphSample::mask('▜', [on, on, off, on]),
            GlyphSample::mask('▄', [off, off, on, on]),
            GlyphSample::mask('▙', [on, off, on, on]),
            GlyphSample::mask('▟', [off, on, on, on]),
            GlyphSample::mask('█', [on, on, on, on]),
        ];
        Self { glyphs }
    }

    /// Construct a ramp from a set of characters.
    pub fn from_chars(chars: impl AsRef<str>) -> Result<Self> {
        let glyphs: Vec<char> = chars.as_ref().chars().collect();
        Self::from_glyphs(glyphs)
    }

    /// Construct a ramp from explicit glyph characters.
    pub fn from_glyphs(glyphs: impl IntoIterator<Item = char>) -> Result<Self> {
        let glyphs: Vec<char> = glyphs.into_iter().collect();
        if glyphs.is_empty() {
            return Err(Error::EmptyGlyphRamp);
        }
        let len = glyphs.len();
        let samples = if len == 1 {
            vec![GlyphSample::uniform(glyphs[0], 255)]
        } else {
            glyphs
                .into_iter()
                .enumerate()
                .map(|(idx, ch)| {
                    let coverage = ((idx as u32 * 255) / (len.saturating_sub(1) as u32)) as u8;
                    GlyphSample::uniform(ch, coverage)
                })
                .collect()
        };
        Ok(Self { glyphs: samples })
    }

    /// Convert per-quadrant coverage into a glyph choice.
    fn sample(&self, coverage: [u8; 4]) -> GlyphChoice {
        let mut best = GlyphChoice {
            ch: ' ',
            mask: [0; 4],
        };
        let mut best_error = u32::MAX;
        for sample in &self.glyphs {
            let mut error = 0u32;
            for (value, expected) in coverage.iter().zip(sample.mask.iter()) {
                let diff = value.abs_diff(*expected) as u32;
                error = error.saturating_add(diff.saturating_mul(diff));
            }
            if error < best_error {
                best_error = error;
                best = GlyphChoice {
                    ch: sample.ch,
                    mask: sample.mask,
                };
            }
        }
        best
    }
}

/// Rasterized font data for terminal rendering.
#[derive(Clone)]
pub struct Font {
    /// Parsed font data.
    font: FontdueFont,
    /// Extra spacing added after each glyph.
    spacing: f32,
}

impl Font {
    /// Load a font from in-memory bytes.
    pub fn from_bytes(data: impl AsRef<[u8]>) -> Result<Self> {
        let font = FontdueFont::from_bytes(data.as_ref(), FontSettings::default())
            .map_err(Error::FontLoad)?;
        Ok(Self { font, spacing: 0.0 })
    }

    /// Load a font from a reader.
    pub fn from_reader(mut reader: impl Read) -> Result<Self> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Self::from_bytes(buf)
    }

    /// Parse an ASCII-art font payload.
    pub fn from_ascii_art(_contents: &str) -> Result<Self> {
        Err(Error::UnsupportedFormat("ascii-art"))
    }

    /// Adjust spacing added after each glyph.
    pub fn with_spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Return the font name, if provided in metadata.
    pub fn name(&self) -> Option<&str> {
        self.font.name()
    }
}

impl FromStr for Font {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_ascii_art(s)
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
pub struct FontCell {
    /// Rendered character for this cell.
    pub ch: char,
    /// Foreground coverage weight (0-255).
    pub fg_coverage: u8,
    /// Background coverage weight (0-255).
    pub bg_coverage: u8,
}

/// Cached layout for rasterized font text.
#[derive(Debug, Clone)]
pub struct FontLayout {
    /// Target canvas size.
    pub size: Expanse,
    /// Size of the rendered content before clipping.
    pub content_size: Expanse,
    /// Rendered cell data for each row.
    pub cells: Vec<Vec<FontCell>>,
}

/// Renderer that converts fonts into terminal text.
pub struct FontRenderer {
    /// Font used for rasterization.
    font: Font,
    /// Glyph ramp for coverage matching.
    ramp: GlyphRamp,
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
            ramp: GlyphRamp::blocks(),
            fallback: '?',
            cache: HashMap::new(),
        }
    }

    /// Configure the glyph ramp for this renderer.
    pub fn with_ramp(mut self, ramp: GlyphRamp) -> Self {
        self.ramp = ramp;
        self
    }

    /// Configure the fallback glyph used for missing characters.
    pub fn with_fallback(mut self, fallback: char) -> Self {
        self.fallback = fallback;
        self.cache.clear();
        self
    }

    /// Render text into a layout that fits within the target canvas.
    pub fn layout(
        &mut self,
        text: &str,
        size: Expanse,
        options: LayoutOptions,
        effects: FontEffects,
    ) -> FontLayout {
        if size.w == 0 || size.h == 0 {
            return FontLayout {
                size,
                content_size: Expanse::new(0, 0),
                cells: Vec::new(),
            };
        }

        let lines: Vec<&str> = text.split('\n').collect();
        let line_count = max(lines.len(), 1);
        let px = self.scale_for_height(size.h, line_count, COVERAGE_SCALE);
        let metrics = self.line_metrics(px);
        let ascent = metrics.ascent.round() as i32;
        let descent = metrics.descent.abs().round() as i32;
        let baseline = ascent;
        let mut line_advance = metrics.new_line_size.max(1.0).round() as i32;

        let mut layout_lines = Vec::with_capacity(lines.len());
        let mut content_width_px = 0u32;
        let mut max_line_height_px = 0u32;

        for line in lines {
            let layout = self.layout_line(line, px, baseline, ascent, descent);
            content_width_px = content_width_px.max(layout.width);
            max_line_height_px = max_line_height_px.max(layout.height);
            layout_lines.push(layout);
        }

        if max_line_height_px > 0 {
            line_advance = line_advance.max(max_line_height_px as i32);
        }

        let line_advance_px = max(line_advance, 0) as u32;
        let content_height_px = line_advance_px.saturating_mul(layout_lines.len() as u32);
        if content_width_px > 0 {
            if effects.italic {
                let slant = italic_slant(line_advance).max(0) as u32;
                content_width_px = content_width_px.saturating_add(slant);
            }
            if effects.bold {
                content_width_px = content_width_px.saturating_add(1);
            }
        }

        let content_width = div_ceil(content_width_px, COVERAGE_SCALE);
        let content_height = div_ceil(content_height_px, COVERAGE_SCALE);
        let offset_x = align_offset(content_width, size.w, options.h_align) as i32;
        let offset_y = align_offset(content_height, size.h, options.v_align) as i32;

        let buffer_width = size.w.saturating_mul(COVERAGE_SCALE);
        let buffer_height = size.h.saturating_mul(COVERAGE_SCALE);
        let mut buffer = vec![0u8; (buffer_width * buffer_height) as usize];
        let offset_x_px = offset_x * COVERAGE_SCALE as i32;
        let offset_y_px = offset_y * COVERAGE_SCALE as i32;

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
            };
            draw_effect_lines(&mut line_ctx);
        }

        if effects.dim {
            apply_dim(&mut buffer, 0.6);
        }

        let mut cells = Vec::with_capacity(size.h as usize);
        let quad = (COVERAGE_SCALE / 2).max(1);
        let quad_area = quad.saturating_mul(quad);
        for cell_y in 0..size.h {
            let mut row_cells = Vec::with_capacity(size.w as usize);
            let start_y = cell_y * COVERAGE_SCALE;
            for cell_x in 0..size.w {
                let start_x = cell_x * COVERAGE_SCALE;
                let mut sums = [0u32; 4];
                for sub_y in 0..COVERAGE_SCALE {
                    let y = start_y + sub_y;
                    let row_start = y as usize * buffer_width as usize;
                    let y_band = if sub_y < quad { 0 } else { 2 };
                    for sub_x in 0..COVERAGE_SCALE {
                        let x = start_x + sub_x;
                        let idx = row_start + x as usize;
                        let x_band = if sub_x < quad { 0 } else { 1 };
                        let quadrant = (y_band + x_band) as usize;
                        sums[quadrant] += buffer[idx] as u32;
                    }
                }
                let coverage = [
                    (sums[0] / quad_area) as u8,
                    (sums[1] / quad_area) as u8,
                    (sums[2] / quad_area) as u8,
                    (sums[3] / quad_area) as u8,
                ];
                let choice = self.ramp.sample(coverage);
                let (fg_coverage, bg_coverage) = coverage_weights(coverage, choice.mask);
                row_cells.push(FontCell {
                    ch: choice.ch,
                    fg_coverage,
                    bg_coverage,
                });
            }
            cells.push(row_cells);
        }

        FontLayout {
            size,
            content_size: Expanse::new(content_width, content_height),
            cells,
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
            cursor_x += advance + self.font.spacing;
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
        let glyph = Arc::new(self.rasterize_glyph(metrics, &bitmap));
        self.cache.insert(key, Arc::clone(&glyph));
        glyph
    }

    /// Convert a font raster into a glyph.
    fn rasterize_glyph(&self, metrics: Metrics, bitmap: &[u8]) -> Glyph {
        let width = metrics.width;
        let height = metrics.height;
        let raster = bitmap.to_vec();

        let advance = metrics.advance_width;
        let advance_cells = advance.round() as i32;
        let bearing_left = metrics.xmin;
        let bearing_right = advance_cells - (metrics.xmin + width as i32);

        Glyph {
            bitmap: raster,
            width: width as u32,
            height: height as u32,
            bearing_left,
            bearing_right,
            bearing_bottom: metrics.ymin,
            advance,
        }
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

/// Compute an offset for aligning content inside a span.
fn align_offset(content: u32, available: u32, align: Align) -> u32 {
    if available <= content {
        return 0;
    }
    match align {
        Align::Start => 0,
        Align::Center => (available - content) / 2,
        Align::End => available - content,
    }
}

/// Integer division with rounding up.
fn div_ceil(value: u32, divisor: u32) -> u32 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor - 1) / divisor
}

/// Compute foreground/background coverage weights from quadrant coverage.
fn coverage_weights(coverage: [u8; 4], mask: [u8; 4]) -> (u8, u8) {
    let uniform = mask.iter().all(|value| *value == mask[0]);
    let total = coverage
        .iter()
        .fold(0u32, |sum, value| sum.saturating_add(*value as u32));
    let total_avg = (total / 4) as u8;
    if uniform {
        return (total_avg, 0);
    }

    let mut fg_sum = 0u32;
    let mut fg_count = 0u32;
    let mut bg_sum = 0u32;
    let mut bg_count = 0u32;

    for (value, sample) in coverage.iter().zip(mask.iter()) {
        if *sample >= 128 {
            fg_sum = fg_sum.saturating_add(*value as u32);
            fg_count = fg_count.saturating_add(1);
        } else {
            bg_sum = bg_sum.saturating_add(*value as u32);
            bg_count = bg_count.saturating_add(1);
        }
    }

    let fg_avg = if fg_count > 0 {
        (fg_sum / fg_count) as u8
    } else {
        0
    };
    let bg_avg = if bg_count > 0 {
        (bg_sum / bg_count) as u8
    } else {
        fg_avg
    };

    (fg_avg, bg_avg)
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
}

/// Draw underline, overline, and strike-through lines into the buffer.
fn draw_effect_lines(ctx: &mut EffectLineContext<'_>) {
    if ctx.width == 0 || ctx.height == 0 {
        return;
    }
    let line_height = ctx.line_advance.max(1);
    let thickness = max((COVERAGE_SCALE / 4) as i32, 1);
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
    use canopy::geom::Expanse;

    use super::*;

    const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/Bungee-Regular.ttf");

    fn test_font() -> Font {
        Font::from_bytes(TEST_FONT).expect("font loads")
    }

    #[test]
    fn layout_sizes_match_target() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Expanse::new(24, 8);
        let layout = renderer.layout(
            "Hello",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
        );
        assert_eq!(layout.size, size);
        assert_eq!(layout.cells.len(), size.h as usize);
        assert!(layout.content_size.w > 0);
    }

    #[test]
    fn layout_clips_overflow() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Expanse::new(8, 4);
        let layout = renderer.layout(
            "Overflow",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
        );
        assert_eq!(layout.cells.len(), size.h as usize);
        assert_eq!(layout.cells[0].len(), size.w as usize);
        assert!(layout.content_size.w >= size.w);
    }

    #[test]
    fn layout_handles_multiline() {
        let mut renderer = FontRenderer::new(test_font());
        let size = Expanse::new(24, 8);
        let layout = renderer.layout(
            "Hi\nThere",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
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
        let size = Expanse::new(24, 8);
        let missing = renderer.layout(
            "\u{10ffff}",
            size,
            LayoutOptions::default(),
            FontEffects::default(),
        );
        let fallback = renderer.layout("?", size, LayoutOptions::default(), FontEffects::default());
        assert_eq!(missing.cells, fallback.cells);
    }
}
