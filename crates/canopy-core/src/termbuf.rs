use crate::{
    geom::{Expanse, Frame, Line, Point, Rect},
    render::RenderBackend,
    style::Style,
};

/// NULL character constant
const NULL: char = '\0';

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

#[derive(Clone, Debug)]
pub struct TermBuf {
    pub(crate) size: Expanse,
    pub(crate) cells: Vec<Cell>,
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
    /// Create an empty TermBuf filled with NULL characters
    pub fn empty_with_style(size: impl Into<Expanse>, style: Style) -> Self {
        Self::new(size, NULL, style)
    }

    /// Create an empty TermBuf filled with NULL characters
    pub fn empty(size: impl Into<Expanse>) -> Self {
        let default_style = Style {
            fg: crate::style::Color::White,
            bg: crate::style::Color::Black,
            attrs: crate::style::AttrSet::default(),
        };
        Self::new(size, NULL, default_style)
    }

    /// Copy non-NULL characters from a rectangle of another TermBuf into this one
    pub fn copy(&mut self, src: &TermBuf, rect: Rect) {
        if src.size != self.size {
            return;
        }

        // Intersect the rectangle with our bounds
        if let Some(isec) = self.rect().intersect(&rect) {
            for y in isec.tl.y..isec.tl.y + isec.h {
                for x in isec.tl.x..isec.tl.x + isec.w {
                    let p = Point { x, y };
                    if let Some(cell) = src.get(p) {
                        if cell.ch != NULL {
                            self.put(p, cell.ch, cell.style.clone());
                        }
                    }
                }
            }
        }
    }

