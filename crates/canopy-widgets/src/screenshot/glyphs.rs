//! Box drawing, block elements, braille, and triangles, drawn from geometry.
//!
//! Terminals draw these characters themselves rather than from a font, so
//! that lines meet at cell edges and blocks fill their cells exactly.

use super::{Canvas, Rect};

/// The weight of one arm of a box drawing character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Weight {
    /// No arm.
    None,
    /// A light line.
    Light,
    /// A heavy line.
    Heavy,
    /// Two light lines.
    Double,
}

/// Arms of a box drawing character: up, right, down, and left.
type Arms = [Weight; 4];

/// The arms of U+2500 to U+257F, by offset from U+2500, in the order up,
/// right, down, left: `.` none, `l` light, `h` heavy, `d` double. Dashed
/// lines, rounded corners, and diagonals are `-`, and draw elsewhere.
const ARMS: [&str; 128] = [
    ".l.l", ".h.h", "l.l.", "h.h.", "----", "----", "----", "----", // 2500
    "----", "----", "----", "----", ".ll.", ".hl.", ".lh.", ".hh.", // 2508
    "..ll", "..lh", "..hl", "..hh", "ll..", "lh..", "hl..", "hh..", // 2510
    "l..l", "l..h", "h..l", "h..h", "lll.", "lhl.", "hll.", "llh.", // 2518
    "hlh.", "hhl.", "lhh.", "hhh.", "l.ll", "l.lh", "h.ll", "l.hl", // 2520
    "h.hl", "h.lh", "l.hh", "h.hh", ".lll", ".llh", ".hll", ".hlh", // 2528
    ".lhl", ".lhh", ".hhl", ".hhh", "ll.l", "ll.h", "lh.l", "lh.h", // 2530
    "hl.l", "hl.h", "hh.l", "hh.h", "llll", "lllh", "lhll", "lhlh", // 2538
    "hlll", "llhl", "hlhl", "hllh", "hhll", "llhh", "lhhl", "hhlh", // 2540
    "lhhh", "hlhh", "hhhl", "hhhh", "----", "----", "----", "----", // 2548
    ".d.d", "d.d.", ".dl.", ".ld.", ".dd.", "..ld", "..dl", "..dd", // 2550
    "ld..", "dl..", "dd..", "l..d", "d..l", "d..d", "ldl.", "dld.", // 2558
    "ddd.", "l.ld", "d.dl", "d.dd", ".dld", ".ldl", ".ddd", "ld.d", // 2560
    "dl.l", "dd.d", "ldld", "dldl", "dddd", "----", "----", "----", // 2568
    "----", "----", "----", "----", "...l", "l...", ".l..", "..l.", // 2570
    "...h", "h...", ".h..", "..h.", ".h.l", "l.h.", ".l.h", "h.l.", // 2578
];

/// Draw `ch` in `cell` if it is a box drawing, block element, braille, or
/// triangle character, and return whether it was.
pub(super) fn draw(canvas: &mut Canvas, ch: char, cell: Rect, color: [u8; 3], stroke: u32) -> bool {
    let code = ch as u32;
    match code {
        0x2500..=0x257F => box_drawing(canvas, code - 0x2500, cell, color, stroke),
        0x2580..=0x259F => {
            block(canvas, code - 0x2580, cell, color);
            true
        }
        0x25B2..=0x25C5 => {
            triangle(canvas, code - 0x25B2, cell, color, stroke);
            true
        }
        0x2800..=0x28FF => {
            braille(canvas, (code - 0x2800) as u8, cell, color);
            true
        }
        _ => false,
    }
}

/// Draw the box of a character that the font lacks.
pub(super) fn tofu(canvas: &mut Canvas, cell: Rect, color: [u8; 3], stroke: u32) {
    let inset = (cell.w.min(cell.h) / 6) as i32;
    let (x0, y0) = (cell.x + inset, cell.y + inset * 2);
    let (x1, y1) = (cell.right() - inset, cell.bottom() - inset * 2);
    let s = stroke as i32;
    for rect in [
        Rect::between(x0, y0, x1, y0 + s),
        Rect::between(x0, y1 - s, x1, y1),
        Rect::between(x0, y0, x0 + s, y1),
        Rect::between(x1 - s, y0, x1, y1),
    ] {
        canvas.fill(rect, color);
    }
}

