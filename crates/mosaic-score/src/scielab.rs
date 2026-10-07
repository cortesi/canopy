//! S-CIELAB, the spatial extension of CIELAB by Zhang and Wandell (1996).
//!
//! S-CIELAB models how the eye blurs a picture at a viewing distance. It
//! takes an image to the opponent space of Poirson and Wandell, a luminance
//! channel and two color channels, and filters each channel with the
//! spatial response of the eye to it, expressed in degrees of visual angle.
//! The color channels blur more than the luminance. Then it takes the result
//! to CIELAB, where a color difference measures what remains visible.
//!
//! The filters follow the reference implementation, `separableFilters.m` of
//! <https://github.com/wandell/SCIELAB-1996>, step by step: its Gaussians,
//! its support of one degree, and its filters for low sampling rates. Images
//! reflect at their edges, as there, and the reflection repeats where a
//! filter is wider than the image.

use crate::color::{self, Matrix};

/// CIE XYZ to the opponent space, for the 2° observer, from `cmatrix.m`.
const XYZ_TO_OPPONENT: Matrix = [
    [0.278_733_6, 0.721_803_1, -0.106_552],
    [-0.448_773_6, 0.289_805_6, 0.077_156_9],
    [0.085_951_3, -0.589_985_9, 0.501_108_9],
];

/// The spatial response of each opponent channel: Gaussians as their half
/// width in degrees and their weight. The weights of each channel sum to 1.
const RESPONSES: [&[(f64, f64)]; 3] = [
    &[(0.05, 1.00327), (0.225, 0.114_416), (7.0, -0.117_686)],
    &[(0.0685, 0.616_725), (0.826, 0.383_275)],
    &[(0.0920, 0.567_885), (0.6451, 0.432_115)],
];

/// The sampling rate below which the reference builds its filters at a
/// multiple of the rate, then samples them down.
const MIN_SAMPLES_PER_DEGREE: f64 = 224.0;

/// The filters of S-CIELAB at one sampling rate.
pub struct Scielab {
    /// For each opponent channel, its separable kernels. Each filters the
    /// rows as it is and the columns by its magnitude, so the product
    /// carries the sign of the weight once.
    channels: [Vec<Vec<f32>>; 3],
    /// Linear sRGB to the opponent space.
    to_opponent: Matrix,
    /// The opponent space to CIE XYZ.
    to_xyz: Matrix,
    /// The CIE XYZ of the white.
    white: [f32; 3],
}

impl Scielab {
    /// The filters for images of `samples_per_degree` pixels to each degree
    /// of visual angle.
    pub fn new(samples_per_degree: f64) -> Self {
        Self {
            channels: RESPONSES.map(|response| {
                kernels(samples_per_degree, response)
                    .into_iter()
                    .map(|kernel| kernel.into_iter().map(|v| v as f32).collect())
                    .collect()
            }),
            to_opponent: color::product(&XYZ_TO_OPPONENT, &color::SRGB_TO_XYZ),
            to_xyz: color::inverse(&XYZ_TO_OPPONENT),
            white: color::white(),
        }
    }

    /// The CIELAB of an image of linear sRGB pixels, `width` across, after
    /// the eye's spatial response.
    pub fn lab(&self, linear: &[[f32; 3]], width: usize) -> Vec<[f32; 3]> {
        let height = linear.len() / width;
        let opponent: Vec<[f32; 3]> = linear
            .iter()
            .map(|p| color::mul(&self.to_opponent, *p))
            .collect();
        let planes: Vec<Vec<f32>> = (0..3)
            .map(|channel| {
                let plane: Vec<f32> = opponent.iter().map(|p| p[channel]).collect();
                filter(&plane, width, height, &self.channels[channel])
            })
            .collect();
        (0..linear.len())
            .map(|i| {
                let xyz = color::mul(&self.to_xyz, [planes[0][i], planes[1][i], planes[2][i]]);
                color::lab(xyz, self.white)
            })
            .collect()
    }
}

/// The CIEDE2000 difference of each pixel of two CIELAB images.
pub fn differences(a: &[[f32; 3]], b: &[[f32; 3]]) -> Vec<f64> {
    assert_eq!(a.len(), b.len(), "images of one size");
    a.iter()
        .zip(b)
        .map(|(p, q)| color::ciede2000(p.map(f64::from), q.map(f64::from)))
        .collect()
}

/// The mean CIEDE2000 difference between two CIELAB images.
pub fn mean_difference(a: &[[f32; 3]], b: &[[f32; 3]]) -> f64 {
    differences(a, b).iter().sum::<f64>() / a.len().max(1) as f64
}

/// MATLAB's `round`: halves away from zero.
fn matlab_round(v: f64) -> f64 {
    v.signum() * (v.abs() + 0.5).floor()
}

