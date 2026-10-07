//! The standard suite: images and text, each with the picture that the eye
//! should see, and the cells that Canopy draws for it in each tier.
//!
//! Images take the path of image views: an area resample in linear light to
//! the matrix of each cell, then matching in RGB, and the pens as the colors
//! of the cell. Text takes the path of font banners: ink coverage, matched
//! with free pens, or with fixed pens for crisp text, and the pens mixed
//! into the banner's colors by `Coverage::apply`.
//!
//! The reference of each item is its picture at the device pixels of the
//! display model, in linear light: the source image resampled by area, or
//! the text rasterized at device pixels with its coverage mixed in light.

use std::{fs, path::PathBuf};

use canopy::style::{AttrSet, Color, Coverage, ResolvedStyle};
use canopy_mosaic::{
    RasterView, SymbolSet, Tier, coverage_into, fixed_into,
    raster::{Image, LinearRaster, Placement, composite, resample},
    rgb_into,
};

use crate::{
    color,
    display::{CELL_H, CELL_W, Shown},
};

/// Matrix pixels across and down a cell.
const MATRIX: usize = 8;

/// The text of each text item: capitals, round and straight lowercase,
/// ascenders, descenders, and digits.
const TEXT: &str = "Hamburgefonstiv 0123";

/// The text of the banner item: the word the font gym first shows, in
/// Bungee on square cells. The rounded ends of its strokes, such as the
/// bottom right of the C and of the stem of the Y, once hung spurs of ink
/// below them.
const BANNER: &str = "Canopy";

/// The offsets of the banner item's lines, in cells across and down. Where
/// a stroke ends within a cell decides which symbol draws it, so each line
/// sets the word at another offset.
const BANNER_PHASES: [[f32; 2]; 4] = [[0.0, 0.0], [0.25, 0.125], [0.5, 0.25], [0.75, 0.375]];

/// The fonts of the text items, by name and file.
const FONTS: [(&str, &str); 4] = [
    ("bungee", "Bungee-Regular.ttf"),
    ("fira-mono", "FiraMono-Regular.ttf"),
    ("tangerine", "Tangerine-Regular.ttf"),
    ("jetbrains-bold", "JetBrainsMonoNL-Bold.ttf"),
];

/// The heights of the text items, in cell rows.
const TEXT_ROWS: [usize; 4] = [2, 3, 5, 8];

/// The heights of the text items drawn on square cells, in cell rows. A font
/// banner draws this way unless it asks for tall cells: each glyph at twice
/// its height.
const SQUARE_ROWS: [usize; 3] = [4, 8, 16];

/// Light text on a dark ground, as in a dark theme.
const LIGHT: ([u8; 3], [u8; 3]) = ([0xdc, 0xdf, 0xe4], [0x28, 0x2c, 0x34]);
/// Dark text on a light ground, as in a light theme.
const DARK: ([u8; 3], [u8; 3]) = ([0x38, 0x3a, 0x42], [0xfa, 0xfa, 0xfa]);

/// The group of an item. The report scores each group apart, so that the
/// many small text items do not outweigh the images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Images, as image views draw them.
    Images,
    /// Antialiased text, as font banners draw it.
    Text,
    /// Crisp text, with fixed pens.
    Crisp,
}

impl Group {
    /// Every group, in report order.
    pub const ALL: [Self; 3] = [Self::Images, Self::Text, Self::Crisp];

    /// The name of the group.
    pub fn name(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Text => "text",
            Self::Crisp => "crisp",
        }
    }
}

/// What the matcher takes for an item.
enum Input {
    /// An opaque sRGB matrix, matched with free pens.
    Rgb(Vec<[u8; 3]>),
    /// Ink coverage, matched as a font banner matches it, and drawn in a
    /// banner's colors.
    Coverage {
        /// The coverage of each matrix pixel.
        matrix: Vec<u8>,
        /// The color of the ink.
        fg: [u8; 3],
        /// The color of the ground.
        bg: [u8; 3],
        /// Whether the pens are fixed at full ink and bare ground.
        crisp: bool,
    },
}

/// An item, built.
pub struct Item {
    /// Width in cells.
    pub cols: usize,
    /// Height in cells.
    pub rows: usize,
    /// What the eye should see: linear sRGB device pixels, row by row.
    pub reference: Vec<[f32; 3]>,
    /// What the matcher takes.
    input: Input,
}

