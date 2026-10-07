//! Stray marks: error that the eye reads as a mark of its own.
//!
//! The mean difference of S-CIELAB spreads a few wrong pixels over a whole
//! picture, so it barely sees a spur of ink that hangs off a stroke, a notch
//! cut into one, or a tinted patch on flat ground. The eye sees each of
//! these at once, because nothing like it is near in the picture.
//!
//! An edge that moves by a device pixel or two is not a mark. At each device
//! pixel, the reference holds a range of each linear channel within a reach
//! of it, a few device pixels each way. Clamping the display into those
//! ranges gives the display that the reference explains: every edge where it
//! was, give or take the reach, and every mark that nothing nearby explains
//! taken out. The stray of the display is its S-CIELAB difference from that
//! explained display, so the eye's blur decides how much a mark shows.
//!
//! The differences pool by probability summation, as a Minkowski mean with
//! an exponent of [`POOLING`]: a few strong marks count for more than a
//! wide faint shade. A spur of ink at full contrast then outweighs the
//! slight shading that free pens leave inside a stroke, as it does to the
//! eye.

/// The exponent of the Minkowski mean that pools stray, the usual exponent
/// of probability summation over space.
pub const POOLING: i32 = 4;

/// The Minkowski mean of stray differences, with the exponent
/// [`POOLING`].
pub fn pooled(differences: &[f64]) -> f64 {
    let total: f64 = differences.iter().map(|d| d.powi(POOLING)).sum();
    (total / differences.len().max(1) as f64).powf(1.0 / f64::from(POOLING))
}

/// The display that the reference explains: each pixel of `shown` clamped
/// into the range of each channel that `reference` holds within `reach`
/// pixels of it, `[across, down]`. Both images are linear sRGB, `width`
/// pixels across.
pub fn explained(
    reference: &[[f32; 3]],
    shown: &[[f32; 3]],
    width: usize,
    reach: [usize; 2],
) -> Vec<[f32; 3]> {
    assert_eq!(reference.len(), shown.len(), "images of one size");
    let (low, high) = envelope(reference, width, reach);
    shown
        .iter()
        .zip(low.iter().zip(&high))
        .map(|(p, (lo, hi))| [0, 1, 2].map(|c| p[c].clamp(lo[c], hi[c])))
        .collect()
}

/// The least and greatest value of each channel within `reach` of each
/// pixel, clamped at the edges of the image.
fn envelope(
    pixels: &[[f32; 3]],
    width: usize,
    [rx, ry]: [usize; 2],
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let height = pixels.len() / width;
    // The extremes along one axis of the samples within `reach` of each.
    // Two passes, across and then down, cover the rectangle.
    let pass = |input: &[[f32; 3]], pick: fn(f32, f32) -> f32, reach: usize, across: bool| {
        let mut out = input.to_vec();
        for (i, o) in out.iter_mut().enumerate() {
            let (x, y) = (i % width, i / width);
            let (at, len) = if across { (x, width) } else { (y, height) };
            for j in at.saturating_sub(reach)..(at + reach + 1).min(len) {
                let q = if across {
                    input[y * width + j]
                } else {
                    input[j * width + x]
                };
                *o = [0, 1, 2].map(|c| pick(o[c], q[c]));
            }
        }
        out
    };
    let low = pass(&pass(pixels, f32::min, rx, true), f32::min, ry, false);
    let high = pass(&pass(pixels, f32::max, rx, true), f32::max, ry, false);
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field 32 pixels square of one color with a block of another, from
    /// `x0` to `x1` across and `y0` to `y1` down.
    fn block([x0, x1, y0, y1]: [usize; 4], ground: [f32; 3], ink: [f32; 3]) -> Vec<[f32; 3]> {
        (0..32 * 32)
            .map(|i| {
                let (x, y) = (i % 32, i / 32);
                if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                    ink
                } else {
                    ground
                }
            })
            .collect()
    }

    /// The display that `reference` explains, within two pixels across and
    /// four down.
    fn explain(reference: &[[f32; 3]], shown: &[[f32; 3]]) -> Vec<[f32; 3]> {
        explained(reference, shown, 32, [2, 4])
    }

    const GROUND: [f32; 3] = [0.02, 0.02, 0.03];
    const INK: [f32; 3] = [0.8, 0.7, 0.6];

    #[test]
    fn pooling_weighs_a_strong_mark_over_a_faint_shade_of_the_same_sum() {
        let mut strong = vec![0.0; 100];
        strong[0] = 40.0;
        let faint = vec![0.4; 100];
        assert!(pooled(&strong) > 10.0 * pooled(&faint));
        assert!((pooled(&[2.0; 8]) - 2.0).abs() < 1e-12);
        assert_eq!(pooled(&[]), 0.0);
    }

    #[test]
    fn a_moved_edge_within_reach_is_explained() {
        let reference = block([8, 24, 8, 16], GROUND, INK);
        for shown in [
            reference.clone(),
            block([8, 24, 8, 20], GROUND, INK),
            block([6, 26, 4, 16], GROUND, INK),
        ] {
            assert_eq!(explain(&reference, &shown), shown);
        }
    }

    #[test]
    fn a_spur_beyond_reach_is_taken_out() {
        let reference = block([8, 24, 8, 16], GROUND, INK);
        // A spur two pixels wide hangs ten pixels below the block's bottom
        // edge: the four rows within reach of the edge are explained, and
        // the six beyond are not.
        let mut spur = reference.clone();
        for y in 16..26 {
            spur[y * 32 + 20] = INK;
            spur[y * 32 + 21] = INK;
        }
        let explained = explain(&reference, &spur);
        for y in 16..26 {
            let expected = if y < 20 { INK } else { GROUND };
            assert_eq!(explained[y * 32 + 20], expected, "row {y}");
        }
        // A dimmer spur is taken out to the same ground.
        let mut dim = reference.clone();
        dim[24 * 32 + 20] = [0.4, 0.35, 0.3];
        assert_eq!(explain(&reference, &dim)[24 * 32 + 20], GROUND);
    }

    #[test]
    fn a_notch_and_a_tint_are_taken_out_but_a_color_from_nearby_is_not() {
        let reference = block([4, 28, 4, 28], GROUND, INK);
        // A notch: ground deep inside the block, far from any ground.
        let mut notch = reference.clone();
        notch[16 * 32 + 16] = GROUND;
        assert_eq!(explain(&reference, &notch)[16 * 32 + 16], INK);
        // A tint: a color that the reference holds nowhere near.
        let mut tint = reference.clone();
        tint[16 * 32 + 16] = [0.8, 0.2, 0.6];
        assert_eq!(explain(&reference, &tint)[16 * 32 + 16], INK);
        // Ground at the block's edge is a color the reference holds nearby,
        // and so is a mix of the two.
        let mut edge = reference.clone();
        edge[16 * 32 + 4] = GROUND;
        edge[16 * 32 + 5] = [0.4, 0.35, 0.3];
        assert_eq!(explain(&reference, &edge), edge);
    }
}
