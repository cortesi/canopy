/// A terminal color value.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Color {
    /// Black.
    Black,
    /// Dark grey.
    DarkGrey,
    /// Red.
    Red,
    /// Dark red.
    DarkRed,
    /// Green.
    Green,
    /// Dark green.
    DarkGreen,
    /// Yellow.
    Yellow,
    /// Dark yellow.
    DarkYellow,
    /// Blue.
    Blue,
    /// Dark blue.
    DarkBlue,
    /// Magenta.
    Magenta,
    /// Dark magenta.
    DarkMagenta,
    /// Cyan.
    Cyan,
    /// Dark cyan.
    DarkCyan,
    /// White.
    White,
    /// Grey.
    Grey,
    /// RGB color.
    Rgb {
        /// Red channel.
        r: u8,
        /// Green channel.
        g: u8,
        /// Blue channel.
        b: u8,
    },

    /// An ANSI color. See [256 colors - cheat
    /// sheet](https://jonasjacek.github.io/colors/) for more info.
    AnsiValue(u8),
}

/// Parse one hex byte from its two digits.
///
/// This supports the [`rgb!`](crate::rgb) macro and is not part of the stable surface.
#[doc(hidden)]
pub const fn hex_byte(high: u8, low: u8) -> u8 {
    const fn digit(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => panic!("invalid hex colour digit"),
        }
    }
    digit(high) * 16 + digit(low)
}

/// Build a [`Color`](crate::style::Color) from a `#RRGGBB` or `RRGGBB` literal at compile time.
#[macro_export]
macro_rules! rgb {
    ($hex:literal) => {{
        const BYTES: &[u8] = $hex.as_bytes();
        const START: usize = if BYTES[0] == b'#' { 1 } else { 0 };
        const _: () = assert!(
            BYTES.len() - START == 6,
            "invalid hex colour: expected six hex digits"
        );
        $crate::style::Color::Rgb {
            r: $crate::style::hex_byte(BYTES[START], BYTES[START + 1]),
            g: $crate::style::hex_byte(BYTES[START + 2], BYTES[START + 3]),
            b: $crate::style::hex_byte(BYTES[START + 4], BYTES[START + 5]),
        }
    }};
}

/// RGB values for the sixteen named and ANSI-16 colors, in ANSI order.
const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

impl Color {
    /// Return this color's RGB channels.
    ///
    /// Named colors and ANSI-256 values use the standard palette mappings.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Rgb { r, g, b } => (r, g, b),
            Self::Black => ANSI16[0],
            Self::DarkRed => ANSI16[1],
            Self::DarkGreen => ANSI16[2],
            Self::DarkYellow => ANSI16[3],
            Self::DarkBlue => ANSI16[4],
            Self::DarkMagenta => ANSI16[5],
            Self::DarkCyan => ANSI16[6],
            Self::Grey => ANSI16[7],
            Self::DarkGrey => ANSI16[8],
            Self::Red => ANSI16[9],
            Self::Green => ANSI16[10],
            Self::Yellow => ANSI16[11],
            Self::Blue => ANSI16[12],
            Self::Magenta => ANSI16[13],
            Self::Cyan => ANSI16[14],
            Self::White => ANSI16[15],
            Self::AnsiValue(n) => ansi_to_rgb(n),
        }
    }

    /// Scale brightness by a factor. 0.0 = black, 1.0 = unchanged, 2.0 = double brightness.
    pub fn scale_brightness(self, factor: f32) -> Self {
        let (r, g, b) = self.rgb();
        let scale = |v: u8| ((v as f32 * factor).clamp(0.0, 255.0)) as u8;
        Self::Rgb {
            r: scale(r),
            g: scale(g),
            b: scale(b),
        }
    }

    /// Adjust saturation. 0.0 = grayscale, 1.0 = unchanged, 2.0 = double saturation.
    pub fn saturation(self, factor: f32) -> Self {
        let (r, g, b) = self.rgb();
        let (hue, sat, light) = rgb_to_hsl(r, g, b);
        let (nr, ng, nb) = hsl_to_rgb(hue, (sat * factor).clamp(0.0, 1.0), light);
        Self::Rgb {
            r: nr,
            g: ng,
            b: nb,
        }
    }

    /// Blend this color with another. ratio 0.0 = self, 1.0 = other.
    pub fn blend(self, other: Self, ratio: f32) -> Self {
        let (r1, g1, b1) = self.rgb();
        let (r2, g2, b2) = other.rgb();
        let mix = |a: u8, b: u8| {
            let a = a as f32;
            let b = b as f32;
            ((a + (b - a) * ratio).clamp(0.0, 255.0)) as u8
        };
        Self::Rgb {
            r: mix(r1, r2),
            g: mix(g1, g2),
            b: mix(b1, b2),
        }
    }

    /// Invert RGB channels (255 - value for each channel).
    pub fn invert_rgb(self) -> Self {
        let (r, g, b) = self.rgb();
        Self::Rgb {
            r: 255 - r,
            g: 255 - g,
            b: 255 - b,
        }
    }

    /// Shift hue by degrees (0-360).
    pub fn shift_hue(self, degrees: f32) -> Self {
        let (r, g, b) = self.rgb();
        let (hue, sat, light) = rgb_to_hsl(r, g, b);
        let (nr, ng, nb) = hsl_to_rgb((hue + degrees).rem_euclid(360.0), sat, light);
        Self::Rgb {
            r: nr,
            g: ng,
            b: nb,
        }
    }
}