impl Item {
    /// Width in device pixels.
    pub fn width(&self) -> usize {
        self.cols * CELL_W
    }

    /// The cells that Canopy draws for the item in a tier, row by row.
    pub fn shown(&self, tier: Tier) -> Vec<Shown> {
        let set = SymbolSet::tier(tier);
        let (width, height) = (self.cols * MATRIX, self.rows * MATRIX);
        match &self.input {
            Input::Rgb(pixels) => {
                let view = RasterView::new(pixels, width, height, width).expect("raster");
                let mut cells = Vec::new();
                rgb_into(&view, set, &mut cells);
                cells
                    .iter()
                    .map(|cell| Shown {
                        ch: cell.ch,
                        fg: cell.fg,
                        bg: cell.bg,
                    })
                    .collect()
            }
            Input::Coverage {
                matrix,
                fg,
                bg,
                crisp,
            } => {
                let view = RasterView::new(matrix, width, height, width).expect("raster");
                let mut cells = Vec::new();
                if *crisp {
                    fixed_into(&view, set, &mut cells);
                } else {
                    coverage_into(&view, set, &mut cells);
                }
                let rgb = |[r, g, b]: [u8; 3]| Color::Rgb { r, g, b };
                let style = ResolvedStyle::new(rgb(*fg), rgb(*bg), AttrSet::default());
                cells
                    .iter()
                    .map(|cell| {
                        let covered = Coverage {
                            fg: cell.fg,
                            bg: cell.bg,
                        }
                        .apply(style);
                        let (fr, fgr, fb) = covered.fg.rgb();
                        let (br, bgr, bb) = covered.bg.rgb();
                        Shown {
                            ch: cell.ch,
                            fg: [fr, fgr, fb],
                            bg: [br, bgr, bb],
                        }
                    })
                    .collect()
            }
        }
    }
}

/// A generator of an image: it takes `x` from 0 to the width over the
/// height, and `y` from 0 to 1, and returns sRGB from 0 to 1.
type Generator = fn(f32, f32) -> [f32; 3];

/// How to build an item.
enum Build {
    /// A photograph from the workspace, fitted to `cols` cells across.
    Photo {
        /// The path from the workspace root.
        path: &'static str,
        /// Width in cells.
        cols: usize,
    },
    /// A generated image.
    Generated {
        /// Width in cells.
        cols: usize,
        /// Height in cells.
        rows: usize,
        /// The generator.
        f: Generator,
    },
    /// Text in a font.
    Text(Text),
}

/// Text in a font, as an item draws it.
struct Text {
    /// The words.
    words: &'static str,
    /// The font file in the fonts of canopy-widgets.
    font: &'static str,
    /// The height of the line box in cell rows.
    rows: usize,
    /// The ink and the ground.
    colors: ([u8; 3], [u8; 3]),
    /// Whether the pens are fixed.
    crisp: bool,
    /// Whether the text takes a cell as square, and so draws at twice its
    /// height.
    square: bool,
    /// The offset of each line, in cells across and down. Each line draws
    /// the words once.
    phases: &'static [[f32; 2]],
}

/// An item of the suite, before it is built.
pub struct Spec {
    /// The name of the item.
    pub name: String,
    /// The group of the item.
    pub group: Group,
    /// How to build it.
    build: Build,
}

impl Spec {
    /// Build the item.
    pub fn build(&self) -> Item {
        match self.build {
            Build::Photo { path, cols } => photo(path, cols),
            Build::Generated { cols, rows, f } => generated(cols, rows, f),
            Build::Text(ref spec) => text(spec),
        }
    }

    /// A rough cost of scoring the item, to schedule the largest first.
    pub fn cost(&self) -> usize {
        match self.build {
            Build::Photo { cols, .. } => cols * cols,
            Build::Generated { cols, rows, .. } => cols * rows,
            Build::Text(ref text) => {
                let rows = if text.square {
                    text.rows / 2
                } else {
                    text.rows
                };
                rows * rows * 12 * text.phases.len()
            }
        }
    }
}