/// Return the weight that a letter of [`ARMS`] names.
fn weight(letter: u8) -> Weight {
    match letter {
        b'l' => Weight::Light,
        b'h' => Weight::Heavy,
        b'd' => Weight::Double,
        _ => Weight::None,
    }
}

/// Draw one box drawing character, by offset from U+2500.
fn box_drawing(canvas: &mut Canvas, offset: u32, cell: Rect, color: [u8; 3], stroke: u32) -> bool {
    let pattern = ARMS[offset as usize].as_bytes();
    if pattern[0] != b'-' {
        let arms = [
            weight(pattern[0]),
            weight(pattern[1]),
            weight(pattern[2]),
            weight(pattern[3]),
        ];
        lines(canvas, arms, cell, color, stroke);
        return true;
    }
    let code = 0x2500 + offset;
    match code {
        // Dashed lines: light and heavy, horizontal and vertical, with three,
        // four, or two dashes.
        0x2504..=0x250B => {
            let local = code - 0x2504;
            let dashes = if local < 4 { 3 } else { 4 };
            dashed(
                canvas,
                local % 2 == 1,
                local % 4 < 2,
                dashes,
                cell,
                color,
                stroke,
            );
        }
        0x254C..=0x254F => {
            let local = code - 0x254C;
            dashed(canvas, local % 2 == 1, local < 2, 2, cell, color, stroke);
        }
        0x256D..=0x2570 => arc(canvas, code - 0x256D, cell, color, stroke),
        0x2571..=0x2573 => {
            let width = stroke as f32;
            let (x0, y0) = (cell.x as f32, cell.y as f32);
            let (x1, y1) = (cell.right() as f32, cell.bottom() as f32);
            if code != 0x2572 {
                segment(canvas, (x0, y1), (x1, y0), width, cell, color);
            }
            if code != 0x2571 {
                segment(canvas, (x0, y0), (x1, y1), width, cell, color);
            }
        }
        _ => return false,
    }
    true
}

/// Return the start of a line of thickness `t` centered on `center`.
fn start(center: i32, t: u32) -> i32 {
    center - (t / 2) as i32
}

/// Return the lines of an arm of a weight, centered on `center` across the
/// arm, as the start and the end of each across it. A double arm has two
/// lines, the one nearer the start first.
fn across(weight: Weight, center: i32, stroke: u32) -> Vec<(i32, i32)> {
    let line = |at: i32, t: u32| (start(at, t), start(at, t) + t as i32);
    let gap = stroke as i32;
    match weight {
        Weight::None => Vec::new(),
        Weight::Light => vec![line(center, stroke)],
        Weight::Heavy => vec![line(center, stroke * 2)],
        Weight::Double => vec![line(center - gap, stroke), line(center + gap, stroke)],
    }
}

/// Draw the arms of a box drawing character.
///
/// An arm runs from the cell edge across the lines of the arms that cross
/// it, so corners and junctions meet with no stub. A line of a double arm
/// stops at the near line of a double arm on its side instead, which keeps
/// the gap between double lines open.
fn lines(canvas: &mut Canvas, arms: Arms, cell: Rect, color: [u8; 3], stroke: u32) {
    let [up, right, down, left] = arms;
    let cx = cell.x + cell.w as i32 / 2;
    let cy = cell.y + cell.h as i32 / 2;
    // The span of the lines of one axis, or its center when it has none.
    let span = |arms: [Weight; 2], center: i32| {
        let lines: Vec<_> = arms
            .into_iter()
            .flat_map(|weight| across(weight, center, stroke))
            .collect();
        (
            lines.iter().map(|line| line.0).min().unwrap_or(center),
            lines.iter().map(|line| line.1).max().unwrap_or(center),
        )
    };
    let (vertical_from, vertical_to) = span([up, down], cx);
    let (horizontal_from, horizontal_to) = span([left, right], cy);
    let double_x = across(Weight::Double, cx, stroke);
    let double_y = across(Weight::Double, cy, stroke);
    for (weight, forward) in [(left, false), (right, true)] {
        for (index, (y0, y1)) in across(weight, cy, stroke).into_iter().enumerate() {
            // The upper line meets the up arm, and the lower the down arm.
            let side = if index == 0 { up } else { down };
            let stops = weight == Weight::Double && side == Weight::Double;
            let (x0, x1) = match (forward, stops) {
                (true, true) => (double_x[1].0, cell.right()),
                (true, false) => (vertical_from, cell.right()),
                (false, true) => (cell.x, double_x[0].1),
                (false, false) => (cell.x, vertical_to),
            };
            canvas.fill(Rect::between(x0, y0, x1, y1), color);
        }
    }
    for (weight, forward) in [(up, false), (down, true)] {
        for (index, (x0, x1)) in across(weight, cx, stroke).into_iter().enumerate() {
            // The left line meets the left arm, and the right the right arm.
            let side = if index == 0 { left } else { right };
            let stops = weight == Weight::Double && side == Weight::Double;
            let (y0, y1) = match (forward, stops) {
                (true, true) => (double_y[1].0, cell.bottom()),
                (true, false) => (horizontal_from, cell.bottom()),
                (false, true) => (cell.y, double_y[0].1),
                (false, false) => (cell.y, horizontal_to),
            };
            canvas.fill(Rect::between(x0, y0, x1, y1), color);
        }
    }
}

