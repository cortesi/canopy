use super::{Direction, LineSegment, Point};
use crate::{Error, Result};

/// A rectangle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Rect {
    /// Top-left corner
    pub tl: Point,
    /// Width
    pub w: u16,
    /// Height
    pub h: u16,
}

impl Default for Rect {
    fn default() -> Rect {
        Rect::new(0, 0, 0, 0)
    }
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Rect {
            tl: Point { x, y },
            w,
            h,
        }
    }

    pub fn at(&self, p: &Point) -> Self {
        Rect {
            tl: *p,
            w: self.w,
            h: self.h,
        }
    }

    /// Clamp this rectangle, constraining it lie within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, return an error.
    pub fn clamp(&self, rect: Rect) -> Result<Self> {
        if rect.w < self.w || rect.h < self.h {
            Err(Error::Geometry("can't clamp to smaller rectangle".into()))
        } else {
            Ok(Rect {
                tl: self.tl.clamp(Rect {
                    tl: rect.tl,
                    h: rect.h.saturating_sub(self.h),
                    w: rect.w.saturating_sub(self.w),
                }),
                w: self.w,
                h: self.h,
            })
        }
    }

    /// Does this rectangle contain the point?
    pub fn contains_point(&self, p: Point) -> bool {
        if p.x < self.tl.x || p.x >= self.tl.x + self.w {
            false
        } else {
            !(p.y < self.tl.y || p.y >= self.tl.y + self.h)
        }
    }

    /// Does this rectangle completely enclose the other?
    pub fn contains_rect(&self, other: &Rect) -> bool {
        // The rectangle is completely contained if both the upper left and the
        // lower right points are inside self.
        self.contains_point(other.tl)
            && self.contains_point(Point {
                x: other.tl.x + other.w - 1,
                y: other.tl.y + other.h - 1,
            })
    }

    /// Extracts an inner rectangle, given a border width.
    pub fn inner(&self, border: u16) -> Result<Rect> {
        if self.w < (border * 2) || self.h < (border * 2) {
            return Err(Error::Geometry("rectangle too small".into()));
        }
        Ok(Rect::new(
            self.tl.x + border,
            self.tl.y + border,
            self.w - (border * 2),
            self.h - (border * 2),
        ))
    }

    /// Extract a horizontal section of this rect based on an extent.
    pub fn hextract(&self, e: &LineSegment) -> Result<Self> {
        if !self.hextent().contains(e) {
            Err(Error::Geometry("extract extent outside rectangle".into()))
        } else {
            Ok(Rect::new(e.off, self.tl.y, e.len, self.h))
        }
    }

    /// The horizontal extent of this rect.
    pub fn hextent(&self) -> LineSegment {
        LineSegment {
            off: self.tl.x,
            len: self.w,
        }
    }

    /// Calculate the intersection of this rectangle and another.
    pub fn intersect(&self, other: &Rect) -> Option<Self> {
        let h = self.hextent().intersect(&other.hextent())?;
        let v = self.vextent().intersect(&other.vextent())?;
        Some(Rect::new(h.off, v.off, h.len, v.len))
    }

    /// Given a point that falls within this rectangle, shift the point to be
    /// relative to our origin. If the point falls outside the rect, an error is
    /// returned.
    pub fn rebase_point(&self, pt: Point) -> Result<Point> {
        if !self.contains_point(pt) {
            return Err(Error::Geometry("rebase of non-contained point".into()));
        }
        Ok(Point {
            x: pt.x - self.tl.x,
            y: pt.y - self.tl.y,
        })
    }

    /// Given a rectangle contained within this rectangle, shift the inner
    /// rectangle to be relative to our origin. If the rect is not entirley
    /// contained, an error is returned.
    pub fn rebase_rect(&self, other: &Rect) -> Result<Rect> {
        if !self.contains_rect(other) {
            return Err(Error::Geometry("rebase of non-contained rect".into()));
        }
        Ok(Rect {
            tl: self.rebase_point(other.tl)?,
            w: other.w,
            h: other.h,
        })
    }

    /// A safe function for shifting the rectangle by an offset, which won't
    /// under- or overflow.
    pub fn shift(&self, x: i16, y: i16) -> Rect {
        Rect {
            tl: self.tl.scroll(x, y),
            w: self.w,
            h: self.h,
        }
    }

    /// Shift this rectangle, constrained to be within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, self unchanged.
    pub fn shift_within(&self, x: i16, y: i16, rect: Rect) -> Self {
        if rect.w < self.w || rect.h < self.h {
            *self
        } else {
            Rect {
                tl: self.tl.scroll_within(
                    x,
                    y,
                    Rect {
                        tl: rect.tl,
                        h: rect.h.saturating_sub(self.h),
                        w: rect.w.saturating_sub(self.w),
                    },
                ),
                w: self.w,
                h: self.h,
            }
        }
    }

    /// Splits the rectangle horizontally into n sections, as close to equally
    /// sized as possible.
    pub fn split_horizontal(&self, n: u16) -> Result<Vec<Rect>> {
        let widths = split(self.w, n)?;
        let mut off: u16 = self.tl.x;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Rect::new(off, self.tl.y, widths[i as usize], self.h));
            off += widths[i as usize];
        }
        Ok(ret)
    }

    /// Splits the rectangle vertically into n sections, as close to equally
    /// sized as possible.
    pub fn split_vertical(&self, n: u16) -> Result<Vec<Rect>> {
        let heights = split(self.h, n)?;
        let mut off: u16 = self.tl.y;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Rect::new(self.tl.x, off, self.w, heights[i as usize]));
            off += heights[i as usize];
        }
        Ok(ret)
    }

    /// Splits the rectangle into columns, with each column split into rows.
    /// Returns a Vec of rects per column.
    pub fn split_panes(&self, spec: &[u16]) -> Result<Vec<Vec<Rect>>> {
        let mut ret = vec![];

        let cols = split(self.w, spec.len() as u16)?;
        let mut x = self.tl.x;
        for (ci, width) in cols.iter().enumerate() {
            let mut y = self.tl.y;
            let mut colret = vec![];
            for height in split(self.h, spec[ci])? {
                colret.push(Rect {
                    tl: Point { x, y },
                    w: *width,
                    h: height,
                });
                y += height;
            }
            ret.push(colret);
            x += width;
        }
        Ok(ret)
    }

    // Sweeps upwards from the top of the rectangle.
    pub fn search_up(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for y in (0..self.tl.y).rev() {
            for x in self.tl.x..(self.tl.x + self.w) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    // Sweeps downwards from the bottom of the rectangle.
    pub fn search_down(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for y in self.tl.y + self.h..u16::MAX {
            for x in self.tl.x..(self.tl.x + self.w) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    // Sweeps leftwards the left of the rectangle.
    pub fn search_left(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for x in (0..self.tl.x).rev() {
            for y in self.tl.y..self.tl.y + self.h {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    // Sweeps rightwards from the right of the rectangle.
    pub fn search_right(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for x in self.tl.x + self.w..u16::MAX {
            for y in self.tl.y..self.tl.y + self.h {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    // Sweeps to and fro from the right of the rectangle to the left.
    pub fn search(&self, dir: Direction, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        match dir {
            Direction::Up => self.search_up(f),
            Direction::Down => self.search_down(f),
            Direction::Left => self.search_left(f),
            Direction::Right => self.search_right(f),
        }
    }

    /// Extract a slice of this rect based on a vertical extent.
    pub fn vextract(&self, e: &LineSegment) -> Result<Self> {
        if !self.vextent().contains(e) {
            Err(Error::Geometry("extract extent outside rectangle".into()))
        } else {
            Ok(Rect::new(self.tl.x, e.off, self.w, e.len))
        }
    }

    /// The vertical extent of this rect.
    pub fn vextent(&self) -> LineSegment {
        LineSegment {
            off: self.tl.y,
            len: self.h,
        }
    }
}

/// Split a length into n sections, as evenly as possible.
fn split(len: u16, n: u16) -> Result<Vec<u16>> {
    if n == 0 {
        return Err(Error::Geometry("divide by zero".into()));
    }
    let w = len / n;
    let rem = len % n;
    let mut v = Vec::with_capacity(n as usize);
    for i in 0..n {
        v.push(if i < rem { w + 1 } else { w })
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsearch() -> Result<()> {
        let bounds = Rect::new(0, 0, 6, 6);
        let r = Rect::new(2, 2, 2, 2);

        let mut v: Vec<Point> = vec![];
        r.search_up(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 2, y: 1 },
                Point { x: 3, y: 1 },
                Point { x: 2, y: 0 },
                Point { x: 3, y: 0 }
            ]
        );

        let mut v: Vec<Point> = vec![];
        r.search_left(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 1, y: 2 },
                Point { x: 1, y: 3 },
                Point { x: 0, y: 2 },
                Point { x: 0, y: 3 }
            ]
        );

        let mut v: Vec<Point> = vec![];
        r.search_down(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 2, y: 4 },
                Point { x: 3, y: 4 },
                Point { x: 2, y: 5 },
                Point { x: 3, y: 5 }
            ]
        );

        let mut v: Vec<Point> = vec![];
        r.search_right(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 4, y: 2 },
                Point { x: 4, y: 3 },
                Point { x: 5, y: 2 },
                Point { x: 5, y: 3 }
            ]
        );

        Ok(())
    }

    #[test]
    fn intersect() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        let r2 = Rect::new(11, 11, 2, 2);
        assert_eq!(r.intersect(&r2), Some(r2));
        assert_eq!(r2.intersect(&r), Some(r2));
        assert_eq!(r.intersect(&r), Some(r));
        assert_eq!(
            r.intersect(&Rect::new(9, 9, 3, 3)),
            Some(Rect::new(10, 10, 2, 2))
        );
        assert_eq!(
            r.intersect(&Rect::new(19, 19, 3, 3)),
            Some(Rect::new(19, 19, 1, 1))
        );
        Ok(())
    }

    #[test]
    fn inner() -> Result<()> {
        let r = Rect::new(0, 0, 10, 10);
        assert_eq!(r.inner(1)?, Rect::new(1, 1, 8, 8),);
        Ok(())
    }

    #[test]
    fn contains() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        assert!(r.contains_point(Point { x: 10, y: 10 }));
        assert!(!r.contains_point(Point { x: 9, y: 10 }));
        assert!(!r.contains_point(Point { x: 20, y: 20 }));
        assert!(r.contains_point(Point { x: 19, y: 19 }));
        assert!(!r.contains_point(Point { x: 20, y: 21 }));

        assert!(r.contains_rect(&Rect::new(10, 10, 1, 1)));
        assert!(r.contains_rect(&r));

        Ok(())
    }

    #[test]
    fn tsplit() -> Result<()> {
        assert_eq!(split(7, 3)?, vec![3, 2, 2]);
        assert_eq!(split(6, 3)?, vec![2, 2, 2]);
        assert_eq!(split(9, 1)?, vec![9]);
        Ok(())
    }

    #[test]
    fn trebase() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        assert_eq!(
            r.rebase_point(Point { x: 11, y: 11 })?,
            Point { x: 1, y: 1 }
        );
        assert_eq!(
            r.rebase_point(Point { x: 10, y: 10 })?,
            Point { x: 0, y: 0 }
        );

        if let Ok(_) = r.rebase_point(Point { x: 9, y: 9 }) {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn tscroll() -> Result<()> {
        assert_eq!(
            Rect::new(5, 5, 10, 10).shift(-10, -10),
            Rect::new(0, 0, 10, 10)
        );
        assert_eq!(
            Rect::new(u16::MAX - 5, u16::MAX - 5, 10, 10).shift(10, 10),
            Rect::new(u16::MAX, u16::MAX, 10, 10)
        );
        Ok(())
    }

    #[test]
    fn trect_clamp() -> Result<()> {
        assert_eq!(
            Rect::new(11, 11, 5, 5).clamp(Rect::new(10, 10, 10, 10))?,
            Rect::new(11, 11, 5, 5),
        );
        assert_eq!(
            Rect::new(19, 19, 5, 5).clamp(Rect::new(10, 10, 10, 10))?,
            Rect::new(15, 15, 5, 5),
        );
        assert_eq!(
            Rect::new(5, 5, 5, 5).clamp(Rect::new(10, 10, 10, 10))?,
            Rect::new(10, 10, 5, 5),
        );
        Ok(())
    }

    #[test]
    fn trect_scroll_within() -> Result<()> {
        let r = Rect::new(10, 10, 5, 5);
        assert_eq!(
            Rect::new(11, 11, 5, 5),
            r.shift_within(1, 1, Rect::new(10, 10, 10, 10),)
        );
        assert_eq!(
            Rect::new(15, 15, 5, 5),
            r.shift_within(10, 10, Rect::new(10, 10, 10, 10),)
        );
        // Degenerate case - trying to scroll within a smaller rect.
        assert_eq!(r.shift_within(1, 1, Rect::new(10, 10, 2, 2),), r);
        Ok(())
    }

    #[test]
    fn tpoint_scroll_within() -> Result<()> {
        let p = Point { x: 15, y: 15 };
        assert_eq!(
            Point { x: 10, y: 10 },
            p.scroll_within(-10, -10, Rect::new(10, 10, 10, 10),)
        );
        assert_eq!(
            Point { x: 20, y: 20 },
            p.scroll_within(10, 10, Rect::new(10, 10, 10, 10),)
        );
        assert_eq!(
            Point { x: 16, y: 15 },
            p.scroll_within(1, 0, Rect::new(10, 10, 10, 10),)
        );
        Ok(())
    }

    #[test]
    fn tsplit_panes() -> Result<()> {
        let r = Rect::new(10, 10, 40, 40);
        assert_eq!(
            r.split_panes(&vec![2, 2])?,
            vec![
                [Rect::new(10, 10, 20, 20), Rect::new(10, 30, 20, 20)],
                [Rect::new(30, 10, 20, 20), Rect::new(30, 30, 20, 20)]
            ],
        );
        assert_eq!(
            r.split_panes(&vec![2, 1])?,
            vec![
                vec![Rect::new(10, 10, 20, 20), Rect::new(10, 30, 20, 20)],
                vec![Rect::new(30, 10, 20, 40)],
            ],
        );
        Ok(())
    }
}