/// The standard suite.
pub fn suite() -> Vec<Spec> {
    let mut specs = vec![Spec {
        name: "photo".into(),
        group: Group::Images,
        build: Build::Photo {
            path: ".assets/shyness.jpg",
            cols: 96,
        },
    }];
    let generated: [(&str, usize, usize, Generator); 7] = [
        ("zone-plate", 48, 24, zone_plate),
        ("lines", 48, 24, lines),
        ("disc", 60, 20, disc),
        ("ramps", 64, 16, ramps),
        ("noise", 64, 24, noise),
        ("checker", 48, 16, checker),
        ("shapes", 64, 24, shapes),
    ];
    for (name, cols, rows, f) in generated {
        specs.push(Spec {
            name: name.into(),
            group: Group::Images,
            build: Build::Generated { cols, rows, f },
        });
    }
    let sizes = TEXT_ROWS
        .map(|rows| (rows, false))
        .into_iter()
        .chain(SQUARE_ROWS.map(|rows| (rows, true)));
    for (rows, square) in sizes {
        for (name, font) in FONTS {
            for (suffix, group, colors, crisp) in [
                ("light", Group::Text, LIGHT, false),
                ("dark", Group::Text, DARK, false),
                ("crisp", Group::Crisp, LIGHT, true),
            ] {
                let shape = if square { "-square" } else { "" };
                specs.push(Spec {
                    name: format!("{name}-{rows}{shape}-{suffix}"),
                    group,
                    build: Build::Text(Text {
                        words: TEXT,
                        font,
                        rows,
                        colors,
                        crisp,
                        square,
                        phases: &[[0.0, 0.0]],
                    }),
                });
            }
        }
    }
    specs.push(Spec {
        name: "banner".into(),
        group: Group::Text,
        build: Build::Text(Text {
            words: BANNER,
            font: "Bungee-Regular.ttf",
            rows: 16,
            colors: LIGHT,
            crisp: false,
            square: true,
            phases: &BANNER_PHASES,
        }),
    });
    specs
}

/// The workspace root.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The matrix of an RGBA image fitted to `cols × rows` cells, as image
/// views resample it, laid over black.
fn matrix(data: &[u8], width: usize, height: usize, cols: usize, rows: usize) -> Vec<[u8; 3]> {
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

/// The weights of an area resample of `from` samples to `to`: for each
/// output sample, the input samples it covers and the share of each.
fn area_weights(from: usize, to: usize) -> Vec<Vec<(usize, f32)>> {
    let step = from as f64 / to as f64;
    (0..to)
        .map(|i| {
            let (start, end) = (i as f64 * step, (i + 1) as f64 * step);
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).min(from);
            (first..last)
                .map(|j| {
                    let overlap = end.min((j + 1) as f64) - start.max(j as f64);
                    (j, (overlap / step) as f32)
                })
                .filter(|(_, w)| *w > 0.0)
                .collect()
        })
        .collect()
}

/// Resample linear pixels by area, separably.
fn resample_area(
    pixels: &[[f32; 3]],
    width: usize,
    height: usize,
    to_width: usize,
    to_height: usize,
) -> Vec<[f32; 3]> {
    let across = area_weights(width, to_width);
    let down = area_weights(height, to_height);
    let mut rows = vec![[0.0f32; 3]; to_width * height];
    for y in 0..height {
        for (x, weights) in across.iter().enumerate() {
            let mut sum = [0.0f32; 3];
            for &(j, w) in weights {
                let p = pixels[y * width + j];
                sum = [0, 1, 2].map(|c| sum[c] + w * p[c]);
            }
            rows[y * to_width + x] = sum;
        }
    }
    let mut out = vec![[0.0f32; 3]; to_width * to_height];
    for (y, weights) in down.iter().enumerate() {
        for &(j, w) in weights {
            for x in 0..to_width {
                let p = rows[j * to_width + x];
                let o = &mut out[y * to_width + x];
                *o = [0, 1, 2].map(|c| o[c] + w * p[c]);
            }
        }
    }
    out
}

/// A photograph fitted to `cols` cells across, with the rows that keep its
/// shape.
fn photo(path: &str, cols: usize) -> Item {
    let source = image::open(root().join(path)).expect("photo").into_rgba8();
    let (w, h) = (source.width() as usize, source.height() as usize);
    let rows = ((cols * CELL_W * h) as f64 / (w * CELL_H) as f64).round() as usize;
    let linear: Vec<[f32; 3]> = source
        .as_raw()
        .chunks(4)
        .map(|p| color::decode_rgb([p[0], p[1], p[2]]))
        .collect();
    Item {
        cols,
        rows,
        reference: resample_area(&linear, w, h, cols * CELL_W, rows * CELL_H),
        input: Input::Rgb(matrix(source.as_raw(), w, h, cols, rows)),
    }
}

