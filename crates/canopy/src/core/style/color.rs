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

/// Macro to create a Color from a hex string at compile time
#[macro_export]
macro_rules! rgb {
    ($hex:literal) => {{
        const fn hex_char_to_num(c: u8) -> u8 {
            match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => panic!("Invalid hex character"),
            }
        }

        const fn parse_hex_byte(high: u8, low: u8) -> u8 {
            hex_char_to_num(high) * 16 + hex_char_to_num(low)
        }

        let bytes = $hex.as_bytes();
        let start = if bytes[0] == b'#' { 1 } else { 0 };

        if bytes.len() - start != 6 {
            panic!("Invalid hex color: must be 6 hex digits");
        }

        Color::Rgb {
            r: parse_hex_byte(bytes[start], bytes[start + 1]),
            g: parse_hex_byte(bytes[start + 2], bytes[start + 3]),
            b: parse_hex_byte(bytes[start + 4], bytes[start + 5]),
        }
    }};
}

impl Color {
    /// Construct a color from a hex RGB string.
    /// Accepts "#RRGGBB" or "RRGGBB" and panics on invalid input.
    pub fn rgb(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');

        if hex.len() != 6 {
            panic!(
                "Invalid hex color string: expected 6 hex digits, got {}",
                hex.len()
            );
        }

        let r = u8::from_str_radix(&hex[0..2], 16)
            .expect("Invalid hex color: failed to parse red component");
        let g = u8::from_str_radix(&hex[2..4], 16)
            .expect("Invalid hex color: failed to parse green component");
        let b = u8::from_str_radix(&hex[4..6], 16)
            .expect("Invalid hex color: failed to parse blue component");

        Self::Rgb { r, g, b }
    }

    /// Convert any color variant to RGB for transformation.
    /// Named colors and ANSI-256 use standard palette mappings.
    pub fn to_rgb(self) -> Self {
        match self {
            Self::Rgb { r, g, b } => Self::Rgb { r, g, b },
            Self::Black => Self::Rgb { r: 0, g: 0, b: 0 },
            Self::DarkGrey => Self::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
            Self::Red => Self::Rgb { r: 255, g: 0, b: 0 },
            Self::DarkRed => Self::Rgb { r: 128, g: 0, b: 0 },
            Self::Green => Self::Rgb { r: 0, g: 255, b: 0 },
            Self::DarkGreen => Self::Rgb { r: 0, g: 128, b: 0 },
            Self::Yellow => Self::Rgb {
                r: 255,
                g: 255,
                b: 0,
            },
            Self::DarkYellow => Self::Rgb {
                r: 128,
                g: 128,
                b: 0,
            },
            Self::Blue => Self::Rgb { r: 0, g: 0, b: 255 },
            Self::DarkBlue => Self::Rgb { r: 0, g: 0, b: 128 },
            Self::Magenta => Self::Rgb {
                r: 255,
                g: 0,
                b: 255,
            },
            Self::DarkMagenta => Self::Rgb {
                r: 128,
                g: 0,
                b: 128,
            },
            Self::Cyan => Self::Rgb {
                r: 0,
                g: 255,
                b: 255,
            },
            Self::DarkCyan => Self::Rgb {
                r: 0,
                g: 128,
                b: 128,
            },
            Self::White => Self::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            Self::Grey => Self::Rgb {
                r: 192,
                g: 192,
                b: 192,
            },
            Self::AnsiValue(n) => ansi_to_rgb(n),
        }
    }

    /// Scale brightness by a factor. 0.0 = black, 1.0 = unchanged, 2.0 = double brightness.
    pub fn scale_brightness(self, factor: f32) -> Self {
        let Self::Rgb { r, g, b } = self.to_rgb() else {
            unreachable!()
        };
        let scale = |v: u8| ((v as f32 * factor).clamp(0.0, 255.0)) as u8;
        Self::Rgb {
            r: scale(r),
            g: scale(g),
            b: scale(b),
        }
    }

    /// Adjust saturation. 0.0 = grayscale, 1.0 = unchanged, 2.0 = double saturation.
    pub fn saturation(self, factor: f32) -> Self {
        let Self::Rgb { r, g, b } = self.to_rgb() else {
            unreachable!()
        };
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let new_s = (s * factor).clamp(0.0, 1.0);
        let (nr, ng, nb) = hsl_to_rgb(h, new_s, l);
        Self::Rgb {
            r: nr,
            g: ng,
            b: nb,
        }
    }

    /// Blend this color with another. ratio 0.0 = self, 1.0 = other.
    pub fn blend(self, other: Self, ratio: f32) -> Self {
        let Self::Rgb {
            r: r1,
            g: g1,
            b: b1,
        } = self.to_rgb()
        else {
            unreachable!()
        };
        let Self::Rgb {
            r: r2,
            g: g2,
            b: b2,
        } = other.to_rgb()
        else {
            unreachable!()
        };
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
        let Self::Rgb { r, g, b } = self.to_rgb() else {
            unreachable!()
        };
        Self::Rgb {
            r: 255 - r,
            g: 255 - g,
            b: 255 - b,
        }
    }

    /// Shift hue by degrees (0-360).
    pub fn shift_hue(self, degrees: f32) -> Self {
        let Self::Rgb { r, g, b } = self.to_rgb() else {
            unreachable!()
        };
        let (mut h, s, l) = rgb_to_hsl(r, g, b);
        h = (h + degrees).rem_euclid(360.0);
        let (nr, ng, nb) = hsl_to_rgb(h, s, l);
        Self::Rgb {
            r: nr,
            g: ng,
            b: nb,
        }
    }
}

