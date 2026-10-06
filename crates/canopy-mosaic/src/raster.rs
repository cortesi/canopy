//! Preparing the raster of an image for matching.
//!
//! [`resample`] averages the source pixels under each matrix pixel's
//! footprint, in linear light with premultiplied alpha. [`composite`] then
//! lays each cell's pixels over that cell's ground color, and yields the
//! opaque sRGB raster that [`crate::rgb_into`] matches.

use std::sync::OnceLock;

use crate::{geometry::MATRIX, matcher::RasterError};

/// A borrowed RGBA8 image in sRGB, with unassociated alpha.
#[derive(Debug, Clone, Copy)]
pub struct Image<'a> {
    /// The pixels, 4 bytes each, row by row.
    data: &'a [u8],
    /// Width in pixels.
    width: usize,
    /// Height in pixels.
    height: usize,
}

impl<'a> Image<'a> {
    /// View `data` as an image of `width` by `height` RGBA8 pixels.
    pub fn new(data: &'a [u8], width: usize, height: usize) -> Result<Self, RasterError> {
        let needed = width * height * 4;
        if data.len() < needed {
            return Err(RasterError::Length {
                needed,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            width,
            height,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }
}

/// Where the pixels of a raster fall in a source image.
///
/// Output pixel `(x, y)` covers the source rectangle from
/// `origin + (x, y) × step` to `origin + (x + 1, y + 1) × step`, in source
/// pixels. The rectangle can span many source pixels or part of one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Source coordinates of the top-left corner of the first output pixel.
    pub origin: [f32; 2],
    /// Source pixels per output pixel, on each axis.
    pub step: [f32; 2],
}

/// A raster in linear light with premultiplied alpha, 16 bits a channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinearRaster {
    /// Width in pixels.
    width: usize,
    /// Height in pixels.
    height: usize,
    /// Red, green, blue, and alpha of each pixel, row by row.
    pixels: Vec<[u16; 4]>,
}

impl LinearRaster {
    /// An empty raster.
    pub fn new() -> Self {
        Self::default()
    }

    /// Width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// The pixels, row by row.
    pub fn pixels(&self) -> &[[u16; 4]] {
        &self.pixels
    }
}

/// The source pixels under each output pixel on one axis, and their weights.
struct Spans {
    /// For each output pixel, its first source pixel and its run in
    /// `weights`.
    spans: Vec<(usize, usize, usize)>,
    /// The share of each output pixel's footprint that each source pixel
    /// covers.
    weights: Vec<f32>,
}

impl Spans {
    /// The spans of `count` output pixels from `origin` in steps of `step`,
    /// over `limit` source pixels. A footprint that leaves the source keeps
    /// its full size, so the part outside adds transparency.
    fn new(origin: f32, step: f32, count: usize, limit: usize) -> Self {
        let mut spans = Vec::with_capacity(count);
        let mut weights = Vec::new();
        for k in 0..count {
            let start = weights.len();
            let a = origin + k as f32 * step;
            let b = a + step;
            let first = a.floor().max(0.0) as usize;
            let last = (b.ceil().max(0.0) as usize).min(limit);
            if step > 0.0 {
                for i in first..last {
                    let overlap = b.min(i as f32 + 1.0) - a.max(i as f32);
                    if overlap > 0.0 {
                        weights.push(overlap / step);
                    } else {
                        weights.push(0.0);
                    }
                }
            }
            spans.push((first, start, weights.len() - start));
        }
        Self { spans, weights }
    }

    /// The source pixels and weights of output pixel `k`.
    fn of(&self, k: usize) -> impl Iterator<Item = (usize, f32)> + '_ {
        let (first, start, len) = self.spans[k];
        self.weights[start..start + len]
            .iter()
            .enumerate()
            .map(move |(i, weight)| (first + i, *weight))
    }
}

/// sRGB channel values in linear light.
fn decode_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; 256];
        for (value, linear) in table.iter_mut().enumerate() {
            let v = value as f32 / 255.0;
            *linear = if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            };
        }
        table
    })
}

/// sRGB channel values for each 16-bit linear value.
fn encode_table() -> &'static [u8] {
    static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=u16::MAX)
            .map(|linear| {
                let v = f32::from(linear) / f32::from(u16::MAX);
                let encoded = if v <= 0.003_130_8 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                };
                (encoded * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    })
}