/// `gauss.m`: a Gaussian of `width` samples that sums to 1, falling to half
/// its peak at `(half_width - 1) / 2` samples from the center.
fn gauss(half_width: f64, width: usize) -> Vec<f64> {
    let alpha = 2.0 * 2f64.ln().sqrt() / (half_width - 1.0);
    let center = matlab_round(width as f64 / 2.0);
    let g: Vec<f64> = (1..=width)
        .map(|i| {
            let x = i as f64 - center;
            (-alpha * alpha * x * x).exp()
        })
        .collect();
    let total: f64 = g.iter().sum();
    g.into_iter().map(|v| v / total).collect()
}

/// `separableFilters.m` with dimension 3: the row kernels of a channel's
/// Gaussians at a sampling rate, each scaled by the square root of the
/// magnitude of its weight and by the sign of the weight.
fn kernels(samples_per_degree: f64, response: &[(f64, f64)]) -> Vec<Vec<f64>> {
    let (uprate, rate) = if samples_per_degree < MIN_SAMPLES_PER_DEGREE {
        let uprate = (MIN_SAMPLES_PER_DEGREE / samples_per_degree).ceil();
        (uprate as usize, samples_per_degree * uprate)
    } else {
        (1, samples_per_degree)
    };
    // One degree of support, in an odd number of samples.
    let width = ((rate / 2.0).ceil() * 2.0 - 1.0) as usize;
    let rows = response.iter().map(|&(half_width, weight)| {
        let scale = weight.abs().sqrt() * weight.signum();
        gauss(half_width * rate, width)
            .into_iter()
            .map(|v| v * scale)
            .collect::<Vec<f64>>()
    });
    if uprate == 1 {
        return rows.collect();
    }
    // At a low rate, the reference convolves each kernel with the triangle
    // of linear upsampling, then keeps every `uprate`th sample about the
    // center.
    let up_reach = uprate as isize - 1;
    let center = width / 2;
    rows.map(|row| {
        let up: Vec<f64> = (0..width as isize)
            .map(|i| {
                (-up_reach..=up_reach)
                    .filter_map(|d| {
                        let j = usize::try_from(i - d).ok()?;
                        let weight = (uprate - d.unsigned_abs()) as f64 / uprate as f64;
                        row.get(j).map(|v| v * weight)
                    })
                    .sum()
            })
            .collect();
        let reach = center / uprate;
        (0..=2 * reach)
            .map(|k| up[center + k * uprate - reach * uprate])
            .collect()
    })
    .collect()
}

/// The index that `i` reflects to in `0..n`, with the edge sample repeated,
/// as `[x2 x1 | x1 x2 … xn | xn xn-1]`.
fn reflect(i: isize, n: usize) -> usize {
    let n = n as isize;
    let m = i.rem_euclid(2 * n);
    (if m < n { m } else { 2 * n - 1 - m }) as usize
}