/// Convert ANSI 256-color to RGB.
fn ansi_to_rgb(n: u8) -> Color {
    match n {
        0 => Color::Rgb { r: 0, g: 0, b: 0 },
        1 => Color::Rgb { r: 128, g: 0, b: 0 },
        2 => Color::Rgb { r: 0, g: 128, b: 0 },
        3 => Color::Rgb {
            r: 128,
            g: 128,
            b: 0,
        },
        4 => Color::Rgb { r: 0, g: 0, b: 128 },
        5 => Color::Rgb {
            r: 128,
            g: 0,
            b: 128,
        },
        6 => Color::Rgb {
            r: 0,
            g: 128,
            b: 128,
        },
        7 => Color::Rgb {
            r: 192,
            g: 192,
            b: 192,
        },
        8 => Color::Rgb {
            r: 128,
            g: 128,
            b: 128,
        },
        9 => Color::Rgb { r: 255, g: 0, b: 0 },
        10 => Color::Rgb { r: 0, g: 255, b: 0 },
        11 => Color::Rgb {
            r: 255,
            g: 255,
            b: 0,
        },
        12 => Color::Rgb { r: 0, g: 0, b: 255 },
        13 => Color::Rgb {
            r: 255,
            g: 0,
            b: 255,
        },
        14 => Color::Rgb {
            r: 0,
            g: 255,
            b: 255,
        },
        15 => Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        // 216 color cube (16-231)
        16..=231 => {
            let n = n - 16;
            let r = (n / 36) % 6;
            let g = (n / 6) % 6;
            let b = n % 6;
            let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color::Rgb {
                r: to_val(r),
                g: to_val(g),
                b: to_val(b),
            }
        }
        // Grayscale (232-255)
        232..=255 => {
            let v = 8 + (n - 232) * 10;
            Color::Rgb { r: v, g: v, b: v }
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
    fn test_rgb_from_hex() {
        let test_cases = vec![
            // (input, expected_r, expected_g, expected_b)
            ("#FF0000", 255, 0, 0),     // Red
            ("FF0000", 255, 0, 0),      // Red without #
            ("#00FF00", 0, 255, 0),     // Green
            ("00FF00", 0, 255, 0),      // Green without #
            ("#0000FF", 0, 0, 255),     // Blue
            ("0000FF", 0, 0, 255),      // Blue without #
            ("#FFFFFF", 255, 255, 255), // White
            ("#000000", 0, 0, 0),       // Black
            ("#123456", 18, 52, 86),    // Random color
            ("ABCDEF", 171, 205, 239),  // Hex with letters
            ("abcdef", 171, 205, 239),  // Lowercase hex
        ];

        for (input, expected_r, expected_g, expected_b) in test_cases {
            let color = Color::rgb(input);
            match color {
                Color::Rgb { r, g, b } => {
                    assert_eq!(r, expected_r, "Red component mismatch for input: {input}");
                    assert_eq!(g, expected_g, "Green component mismatch for input: {input}");
                    assert_eq!(b, expected_b, "Blue component mismatch for input: {input}");
                }
                _ => panic!("Expected Color::Rgb variant for input: {input}"),
            }
        }
    }

    #[test]
    #[should_panic(expected = "Invalid hex color string: expected 6 hex digits")]
    fn test_rgb_invalid_length() {
        Color::rgb("#FFF");
    }

    #[test]
    #[should_panic(expected = "Invalid hex color: failed to parse")]
    fn test_rgb_invalid_hex() {
        Color::rgb("#GGGGGG");
    }

    #[test]
    fn test_rgb_macro() {
        // Test that the macro produces the same results as the function
        const MACRO_RED: Color = rgb!("#FF0000");
        const MACRO_GREEN: Color = rgb!("00FF00");
        const MACRO_BLUE: Color = rgb!("#0000FF");

        let func_red = Color::rgb("#FF0000");
        assert_eq!(MACRO_RED, func_red);

        let func_green = Color::rgb("00FF00");
        assert_eq!(MACRO_GREEN, func_green);

        let func_blue = Color::rgb("#0000FF");
        assert_eq!(MACRO_BLUE, func_blue);
    }

    #[test]
    fn test_to_rgb_named_colors() {
        assert_eq!(Color::Black.to_rgb(), Color::Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            Color::White.to_rgb(),
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(Color::Red.to_rgb(), Color::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(Color::Green.to_rgb(), Color::Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(Color::Blue.to_rgb(), Color::Rgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn test_to_rgb_ansi() {
        // Test a few ANSI colors
        assert_eq!(
            Color::AnsiValue(0).to_rgb(),
            Color::Rgb { r: 0, g: 0, b: 0 }
        );
        assert_eq!(
            Color::AnsiValue(15).to_rgb(),
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        // Color cube: red at index 196 (5,0,0) should be bright red
        assert_eq!(
            Color::AnsiValue(196).to_rgb(),
            Color::Rgb { r: 255, g: 0, b: 0 }
        );
        // Grayscale at 232 should be dark gray
        assert_eq!(
            Color::AnsiValue(232).to_rgb(),
            Color::Rgb { r: 8, g: 8, b: 8 }
        );
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
