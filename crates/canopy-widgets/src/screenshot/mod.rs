//! A [`Screenshot`] draws a [`ScreenCapture`] in a monospace font, one cell
//! to each grid square. Box drawing, block elements, braille, and triangles
//! are drawn from geometry, as terminals draw them, so that lines join across
//! cells and shapes fill their cells exactly.

/// Box drawing, block elements, braille, and triangles, drawn from geometry.
mod glyphs;

use std::{
    collections::{BTreeSet, HashMap},
    io::Cursor,
    mem,
};

use canopy::{
    error::{Error, Result},
    render::ScreenCapture,
    text,
};
use fontdue::{Font, FontSettings, Metrics};
use image::{ImageFormat, RgbImage};
use unicode_segmentation::UnicodeSegmentation;

/// The font of screenshots: Fira Mono, under the SIL Open Font License.
const FONT: &[u8] = include_bytes!("../../assets/fonts/FiraMono-Regular.ttf");

/// How far italic text leans: columns of shift for each row of height.
const ITALIC_SLANT: f32 = 0.2;

/// How far dim text fades toward its background.
const DIM: f32 = 0.5;

/// The most pixels of one image, which bounds the memory of a capture of
/// any size: 64 MP takes 192 MB of RGB.
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

/// How a screenshot draws a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenshotOptions {
    /// Font size in pixels, before the scale.
    pub font_size: f32,
    /// Image pixels for each pixel of the font size. A scale of 2 suits a
    /// high density display.
    pub scale: f32,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            scale: 2.0,
        }
    }
}

/// One drawn frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Png {
    /// The PNG file.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Characters that the font lacks, which show as boxes.
    pub missing: Vec<char>,
}

/// Draws frames as PNG images.
///
/// The font has one regular face: bold text widens its strokes, and italic
/// text leans. A character that the font lacks shows as a box.
pub struct Screenshot {
    /// The font.
    font: Font,
    /// Font size in image pixels.
    px: f32,
    /// Cell width in pixels.
    cell_w: u32,
    /// Cell height in pixels.
    cell_h: u32,
    /// Distance from the top of a cell to the baseline, in pixels.
    baseline: i32,
    /// Width of a light line, in pixels.
    stroke: u32,
    /// Rasterized glyphs, by character and boldness.
    glyphs: HashMap<(char, bool), Glyph>,
    /// Characters that the font lacks, in the frame that is drawing.
    missing: BTreeSet<char>,
}

/// One rasterized glyph.
struct Glyph {
    /// Placement metrics.
    metrics: Metrics,
    /// Width of the coverage rows, which bold text widens.
    width: usize,
    /// Coverage, row by row.
    coverage: Vec<u8>,
}

impl Screenshot {
    /// Create a screenshot renderer.
    pub fn new(options: ScreenshotOptions) -> Result<Self> {
        let font = Font::from_bytes(FONT, FontSettings::default())
            .map_err(|error| Error::Invalid(format!("screenshot font: {error}")))?;
        let px = options.font_size * options.scale;
        if !px.is_finite() || px < 4.0 {
            return Err(Error::Invalid(format!(
                "screenshot font size {px} is too small"
            )));
        }
        let lines = font
            .horizontal_line_metrics(px)
            .ok_or_else(|| Error::Invalid("screenshot font has no line metrics".into()))?;
        let height = lines.ascent - lines.descent;
        let cell_h = (height + lines.line_gap).ceil();
        let baseline = ((cell_h - height) / 2.0 + lines.ascent).round();
        let cell_w = font.metrics('M', px).advance_width.round();
        Ok(Self {
            font,
            px,
            cell_w: cell_w as u32,
            cell_h: cell_h as u32,
            baseline: baseline as i32,
            stroke: (px / 14.0).round().max(1.0) as u32,
            glyphs: HashMap::new(),
            missing: BTreeSet::new(),
        })
    }