    /// Copy non-NULL characters from a source TermBuf into a destination rectangle
    pub fn copy_to_rect(&mut self, src: &TermBuf, dest_rect: Rect) {
        // The source buffer represents content to be placed at dest_rect
        // We need to map from source coordinates to destination coordinates

        // Intersect the destination rectangle with our bounds
        if let Some(clipped_dest) = self.rect().intersect(&dest_rect) {
            // Calculate the offset into the source buffer based on clipping
            let src_offset_x = (clipped_dest.tl.x - dest_rect.tl.x) as i32;
            let src_offset_y = (clipped_dest.tl.y - dest_rect.tl.y) as i32;

            // Copy the visible portion
            for dy in 0..clipped_dest.h {
                for dx in 0..clipped_dest.w {
                    let src_x = (dx as i32 + src_offset_x) as u32;
                    let src_y = (dy as i32 + src_offset_y) as u32;
                    let src_p = Point { x: src_x, y: src_y };

                    if let Some(cell) = src.get(src_p) {
                        if cell.ch != NULL {
                            let dest_p = Point {
                                x: clipped_dest.tl.x + dx,
                                y: clipped_dest.tl.y + dy,
                            };
                            self.put(dest_p, cell.ch, cell.style.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn size(&self) -> Expanse {
        self.size
    }

    pub fn rect(&self) -> Rect {
        self.size.rect()
    }

    fn idx(&self, p: Point) -> Option<usize> {
        if self.rect().contains_point(p) {
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

    /// Fill all empty (NULL) cells with the given character and style
    pub fn fill_empty(&mut self, ch: char, style: Style) {
        for i in 0..self.cells.len() {
            if self.cells[i].ch == NULL {
                self.cells[i] = Cell {
                    ch,
                    style: style.clone(),
                };
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
    use crate::buf;
    use crate::style::{AttrSet, Color, PartialStyle};
    use crate::tutils::buf::BufTest;

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

        BufTest::new(&tb).assert_matches(buf![
            " xx "
            " xx "
        ]);
    }

    #[test]
    fn text_write() {
        let mut tb = TermBuf::new(Expanse::new(5, 1), ' ', def_style());
        tb.text(def_style(), Line::new(0, 0, 5), "hi");

        BufTest::new(&tb).assert_matches(buf!["hi   "]);
    }

    #[test]
    fn solid_frame_draw() {
        let mut tb = TermBuf::new(Expanse::new(4, 4), ' ', def_style());
        let f = Frame::new(Rect::new(0, 0, 4, 4), 1);
        tb.solid_frame(def_style(), f, '#');

        BufTest::new(&tb).assert_matches(buf![
            "####"
            "#  #"
            "#  #"
            "####"
        ]);
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

        let bt = BufTest::new(&tb);
        assert!(bt.contains_text("hello"));
        assert!(bt.contains_text("world"));
        assert!(!bt.contains_text("goodbye"));
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
        assert!(BufTest::new(&tb).contains_text_style( "hello", &PartialStyle::fg(Color::Red)));
        assert!(!BufTest::new(&tb).contains_text_style(
            "world",
            &PartialStyle::fg(Color::Red)
        ));

        assert!(BufTest::new(&tb).contains_text_style(
            "world",
            &PartialStyle::fg(Color::Blue)
        ));
        assert!(!BufTest::new(&tb).contains_text_style(
            "hello",
            &PartialStyle::fg(Color::Blue)
        ));

        // Test with empty partial style (matches any style)
        let partial_any = PartialStyle::default();
        assert!(BufTest::new(&tb).contains_text_style( "hello", &partial_any));
        assert!(BufTest::new(&tb).contains_text_style( "world", &partial_any));
        assert!(BufTest::new(&tb).contains_text_style( "test", &partial_any));

        // Test with multiple style attributes
        let partial_white_bg = PartialStyle::fg(Color::White).with_bg(Color::Black);
        assert!(BufTest::new(&tb).contains_text_style( "test", &partial_white_bg));
    }

    #[test]
    fn contains_text_fg_compat() {
        use crate::style::solarized;
        let mut tb = TermBuf::new(Expanse::new(10, 1), ' ', def_style());

        let mut blue_style = def_style();
        blue_style.fg = solarized::BLUE;

        tb.text(blue_style, Line::new(0, 0, 3), "two");

        // Test the old method
        assert!(BufTest::new(&tb).contains_text_fg("two", solarized::BLUE));

        // Test that it works the same as contains_text_style
        assert!(BufTest::new(&tb).contains_text_style(
            "two",
            &PartialStyle::fg(solarized::BLUE)
        ));
    }

    #[test]
    fn test_empty_and_copy() {
        // Test empty constructor
        let empty = TermBuf::empty(Expanse::new(5, 3));
        assert_eq!(empty.size(), Expanse::new(5, 3));
        BufTest::new(&empty).assert_matches(buf![
            "XXXXX"
            "XXXXX"
            "XXXXX"
        ]);

        // Test copy functionality
        let mut src = TermBuf::new(Expanse::new(5, 3), ' ', def_style());
        src.text(def_style(), Line::new(1, 1, 3), "ABC");

        BufTest::new(&src).assert_matches(buf![
            "     "
            " ABC "
            "     "
        ]);

        let mut dst = TermBuf::empty(Expanse::new(5, 3));
        dst.copy(&src, Rect::new(1, 1, 3, 1));

        // Check that only the text was copied (spaces are not copied)
        BufTest::new(&dst).assert_matches(buf![
            "XXXXX"
            "XABCX"
            "XXXXX"
        ]);

        // Test copy with partial rectangle
        let mut dst2 = TermBuf::empty(Expanse::new(5, 3));
        dst2.copy(&src, Rect::new(2, 1, 2, 1));

        BufTest::new(&dst2).assert_matches(buf![
            "XXXXX"
            "XXBCX"
            "XXXXX"
        ]);

        // Test copy with different sizes (should do nothing)
        let mut wrong_size = TermBuf::empty(Expanse::new(4, 3));
        wrong_size.copy(&src, Rect::new(0, 0, 5, 3));

        BufTest::new(&wrong_size).assert_matches(buf![
            "XXXX"
            "XXXX"
            "XXXX"
        ]);
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
        assert!(BufTest::new(&tb).contains_text_style( "bold", &PartialStyle::fg(Color::Red)));
        assert!(BufTest::new(&tb).contains_text_style(
            "italic",
            &PartialStyle::fg(Color::Blue)
        ));

        // Test with attributes
        assert!(BufTest::new(&tb).contains_text_style(
            "bold",
            &PartialStyle::attrs(AttrSet::new(Attr::Bold))
        ));
        assert!(BufTest::new(&tb).contains_text_style(
            "italic",
            &PartialStyle::attrs(AttrSet::new(Attr::Italic))
        ));

        // Test chaining
        let bold_red_style = PartialStyle::fg(Color::Red).with_attrs(AttrSet::new(Attr::Bold));
        assert!(BufTest::new(&tb).contains_text_style( "bold", &bold_red_style));

        // Test that it doesn't match wrong combinations
        let italic_red = PartialStyle::fg(Color::Red).with_attrs(AttrSet::new(Attr::Italic));
        assert!(!BufTest::new(&tb).contains_text_style( "bold", &italic_red));
    }

    #[test]
    fn test_fill_empty() {
        // Create an empty buffer
        let mut tb = TermBuf::empty(Expanse::new(5, 3));

        // Verify all cells are NULL initially using buf macro
        BufTest::new(&tb).assert_matches(buf![
            "XXXXX"
            "XXXXX"
            "XXXXX"
        ]);

        // Add some content to part of the buffer
        tb.text(def_style(), Line::new(1, 1, 3), "ABC");

        // Verify the content before fill_empty
        BufTest::new(&tb).assert_matches(buf![
            "XXXXX"
            "XABCX"
            "XXXXX"
        ]);

        // Fill empty cells with a specific character and style
        let mut fill_style = def_style();
        fill_style.fg = Color::Red;
        tb.fill_empty('.', fill_style.clone());

        // Check that the buffer now has dots where there were NULLs
        BufTest::new(&tb).assert_matches(buf![
            "....."
            ".ABC."
            "....."
        ]);

        // Verify specific style properties
        assert_eq!(tb.get(Point { x: 0, y: 0 }).unwrap().style.fg, Color::Red);
        assert_eq!(tb.get(Point { x: 1, y: 1 }).unwrap().style.fg, Color::White);
    }
}
