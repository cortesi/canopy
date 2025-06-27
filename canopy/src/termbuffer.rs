use crate::{
    geom::{Expanse, Frame, Line, Point, Rect},
    render::RenderBackend,
    style::Style,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

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
}

impl TermBuf {
    /// Diff this terminal buffer against a previous state, emitting changes
    /// to the provided render backend.
    pub fn diff<R: RenderBackend>(&self, prev: &TermBuf, backend: &mut R) -> crate::Result<()> {
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
            }
        }
        Ok(())
    }

    /// Render this terminal buffer in full using the provided backend,
    /// batching runs of text with the same style.
    pub fn render<R: RenderBackend>(&self, backend: &mut R) -> crate::Result<()> {
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
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{AttrSet, Color};

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
}