    /// Draw a capture as a PNG image. A capture whose image would exceed
    /// 64 megapixels fails.
    pub fn png(&mut self, capture: &ScreenCapture) -> Result<Png> {
        let canvas = self.draw(capture)?;
        let (width, height) = (canvas.width, canvas.height);
        let image = RgbImage::from_raw(width, height, canvas.pixels)
            .ok_or_else(|| Error::Internal("screenshot canvas size".into()))?;
        let mut data = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut data), ImageFormat::Png)
            .map_err(|error| Error::Invalid(format!("encode screenshot: {error}")))?;
        Ok(Png {
            data,
            width,
            height,
            missing: mem::take(&mut self.missing).into_iter().collect(),
        })
    }

    /// Draw a capture on a canvas: every ground first, so that no glyph that
    /// overhangs its cell loses its edge to the next ground.
    fn draw(&mut self, capture: &ScreenCapture) -> Result<Canvas> {
        self.missing.clear();
        let width = capture.width.checked_mul(self.cell_w);
        let height = capture.height.checked_mul(self.cell_h);
        let (width, height) = width
            .zip(height)
            .filter(|(width, height)| u64::from(*width) * u64::from(*height) <= MAX_PIXELS)
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "a screenshot of {}x{} cells exceeds {MAX_PIXELS} pixels",
                    capture.width, capture.height
                ))
            })?;
        let mut canvas = Canvas::new(width, height);
        let cells = self.cells(capture)?;
        for cell in &cells {
            canvas.fill(cell.rect, cell.bg);
        }
        for cell in &cells {
            self.draw_cell(&mut canvas, cell);
        }
        Ok(canvas)
    }

    /// Return the cells of a capture, with their pixel rectangles.
    fn cells<'a>(&self, capture: &'a ScreenCapture) -> Result<Vec<CellDraw<'a>>> {
        let mut cells = Vec::new();
        for (row, runs) in capture.rows.iter().enumerate() {
            let mut column = 0u32;
            for run in runs {
                let style = capture.styles.get(run.style).ok_or_else(|| {
                    Error::Invalid(format!("capture run names missing style {}", run.style))
                })?;
                let attrs = style.attrs;
                // The run places its graphemes within its own cells, and the
                // next run starts after them, whatever the text measures.
                let start = column;
                let end = start.saturating_add(run.cells).min(capture.width);
                let fg = if attrs.dim {
                    mix(style.fg, style.bg, DIM)
                } else {
                    style.fg
                };
                for grapheme in run.text.graphemes(true) {
                    let width = text::grapheme_width(grapheme).max(1) as u32;
                    if column >= end {
                        break;
                    }
                    cells.push(CellDraw {
                        grapheme,
                        rect: Rect {
                            x: (column * self.cell_w) as i32,
                            y: row as i32 * self.cell_h as i32,
                            w: width.min(end - column) * self.cell_w,
                            h: self.cell_h,
                        },
                        fg,
                        bg: style.bg,
                        bold: attrs.bold,
                        italic: attrs.italic,
                        underline: attrs.underline,
                        overline: attrs.overline,
                        crossedout: attrs.crossedout,
                    });
                    column += width;
                }
                column = start.saturating_add(run.cells);
            }
        }
        Ok(cells)
    }

    /// Draw the glyph and the lines of one cell over its ground.
    fn draw_cell(&mut self, canvas: &mut Canvas, cell: &CellDraw<'_>) {
        let rect = cell.rect;
        let first = cell.grapheme.chars().next().unwrap_or(' ');
        if first != ' ' && !glyphs::draw(canvas, first, rect, cell.fg, self.stroke) {
            for ch in cell.grapheme.chars().filter(|ch| !is_invisible(*ch)) {
                self.draw_char(canvas, ch, cell);
            }
        }
        let stroke = self.stroke;
        let line = |canvas: &mut Canvas, y: i32| {
            canvas.fill(
                Rect {
                    x: rect.x,
                    y,
                    w: rect.w,
                    h: stroke,
                },
                cell.fg,
            );
        };
        if cell.underline {
            line(canvas, rect.y + self.baseline + stroke as i32);
        }
        if cell.crossedout {
            line(canvas, rect.y + self.baseline - self.cell_h as i32 * 3 / 10);
        }
        if cell.overline {
            line(canvas, rect.y);
        }
    }

    /// Draw one character of the font at the origin of a cell.
    fn draw_char(&mut self, canvas: &mut Canvas, ch: char, cell: &CellDraw<'_>) {
        if self.font.lookup_glyph_index(ch) == 0 {
            self.missing.insert(ch);
            glyphs::tofu(canvas, cell.rect, cell.fg, self.stroke);
            return;
        }
        let (px, stroke) = (self.px, self.stroke);
        let font = &self.font;
        let glyph = self
            .glyphs
            .entry((ch, cell.bold))
            .or_insert_with(|| rasterize(font, ch, px, cell.bold.then_some(stroke)));
        let metrics = glyph.metrics;
        let left = cell.rect.x + metrics.xmin;
        let baseline = cell.rect.y + self.baseline;
        let top = baseline - metrics.ymin - metrics.height as i32;
        for (row, coverage) in glyph.coverage.chunks(glyph.width.max(1)).enumerate() {
            let y = top + row as i32;
            // Italic text leans about the baseline, so descenders lean back.
            let lean = if cell.italic {
                ((baseline - y) as f32 * ITALIC_SLANT).round() as i32
            } else {
                0
            };
            for (column, alpha) in coverage.iter().enumerate() {
                canvas.blend(left + lean + column as i32, y, cell.fg, *alpha);
            }
        }
    }
}

