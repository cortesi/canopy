//! The quality corpus: images and text matched in each tier, and the metrics
//! that score them.

use std::{fs, path::PathBuf};

use canopy_mosaic::{
    Cell, RasterView, SymbolSet, Tier, coverage_into, fixed_into,
    geometry::Shape,
    raster::{Image, LinearRaster, Placement, composite, resample},
    rgb_into,
};

/// Matrix pixels across and down a cell.
const MATRIX: usize = 8;

/// One item of the corpus, at the resolution of the matrix.
pub struct Item {
    /// Name of the item.
    pub name: &'static str,
    /// Width in cells.
    pub cols: usize,
    /// Height in cells.
    pub rows: usize,
    /// The pixels.
    pub pixels: Pixels,
}

/// The pixels of an item, row by row, `cols × 8` by `rows × 8`.
pub enum Pixels {
    /// Opaque sRGB colors, matched with free pens.
    Rgb(Vec<[u8; 3]>),
    /// Ink coverage of text, matched with free pens and with fixed pens.
    Coverage(Vec<u8>),
}

/// The objective that matched a reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Objective {
    /// Free pens on RGB.
    Rgb,
    /// Free pens on coverage.
    Coverage,
    /// Fixed pens on coverage: crisp text.
    Fixed,
}

impl Objective {
    /// The name of the objective.
    pub fn name(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Coverage => "coverage",
            Self::Fixed => "fixed",
        }
    }
}

/// The workspace root.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Resample an RGBA image to the matrix of `cols × rows` cells, fitting the
/// whole image, and lay it over black.
fn fit(data: &[u8], width: usize, height: usize, cols: usize, rows: usize) -> Vec<[u8; 3]> {
    let image = Image::new(data, width, height).expect("image");
    let placement = Placement {
        origin: [0.0, 0.0],
        step: [
            width as f32 / (cols * MATRIX) as f32,
            height as f32 / (rows * MATRIX) as f32,
        ],
    };
    let mut linear = LinearRaster::new();
    resample(&image, placement, cols * MATRIX, rows * MATRIX, &mut linear);
    let mut out = Vec::new();
    composite(&linear, &vec![[0, 0, 0]; cols * rows], &mut out);
    out
}

/// A generated RGBA image of `width` by `height` pixels, each the mean of
/// 4×4 samples of `f` over unit coordinates, so edges are antialiased.
fn generate(width: usize, height: usize, f: impl Fn(f32, f32) -> [f32; 3]) -> Vec<u8> {
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];
            for j in 0..4 {
                for i in 0..4 {
                    let u = (x as f32 + (i as f32 + 0.5) / 4.0) / width as f32;
                    let v = (y as f32 + (j as f32 + 0.5) / 4.0) / height as f32;
                    let c = f(u, v);
                    for ch in 0..3 {
                        sum[ch] += c[ch] / 16.0;
                    }
                }
            }
            data.extend(sum.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
            data.push(255);
        }
    }
    data
}

/// Text in a font, `rows` cells high, as matrix coverage: 8×16 square
/// pixels to each cell, each pair of rows averaged into one matrix row.
fn text(font: &str, word: &str, rows: usize) -> (usize, Vec<u8>) {
    let path = root().join("crates/canopy-widgets/assets/fonts").join(font);
    let bytes = fs::read(&path).expect("font");
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("font");
    let px = rows as f32 * 16.0 * 0.7;
    let metrics = font.horizontal_line_metrics(px).expect("line metrics");
    let glyphs: Vec<_> = word.chars().map(|ch| font.rasterize(ch, px)).collect();
    let advance: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
    let cols = (advance / 8.0).ceil() as usize + 2;
    let (width, height) = (cols * MATRIX, rows * 16);
    let mut coverage = vec![0u8; width * height];
    let baseline = (height as f32 - (metrics.ascent - metrics.descent)) / 2.0 + metrics.ascent;
    let mut pen = 8.0;
    for (m, bitmap) in &glyphs {
        let x0 = (pen + m.xmin as f32).round() as i64;
        let y0 = (baseline - m.ymin as f32 - m.height as f32).round() as i64;
        for gy in 0..m.height {
            for gx in 0..m.width {
                let (x, y) = (x0 + gx as i64, y0 + gy as i64);
                if (0..width as i64).contains(&x) && (0..height as i64).contains(&y) {
                    let at = y as usize * width + x as usize;
                    coverage[at] = coverage[at].max(bitmap[gy * m.width + gx]);
                }
            }
        }
        pen += m.advance_width;
    }
    let matrix = (0..rows * MATRIX)
        .flat_map(|y| {
            let coverage = &coverage;
            (0..width).map(move |x| {
                let pair = u16::from(coverage[2 * y * width + x])
                    + u16::from(coverage[(2 * y + 1) * width + x]);
                pair.div_ceil(2) as u8
            })
        })
        .collect();
    (cols, matrix)
}