/// Draw a dashed line across a cell.
fn dashed(
    canvas: &mut Canvas,
    heavy: bool,
    horizontal: bool,
    dashes: i32,
    cell: Rect,
    color: [u8; 3],
    stroke: u32,
) {
    let t = if heavy { stroke * 2 } else { stroke };
    let length = if horizontal { cell.w } else { cell.h } as i32;
    // Each dash takes its share of the cell, and leaves a gap of a third of
    // the share after it.
    for dash in 0..dashes {
        let from = length * dash / dashes;
        let to = from + (length / dashes) * 2 / 3;
        let rect = if horizontal {
            let y = start(cell.y + cell.h as i32 / 2, t);
            Rect::between(cell.x + from, y, cell.x + to, y + t as i32)
        } else {
            let x = start(cell.x + cell.w as i32 / 2, t);
            Rect::between(x, cell.y + from, x + t as i32, cell.y + to)
        };
        canvas.fill(rect, color);
    }
}

/// Draw a rounded corner: `0` joins right and down, `1` left and down, `2`
/// left and up, and `3` right and up.
fn arc(canvas: &mut Canvas, corner: u32, cell: Rect, color: [u8; 3], stroke: u32) {
    let t = stroke as f32;
    // The centers of the vertical and the horizontal lines, as straight arms
    // draw them.
    let line_x = start(cell.x + cell.w as i32 / 2, stroke) as f32 + t / 2.0;
    let line_y = start(cell.y + cell.h as i32 / 2, stroke) as f32 + t / 2.0;
    let rightward = matches!(corner, 0 | 3);
    let downward = matches!(corner, 0 | 1);
    let room_x = if rightward {
        cell.right() as f32 - line_x
    } else {
        line_x - cell.x as f32
    };
    let room_y = if downward {
        cell.bottom() as f32 - line_y
    } else {
        line_y - cell.y as f32
    };
    let radius = room_x.min(room_y);
    let sx = if rightward { 1.0 } else { -1.0 };
    let sy = if downward { 1.0 } else { -1.0 };
    let center = (line_x + sx * radius, line_y + sy * radius);
    // The straight parts run from the ends of the arc to the cell edges.
    let horizontal_end = line_x + sx * radius;
    let vertical_end = line_y + sy * radius;
    let (hx0, hx1) = if rightward {
        (horizontal_end, cell.right() as f32)
    } else {
        (cell.x as f32, horizontal_end)
    };
    let (vy0, vy1) = if downward {
        (vertical_end, cell.bottom() as f32)
    } else {
        (cell.y as f32, vertical_end)
    };
    let top = line_y - t / 2.0;
    let left = line_x - t / 2.0;
    canvas.fill(
        Rect::between(
            hx0.round() as i32,
            top as i32,
            hx1.round() as i32,
            top as i32 + stroke as i32,
        ),
        color,
    );
    canvas.fill(
        Rect::between(
            left as i32,
            vy0.round() as i32,
            left as i32 + stroke as i32,
            vy1.round() as i32,
        ),
        color,
    );
    for y in cell.y..cell.bottom() {
        for x in cell.x..cell.right() {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Only the quarter of the circle that faces the corner.
            if (px - center.0) * sx > 0.0 || (py - center.1) * sy > 0.0 {
                continue;
            }
            let distance = ((px - center.0).hypot(py - center.1) - radius).abs();
            canvas.blend_f(x, y, color, t / 2.0 + 0.5 - distance);
        }
    }
}

