//! Utilities for working with TermBufs in tests.
use crate::{
    core::termbuf::TermBuf,
    geom::Point,
    style::{Color, Paint, PartialStyle},
};

/// A helper macro to create buffers for the termbuf match assertions.
#[macro_export]
macro_rules! buf {
    ($($line:literal)*) => {
        &[$($line),*]
    };
}

/// Marker character that stands for a NULL cell in an expected pattern.
const NULL_MARKER: char = 'X';

/// A buffer matcher for tests. A NULL cell renders as `X` in the compared text, which lets a
/// test pin a partial render.
pub struct BufTest<'a> {
    /// Reference to the buffer under test.
    buf: &'a TermBuf,
}

impl<'a> BufTest<'a> {
    /// Create a new BufTest with a reference to a TermBuf.
    pub fn new(buf: &'a TermBuf) -> Self {
        Self { buf }
    }

    /// Return one row as a string, rendering NULL cells as the NULL marker.
    fn row_string(&self, y: u32) -> String {
        (0..self.buf.size().w)
            .filter_map(|x| self.buf.get(Point { x, y }))
            .map(|cell| match cell.display_char() {
                '\0' => NULL_MARKER,
                ch => ch,
            })
            .collect()
    }

    /// Returns true if the buffer content matches the expected lines.
    pub fn matches(&self, expected: &[&str]) -> bool {
        if expected.len() != self.buf.size().h as usize {
            return false;
        }

        for (y, expected_line) in expected.iter().enumerate() {
            let actual_line = self.row_string(y as u32);

            // Compare lines character by character to handle any_char
            let expected_trimmed = expected_line.trim_end();
            let actual_trimmed = actual_line.trim_end();

            if expected_trimmed != actual_trimmed {
                return false;
            }
        }

        true
    }

    /// Assert that the buffer matches the expected lines with pretty printed output on failure.
    pub fn assert_matches(&self, expected: &[&str]) {
        self.assert_matches_with_context(expected, None);
    }

    /// Assert that the buffer matches the expected lines with pretty printed output on failure,
    /// with optional context information.
    pub fn assert_matches_with_context(&self, expected: &[&str], context: Option<&str>) {
        if !self.matches(expected) {
            let actual_lines = self.lines();
            let width = expected.first().map(|l| l.len()).unwrap_or(10).max(10);

            if let Some(ctx) = context {
                println!("\n{ctx}");
            }

            println!("\nExpected:");
            println!("┌{}┐", "─".repeat(width));
            for line in expected {
                println!("│{line:width$}│");
            }
            println!("└{}┘", "─".repeat(width));

            println!("\nActual:");
            println!("┌{}┐", "─".repeat(width));
            for line in &actual_lines {
                println!("│{line:width$}│");
            }
            println!("└{}┘", "─".repeat(width));

            panic!("Buffer contents did not match expected pattern");
        }
    }

    /// Does the buffer contain the supplied substring?
    pub fn contains_text(&self, txt: &str) -> bool {
        self.lines().iter().any(|l| l.contains(txt))
    }

    /// Does the buffer contain the supplied substring in the given foreground colour?
    pub fn contains_text_fg(&self, txt: &str, fg: Color) -> bool {
        self.contains_text_style(txt, &PartialStyle::fg(fg))
    }

    /// Does the buffer contain the supplied substring with the given style?
    pub fn contains_text_style(&self, txt: &str, style: &PartialStyle) -> bool {
        let tl = txt.chars().count() as u32;
        if tl == 0 || tl > self.buf.size().w {
            return false;
        }
        for y in 0..self.buf.size().h {
            for x in 0..=self.buf.size().w.saturating_sub(tl) {
                let mut m = true;
                let mut c = false;
                for (i, ch) in txt.chars().enumerate() {
                    if let Some(cell) = self.buf.get(Point { x: x + i as u32, y }) {
                        if cell.display_char() != ch {
                            m = false;
                            break;
                        }
                        // Check if the cell style matches the partial style
                        let fg_matches = match &style.fg {
                            None => true,
                            Some(Paint::Solid(color)) => *color == cell.style.fg,
                            Some(Paint::Gradient(_)) => false,
                        };
                        let bg_matches = match &style.bg {
                            None => true,
                            Some(Paint::Solid(color)) => *color == cell.style.bg,
                            Some(Paint::Gradient(_)) => false,
                        };
                        let attr_matches =
                            style.attrs.is_none() || style.attrs == Some(cell.style.attrs);
                        let style_matches = fg_matches && bg_matches && attr_matches;
                        if style_matches {
                            c = true;
                        }
                    } else {
                        m = false;
                        break;
                    }
                }
                if m && c {
                    return true;
                }
            }
        }
        false
    }