/// Rasterize a glyph, widening its strokes by `bold` pixels.
fn rasterize(font: &Font, ch: char, px: f32, bold: Option<u32>) -> Glyph {
    let (metrics, coverage) = font.rasterize(ch, px);
    let Some(extra) = bold.map(|bold| bold as usize).filter(|_| metrics.width > 0) else {
        return Glyph {
            metrics,
            width: metrics.width,
            coverage,
        };
    };
    let width = metrics.width + extra;
    let mut widened = vec![0u8; width * metrics.height];
    for (row, source) in coverage.chunks(metrics.width).enumerate() {
        let target = &mut widened[row * width..(row + 1) * width];
        for (column, alpha) in source.iter().enumerate() {
            for shift in 0..=extra {
                let cell = &mut target[column + shift];
                *cell = (*cell).max(*alpha);
            }
        }
    }
    Glyph {
        metrics,
        width,
        coverage: widened,
    }
}

/// Return whether a character draws nothing of its own, such as a variation
/// selector or a joiner.
fn is_invisible(ch: char) -> bool {
    matches!(ch, '\u{200B}'..='\u{200F}' | '\u{FE00}'..='\u{FE0F}' | '\u{2060}')
}

/// Mix `from` toward `to` by `amount`, 0.0 to 1.0.
fn mix(from: [u8; 3], to: [u8; 3], amount: f32) -> [u8; 3] {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    [
        channel(from[0], to[0]),
        channel(from[1], to[1]),
        channel(from[2], to[2]),
    ]
}

/// One cell to draw.
struct CellDraw<'a> {
    /// The grapheme.
    grapheme: &'a str,
    /// Pixel rectangle, two cells wide for a wide grapheme.
    rect: Rect,
    /// Foreground color, after dimming.
    fg: [u8; 3],
    /// Background color.
    bg: [u8; 3],
    /// Bold text.
    bold: bool,
    /// Italic text.
    italic: bool,
    /// Underlined text.
    underline: bool,
    /// Overlined text.
    overline: bool,
    /// Crossed out text.
    crossedout: bool,
}

/// A pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    /// Left edge.
    x: i32,
    /// Top edge.
    y: i32,
    /// Width.
    w: u32,
    /// Height.
    h: u32,
}

impl Rect {
    /// Return the right edge, exclusive.
    fn right(self) -> i32 {
        self.x + self.w as i32
    }

    /// Return the bottom edge, exclusive.
    fn bottom(self) -> i32 {
        self.y + self.h as i32
    }

    /// Return the rectangle between two corners, empty when they cross.
    fn between(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0) as u32,
            h: (y1 - y0).max(0) as u32,
        }
    }
}

/// An RGB image under construction.
struct Canvas {
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// Pixels, row by row, three bytes each.
    pixels: Vec<u8>,
}