/// The corpus.
pub fn items() -> Vec<Item> {
    let mut items = Vec::new();
    let photo = image::open(root().join(".assets/shyness.jpg"))
        .expect("photo")
        .into_rgba8();
    let (w, h) = (photo.width() as usize, photo.height() as usize);
    items.push(Item {
        name: "photo",
        cols: 120,
        rows: 40,
        pixels: Pixels::Rgb(fit(photo.as_raw(), w, h, 120, 40)),
    });
    let zone = generate(480, 480, |u, v| {
        let r2 = (u - 0.5).powi(2) + (v - 0.5).powi(2);
        let value = 0.5 + 0.5 * (r2 * 260.0).cos();
        [value, value, value]
    });
    items.push(Item {
        name: "zone-plate",
        cols: 60,
        rows: 30,
        pixels: Pixels::Rgb(fit(&zone, 480, 480, 60, 30)),
    });
    let lines = generate(480, 480, |u, v| {
        // Thin dark lines through the center, every 15 degrees.
        let (dx, dy) = (u - 0.5, v - 0.5);
        let near = (0..12).any(|k| {
            let angle = (k as f32 * 15.0).to_radians();
            (dx * angle.sin() - dy * angle.cos()).abs() < 0.004
        });
        if near {
            [0.1, 0.1, 0.15]
        } else {
            [0.95, 0.92, 0.85]
        }
    });
    items.push(Item {
        name: "lines",
        cols: 60,
        rows: 30,
        pixels: Pixels::Rgb(fit(&lines, 480, 480, 60, 30)),
    });
    let disc = generate(480, 320, |u, v| {
        let inside = (u - 0.5).powi(2) + ((v - 0.5) * 1.5).powi(2) < 0.09;
        if inside {
            [0.9, 0.3, 0.2]
        } else {
            [u * 0.3, 0.3 + v * 0.4, 0.6]
        }
    });
    items.push(Item {
        name: "disc",
        cols: 60,
        rows: 20,
        pixels: Pixels::Rgb(fit(&disc, 480, 320, 60, 20)),
    });
    for (name, font) in [
        ("text-bungee", "Bungee-Regular.ttf"),
        ("text-fira-mono", "FiraMono-Regular.ttf"),
        ("text-tangerine", "Tangerine-Regular.ttf"),
    ] {
        let (cols, coverage) = text(font, "Canopy", 6);
        items.push(Item {
            name,
            cols,
            rows: 6,
            pixels: Pixels::Coverage(coverage),
        });
    }
    items
}

/// The objectives of an item.
pub fn objectives(item: &Item) -> &'static [Objective] {
    match item.pixels {
        Pixels::Rgb(_) => &[Objective::Rgb],
        Pixels::Coverage(_) => &[Objective::Coverage, Objective::Fixed],
    }
}

/// Reconstruct cells to matrix pixels, each pixel the pen of its part of
/// the character's mask: the display of an ideal terminal.
fn reconstruct<P: Copy>(cells: &[Cell<P>], cols: usize, rows: usize, blank: P) -> Vec<P> {
    let mut out = vec![blank; cols * rows * MATRIX * MATRIX];
    let width = cols * MATRIX;
    for (index, cell) in cells.iter().enumerate() {
        let mask = Shape::of(cell.ch)
            .expect("drawn characters have shapes")
            .mask();
        let (cx, cy) = (index % cols, index / cols);
        for y in 0..MATRIX {
            for x in 0..MATRIX {
                let pen = if mask.get(x, y) { cell.fg } else { cell.bg };
                out[(cy * MATRIX + y) * width + cx * MATRIX + x] = pen;
            }
        }
    }
    out
}

/// Match an item in a tier with an objective, and return the source and
/// the ideal display, both as three-channel matrix pixels.
pub fn matched(item: &Item, tier: Tier, objective: Objective) -> (Vec<[u8; 3]>, Vec<[u8; 3]>) {
    let set = SymbolSet::tier(tier);
    let (width, height) = (item.cols * MATRIX, item.rows * MATRIX);
    match &item.pixels {
        Pixels::Rgb(pixels) => {
            let view = RasterView::new(pixels, width, height, width).expect("raster");
            let mut cells = Vec::new();
            rgb_into(&view, set, &mut cells);
            (
                pixels.clone(),
                reconstruct(&cells, item.cols, item.rows, [0; 3]),
            )
        }
        Pixels::Coverage(coverage) => {
            let view = RasterView::new(coverage, width, height, width).expect("raster");
            let mut cells = Vec::new();
            if objective == Objective::Fixed {
                fixed_into(&view, set, &mut cells);
            } else {
                coverage_into(&view, set, &mut cells);
            }
            let shown = reconstruct(&cells, item.cols, item.rows, 0);
            (
                coverage.iter().map(|v| [*v; 3]).collect(),
                shown.iter().map(|v| [*v; 3]).collect(),
            )
        }
    }
}

