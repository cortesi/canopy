//! The display model: how a terminal shows a grid of mosaic cells.
//!
//! The terminal draws each cell's character from its Unicode geometry, at
//! cells of [`CELL_W`] by [`CELL_H`] device pixels. A device pixel on the
//! edge of the ink takes the share of each color that its area covers, and
//! the light of the two colors adds. Terminals that draw these characters
//! from geometry draw them this way, and fonts that have them follow the
//! same shapes.

use std::{collections::HashMap, sync::OnceLock};

use canopy_mosaic::{Family, geometry::Shape};

use crate::color;

/// Device pixels across a cell.
pub const CELL_W: usize = 16;
/// Device pixels down a cell: a cell is twice as tall as it is wide.
pub const CELL_H: usize = 32;

/// One cell as the terminal shows it.
#[derive(Debug, Clone, Copy)]
pub struct Shown {
    /// The character.
    pub ch: char,
    /// The sRGB color of the ink.
    pub fg: [u8; 3],
    /// The sRGB color of the rest of the cell.
    pub bg: [u8; 3],
}

/// The ink coverage of each character that the matcher can emit, 0 to 1 for
/// each device pixel of a cell.
fn coverage(ch: char) -> &'static [f32] {
    static TABLE: OnceLock<HashMap<char, Vec<f32>>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        Family::ALL
            .iter()
            .flat_map(|family| family.chars())
            .chain([' '])
            .map(|ch| {
                let shape = Shape::of(ch).expect("every symbol has a shape");
                let (sx, sy) = (1.0 / CELL_W as f32, 1.0 / CELL_H as f32);
                let ink = (0..CELL_W * CELL_H)
                    .map(|i| {
                        let (x, y) = ((i % CELL_W) as f32 * sx, (i / CELL_W) as f32 * sy);
                        shape.coverage_of(x, y, x + sx, y + sy)
                    })
                    .collect();
                (ch, ink)
            })
            .collect()
    });
    table
        .get(&ch)
        .unwrap_or_else(|| panic!("no shape for {ch:?}"))
}

/// Draw `cols` cells across, row by row, as linear sRGB device pixels.
pub fn draw(cells: &[Shown], cols: usize) -> Vec<[f32; 3]> {
    let rows = cells.len() / cols;
    let width = cols * CELL_W;
    let mut out = vec![[0.0; 3]; width * rows * CELL_H];
    for (index, cell) in cells.iter().enumerate() {
        let (fg, bg) = (color::decode_rgb(cell.fg), color::decode_rgb(cell.bg));
        let ink = coverage(cell.ch);
        let (cx, cy) = (index % cols, index / cols);
        for y in 0..CELL_H {
            let row = (cy * CELL_H + y) * width + cx * CELL_W;
            for x in 0..CELL_W {
                let a = ink[y * CELL_W + x];
                out[row + x] = [0, 1, 2].map(|c| bg[c] + a * (fg[c] - bg[c]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_block_splits_the_cell_and_a_diagonal_blends_its_edge() {
        let (white, black) = ([255; 3], [0; 3]);
        let half = draw(
            &[Shown {
                ch: '▀',
                fg: white,
                bg: black,
            }],
            1,
        );
        assert_eq!(half[0], [1.0; 3]);
        assert_eq!(half[CELL_W * CELL_H - 1], [0.0; 3]);
        let lit: f32 = half.iter().map(|p| p[0]).sum();
        assert_eq!(lit, (CELL_W * CELL_H / 2) as f32);
        // A smooth mosaic lights its area, and its edge pixels mix the two
        // colors.
        let ch = '\u{1FB61}';
        let area = Shape::of(ch)
            .expect("shape")
            .coverage_of(0.0, 0.0, 1.0, 1.0);
        let diagonal = draw(
            &[Shown {
                ch,
                fg: white,
                bg: black,
            }],
            1,
        );
        let lit: f32 = diagonal.iter().map(|p| p[1]).sum();
        assert!(
            (lit - area * (CELL_W * CELL_H) as f32).abs() < 1e-2,
            "{lit}"
        );
        assert!(diagonal.iter().any(|p| p[0] > 0.1 && p[0] < 0.9));
    }

    #[test]
    fn cells_land_in_their_places() {
        let cells = [
            Shown {
                ch: ' ',
                fg: [0; 3],
                bg: [255, 0, 0],
            },
            Shown {
                ch: ' ',
                fg: [0; 3],
                bg: [0, 0, 255],
            },
        ];
        let shown = draw(&cells, 1);
        assert_eq!(shown.len(), CELL_W * CELL_H * 2);
        assert_eq!(shown[0], [1.0, 0.0, 0.0]);
        assert_eq!(shown[CELL_W * CELL_H], [0.0, 0.0, 1.0]);
    }
}
