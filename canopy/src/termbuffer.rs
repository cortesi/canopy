use crate::geom::{Expanse, Frame, Line, Point, Rect};
use crate::style::Style;

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
}
