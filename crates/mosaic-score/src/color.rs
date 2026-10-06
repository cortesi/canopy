//! Color science: sRGB, CIE XYZ, CIELAB, and the CIEDE2000 color difference.

/// A 3×3 matrix, row by row.
pub type Matrix = [[f32; 3]; 3];

/// The sRGB to CIE XYZ matrix of IEC 61966-2-1, for a D65 white of Y = 1.
pub const SRGB_TO_XYZ: Matrix = [
    [0.4124, 0.3576, 0.1805],
    [0.2126, 0.7152, 0.0722],
    [0.0193, 0.1192, 0.9505],
];

/// Multiply a vector by a matrix.
pub fn mul(m: &Matrix, v: [f32; 3]) -> [f32; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

/// The product of two matrices, `a` after `b`.
pub fn product(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// The inverse of a matrix, by cofactors in double precision.
///
/// # Panics
///
/// Panics when the matrix is singular.
pub fn inverse(m: &Matrix) -> Matrix {
    let a = m.map(|row| row.map(f64::from));
    let cofactor = |r: usize, c: usize| {
        let (r0, r1) = ((r + 1) % 3, (r + 2) % 3);
        let (c0, c1) = ((c + 1) % 3, (c + 2) % 3);
        a[r0][c0] * a[r1][c1] - a[r0][c1] * a[r1][c0]
    };
    let det: f64 = (0..3).map(|c| a[0][c] * cofactor(0, c)).sum();
    assert!(det.abs() > 1e-12, "the matrix is singular");
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            // The adjugate is the transpose of the cofactor matrix.
            *value = (cofactor(j, i) / det) as f32;
        }
    }
    out
}

