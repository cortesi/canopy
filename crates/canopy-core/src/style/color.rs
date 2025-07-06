#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Color {
    Black,
    DarkGrey,
    Red,
    DarkRed,
    Green,
    DarkGreen,
    Yellow,
    DarkYellow,
    Blue,
    DarkBlue,
    Magenta,
    DarkMagenta,
    Cyan,
    DarkCyan,
    White,
    Grey,
    Rgb {
        r: u8,
        g: u8,
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
    /// Constructs a Color from a hex RGB string.
    ///
    /// # Arguments
    /// * `hex` - A hex string in the format "#RRGGBB" or "RRGGBB"
    ///
    /// # Examples
    /// ```
    /// use canopy_core::style::Color;
    ///
    /// let color = Color::rgb("#FF0000"); // Red
    /// let color = Color::rgb("00FF00");  // Green
    /// ```
    ///
    /// # Panics
    /// Panics if the string is not a valid hex color.
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

        Color::Rgb { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgb;

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
        let func_red = Color::rgb("#FF0000");
        assert_eq!(MACRO_RED, func_red);

        const MACRO_GREEN: Color = rgb!("00FF00");
        let func_green = Color::rgb("00FF00");
        assert_eq!(MACRO_GREEN, func_green);

        const MACRO_BLUE: Color = rgb!("#0000FF");
        let func_blue = Color::rgb("#0000FF");
        assert_eq!(MACRO_BLUE, func_blue);
    }
}
