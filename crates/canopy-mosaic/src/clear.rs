//! Matching a raster with alpha against a clear ground.
//!
//! [`rgb_into`](crate::rgb_into) needs an opaque raster, so a caller first
//! lays the image over the ground of each cell. That suits a terminal, whose
//! ground is known. An export to a page of unknown color needs cells whose
//! empty part stays clear. [`clear_into`] gives such cells: each pen is a
//! color, or `None` for clear.

use crate::{
    Cell, RasterView, coverage_into,
    geometry::{MATRIX, Mask, Shape},
    raster::{LinearRaster, encode_table, to_u16},
    rgb_into,
    symbols::SymbolSet,
};

/// A pixel counts as ink when its alpha is at least half.
const INK_ALPHA: u16 = u16::MAX / 2;

/// Match each cell of `raster`, in linear light with premultiplied alpha,
/// against a clear ground. Writes the cells row by row into `out`, replacing
/// what it held.
///
/// A cell with no ink is clear on both pens. A cell with ink in every pixel
/// matches as [`rgb_into`] does, with two color pens. In any other cell, the
/// alpha of the pixels chooses the character, the ink takes the mean color
/// of its pixels, and the other pen is clear.
///
/// # Panics
///
/// Panics when the width or the height of `raster` is not a multiple of 8.
pub fn clear_into(raster: &LinearRaster, set: &SymbolSet, out: &mut Vec<Cell<Option<[u8; 3]>>>) {
    let (width, height) = (raster.width(), raster.height());
    let pixels = raster.pixels();
    let encode = encode_table();
    let straight: Vec<[u8; 3]> = pixels
        .iter()
        .map(|pixel| straight_color(*pixel, encode))
        .collect();
    let alpha: Vec<u8> = pixels.iter().map(|pixel| (pixel[3] >> 8) as u8).collect();
    let opaque = RasterView::new(&straight, width, height, width).expect("raster in whole cells");
    let coverage = RasterView::new(&alpha, width, height, width).expect("raster in whole cells");
    let mut colors = Vec::new();
    rgb_into(&opaque, set, &mut colors);
    let mut shapes = Vec::new();
    coverage_into(&coverage, set, &mut shapes);
    let cols = width / MATRIX;
    out.clear();
    out.reserve(colors.len());
    for (index, (color, shape)) in colors.iter().zip(&shapes).enumerate() {
        let (col, row) = (index % cols, index / cols);
        let cell_pixels = |mask: Mask| {
            (0..MATRIX * MATRIX)
                .filter(move |at| mask.get(at % MATRIX, at / MATRIX))
                .map(move |at| {
                    let (x, y) = (col * MATRIX + at % MATRIX, row * MATRIX + at / MATRIX);
                    pixels[y * width + x]
                })
        };
        let ink = cell_pixels(Mask(u64::MAX))
            .filter(|pixel| pixel[3] >= INK_ALPHA)
            .count();
        let clear = Cell {
            ch: ' ',
            fg: None,
            bg: None,
        };
        let cell = if ink == 0 {
            clear
        } else if ink == MATRIX * MATRIX {
            Cell {
                ch: color.ch,
                fg: Some(color.fg),
                bg: Some(color.bg),
            }
        } else {
            let half = u8::MAX / 2;
            match (shape.fg > half, shape.bg > half) {
                (true, true) => Cell {
                    ch: color.ch,
                    fg: Some(color.fg),
                    bg: Some(color.bg),
                },
                (false, false) => clear,
                (fg_ink, _) => {
                    let mask = Shape::of(shape.ch).map_or(Mask(u64::MAX), |s| s.mask());
                    let mask = if fg_ink { mask } else { mask.inverse() };
                    let pen = Some(mean_color(cell_pixels(mask), encode));
                    if fg_ink {
                        Cell {
                            ch: shape.ch,
                            fg: pen,
                            bg: None,
                        }
                    } else {
                        Cell {
                            ch: shape.ch,
                            fg: None,
                            bg: pen,
                        }
                    }
                }
            }
        };
        out.push(cell);
    }
}

/// The sRGB color of one premultiplied linear pixel without its alpha. A
/// clear pixel is black.
fn straight_color(pixel: [u16; 4], encode: &[u8]) -> [u8; 3] {
    if pixel[3] == 0 {
        return [0; 3];
    }
    let alpha = f32::from(pixel[3]);
    [0, 1, 2].map(|ch| encode[usize::from(to_u16(f32::from(pixel[ch]) / alpha))])
}

/// The alpha-weighted mean sRGB color of `pixels`, which are premultiplied
/// and linear. No coverage at all gives black.
fn mean_color(pixels: impl Iterator<Item = [u16; 4]>, encode: &[u8]) -> [u8; 3] {
    let mut sum = [0f64; 4];
    for pixel in pixels {
        for (total, value) in sum.iter_mut().zip(pixel) {
            *total += f64::from(value);
        }
    }
    if sum[3] == 0.0 {
        return [0; 3];
    }
    [0, 1, 2].map(|ch| encode[usize::from(to_u16((sum[ch] / sum[3]) as f32))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Tier,
        raster::{Image, Placement, resample},
    };

    /// Resample an RGBA image of `width` by `height` at one source pixel for
    /// each raster pixel.
    fn raster(data: &[u8], width: usize, height: usize) -> LinearRaster {
        let image = Image::new(data, width, height).unwrap();
        let mut out = LinearRaster::new();
        resample(
            &image,
            Placement {
                origin: [0.0, 0.0],
                step: [1.0, 1.0],
            },
            width,
            height,
            &mut out,
        );
        out
    }

    /// An RGBA image of `width` by `height` from a function of the pixel.
    fn image(width: usize, height: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .collect()
    }

    fn matched(data: &[u8], width: usize, height: usize) -> Vec<Cell<Option<[u8; 3]>>> {
        let mut cells = Vec::new();
        clear_into(
            &raster(data, width, height),
            SymbolSet::tier(Tier::Halves),
            &mut cells,
        );
        cells
    }

    #[test]
    fn an_opaque_cell_has_two_pens_and_a_clear_cell_none() {
        let data = image(16, 8, |x, _| {
            if x < 8 {
                [200, 30, 30, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let cells = matched(&data, 16, 8);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].fg, Some([200, 30, 30]));
        assert!(cells[0].bg.is_some());
        assert_eq!(
            cells[1],
            Cell {
                ch: ' ',
                fg: None,
                bg: None
            }
        );
    }

    #[test]
    fn a_part_clear_cell_inks_its_shape_and_leaves_the_rest_clear() {
        let data = image(8, 8, |_, y| {
            if y < 4 {
                [20, 40, 220, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let cells = matched(&data, 8, 8);
        let cell = cells[0];
        let (ink, rest) = match cell.ch {
            '▀' => (cell.fg, cell.bg),
            '▄' => (cell.bg, cell.fg),
            other => panic!("unexpected character {other:?}"),
        };
        assert_eq!(ink, Some([20, 40, 220]));
        assert_eq!(rest, None);
    }

    #[test]
    fn a_faint_cell_is_clear() {
        let data = image(8, 8, |_, _| [255, 255, 255, 40]);
        let cells = matched(&data, 8, 8);
        assert_eq!(cells[0].fg, None);
        assert_eq!(cells[0].bg, None);
    }
}