/// A generated image at device pixels, each the mean of 4×4 samples, so
/// edges are antialiased.
fn generated(cols: usize, rows: usize, f: Generator) -> Item {
    let (width, height) = (cols * CELL_W, rows * CELL_H);
    let scale = 1.0 / height as f32;
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];
            for j in 0..4 {
                for i in 0..4 {
                    let u = (x as f32 + (i as f32 + 0.5) / 4.0) * scale;
                    let v = (y as f32 + (j as f32 + 0.5) / 4.0) * scale;
                    let c = f(u, v);
                    sum = [0, 1, 2].map(|k| sum[k] + c[k] / 16.0);
                }
            }
            data.extend(sum.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
            data.push(255);
        }
    }
    let reference = data
        .chunks(4)
        .map(|p| color::decode_rgb([p[0], p[1], p[2]]))
        .collect();
    Item {
        cols,
        rows,
        reference,
        input: Input::Rgb(matrix(&data, width, height, cols, rows)),
    }
}

/// Text in a font: a line for each phase, its line box `rows` cells high,
/// with a margin of one cell around each. Square text takes a cell as
/// square, as a font banner does by default, and so draws at twice its
/// height.
fn text(spec: &Text) -> Item {
    let Text {
        words,
        font,
        rows,
        colors: (fg, bg),
        crisp,
        square,
        phases,
    } = *spec;
    let path = root().join("crates/canopy-widgets/assets/fonts").join(font);
    let bytes = fs::read(&path).expect("font");
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("font");
    // Square text rasterizes on cells as tall as they are wide, then
    // stretches each pixel row to two.
    let (cell_h, stretch) = if square {
        (CELL_W, CELL_H / CELL_W)
    } else {
        (CELL_H, 1)
    };
    let unit = font.horizontal_line_metrics(100.0).expect("line metrics");
    let px = (rows * cell_h) as f32 * 100.0 / (unit.ascent - unit.descent);
    let metrics = font.horizontal_line_metrics(px).expect("line metrics");
    let glyphs: Vec<_> = words.chars().map(|ch| font.rasterize(ch, px)).collect();
    let advance: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
    let cols = (advance / CELL_W as f32).ceil() as usize + 2;
    let total_rows = phases.len() * (rows + 1) + 1;
    let (width, height) = (cols * CELL_W, total_rows * cell_h);
    let mut coverage = vec![0u8; width * height];
    for (line, [phase_x, phase_y]) in phases.iter().enumerate() {
        let top = (line * (rows + 1) + 1) as f32;
        let baseline = cell_h as f32 * (top + phase_y) + metrics.ascent;
        let mut pen = CELL_W as f32 * (1.0 + phase_x);
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
    }
    let coverage: Vec<u8> = coverage
        .chunks(width)
        .flat_map(|row| (0..stretch).flat_map(move |_| row.iter().copied()))
        .collect();
    let (fg_linear, bg_linear) = (color::decode_rgb(fg), color::decode_rgb(bg));
    let reference = coverage
        .iter()
        .map(|c| {
            let a = f32::from(*c) / 255.0;
            [0, 1, 2].map(|k| bg_linear[k] + a * (fg_linear[k] - bg_linear[k]))
        })
        .collect();
    // Each matrix pixel is the mean coverage of the device pixels it spans.
    let (sx, sy) = (CELL_W / MATRIX, CELL_H / MATRIX);
    let matrix_width = cols * MATRIX;
    let matrix = (0..total_rows * MATRIX * matrix_width)
        .map(|i| {
            let (mx, my) = (i % matrix_width, i / matrix_width);
            let sum: u32 = (0..sy)
                .flat_map(|dy| (0..sx).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| u32::from(coverage[(my * sy + dy) * width + mx * sx + dx]))
                .sum();
            let n = (sx * sy) as u32;
            ((sum + n / 2) / n) as u8
        })
        .collect();
    Item {
        cols,
        rows: total_rows,
        reference,
        input: Input::Coverage {
            matrix,
            fg,
            bg,
            crisp,
        },
    }
}

/// A zone plate: rings whose frequency rises to the matrix's limit at the
/// corners.
fn zone_plate(x: f32, y: f32) -> [f32; 3] {
    let r2 = (x - 0.5).powi(2) + (y - 0.5).powi(2);
    let v = 0.5 + 0.5 * (r2 * 400.0).cos();
    [v; 3]
}

