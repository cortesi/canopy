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

/// Parse one hex digit.
const fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("invalid hex colour digit"),
    }
}

/// Parse one hex byte from its two digits.
///
/// This supports the [`rgb!`](crate::rgb) macro and is not part of the stable
/// surface.
#[doc(hidden)]
pub const fn hex_byte(high: u8, low: u8) -> u8 {
    hex_digit(high) * 16 + hex_digit(low)
}

/// Build a [`Color`](crate::style::Color) from a `#RRGGBB` or `RRGGBB` literal
/// at compile time.
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

    /// Scale brightness by a factor. 0.0 = black, 1.0 = unchanged, 2.0 = double
    /// brightness.
    #[must_use]
    pub fn scale_brightness(self, factor: f32) -> Self {
        let (r, g, b) = self.rgb();
        let scale = |v: u8| ((v as f32 * factor).clamp(0.0, 255.0)) as u8;
        Self::Rgb {
            r: scale(r),
            g: scale(g),
            b: scale(b),
        }
    }

    /// Adjust saturation. 0.0 = grayscale, 1.0 = unchanged, 2.0 = double
    /// saturation.
    #[must_use]
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

    /// Mix this color with another in a color space. `t` 0.0 is `self`, and
    /// 1.0 is `other`.
    ///
    /// `Mix::Rgb` mixes the sRGB channels. `Mix::Oklab` mixes in the
    /// perceptual OKLab space, so equal steps look equal. `Mix::Oklch` also
    /// takes the shorter way around the hue circle.
    #[must_use]
    pub fn mix(self, other: Self, t: f32, space: Mix) -> Self {
        let t = t.clamp(0.0, 1.0);
        match space {
            Mix::Rgb => {
                let (r1, g1, b1) = self.rgb();
                let (r2, g2, b2) = other.rgb();
                let mix = |a: u8, b: u8| {
                    let a = f32::from(a);
                    let b = f32::from(b);
                    (a + (b - a) * t).clamp(0.0, 255.0) as u8
                };
                Self::Rgb {
                    r: mix(r1, r2),
                    g: mix(g1, g2),
                    b: mix(b1, b2),
                }
            }
            Mix::Oklab => {
                let a = Oklab::from_color(self);
                let b = Oklab::from_color(other);
                Oklab {
                    l: lerp(a.l, b.l, t),
                    a: lerp(a.a, b.a, t),
                    b: lerp(a.b, b.b, t),
                }
                .to_color()
            }
            Mix::Oklch => {
                let a = Oklab::from_color(self).to_lch();
                let b = Oklab::from_color(other).to_lch();
                // An achromatic end has no hue, so it takes the other's.
                let (ha, hb) = match (a.c < ACHROMATIC, b.c < ACHROMATIC) {
                    (true, false) => (b.h, b.h),
                    (false, true) => (a.h, a.h),
                    _ => (a.h, b.h),
                };
                let mut dh = (hb - ha).rem_euclid(360.0);
                if dh > 180.0 {
                    dh -= 360.0;
                }
                Oklch {
                    l: lerp(a.l, b.l, t),
                    c: lerp(a.c, b.c, t),
                    h: ha + dh * t,
                }
                .to_oklab()
                .to_color()
            }
        }
    }

    /// Return the WCAG relative luminance, from 0.0 for black to 1.0 for
    /// white.
    pub fn relative_luminance(self) -> f32 {
        let (r, g, b) = self.rgb();
        0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b)
    }

    /// Return the WCAG contrast ratio with another color, from 1.0 for equal
    /// luminance to 21.0 for black on white.
    pub fn contrast_ratio(self, other: Self) -> f32 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Invert RGB channels (255 - value for each channel).
    #[must_use]
    pub fn invert_rgb(self) -> Self {
        let (r, g, b) = self.rgb();
        Self::Rgb {
            r: 255 - r,
            g: 255 - g,
            b: 255 - b,
        }
    }

    /// Shift hue by degrees (0-360).
    #[must_use]
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

/// The space in which two colors mix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Mix {
    /// The sRGB channels.
    Rgb,
    /// The perceptual OKLab space, where equal steps look equal.
    #[default]
    Oklab,
    /// OKLab in polar form, which takes the shorter way around the hue circle.
    Oklch,
}

/// Chroma below which an OKLCH color has no meaningful hue.
const ACHROMATIC: f32 = 1e-4;

/// Interpolate linearly between two values.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Convert one sRGB channel to linear light.
fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert linear light to one sRGB channel.
fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let v = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// A color in the OKLab space.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Oklab {
    /// Perceived lightness.
    l: f32,
    /// Green to red axis.
    a: f32,
    /// Blue to yellow axis.
    b: f32,
}