/// Blur three-channel matrix pixels in display space, where a matrix pixel
/// is half as wide as it is tall: `sigma` display pixels across is `sigma`
/// matrix pixels, and down is `sigma / 2`.
fn blur(pixels: &[[u8; 3]], width: usize, sigma: f32) -> Vec<[f32; 3]> {
    let height = pixels.len() / width;
    let kernel = |sigma: f32| -> Vec<f32> {
        let radius = (sigma * 3.0).ceil() as i64;
        let weights: Vec<f32> = (-radius..=radius)
            .map(|d| (-(d * d) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let total: f32 = weights.iter().sum();
        weights.into_iter().map(|w| w / total).collect()
    };
    let pass = |input: &[[f32; 3]], kernel: &[f32], horizontal: bool| -> Vec<[f32; 3]> {
        let radius = (kernel.len() / 2) as i64;
        let mut out = vec![[0.0; 3]; input.len()];
        for y in 0..height {
            for x in 0..width {
                let mut sum = [0.0f32; 3];
                for (k, w) in kernel.iter().enumerate() {
                    let d = k as i64 - radius;
                    let (sx, sy) = if horizontal {
                        ((x as i64 + d).clamp(0, width as i64 - 1) as usize, y)
                    } else {
                        (x, (y as i64 + d).clamp(0, height as i64 - 1) as usize)
                    };
                    let p = input[sy * width + sx];
                    for ch in 0..3 {
                        sum[ch] += w * p[ch];
                    }
                }
                out[y * width + x] = sum;
            }
        }
        out
    };
    let input: Vec<[f32; 3]> = pixels.iter().map(|p| p.map(f32::from)).collect();
    let across = pass(&input, &kernel(sigma), true);
    pass(&across, &kernel(sigma / 2.0), false)
}

/// The display that the source explains: each pixel of `shown` clamped into
/// the range of each channel that `source` holds within a matrix pixel across
/// and half of one down, as the matcher's reach. Half a pixel down holds the
/// values midway between a pixel and those above and below it, rounded down
/// for the least and up for the greatest. What the clamp takes out is stray:
/// a mark that nothing near in the source explains, such as a spur of ink
/// below a stroke.
pub fn explained(source: &[[u8; 3]], shown: &[[u8; 3]], width: usize) -> Vec<[u8; 3]> {
    let height = source.len() / width;
    shown
        .iter()
        .enumerate()
        .map(|(i, pixel)| {
            let (x, y) = (i % width, i / width);
            let mut low = [u8::MAX; 3];
            let mut high = [0u8; 3];
            for u in x.saturating_sub(1)..(x + 2).min(width) {
                let here = source[y * width + u];
                for ch in 0..3 {
                    low[ch] = low[ch].min(here[ch]);
                    high[ch] = high[ch].max(here[ch]);
                }
                for v in [y.checked_sub(1), Some(y + 1).filter(|v| *v < height)]
                    .into_iter()
                    .flatten()
                {
                    let other = source[v * width + u];
                    for ch in 0..3 {
                        let sum = u16::from(here[ch]) + u16::from(other[ch]);
                        low[ch] = low[ch].min((sum / 2) as u8);
                        high[ch] = high[ch].max(sum.div_ceil(2) as u8);
                    }
                }
            }
            [0, 1, 2].map(|ch| pixel[ch].clamp(low[ch], high[ch]))
        })
        .collect()
}

/// PSNR in dB between two images after a display-space blur at `sigma`.
pub fn psnr(a: &[[u8; 3]], b: &[[u8; 3]], width: usize, sigma: f32) -> f64 {
    let (a, b) = (blur(a, width, sigma), blur(b, width, sigma));
    let error: f64 = a
        .iter()
        .zip(&b)
        .map(|(p, q)| {
            (0..3)
                .map(|ch| f64::from(p[ch] - q[ch]).powi(2))
                .sum::<f64>()
        })
        .sum();
    let mse = error / (a.len() * 3) as f64;
    10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10()
}