/// Filter a plane with separable kernels: each kernel filters the rows,
/// its magnitude filters the columns, and the results add.
fn filter(plane: &[f32], width: usize, height: usize, kernels: &[Vec<f32>]) -> Vec<f32> {
    let mut out = vec![0.0f32; plane.len()];
    let mut across = vec![0.0f32; plane.len()];
    let mut padded = Vec::new();
    for kernel in kernels {
        let reach = kernel.len() / 2;
        for y in 0..height {
            let row = &plane[y * width..(y + 1) * width];
            padded.clear();
            padded.extend(
                (0..width + 2 * reach).map(|i| row[reflect(i as isize - reach as isize, width)]),
            );
            let target = &mut across[y * width..(y + 1) * width];
            target.fill(0.0);
            for (d, w) in kernel.iter().enumerate() {
                for (t, v) in target.iter_mut().zip(&padded[d..d + width]) {
                    *t += w * v;
                }
            }
        }
        for y in 0..height {
            let target = &mut out[y * width..(y + 1) * width];
            for (d, w) in kernel.iter().enumerate() {
                let source = reflect(y as isize + d as isize - reach as isize, height);
                let w = w.abs();
                for (t, v) in target
                    .iter_mut()
                    .zip(&across[source * width..(source + 1) * width])
                {
                    *t += w * v;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Taps of the reference kernels, from a line-by-line port of
    /// `separableFilters.m`: for each sampling rate and kernel, its length,
    /// its center, the next tap, the tap a quarter of its length out, and
    /// its sum.
    const REFERENCE: [(f64, usize, usize, [f64; 4]); 7] = [
        (
            48.0,
            0,
            47,
            [
                3.923_686_738e-1,
                2.426_336_766e-1,
                2.723_443_310e-28,
                1.001_633_666,
            ],
        ),
        (
            48.0,
            2,
            47,
            [
                -7.210_446_680e-3,
                -7.210_269_393e-3,
                -7.189_026_540e-3,
                -3.373_640_140e-1,
            ],
        ),
        (
            48.0,
            4,
            47,
            [
                1.741_535_796e-2,
                1.738_437_423e-2,
                1.403_970_211e-2,
                6.139_254_186e-1,
            ],
        ),
        (
            96.0,
            1,
            95,
            [
                1.492_854_885e-2,
                1.483_747_632e-2,
                5.863_665_589e-4,
                3.382_543_326e-1,
            ],
        ),
        (
            96.0,
            3,
            95,
            [
                1.169_488_568e-1,
                1.090_788_948e-1,
                1.083_952_218e-17,
                7.853_184_068e-1,
            ],
        ),
        (
            96.0,
            6,
            95,
            [
                1.074_961_792e-2,
                1.074_176_706e-2,
                7.303_693_403e-3,
                6.559_816_657e-1,
            ],
        ),
        (
            240.0,
            2,
            239,
            [
                -1.442_095_009e-3,
                -1.442_093_591e-3,
                -1.437_166_237e-3,
                -3.430_539_316e-1,
            ],
        ),
    ];

    #[test]
    fn kernels_match_the_reference_implementation() {
        for (rate, index, len, [center, next, quarter, sum]) in REFERENCE {
            let all: Vec<Vec<f64>> = RESPONSES
                .iter()
                .flat_map(|response| kernels(rate, response))
                .collect();
            let kernel = &all[index];
            assert_eq!(kernel.len(), len, "rate {rate} kernel {index}");
            let c = len / 2;
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-8 * b.abs().max(1e-30);
            assert!(close(kernel[c], center), "{rate} {index}: {}", kernel[c]);
            assert!(close(kernel[c + 1], next), "{rate} {index}");
            assert!(close(kernel[c + len / 4], quarter), "{rate} {index}");
            assert!(close(kernel.iter().sum(), sum), "{rate} {index}");
        }
    }

    #[test]
    fn reflection_repeats_the_edge_sample() {
        let n = 4;
        let got: Vec<usize> = (-3..7).map(|i| reflect(i, n)).collect();
        assert_eq!(got, [2, 1, 0, 0, 1, 2, 3, 3, 2, 1]);
        assert_eq!(reflect(-9, 2), 0);
        assert_eq!(reflect(5, 2), 1);
    }

    #[test]
    fn identical_images_differ_by_nothing_and_a_flat_field_keeps_its_color() {
        let scielab = Scielab::new(48.0);
        let (width, height) = (40, 30);
        let flat = vec![[0.2, 0.4, 0.6]; width * height];
        let lab = scielab.lab(&flat, width);
        assert_eq!(mean_difference(&lab, &lab), 0.0);
        let white = color::white();
        let expected = color::lab(color::mul(&color::SRGB_TO_XYZ, [0.2, 0.4, 0.6]), white);
        // The reference's filters gain a little at low rates, where the
        // sampled kernels lose some of their tails.
        let [l, a, b] = lab[width * height / 2];
        assert!((l - expected[0]).abs() < 0.2, "{l} {}", expected[0]);
        assert!((a - expected[1]).abs() < 0.2 && (b - expected[2]).abs() < 0.2);
    }

    #[test]
    fn fine_stripes_fade_with_distance_and_color_fades_first() {
        let (width, height) = (96, 96);
        // Stripes two pixels wide: black and white, then red and green of
        // about one luminance.
        let stripes = |a: [f32; 3], b: [f32; 3]| -> Vec<[f32; 3]> {
            (0..width * height)
                .map(|i| if (i % width) / 2 % 2 == 0 { a } else { b })
                .collect()
        };
        let mean = |pixels: &[[f32; 3]]| {
            let n = pixels.len() as f32;
            pixels.iter().fold([0.0; 3], |s, p| {
                [s[0] + p[0] / n, s[1] + p[1] / n, s[2] + p[2] / n]
            })
        };
        let visible = |rate: f64, a: [f32; 3], b: [f32; 3]| {
            let scielab = Scielab::new(rate);
            let image = stripes(a, b);
            let flat = vec![mean(&image); image.len()];
            mean_difference(&scielab.lab(&image, width), &scielab.lab(&flat, width))
        };
        let gray = ([0.0; 3], [1.0; 3]);
        let chroma = ([0.6, 0.05, 0.0], [0.0, 0.2, 0.0]);
        assert!(visible(30.0, gray.0, gray.1) > visible(120.0, gray.0, gray.1));
        assert!(visible(120.0, chroma.0, chroma.1) < visible(120.0, gray.0, gray.1));
    }
}