/// Draw an antialiased line segment of a width, clipped to a cell.
fn segment(
    canvas: &mut Canvas,
    from: (f32, f32),
    to: (f32, f32),
    width: f32,
    cell: Rect,
    color: [u8; 3],
) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy).max(f32::EPSILON);
    for y in cell.y..cell.bottom() {
        for x in cell.x..cell.right() {
            let (px, py) = (x as f32 + 0.5 - from.0, y as f32 + 0.5 - from.1);
            let distance = (px * dy - py * dx).abs() / length;
            canvas.blend_f(x, y, color, width / 2.0 + 0.5 - distance);
        }
    }
}

/// The parts of U+2580 to U+259F, by offset from U+2580: rectangles in
/// eighths of the cell, as left, top, right, and bottom.
const BLOCKS: [&[[u32; 4]]; 32] = [
    &[[0, 0, 8, 4]],               // ▀
    &[[0, 7, 8, 8]],               // ▁
    &[[0, 6, 8, 8]],               // ▂
    &[[0, 5, 8, 8]],               // ▃
    &[[0, 4, 8, 8]],               // ▄
    &[[0, 3, 8, 8]],               // ▅
    &[[0, 2, 8, 8]],               // ▆
    &[[0, 1, 8, 8]],               // ▇
    &[[0, 0, 8, 8]],               // █
    &[[0, 0, 7, 8]],               // ▉
    &[[0, 0, 6, 8]],               // ▊
    &[[0, 0, 5, 8]],               // ▋
    &[[0, 0, 4, 8]],               // ▌
    &[[0, 0, 3, 8]],               // ▍
    &[[0, 0, 2, 8]],               // ▎
    &[[0, 0, 1, 8]],               // ▏
    &[[4, 0, 8, 8]],               // ▐
    &[],                           // ░
    &[],                           // ▒
    &[],                           // ▓
    &[[0, 0, 8, 1]],               // ▔
    &[[7, 0, 8, 8]],               // ▕
    &[[0, 4, 4, 8]],               // ▖
    &[[4, 4, 8, 8]],               // ▗
    &[[0, 0, 4, 4]],               // ▘
    &[[0, 0, 4, 4], [0, 4, 8, 8]], // ▙
    &[[0, 0, 4, 4], [4, 4, 8, 8]], // ▚
    &[[0, 0, 8, 4], [0, 4, 4, 8]], // ▛
    &[[0, 0, 8, 4], [4, 4, 8, 8]], // ▜
    &[[4, 0, 8, 4]],               // ▝
    &[[4, 0, 8, 4], [0, 4, 4, 8]], // ▞
    &[[4, 0, 8, 4], [0, 4, 8, 8]], // ▟
];

/// Draw one block element, by offset from U+2580.
fn block(canvas: &mut Canvas, offset: u32, cell: Rect, color: [u8; 3]) {
    // The shades cover the cell with a quarter, a half, and three quarters
    // of the color.
    if let 0x11..=0x13 = offset {
        let alpha = ((offset - 0x10) * 64) as u8;
        for y in cell.y..cell.bottom() {
            for x in cell.x..cell.right() {
                canvas.blend(x, y, color, alpha);
            }
        }
        return;
    }
    let scale = |length: u32, eighths: u32| ((length * eighths + 4) / 8) as i32;
    for [left, top, right, bottom] in BLOCKS[offset as usize] {
        canvas.fill(
            Rect::between(
                cell.x + scale(cell.w, *left),
                cell.y + scale(cell.h, *top),
                cell.x + scale(cell.w, *right),
                cell.y + scale(cell.h, *bottom),
            ),
            color,
        );
    }
}

/// Draw one braille pattern from its dot bits.
fn braille(canvas: &mut Canvas, bits: u8, cell: Rect, color: [u8; 3]) {
    // Dots 1 to 8, as column and row.
    const DOTS: [(u32, u32); 8] = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    let radius = (cell.w as f32 / 4.0).min(cell.h as f32 / 8.0) * 0.7;
    for (bit, (column, row)) in DOTS.iter().enumerate() {
        if bits & (1 << bit) == 0 {
            continue;
        }
        let cx = cell.x as f32 + cell.w as f32 * (column * 2 + 1) as f32 / 4.0;
        let cy = cell.y as f32 + cell.h as f32 * (row * 2 + 1) as f32 / 8.0;
        let (x0, x1) = ((cx - radius - 1.0) as i32, (cx + radius + 1.0) as i32);
        let (y0, y1) = ((cy - radius - 1.0) as i32, (cy + radius + 1.0) as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let distance = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
                canvas.blend_f(x, y, color, radius + 0.5 - distance);
            }
        }
    }
}