/// A linear value in 0 to 1 as 16 bits.
fn to_u16(value: f32) -> u16 {
    (value * f32::from(u16::MAX))
        .round()
        .clamp(0.0, f32::from(u16::MAX)) as u16
}

/// Resample `image` to a raster of `width` by `height` pixels at
/// `placement`, into `out`. Each pixel is the area-weighted mean of the
/// source pixels under its footprint, in linear light with premultiplied
/// alpha. Transparent source pixels add nothing, whatever their color, and a
/// footprint outside the image is transparent there.
pub fn resample(
    image: &Image<'_>,
    placement: Placement,
    width: usize,
    height: usize,
    out: &mut LinearRaster,
) {
    out.width = width;
    out.height = height;
    out.pixels.clear();
    out.pixels.resize(width * height, [0; 4]);
    let decode = decode_table();
    let columns = Spans::new(placement.origin[0], placement.step[0], width, image.width);
    let rows = Spans::new(placement.origin[1], placement.step[1], height, image.height);
    let mut row = vec![[0f32; 4]; width];
    for y in 0..height {
        row.fill([0.0; 4]);
        for (source_y, weight_y) in rows.of(y) {
            let line = &image.data[source_y * image.width * 4..(source_y + 1) * image.width * 4];
            for (x, total) in row.iter_mut().enumerate() {
                for (source_x, weight_x) in columns.of(x) {
                    let pixel = &line[source_x * 4..source_x * 4 + 4];
                    let alpha = f32::from(pixel[3]) / 255.0;
                    let weight = weight_x * weight_y;
                    let covered = weight * alpha;
                    total[0] += covered * decode[usize::from(pixel[0])];
                    total[1] += covered * decode[usize::from(pixel[1])];
                    total[2] += covered * decode[usize::from(pixel[2])];
                    total[3] += covered;
                }
            }
        }
        for (pixel, total) in out.pixels[y * width..(y + 1) * width].iter_mut().zip(&row) {
            *pixel = total.map(to_u16);
        }
    }
}