/// Convert an ANSI 256-color index to RGB.
fn ansi_to_rgb(n: u8) -> (u8, u8, u8) {
    match n {
        0..=15 => ANSI16[n as usize],
        // 216 color cube (16-231)
        16..=231 => {
            let n = n - 16;
            let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (to_val((n / 36) % 6), to_val((n / 6) % 6), to_val(n % 6))
        }
        // Grayscale (232-255)
        232..=255 => {
            let v = 8 + (n - 232) * 10;
            (v, v, v)
        }
    }
}

/// Convert RGB to HSL.
#[allow(clippy::many_single_char_names)]
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f32::EPSILON {
        let mut h = (g - b) / d;
        if g < b {
            h += 6.0;
        }
        h
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    (h * 60.0, s, l)
}

/// Convert HSL to RGB.
#[allow(clippy::many_single_char_names)]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < f32::EPSILON {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h = h / 360.0;

    let hue_to_rgb = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };

    let r = (hue_to_rgb(h + 1.0 / 3.0) * 255.0).round() as u8;
    let g = (hue_to_rgb(h) * 255.0).round() as u8;
    let b = (hue_to_rgb(h - 1.0 / 3.0) * 255.0).round() as u8;

    (r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_macro_parses_hex_literals() {
        const RED: Color = rgb!("#FF0000");
        assert_eq!(RED, Color::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(rgb!("00FF00"), Color::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(rgb!("#0000FF"), Color::Rgb { r: 0, g: 0, b: 255 });
        assert_eq!(
            rgb!("#123456"),
            Color::Rgb {
                r: 18,
                g: 52,
                b: 86
            }
        );
        assert_eq!(
            rgb!("abcdef"),
            Color::Rgb {
                r: 171,
                g: 205,
                b: 239
            }
        );
    }

    #[test]
    fn rgb_maps_named_colors() {
        assert_eq!(Color::Black.rgb(), (0, 0, 0));
        assert_eq!(Color::White.rgb(), (255, 255, 255));
        assert_eq!(Color::Red.rgb(), (255, 0, 0));
        assert_eq!(Color::Green.rgb(), (0, 255, 0));
        assert_eq!(Color::Blue.rgb(), (0, 0, 255));
        assert_eq!(Color::Rgb { r: 1, g: 2, b: 3 }.rgb(), (1, 2, 3));
    }

    #[test]
    fn rgb_maps_the_ansi_palette() {
        assert_eq!(Color::AnsiValue(0).rgb(), (0, 0, 0));
        assert_eq!(Color::AnsiValue(15).rgb(), (255, 255, 255));
        // Color cube: index 196 is (5,0,0), bright red.
        assert_eq!(Color::AnsiValue(196).rgb(), (255, 0, 0));
        // Grayscale starts at 232.
        assert_eq!(Color::AnsiValue(232).rgb(), (8, 8, 8));
    }

    #[test]
    fn test_scale_brightness() {
        let red = Color::Rgb {
            r: 200,
            g: 100,
            b: 50,
        };
        // Scale down by half
        let dimmed = red.scale_brightness(0.5);
        assert_eq!(
            dimmed,
            Color::Rgb {
                r: 100,
                g: 50,
                b: 25
            }
        );
        // Scale to black
        let black = red.scale_brightness(0.0);
        assert_eq!(black, Color::Rgb { r: 0, g: 0, b: 0 });
    }

    #[test]
    fn test_saturation() {
        // Red should desaturate to gray
        let red = Color::Rgb { r: 255, g: 0, b: 0 };
        let gray = red.saturation(0.0);
        // Should be gray (equal R, G, B)
        if let Color::Rgb { r, g, b } = gray {
            assert_eq!(r, g);
            assert_eq!(g, b);
        } else {
            panic!("Expected RGB");
        }
    }

    #[test]
    fn test_blend() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        let white = Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        // Blend 50/50 should give gray (127 or 128 due to rounding)
        let gray = black.blend(white, 0.5);
        if let Color::Rgb { r, g, b } = gray {
            assert!((127..=128).contains(&r));
            assert!((127..=128).contains(&g));
            assert!((127..=128).contains(&b));
        } else {
            panic!("Expected RGB");
        }
        // Blend 0 should keep first color
        assert_eq!(black.blend(white, 0.0), black);
        // Blend 1 should give second color
        assert_eq!(black.blend(white, 1.0), white);
    }

    #[test]
    fn test_invert_rgb() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        assert_eq!(
            black.invert_rgb(),
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        let red = Color::Rgb { r: 255, g: 0, b: 0 };
        assert_eq!(
            red.invert_rgb(),
            Color::Rgb {
                r: 0,
                g: 255,
                b: 255
            }
        );
    }

    #[test]
    fn test_shift_hue() {
        // Red shifted 120 degrees should become green-ish
        let red = Color::Rgb { r: 255, g: 0, b: 0 };
        let shifted = red.shift_hue(120.0);
        if let Color::Rgb { r, g, b } = shifted {
            // Should be greenish (g > r, g > b)
            assert!(g > r);
            assert!(g > b);
        } else {
            panic!("Expected RGB");
        }
    }

    #[test]
    fn test_hsl_roundtrip() {
        // Test RGB -> HSL -> RGB roundtrip for various colors
        let colors = [
            (255, 0, 0),     // Red
            (0, 255, 0),     // Green
            (0, 0, 255),     // Blue
            (255, 255, 0),   // Yellow
            (128, 128, 128), // Gray
            (0, 0, 0),       // Black
            (255, 255, 255), // White
        ];
        for (r, g, b) in colors {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (nr, ng, nb) = hsl_to_rgb(h, s, l);
            assert_eq!(r, nr, "Red mismatch for ({}, {}, {})", r, g, b);
            assert_eq!(g, ng, "Green mismatch for ({}, {}, {})", r, g, b);
            assert_eq!(b, nb, "Blue mismatch for ({}, {}, {})", r, g, b);
        }
    }
}