/// Decode an sRGB channel, 0 to 255, to linear light, 0 to 1.
pub fn decode(value: u8) -> f32 {
    let v = f32::from(value) / 255.0;
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Encode linear light, 0 to 1, to an sRGB channel, rounded and clamped.
pub fn encode(value: f32) -> u8 {
    let v = value.clamp(0.0, 1.0);
    let v = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round() as u8
}

/// Decode an sRGB color to linear light.
pub fn decode_rgb(color: [u8; 3]) -> [f32; 3] {
    color.map(decode)
}

/// The CIE XYZ of the sRGB white, which the matrix maps to Y = 1.
pub fn white() -> [f32; 3] {
    mul(&SRGB_TO_XYZ, [1.0; 3])
}

/// The CIELAB of a CIE XYZ color, relative to `white`, with the constants
/// of CIE 15.
pub fn lab(xyz: [f32; 3], white: [f32; 3]) -> [f32; 3] {
    /// The ratio below which the curve is linear, (6/29)³.
    const EPSILON: f32 = 216.0 / 24389.0;
    /// The slope of the linear part, (29/6)² / 3.
    const SLOPE: f32 = 24389.0 / 27.0 / 116.0;
    let f = |t: f32| {
        if t > EPSILON {
            t.cbrt()
        } else {
            SLOPE * t + 16.0 / 116.0
        }
    };
    let [fx, fy, fz] = [0, 1, 2].map(|i| f(xyz[i] / white[i]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// The CIEDE2000 color difference between two CIELAB colors, with the
/// parametric factors at 1, as Sharma, Wu, and Dalal (2005) state it.
pub fn ciede2000(lab1: [f64; 3], lab2: [f64; 3]) -> f64 {
    /// 25 to the seventh power.
    const P25: f64 = 6_103_515_625.0;
    let [l1, a1, b1] = lab1;
    let [l2, a2, b2] = lab2;
    let c_bar = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
    let c7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c7 / (c7 + P25)).sqrt());
    let (ap1, ap2) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (cp1, cp2) = (ap1.hypot(b1), ap2.hypot(b2));
    let hue = |b: f64, a: f64| {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            b.atan2(a).to_degrees().rem_euclid(360.0)
        }
    };
    let (hp1, hp2) = (hue(b1, ap1), hue(b2, ap2));
    let dl = l2 - l1;
    let dc = cp2 - cp1;
    let chroma_product = cp1 * cp2;
    let dh = if chroma_product == 0.0 {
        0.0
    } else {
        let d = hp2 - hp1;
        if d.abs() <= 180.0 {
            d
        } else if d > 180.0 {
            d - 360.0
        } else {
            d + 360.0
        }
    };
    let dh_big = 2.0 * chroma_product.sqrt() * (dh / 2.0).to_radians().sin();
    let l_bar = (l1 + l2) / 2.0;
    let cp_bar = (cp1 + cp2) / 2.0;
    let h_bar = if chroma_product == 0.0 {
        hp1 + hp2
    } else if (hp1 - hp2).abs() <= 180.0 {
        (hp1 + hp2) / 2.0
    } else if hp1 + hp2 < 360.0 {
        (hp1 + hp2 + 360.0) / 2.0
    } else {
        (hp1 + hp2 - 360.0) / 2.0
    };
    let cos = |degrees: f64| degrees.to_radians().cos();
    let t =
        1.0 - 0.17 * cos(h_bar - 30.0) + 0.24 * cos(2.0 * h_bar) + 0.32 * cos(3.0 * h_bar + 6.0)
            - 0.20 * cos(4.0 * h_bar - 63.0);
    let d_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let cp7 = cp_bar.powi(7);
    let rc = 2.0 * (cp7 / (cp7 + P25)).sqrt();
    let l50 = (l_bar - 50.0).powi(2);
    let sl = 1.0 + 0.015 * l50 / (20.0 + l50).sqrt();
    let sc = 1.0 + 0.045 * cp_bar;
    let sh = 1.0 + 0.015 * cp_bar * t;
    let rt = -(2.0 * d_theta).to_radians().sin() * rc;
    let (tl, tc, th) = (dl / sl, dc / sc, dh_big / sh);
    (tl * tl + tc * tc + th * th + rt * tc * th).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The supplementary test data of Sharma, Wu, and Dalal (2005), from
    /// <https://www.ece.rochester.edu/~gsharma/ciede2000/>: two CIELAB colors
    /// and their CIEDE2000 difference on each line.
    const SHARMA: &str = include_str!("../testdata/ciede2000.txt");

    #[test]
    fn ciede2000_matches_the_published_test_data() {
        let mut pairs = 0;
        for line in SHARMA.lines().filter(|line| !line.trim().is_empty()) {
            let v: Vec<f64> = line
                .split_whitespace()
                .map(|field| field.parse().expect("number"))
                .collect();
            assert_eq!(v.len(), 7, "{line}");
            let (a, b) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]]);
            assert!((ciede2000(a, b) - v[6]).abs() < 5e-5, "{line}");
            assert!((ciede2000(b, a) - v[6]).abs() < 5e-5, "symmetric: {line}");
            pairs += 1;
        }
        assert_eq!(pairs, 34);
    }

    #[test]
    fn white_is_lightness_100_and_black_is_0() {
        let white = white();
        let [l, a, b] = lab(white, white);
        assert!((l - 100.0).abs() < 1e-4 && a.abs() < 1e-4 && b.abs() < 1e-4);
        assert_eq!(lab([0.0; 3], white), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn srgb_round_trips_and_middle_gray_is_a_fifth_of_the_light() {
        for v in 0..=255 {
            assert_eq!(encode(decode(v)), v);
        }
        assert!((decode(128) - 0.2158).abs() < 1e-3);
    }

    #[test]
    fn the_inverse_undoes_the_matrix() {
        let back = product(&inverse(&SRGB_TO_XYZ), &SRGB_TO_XYZ);
        for (i, row) in back.iter().enumerate() {
            for (j, value) in row.iter().enumerate() {
                let identity = if i == j { 1.0 } else { 0.0 };
                assert!((value - identity).abs() < 1e-5);
            }
        }
    }
}