/// Draw one triangle of U+25B2 to U+25C5, by offset from U+25B2. Each
/// direction, up, right, down, and left, has a big and a small triangle,
/// filled and outlined. Right and left add a pointer, which draws as the big
/// triangle.
fn triangle(canvas: &mut Canvas, offset: u32, cell: Rect, color: [u8; 3], stroke: u32) {
    let (direction, local) = match offset {
        0..=3 => (0, offset),
        4..=9 => (1, offset - 4),
        10..=13 => (2, offset - 10),
        _ => (3, offset - 14),
    };
    let size = cell.w.min(cell.h) as f32 * if matches!(local, 2 | 3) { 0.55 } else { 0.8 };
    let (cx, cy) = (
        cell.x as f32 + cell.w as f32 / 2.0,
        cell.y as f32 + cell.h as f32 / 2.0,
    );
    // The point of the triangle lies along the direction, and the base
    // across it. The base is the size, and the height a little less.
    let (half, reach) = (size / 2.0, size * 0.45);
    let corners = match direction {
        0 => [
            (cx - half, cy + reach),
            (cx + half, cy + reach),
            (cx, cy - reach),
        ],
        1 => [
            (cx - reach, cy - half),
            (cx - reach, cy + half),
            (cx + reach, cy),
        ],
        2 => [
            (cx - half, cy - reach),
            (cx + half, cy - reach),
            (cx, cy + reach),
        ],
        _ => [
            (cx + reach, cy - half),
            (cx + reach, cy + half),
            (cx - reach, cy),
        ],
    };
    let outline = local % 2 == 1;
    let edges = [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[0]),
    ];
    // The signed distance from an edge, positive inside the triangle.
    let orientation = side(edges[0], corners[2]).signum();
    let inside = |point: (f32, f32)| {
        let distances = edges.map(|edge| side(edge, point) * orientation);
        let nearest = distances.iter().copied().fold(f32::INFINITY, f32::min);
        nearest >= 0.0 && (!outline || nearest < stroke as f32)
    };
    // Four by four samples a pixel smooth the edges.
    for y in cell.y..cell.bottom() {
        for x in cell.x..cell.right() {
            let hits = (0..16)
                .filter(|sample| {
                    let (sx, sy) = ((sample % 4) as f32, (sample / 4) as f32);
                    inside((x as f32 + (sx + 0.5) / 4.0, y as f32 + (sy + 0.5) / 4.0))
                })
                .count();
            canvas.blend_f(x, y, color, hits as f32 / 16.0);
        }
    }
}

/// Return the signed distance of a point from the line through an edge.
fn side(edge: ((f32, f32), (f32, f32)), point: (f32, f32)) -> f32 {
    let ((x0, y0), (x1, y1)) = edge;
    let length = (x1 - x0).hypot(y1 - y0).max(f32::EPSILON);
    ((x1 - x0) * (point.1 - y0) - (y1 - y0) * (point.0 - x0)) / length
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws `ch` in a cell 9 wide and 18 high with a stroke of 1, and
    /// returns its pixels as rows of `#` for drawn and `.` for blank.
    fn rows(ch: char) -> Vec<String> {
        let mut canvas = Canvas::new(9, 18);
        let cell = Rect::between(0, 0, 9, 18);
        assert!(draw(&mut canvas, ch, cell, [255, 255, 255], 1));
        canvas
            .pixels
            .chunks(9 * 3)
            .map(|row| {
                row.chunks(3)
                    .map(|pixel| if pixel[0] > 0 { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn corners_meet_with_no_stub() {
        let corner = rows('┌');
        assert_eq!(corner[8], ".........");
        assert_eq!(corner[9], "....#####");
        assert_eq!(corner[17], "....#....");
    }

    #[test]
    fn double_corners_keep_the_gap_open() {
        let corner = rows('╔');
        assert_eq!(corner[7], ".........");
        assert_eq!(corner[8], "...######");
        assert_eq!(corner[9], "...#.....");
        assert_eq!(corner[10], "...#.####");
        assert_eq!(corner[11], "...#.#...");
    }
}
