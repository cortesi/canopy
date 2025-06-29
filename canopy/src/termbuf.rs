use crate::{
    geom::{Expanse, Frame, Line, Point, Rect},
    render::RenderBackend,
    style::Style,
};

/// A helper macro to create buffers for the termbuf match assertions.
#[macro_export]
macro_rules! buf {
    ($($line:literal)*) => {
        &[$($line),*]
    };
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

#[derive(Clone, Debug)]
pub struct TermBuf {
    size: Expanse,
    cells: Vec<Cell>,
}

impl TermBuf {
    pub fn new(size: impl Into<Expanse>, ch: char, style: Style) -> Self {
        let size = size.into();
        let cell = Cell {
            ch,
            style: style.clone(),
        };
        TermBuf {
            size,
            cells: vec![cell; size.area() as usize],
        }
    }

    pub fn size(&self) -> Expanse {
        self.size
    }

    pub fn rect(&self) -> Rect {
        self.size.rect()
    }

    fn idx(&self, p: Point) -> Option<usize> {
        if p.x < self.size.w && p.y < self.size.h {
            Some(p.y as usize * self.size.w as usize + p.x as usize)
        } else {
            None
        }
    }

    fn put(&mut self, p: Point, ch: char, style: Style) {
        if let Some(i) = self.idx(p) {
            self.cells[i] = Cell { ch, style };
        }
    }

    pub fn fill(&mut self, style: Style, r: Rect, ch: char) {
        if let Some(isec) = self.rect().intersect(&r) {
            for y in isec.tl.y..isec.tl.y + isec.h {
                for x in isec.tl.x..isec.tl.x + isec.w {
                    self.put(Point { x, y }, ch, style.clone());
                }
            }
        }
    }

    pub fn solid_frame(&mut self, style: Style, f: Frame, ch: char) {
        self.fill(style.clone(), f.top, ch);
        self.fill(style.clone(), f.left, ch);
        self.fill(style.clone(), f.right, ch);
        self.fill(style.clone(), f.bottom, ch);
        self.fill(style.clone(), f.topleft, ch);
        self.fill(style.clone(), f.topright, ch);
        self.fill(style.clone(), f.bottomleft, ch);
        self.fill(style, f.bottomright, ch);
    }

    pub fn text(&mut self, style: Style, l: Line, txt: &str) {
        if let Some(isec) = self.rect().intersect(&l.rect()) {
            let offset = isec.tl.x - l.tl.x;
            let out: String = txt
                .chars()
                .skip(offset as usize)
                .take(l.w as usize)
                .collect();
            let mut chars = out.chars();
            for x in 0..isec.w {
                if let Some(ch) = chars.next() {
                    self.put(
                        Point {
                            x: isec.tl.x + x,
                            y: isec.tl.y,
                        },
                        ch,
                        style.clone(),
                    );
                } else {
                    self.put(
                        Point {
                            x: isec.tl.x + x,
                            y: isec.tl.y,
                        },
                        ' ',
                        style.clone(),
                    );
                }
            }
        }
    }

    pub fn get(&self, p: Point) -> Option<&Cell> {
        self.idx(p).map(|i| &self.cells[i])
    }

    /// Return the contents of a line as a `String`.
    pub fn line_text(&self, y: u16) -> Option<String> {
        if y >= self.size.h {
            return None;
        }
        let mut ret = String::new();
        for x in 0..self.size.w {
            if let Some(c) = self.get(Point { x, y }) {
                ret.push(c.ch);
            }
        }
        Some(ret)
    }

    /// Return the contents of the buffer as lines of text.
    pub fn lines(&self) -> Vec<String> {
        (0..self.size.h).filter_map(|y| self.line_text(y)).collect()
    }

    /// Does the buffer contain the supplied substring?
    pub fn contains_text(&self, txt: &str) -> bool {
        self.lines().iter().any(|l| l.contains(txt))
    }

    /// Does the buffer contain the supplied substring in the given foreground
    /// colour?
    pub fn contains_text_fg(&self, txt: &str, fg: crate::style::Color) -> bool {
        self.contains_text_style(txt, &crate::style::PartialStyle::fg(fg))
    }

    /// Does the buffer contain the supplied substring with the given style?
    pub fn contains_text_style(&self, txt: &str, style: &crate::style::PartialStyle) -> bool {
        let tl = txt.chars().count() as u16;
        if tl == 0 || tl > self.size.w {
            return false;
        }
        for y in 0..self.size.h {
            for x in 0..=self.size.w.saturating_sub(tl) {
                let mut m = true;
                let mut c = false;
                for (i, ch) in txt.chars().enumerate() {
                    if let Some(cell) = self.get(Point { x: x + i as u16, y }) {
                        if cell.ch != ch {
                            m = false;
                            break;
                        }
                        // Check if the cell style matches the partial style
                        let style_matches = (style.fg.is_none() || style.fg == Some(cell.style.fg))
                            && (style.bg.is_none() || style.bg == Some(cell.style.bg))
                            && (style.attrs.is_none() || style.attrs == Some(cell.style.attrs));
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

    /// Asserts that the buffer contents match the expected lines of text.
    ///
    /// This function compares the characters in the buffer against an expected
    /// set of lines, ignoring all styling information. It's useful for testing
    /// rendering output where only the text content matters.
    ///
    /// # Arguments
    ///
    /// * `expected` - A vector of strings representing the expected lines.
    ///   Each string should match one line of the buffer.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The number of lines doesn't match
    /// - Any line content doesn't match the expected text
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use canopy::{TermBuf, geom::Expanse, style::Style};
    /// # let buf = TermBuf::new(Expanse::new(5, 3), ' ', Style::default());
    /// buf.assert_buffer_matches(&[
    ///     "Hello",
    ///     "World",
    ///     "     ",
    /// ]);
    /// ```
    #[cfg(test)]
    pub fn assert_buffer_matches(&self, expected: &[&str]) {
        let actual_lines = self.lines();

        assert_eq!(
            expected.len(),
            self.size.h as usize,
            "Expected {} lines, but buffer has {} lines",
            expected.len(),
            self.size.h
        );

        for (y, expected_line) in expected.iter().enumerate() {
            let actual_line = &actual_lines[y];
            assert_eq!(
                actual_line.trim_end(),
                expected_line.trim_end(),
                "Line {y} mismatch:\nExpected: '{expected_line}'\nActual:   '{actual_line}'"
            );
        }
    }

    /// Returns true if the buffer content matches the expected lines.
    /// This is the non-panicking version of assert_buffer_matches.
    pub fn buffer_matches(&self, expected: &[&str]) -> bool {
        let actual_lines = self.lines();

        if expected.len() != self.size.h as usize {
            return false;
        }

        for (y, expected_line) in expected.iter().enumerate() {
            let actual_line = &actual_lines[y];
            if actual_line.trim_end() != expected_line.trim_end() {
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
        if !self.buffer_matches(expected) {
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
}

impl TermBuf {
    /// Diff this terminal buffer against a previous state, emitting changes
    /// to the provided render backend.
    pub fn diff<R: RenderBackend>(&self, prev: &TermBuf, backend: &mut R) -> crate::Result<()> {
        let mut wrote = false;
        if self.size != prev.size {
            return self.render(backend);
        }
        for y in 0..self.size.h {
            let mut x = 0;
            while x < self.size.w {
                let idx = y as usize * self.size.w as usize + x as usize;
                let cell = &self.cells[idx];
                let same = if y < prev.size.h && x < prev.size.w {
                    let pidx = y as usize * prev.size.w as usize + x as usize;
                    prev.cells[pidx] == *cell
                } else {
                    false
                };
                if same {
                    x += 1;
                    continue;
                }

                let style = cell.style.clone();
                let start_x = x;
                let mut text = String::new();
                while x < self.size.w {
                    let idx2 = y as usize * self.size.w as usize + x as usize;
                    let ccell = &self.cells[idx2];
                    let same = if y < prev.size.h && x < prev.size.w {
                        let pidx2 = y as usize * prev.size.w as usize + x as usize;
                        prev.cells[pidx2] == *ccell
                    } else {
                        false
                    };
                    if !same && ccell.style == style {
                        text.push(ccell.ch);
                        x += 1;
                    } else {
                        break;
                    }
                }
                backend.style(style)?;
                backend.text(Point { x: start_x, y }, &text)?;
                wrote = true;
            }
        }
        if wrote {
            backend.flush()?;
        }
        Ok(())
    }

    /// Render this terminal buffer in full using the provided backend,
    /// batching runs of text with the same style.
    pub fn render<R: RenderBackend>(&self, backend: &mut R) -> crate::Result<()> {
        let mut wrote = false;
        for y in 0..self.size.h {
            let mut x = 0;
            while x < self.size.w {
                let idx = y as usize * self.size.w as usize + x as usize;
                let cell = &self.cells[idx];
                let style = cell.style.clone();
                let start_x = x;
                let mut text = String::new();
                while x < self.size.w {
                    let idx2 = y as usize * self.size.w as usize + x as usize;
                    let ccell = &self.cells[idx2];
                    if ccell.style == style {
                        text.push(ccell.ch);
                        x += 1;
                    } else {
                        break;
                    }
                }
                backend.style(style)?;
                backend.text(Point { x: start_x, y }, &text)?;
                wrote = true;
            }
        }
        if wrote {
            backend.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{AttrSet, Color, PartialStyle};

    fn def_style() -> Style {
        Style {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        }
    }

    #[test]
    fn basic_fill() {
        let mut tb = TermBuf::new(Expanse::new(4, 2), ' ', def_style());
        tb.fill(def_style(), Rect::new(1, 0, 2, 2), 'x');
        assert_eq!(tb.get(Point { x: 1, y: 0 }).unwrap().ch, 'x');
        assert_eq!(tb.get(Point { x: 2, y: 1 }).unwrap().ch, 'x');
        assert_eq!(tb.get(Point { x: 3, y: 0 }).unwrap().ch, ' ');
    }

    #[test]
    fn text_write() {
        let mut tb = TermBuf::new(Expanse::new(5, 1), ' ', def_style());
        tb.text(def_style(), Line::new(0, 0, 5), "hi");
        assert_eq!(tb.get((0, 0).into()).unwrap().ch, 'h');
        assert_eq!(tb.get((1, 0).into()).unwrap().ch, 'i');
        assert_eq!(tb.get((2, 0).into()).unwrap().ch, ' ');
    }

    #[test]
    fn solid_frame_draw() {
        let mut tb = TermBuf::new(Expanse::new(4, 4), ' ', def_style());
        let f = Frame::new(Rect::new(0, 0, 4, 4), 1);
        tb.solid_frame(def_style(), f, '#');
        assert_eq!(tb.get((0, 0).into()).unwrap().ch, '#');
        assert_eq!(tb.get((1, 1).into()).unwrap().ch, ' ');
        assert_eq!(tb.get((3, 3).into()).unwrap().ch, '#');
    }

    struct RecBackend {
        ops: Vec<String>,
    }

    impl RecBackend {
        fn new() -> Self {
            RecBackend { ops: Vec::new() }
        }
    }

    impl RenderBackend for RecBackend {
        fn style(&mut self, s: Style) -> crate::Result<()> {
            self.ops.push(format!("style {s:?}"));
            Ok(())
        }

        fn text(&mut self, loc: Point, txt: &str) -> crate::Result<()> {
            self.ops.push(format!("text {} {} {}", loc.x, loc.y, txt));
            Ok(())
        }

        fn flush(&mut self) -> crate::Result<()> {
            Ok(())
        }

        fn exit(&mut self, _code: i32) -> ! {
            unreachable!()
        }

        fn reset(&mut self) -> crate::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diff_no_change() {
        let style = def_style();
        let tb1 = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        let tb2 = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        let mut be = RecBackend::new();
        tb2.diff(&tb1, &mut be).unwrap();
        assert!(be.ops.is_empty());
    }

    #[test]
    fn diff_single_run() {
        let style = def_style();
        let prev = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        let mut cur = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        cur.text(style.clone(), Line::new(0, 0, 3), "ab");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.ops.len(), 2);
        assert_eq!(be.ops[0], format!("style {style:?}"));
        assert_eq!(be.ops[1], "text 0 0 ab");
    }

    #[test]
    fn diff_style_changes() {
        let style1 = def_style();
        let mut style2 = style1.clone();
        style2.fg = Color::Red;

        let prev = TermBuf::new(Expanse::new(2, 1), ' ', style1.clone());
        let mut cur = TermBuf::new(Expanse::new(2, 1), ' ', style1.clone());
        cur.fill(style2.clone(), Rect::new(0, 0, 1, 1), 'a');
        cur.fill(style1.clone(), Rect::new(1, 0, 1, 1), 'b');

        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();

        assert_eq!(be.ops.len(), 4);
        assert_eq!(be.ops[0], format!("style {style2:?}"));
        assert_eq!(be.ops[1], "text 0 0 a");
        assert_eq!(be.ops[2], format!("style {style1:?}"));
        assert_eq!(be.ops[3], "text 1 0 b");
    }

    #[test]
    fn diff_multi_line() {
        let style = def_style();
        let prev = TermBuf::new(Expanse::new(3, 2), ' ', style.clone());
        let mut cur = TermBuf::new(Expanse::new(3, 2), ' ', style.clone());
        cur.fill(style.clone(), Rect::new(0, 1, 2, 1), 'x');
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(be.ops.len(), 2);
        assert_eq!(be.ops[0], format!("style {style:?}"));
        assert_eq!(be.ops[1], "text 0 1 xx");
    }

    #[test]
    fn render_whole_buffer() {
        let style = def_style();
        let mut tb = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        tb.text(style.clone(), Line::new(0, 0, 3), "ab");
        let mut be = RecBackend::new();
        tb.render(&mut be).unwrap();
        assert_eq!(
            be.ops,
            vec![format!("style {style:?}"), "text 0 0 ab ".to_string(),]
        );
    }

    #[test]
    fn diff_size_change_rerender() {
        let style = def_style();
        let prev = TermBuf::new(Expanse::new(2, 1), ' ', style.clone());
        let mut cur = TermBuf::new(Expanse::new(3, 1), ' ', style.clone());
        cur.text(style.clone(), Line::new(0, 0, 3), "abc");
        let mut be = RecBackend::new();
        cur.diff(&prev, &mut be).unwrap();
        assert_eq!(
            be.ops,
            vec![format!("style {style:?}"), "text 0 0 abc".to_string(),]
        );
    }

    #[test]
    fn contains_text() {
        let mut tb = TermBuf::new(Expanse::new(10, 3), ' ', def_style());
        tb.text(def_style(), Line::new(0, 0, 10), "hello");
        tb.text(def_style(), Line::new(0, 1, 10), "world");

        assert!(tb.contains_text("hello"));
        assert!(tb.contains_text("world"));
        assert!(!tb.contains_text("goodbye"));
    }

    #[test]
    fn contains_text_style() {
        let mut tb = TermBuf::new(Expanse::new(10, 3), ' ', def_style());

        // Add text with different styles
        let mut red_style = def_style();
        red_style.fg = Color::Red;

        let mut blue_style = def_style();
        blue_style.fg = Color::Blue;

        tb.text(red_style, Line::new(0, 0, 5), "hello");
        tb.text(blue_style, Line::new(5, 0, 5), "world");
        tb.text(def_style(), Line::new(0, 1, 10), "test line");

        // Test with foreground color partial style
        assert!(tb.contains_text_style("hello", &PartialStyle::fg(Color::Red)));
        assert!(!tb.contains_text_style("world", &PartialStyle::fg(Color::Red)));

        assert!(tb.contains_text_style("world", &PartialStyle::fg(Color::Blue)));
        assert!(!tb.contains_text_style("hello", &PartialStyle::fg(Color::Blue)));

        // Test with empty partial style (matches any style)
        let partial_any = PartialStyle::default();
        assert!(tb.contains_text_style("hello", &partial_any));
        assert!(tb.contains_text_style("world", &partial_any));
        assert!(tb.contains_text_style("test", &partial_any));

        // Test with multiple style attributes
        let partial_white_bg = PartialStyle::fg(Color::White).with_bg(Color::Black);
        assert!(tb.contains_text_style("test", &partial_white_bg));
    }

    #[test]
    fn contains_text_fg_compat() {
        use crate::style::solarized;
        let mut tb = TermBuf::new(Expanse::new(10, 1), ' ', def_style());

        let mut blue_style = def_style();
        blue_style.fg = solarized::BLUE;

        tb.text(blue_style, Line::new(0, 0, 3), "two");

        // Test the old method
        assert!(tb.contains_text_fg("two", solarized::BLUE));

        // Test that it works the same as contains_text_style
        assert!(tb.contains_text_style("two", &PartialStyle::fg(solarized::BLUE)));
    }

    #[test]
    fn contains_text_style_builders() {
        use crate::style::Attr;
        let mut tb = TermBuf::new(Expanse::new(10, 2), ' ', def_style());

        // Create styles with different attributes
        let mut bold_red = def_style();
        bold_red.fg = Color::Red;
        bold_red.attrs = AttrSet::new(Attr::Bold);

        let mut italic_blue = def_style();
        italic_blue.fg = Color::Blue;
        italic_blue.attrs = AttrSet::new(Attr::Italic);

        tb.text(bold_red, Line::new(0, 0, 4), "bold");
        tb.text(italic_blue, Line::new(0, 1, 6), "italic");

        // Test using builder methods
        assert!(tb.contains_text_style("bold", &PartialStyle::fg(Color::Red)));
        assert!(tb.contains_text_style("italic", &PartialStyle::fg(Color::Blue)));

        // Test with attributes
        assert!(tb.contains_text_style("bold", &PartialStyle::attrs(AttrSet::new(Attr::Bold))));
        assert!(tb.contains_text_style("italic", &PartialStyle::attrs(AttrSet::new(Attr::Italic))));

        // Test chaining
        let bold_red_style = PartialStyle::fg(Color::Red).with_attrs(AttrSet::new(Attr::Bold));
        assert!(tb.contains_text_style("bold", &bold_red_style));

        // Test that it doesn't match wrong combinations
        let italic_red = PartialStyle::fg(Color::Red).with_attrs(AttrSet::new(Attr::Italic));
        assert!(!tb.contains_text_style("bold", &italic_red));
    }
}