/// Thin dark lines through the center every 15 degrees, on paper.
fn lines(x: f32, y: f32) -> [f32; 3] {
    let (dx, dy) = (x - 0.5, y - 0.5);
    let near = (0..12).any(|k| {
        let angle = (k as f32 * 15.0).to_radians();
        (dx * angle.sin() - dy * angle.cos()).abs() < 0.003
    });
    if near {
        [0.1, 0.1, 0.15]
    } else {
        [0.95, 0.92, 0.85]
    }
}

/// A red disc on a gradient.
fn disc(x: f32, y: f32) -> [f32; 3] {
    if (x - 0.75).powi(2) + (y - 0.5).powi(2) < 0.35 * 0.35 {
        [0.9, 0.3, 0.2]
    } else {
        [x / 1.5 * 0.3, 0.3 + y * 0.4, 0.6]
    }
}

/// The sRGB of a hue, a saturation, and a value, each from 0 to 1.
fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = |n: f32| {
        let k = (n + h) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [f(5.0), f(3.0), f(1.0)]
}

/// Four bands of smooth ramps: gray, the hues at full saturation, muted
/// hues, and blue to orange.
fn ramps(x: f32, y: f32) -> [f32; 3] {
    let t = x / 2.0;
    match (y * 4.0) as usize {
        0 => [t; 3],
        1 => hsv(t, 1.0, 1.0),
        2 => hsv(t, 0.5, 0.6),
        _ => [0.1 + 0.85 * t, 0.15 + 0.4 * t, 0.6 - 0.5 * t],
    }
}

/// A hash of a lattice point to -1 to 1.
fn lattice(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343)
        ^ (iy as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 32767.5 - 1.0
}

/// Smooth value noise at a point of the lattice's units.
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let top = lattice(ix, iy, seed) * (1.0 - sx) + lattice(ix + 1, iy, seed) * sx;
    let bottom = lattice(ix, iy + 1, seed) * (1.0 - sx) + lattice(ix + 1, iy + 1, seed) * sx;
    top * (1.0 - sy) + bottom * sy
}

/// Color noise whose amplitude falls as one over the frequency, as in
/// natural images.
fn noise(x: f32, y: f32) -> [f32; 3] {
    let field = |seed: u32| {
        (0..7)
            .map(|octave| {
                let f = 3.0 * 2f32.powi(octave);
                value_noise(x * f, y * f, seed + octave as u32) / 2f32.powi(octave)
            })
            .sum::<f32>()
            / 1.6
    };
    let (warm, cool, shade) = (field(1), field(11), field(23));
    [
        0.5 + 0.35 * warm + 0.1 * cool,
        0.45 + 0.3 * warm - 0.15 * shade,
        0.4 + 0.2 * cool + 0.25 * shade,
    ]
}

/// Squares of six saturated colors, two and a half cells across, so that
/// many cells hold three colors.
fn checker(x: f32, y: f32) -> [f32; 3] {
    const COLORS: [[f32; 3]; 6] = [
        [0.85, 0.15, 0.15],
        [0.15, 0.7, 0.2],
        [0.15, 0.3, 0.85],
        [0.95, 0.85, 0.2],
        [0.8, 0.2, 0.75],
        [0.15, 0.8, 0.8],
    ];
    let size = 1.0 / 12.8;
    let (i, j) = ((x / size) as usize, (y / size) as usize);
    COLORS[(i + 2 * j) % COLORS.len()]
}

/// Whether a point is inside a polygon, by the even-odd rule.
fn inside(points: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut odd = false;
    for (i, &(x0, y0)) in points.iter().enumerate() {
        let (x1, y1) = points[(i + 1) % points.len()];
        if (y0 > y) != (y1 > y) && x < x0 + (y - y0) / (y1 - y0) * (x1 - x0) {
            odd = !odd;
        }
    }
    odd
}