    /// Dumps the contents of the buffer to the terminal for debugging purposes.
    pub fn dump(&self) {
        let width = self.buf.size().w as usize;

        println!(
            "\nTermBuf dump ({}x{}):",
            self.buf.size().w,
            self.buf.size().h
        );
        println!("┌{}┐", "─".repeat(width));

        for y in 0..self.buf.size().h {
            println!("│{}│{}", self.row_string(y), y % 10);
        }

        println!("└{}┘", "─".repeat(width));

        // Bottom ruler
        print!(" ");
        for x in 0..width {
            print!("{}", x % 10);
        }
        println!();
    }

    /// Return the contents of the buffer as lines of text.
    pub fn lines(&self) -> Vec<String> {
        (0..self.buf.size().h).map(|y| self.row_string(y)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::{Line, Size},
        style::{AttrSet, Color, ResolvedStyle},
    };

    #[test]
    fn buf_macro_accepts_any_line_count() {
        assert_eq!(crate::buf!("ab" "cd"), &["ab", "cd"]);
        assert_eq!(crate::buf!("test" "more" "text"), &["test", "more", "text"]);
        assert_eq!(crate::buf!("single line"), &["single line"]);
    }

    fn test_style() -> ResolvedStyle {
        ResolvedStyle::new(Color::White, Color::Black, AttrSet::default())
    }

    #[test]
    fn test_bufmatch_default() {
        let mut buf = TermBuf::new(Size::new(5, 3), '\0', test_style())
            .expect("test render target should allocate");
        buf.text(&test_style(), Line::new(0, 0, 5), "hello")
            .expect("test buffer mutation should succeed");

        let matcher = BufTest::new(&buf);
        assert!(matcher.matches(&["hello", "XXXXX", "XXXXX"]));
        assert!(!matcher.matches(&["world", "XXXXX", "XXXXX"]));
    }

    #[test]
    fn test_dump() {
        let mut buf = TermBuf::new(Size::new(5, 3), '\0', test_style())
            .expect("test render target should allocate");
        buf.text(&test_style(), Line::new(0, 0, 5), "hello")
            .expect("test buffer mutation should succeed");
        buf.text(&test_style(), Line::new(1, 1, 3), "abc")
            .expect("test buffer mutation should succeed");

        // This test just verifies dump() runs without panicking
        // The actual output goes to stdout
        BufTest::new(&buf).dump();
    }

    #[test]
    fn test_dump_with_larger_buffer() {
        // Test with a larger buffer to see the ruler wrap around
        let mut buf = TermBuf::new(Size::new(25, 15), '\0', test_style())
            .expect("test render target should allocate");
        buf.text(&test_style(), Line::new(0, 0, 10), "0123456789")
            .expect("test buffer mutation should succeed");
        buf.text(&test_style(), Line::new(10, 5, 15), "Offset at (10,5)")
            .expect("test buffer mutation should succeed");
        buf.text(&test_style(), Line::new(5, 10, 10), "Row 10 test")
            .expect("test buffer mutation should succeed");

        BufTest::new(&buf).dump();
    }

    #[test]
    fn test_buftest_instance_methods() {
        let mut buf = TermBuf::new(Size::new(10, 2), ' ', test_style())
            .expect("test render target should allocate");

        let mut red_style = test_style();
        red_style.fg = Color::Red;

        buf.text(&test_style(), Line::new(0, 0, 5), "hello")
            .expect("test buffer mutation should succeed");
        buf.text(&red_style, Line::new(5, 0, 5), "world")
            .expect("test buffer mutation should succeed");

        let bt = BufTest::new(&buf);

        // Test contains_text
        assert!(bt.contains_text("hello"));
        assert!(bt.contains_text("world"));
        assert!(!bt.contains_text("goodbye"));

        // Test contains_text_fg
        assert!(bt.contains_text_fg("world", Color::Red));
        assert!(!bt.contains_text_fg("hello", Color::Red));

        // Test contains_text_style
        assert!(bt.contains_text_style("world", &PartialStyle::fg(Color::Red)));
        assert!(bt.contains_text_style("hello", &PartialStyle::fg(Color::White)));

        // Test lines
        let lines = bt.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("helloworld"));
    }
}