/// A color in OKLCH, the polar form of OKLab.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Oklch {
    /// Perceived lightness.
    l: f32,
    /// Chroma.
    c: f32,
    /// Hue in degrees.
    h: f32,
}

impl Oklab {
    /// Convert an sRGB color.
    #[allow(clippy::many_single_char_names)]
    fn from_color(color: Color) -> Self {
        let (r, g, b) = color.rgb();
        let (r, g, b) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
        let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
        let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
        let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
        let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
        Self {
            l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        }
    }

    /// Convert to the nearest sRGB color.
    #[allow(clippy::many_single_char_names)]
    fn to_color(self) -> Color {
        let l = self.l + 0.396_337_78 * self.a + 0.215_803_76 * self.b;
        let m = self.l - 0.105_561_346 * self.a - 0.063_854_17 * self.b;
        let s = self.l - 0.089_484_18 * self.a - 1.291_485_5 * self.b;
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        Color::Rgb {
            r: linear_to_srgb(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            g: linear_to_srgb(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            b: linear_to_srgb(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
        }
    }

    /// Convert to polar form.
    fn to_lch(self) -> Oklch {
        Oklch {
            l: self.l,
            c: self.a.hypot(self.b),
            h: self.b.atan2(self.a).to_degrees().rem_euclid(360.0),
        }
    }
}

impl Oklch {
    /// Convert to rectangular form.
    fn to_oklab(self) -> Oklab {
        let h = self.h.to_radians();
        Oklab {
            l: self.l,
            a: self.c * h.cos(),
            b: self.c * h.sin(),
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
    fn rgb_mixes_keep_the_channel_blend() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        let white = Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(
            black.mix(white, 0.5, Mix::Rgb),
            Color::Rgb {
                r: 127,
                g: 127,
                b: 127
            }
        );
        assert_eq!(black.mix(white, 0.0, Mix::Rgb), black);
        assert_eq!(black.mix(white, 1.0, Mix::Rgb), white);
    }

    #[test]
    fn oklab_round_trips_every_named_color() {
        let named = [
            Color::Black,
            Color::White,
            Color::Red,
            Color::Green,
            Color::Blue,
            Color::Grey,
            Color::DarkYellow,
            Color::AnsiValue(208),
            Color::Rgb {
                r: 18,
                g: 52,
                b: 86,
            },
        ];
        for color in named {
            let (r, g, b) = color.rgb();
            assert_eq!(Oklab::from_color(color).to_color(), Color::Rgb { r, g, b });
            assert_eq!(
                Oklab::from_color(color).to_lch().to_oklab().to_color(),
                Color::Rgb { r, g, b }
            );
        }
    }

    #[test]
    fn oklab_mixes_match_reference_values() {
        let black = Color::Black;
        let white = Color::White;
        // OKLab lightness 0.5 is sRGB 99, not the channel midpoint 127.
        assert_eq!(
            black.mix(white, 0.5, Mix::Oklab),
            Color::Rgb {
                r: 99,
                g: 99,
                b: 99
            }
        );
        assert_eq!(
            black.mix(white, 0.0, Mix::Oklab),
            Color::Rgb { r: 0, g: 0, b: 0 }
        );
        assert_eq!(
            black.mix(white, 1.0, Mix::Oklab),
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
    }

    #[test]
    fn oklch_takes_the_shorter_hue_path() {
        // Red (hue 29) to blue (hue 264) goes through magenta, not green.
        let (r, g, b) = Color::Red.mix(Color::Blue, 0.5, Mix::Oklch).rgb();
        assert!(r > g && b > g, "expected a magenta, got ({r}, {g}, {b})");
        // An achromatic end keeps the hue of the other end.
        let (r, g, b) = Color::Grey.mix(Color::Red, 0.5, Mix::Oklch).rgb();
        assert!(r > g && r > b, "expected a red, got ({r}, {g}, {b})");
    }

    #[test]
    fn contrast_follows_the_wcag_formula() {
        assert!((Color::Black.contrast_ratio(Color::White) - 21.0).abs() < 0.01);
        assert!((Color::White.contrast_ratio(Color::White) - 1.0).abs() < f32::EPSILON);
        assert!(Color::Black.relative_luminance().abs() < f32::EPSILON);
        assert!((Color::White.relative_luminance() - 1.0).abs() < 1e-6);
        let grey = Color::Rgb {
            r: 119,
            g: 119,
            b: 119,
        };
        // #777 on white is the textbook 4.48:1.
        assert!((grey.contrast_ratio(Color::White) - 4.48).abs() < 0.01);
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