impl Canvas {
    /// Create a black canvas.
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 3],
        }
    }

    /// Return the byte offset of a pixel, or `None` outside the canvas.
    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        (x < self.width && y < self.height)
            .then(|| (y as usize * self.width as usize + x as usize) * 3)
    }

    /// Fill a rectangle with a color.
    fn fill(&mut self, rect: Rect, color: [u8; 3]) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                if let Some(at) = self.offset(x, y) {
                    self.pixels[at..at + 3].copy_from_slice(&color);
                }
            }
        }
    }

    /// Blend a color over one pixel with coverage `alpha`, 0 to 255.
    fn blend(&mut self, x: i32, y: i32, color: [u8; 3], alpha: u8) {
        if alpha == 0 {
            return;
        }
        let Some(at) = self.offset(x, y) else {
            return;
        };
        let alpha = u16::from(alpha);
        for (channel, target) in self.pixels[at..at + 3].iter_mut().enumerate() {
            let mixed =
                (u16::from(color[channel]) * alpha + u16::from(*target) * (255 - alpha) + 127)
                    / 255;
            *target = mixed as u8;
        }
    }

    /// Blend a color over one pixel with coverage from 0.0 to 1.0.
    fn blend_f(&mut self, x: i32, y: i32, color: [u8; 3], coverage: f32) {
        self.blend(
            x,
            y,
            color,
            (coverage.clamp(0.0, 1.0) * 255.0).round() as u8,
        );
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        geom::{Line, Size},
        render::TermBuf,
        style::{AttrSet, Color, ResolvedStyle},
    };

    use super::*;

    /// Returns the pixel at `(x, y)` of a canvas.
    fn pixel(canvas: &Canvas, x: i32, y: i32) -> [u8; 3] {
        let at = canvas.offset(x, y).expect("inside");
        [
            canvas.pixels[at],
            canvas.pixels[at + 1],
            canvas.pixels[at + 2],
        ]
    }

    #[test]
    fn a_frame_draws_its_grounds_glyphs_and_lines() {
        let ground = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
        let red = ResolvedStyle::new(
            Color::Rgb { r: 255, g: 0, b: 0 },
            Color::Rgb { r: 0, g: 0, b: 255 },
            AttrSet::default(),
        );
        let mut buf = TermBuf::new(Size::new(4, 2), ' ', ground).expect("buffer");
        buf.text(&red, Line::new(0, 0, 2), "M─").expect("text");
        buf.text(&ground, Line::new(0, 1, 4), "│█\u{1F9A9}")
            .expect("text");
        let mut shot = Screenshot::new(ScreenshotOptions::default()).expect("screenshot");
        let canvas = shot.draw(&buf.capture()).expect("draw");
        let (w, h) = (shot.cell_w as i32, shot.cell_h as i32);
        assert_eq!((canvas.width, canvas.height), (4 * w as u32, 2 * h as u32));
        // The ground of a cell holds at its corner, and its glyph covers its
        // middle.
        assert_eq!(pixel(&canvas, 0, 0), [0, 0, 255]);
        let middle = pixel(&canvas, w / 2, h / 2);
        assert_ne!(middle, [0, 0, 255], "the glyph covers the middle");
        // A horizontal line runs from edge to edge through the middle row.
        assert_eq!(pixel(&canvas, w, h / 2), [255, 0, 0]);
        assert_eq!(pixel(&canvas, 2 * w - 1, h / 2), [255, 0, 0]);
        assert_eq!(pixel(&canvas, w + w / 2, 0), [0, 0, 255]);
        // A vertical line runs from top to bottom, and a full block fills
        // its cell.
        assert_eq!(pixel(&canvas, w / 2, h), [255, 255, 255]);
        assert_eq!(pixel(&canvas, w / 2, 2 * h - 1), [255, 255, 255]);
        assert_eq!(pixel(&canvas, w, h), [255, 255, 255]);
        assert_eq!(pixel(&canvas, 2 * w - 1, 2 * h - 1), [255, 255, 255]);
        // The font lacks the emoji, which shows as a box.
        let png = shot.png(&buf.capture()).expect("png");
        assert!(png.data.starts_with(b"\x89PNG"));
        assert_eq!((png.width, png.height), (canvas.width, canvas.height));
        assert_eq!(png.missing, ['\u{1F9A9}']);
    }

    #[test]
    fn a_capture_too_large_to_draw_fails() {
        let ground = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
        let mut capture = TermBuf::new(Size::new(1, 1), ' ', ground)
            .expect("buffer")
            .capture();
        capture.width = u32::MAX;
        let mut shot = Screenshot::new(ScreenshotOptions::default()).expect("screenshot");
        assert!(shot.png(&capture).is_err());
        capture.width = 20_000;
        capture.height = 20_000;
        assert!(shot.png(&capture).is_err());
    }

    #[test]
    fn a_run_holds_its_graphemes_to_its_cells() {
        let ground = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
        let mut buf = TermBuf::new(Size::new(4, 1), ' ', ground).expect("buffer");
        buf.text(&ground, Line::new(0, 0, 4), "abcd").expect("text");
        let mut capture = buf.capture();
        // Text that measures wider than the run's cells stays inside them,
        // and the next run starts where the cells say.
        capture.rows[0][0].text = "abcd".to_owned();
        capture.rows[0][0].cells = 2;
        let mut second = capture.rows[0][0].clone();
        second.text = "xy".to_owned();
        capture.rows[0].push(second);
        let shot = Screenshot::new(ScreenshotOptions::default()).expect("screenshot");
        let cells = shot.cells(&capture).expect("cells");
        let placed: Vec<_> = cells
            .iter()
            .map(|cell| (cell.grapheme, cell.rect.x / shot.cell_w as i32))
            .collect();
        assert_eq!(placed, [("a", 0), ("b", 1), ("x", 2), ("y", 3)]);
    }
}