/// Lay each cell's pixels of `raster` over that cell's ground color, in
/// linear light, and write opaque sRGB pixels into `out`. `grounds` holds one
/// sRGB color for each 8×8 cell, row by row.
///
/// # Panics
///
/// Panics when `grounds` has fewer colors than the raster has cells.
pub fn composite(raster: &LinearRaster, grounds: &[[u8; 3]], out: &mut Vec<[u8; 3]>) {
    let cols = raster.width / MATRIX;
    assert!(
        grounds.len() >= cols * (raster.height / MATRIX),
        "one ground for each cell"
    );
    let decode = decode_table();
    let encode = encode_table();
    out.clear();
    out.reserve(raster.pixels.len());
    for (index, pixel) in raster.pixels.iter().enumerate() {
        let (x, y) = (index % raster.width, index / raster.width);
        let ground = grounds[(y / MATRIX) * cols + x / MATRIX];
        let clear = 1.0 - f32::from(pixel[3]) / f32::from(u16::MAX);
        let channel = |ch: usize| {
            let over = f32::from(pixel[ch]) / f32::from(u16::MAX)
                + clear * decode[usize::from(ground[ch])];
            encode[usize::from(to_u16(over))]
        };
        out.push([channel(0), channel(1), channel(2)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resample and composite a whole image onto one ground.
    fn shown(
        data: &[u8],
        size: (usize, usize),
        placement: Placement,
        out_size: (usize, usize),
        ground: [u8; 3],
    ) -> Vec<[u8; 3]> {
        let image = Image::new(data, size.0, size.1).expect("image");
        let mut raster = LinearRaster::new();
        resample(&image, placement, out_size.0, out_size.1, &mut raster);
        let cells = out_size.0.div_ceil(MATRIX) * out_size.1.div_ceil(MATRIX);
        let mut out = Vec::new();
        composite(&raster, &vec![ground; cells.max(1)], &mut out);
        out
    }

    /// The placement that maps an 8×8 raster onto an image of `size`.
    fn fit(size: (usize, usize)) -> Placement {
        Placement {
            origin: [0.0, 0.0],
            step: [size.0 as f32 / 8.0, size.1 as f32 / 8.0],
        }
    }

    #[test]
    fn the_tables_round_trip_every_srgb_value() {
        let (decode, encode) = (decode_table(), encode_table());
        for value in 0..=255u8 {
            assert_eq!(
                encode[usize::from(to_u16(decode[usize::from(value)]))],
                value
            );
        }
    }

    #[test]
    fn averaging_happens_in_linear_light() {
        // Black and white stripes average to linear half, not sRGB 128.
        let data: Vec<u8> = (0..16 * 8)
            .flat_map(|i| {
                let v = if i % 2 == 0 { 0 } else { 255 };
                [v, v, v, 255]
            })
            .collect();
        let out = shown(&data, (16, 8), fit((16, 8)), (8, 8), [0, 0, 0]);
        assert!(out.iter().all(|p| *p == [188, 188, 188]), "{:?}", out[0]);
    }

    #[test]
    fn transparent_pixels_add_nothing_whatever_their_color() {
        // A transparent red pixel beside an opaque blue one.
        let data: Vec<u8> = (0..16 * 8)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [255, 0, 0, 0]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        let out = shown(&data, (16, 8), fit((16, 8)), (8, 8), [0, 0, 0]);
        assert_eq!(out[0][0], 0, "no red bleeds in");
        assert!(out[0][2] > 128, "half-covered blue over black");
    }

    #[test]
    fn a_half_covered_pixel_lays_over_the_ground() {
        let data = [200u8, 100, 40, 128];
        let placement = Placement {
            origin: [0.0, 0.0],
            step: [1.0 / 8.0, 1.0 / 8.0],
        };
        let out = shown(&data, (1, 1), placement, (8, 8), [0, 0, 200]);
        let decode = decode_table();
        let alpha = 128.0 / 255.0;
        let expected: [u8; 3] = [0, 1, 2].map(|ch| {
            let over = alpha * decode[usize::from(data[ch])]
                + (1.0 - alpha) * decode[usize::from([0u8, 0, 200][ch])];
            encode_table()[usize::from(to_u16(over))]
        });
        assert!(out.iter().all(|p| *p == expected));
    }

    #[test]
    fn magnification_reads_whole_source_pixels() {
        // Two pixels, red and green, magnified to 8 columns each.
        let data = [255u8, 0, 0, 255, 0, 255, 0, 255];
        let placement = Placement {
            origin: [0.0, 0.0],
            step: [2.0 / 16.0, 1.0 / 8.0],
        };
        let image = Image::new(&data, 2, 1).expect("image");
        let mut raster = LinearRaster::new();
        resample(&image, placement, 16, 8, &mut raster);
        let mut out = Vec::new();
        composite(&raster, &[[0, 0, 0]; 2], &mut out);
        assert_eq!(out[0], [255, 0, 0]);
        assert_eq!(out[7], [255, 0, 0]);
        assert_eq!(out[8], [0, 255, 0]);
    }

    #[test]
    fn a_fractional_pan_blends_the_straddled_pixels() {
        // Shift by half a source pixel: the first output pixel straddles
        // black and white.
        let data = [0u8, 0, 0, 255, 255, 255, 255, 255];
        let placement = Placement {
            origin: [0.5, 0.0],
            step: [1.0, 1.0 / 8.0],
        };
        let image = Image::new(&data, 2, 1).expect("image");
        let mut raster = LinearRaster::new();
        resample(&image, placement, 8, 8, &mut raster);
        let half = raster.pixels()[0];
        assert!(half[0].abs_diff(u16::MAX / 2) <= 1, "{half:?}");
        assert_eq!(
            raster.pixels()[1][3],
            u16::MAX / 2 + 1,
            "half outside the image"
        );
    }

    #[test]
    fn footprints_outside_the_image_show_the_ground() {
        let data = [255u8, 255, 255, 255];
        let placement = Placement {
            origin: [-4.0, -4.0],
            step: [1.0, 1.0],
        };
        let out = shown(&data, (1, 1), placement, (8, 8), [10, 20, 30]);
        assert_eq!(out[0], [10, 20, 30]);
        assert_eq!(out[4 * 8 + 4], [255, 255, 255]);
    }

    #[test]
    fn minification_averages_every_covered_pixel() {
        // A 64×64 checkerboard of opaque gray pixels averages to the gray.
        let data: Vec<u8> = (0..64 * 64).flat_map(|_| [90u8, 90, 90, 255]).collect();
        let out = shown(&data, (64, 64), fit((64, 64)), (8, 8), [0, 0, 0]);
        assert!(out.iter().all(|p| *p == [90, 90, 90]));
    }
}