/// Flat shapes on paper: discs of falling sizes, rings, a triangle, and a
/// star.
fn shapes(x: f32, y: f32) -> [f32; 3] {
    let discs = [
        (0.25, 0.3, 0.2, [0.85, 0.3, 0.25]),
        (0.6, 0.25, 0.12, [0.2, 0.45, 0.8]),
        (0.8, 0.5, 0.07, [0.15, 0.6, 0.3]),
        (0.95, 0.3, 0.04, [0.9, 0.7, 0.1]),
        (1.05, 0.15, 0.02, [0.5, 0.2, 0.6]),
    ];
    for (cx, cy, r, c) in discs {
        if (x - cx).powi(2) + (y - cy).powi(2) < r * r {
            return c;
        }
    }
    for (cx, cy, r, w) in [(0.3, 0.75, 0.15, 0.02), (0.65, 0.75, 0.1, 0.008)] {
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        if (d - r).abs() < w / 2.0 {
            return [0.15, 0.15, 0.2];
        }
    }
    if inside(&[(0.9, 0.95), (1.1, 0.6), (1.25, 0.92)], x, y) {
        return [0.3, 0.65, 0.65];
    }
    let star: Vec<(f32, f32)> = (0..10)
        .map(|k| {
            let angle = (k as f32 * 36.0 - 90.0).to_radians();
            let r = if k % 2 == 0 { 0.16 } else { 0.065 };
            (1.2 + r * angle.cos(), 0.3 + r * angle.sin())
        })
        .collect();
    if inside(&star, x, y) {
        return [0.95, 0.55, 0.15];
    }
    [0.95, 0.93, 0.88]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_weights_cover_each_output_once() {
        for (from, to) in [(10, 4), (4, 10), (683, 1024), (7, 7)] {
            for weights in area_weights(from, to) {
                let total: f32 = weights.iter().map(|(_, w)| w).sum();
                assert!((total - 1.0).abs() < 1e-4, "{from} {to} {total}");
            }
        }
    }

    #[test]
    fn suite_names_are_unique() {
        let specs = suite();
        let mut names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), specs.len());
    }

    /// One line of light antialiased text.
    fn line(words: &'static str, font: &'static str, rows: usize, square: bool) -> Text {
        Text {
            words,
            font,
            rows,
            colors: LIGHT,
            crisp: false,
            square,
            phases: &[[0.0, 0.0]],
        }
    }

    #[test]
    fn each_phase_draws_a_line_and_moves_its_ink() {
        let phases = text(&Text {
            phases: &BANNER_PHASES,
            ..line(BANNER, "Bungee-Regular.ttf", 4, true)
        });
        assert_eq!(phases.rows, 4 * 5 + 1);
        // The first ink of each line moves right with its phase.
        let width = phases.width();
        let first_ink = |line: usize| {
            let rows = (line * 5 + 1) * CELL_H..(line * 5 + 5) * CELL_H;
            rows.flat_map(|y| (0..width).map(move |x| (x, y)))
                .filter(|(x, y)| phases.reference[y * width + x][0] > 0.5)
                .map(|(x, _)| x)
                .min()
                .expect("ink")
        };
        let starts: Vec<usize> = (0..4).map(first_ink).collect();
        assert!(starts.windows(2).all(|w| w[1] > w[0]), "{starts:?}");
    }

    #[test]
    fn text_fills_its_rows_and_its_matrix_matches_its_reference() {
        let item = text(&line(TEXT, "FiraMono-Regular.ttf", 3, false));
        assert_eq!(item.rows, 5);
        assert_eq!(item.reference.len(), item.width() * item.rows * CELL_H);
        let Input::Coverage { matrix, .. } = &item.input else {
            panic!("text takes coverage");
        };
        assert_eq!(matrix.len(), item.cols * item.rows * MATRIX * MATRIX);
        // The margin rows hold no ink.
        assert!(
            matrix[..item.cols * MATRIX * MATRIX]
                .iter()
                .all(|v| *v == 0)
        );
        assert!(matrix.iter().any(|v| *v > 200));
    }

    #[test]
    fn square_text_draws_at_twice_its_height() {
        let tall = text(&line(TEXT, "Bungee-Regular.ttf", 4, false));
        let square = text(&line(TEXT, "Bungee-Regular.ttf", 8, true));
        // On square cells, eight rows hold the glyphs of four tall rows, at
        // about the same width.
        assert_eq!(square.rows, 10);
        assert!(
            tall.cols.abs_diff(square.cols) <= 1,
            "{} {}",
            tall.cols,
            square.cols
        );
        // Each pixel row is doubled.
        let width = square.width();
        for y in (0..square.reference.len() / width).step_by(2) {
            assert_eq!(
                square.reference[y * width..(y + 1) * width],
                square.reference[(y + 1) * width..(y + 2) * width]
            );
        }
    }

    #[test]
    fn each_tier_draws_every_cell_of_an_item() {
        let item = generated(6, 3, disc);
        for tier in Tier::ALL {
            assert_eq!(item.shown(tier).len(), 18);
        }
    }
}
