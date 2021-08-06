use super::{Direction, Line, LineSegment, Point, Size};
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

    /// Return a rect with the same size, with the top left at the given point.
    pub fn at(&self, p: impl Into<Point>) -> Self {
        Rect {
            tl: p.into(),
            w: self.w,
            h: self.h,
        }
    }

    /// Carve a rectangle with a fixed width out of the start of the horizontal
    /// extent of this rect. Returns a [left, right] array. Left is either
    /// empty or has the extract width specified.
    ///
    ///```
    /// use canopy::geom::Rect;
    /// # fn main() {
    /// let r = Rect::new(5, 5, 10, 10);
    /// assert_eq!(r.carve_hstart(2), [Rect::new(5, 5, 2, 10), Rect::new(7, 5, 8, 10)]);
    /// assert_eq!(r.carve_hstart(20), [Rect::new(5, 5, 0, 10), Rect::new(5, 5, 10, 10)]);
    /// # }
    ///```
    pub fn carve_hstart(&self, width: u16) -> [Rect; 2] {
        let (h, t) = self.hextent().carve_start(width);
        // We can unwrap, because both extents are within our range by definition.
        [self.hslice(&h).unwrap(), self.hslice(&t).unwrap()]
    }

    /// Carve a rectangle with a fixed width out of the end of the horizontal
    /// extent of this rect. Returns a [left, right] array. Right is either
    /// empty or has the exact width specified.
    ///
    ///```
    /// use canopy::geom::Rect;
    /// # fn main() {
    /// let r = Rect::new(5, 5, 10, 10);
    /// assert_eq!(r.carve_hend(2), [Rect::new(5, 5, 8, 10), Rect::new(13, 5, 2, 10)]);
    /// assert_eq!(r.carve_hend(20), [Rect::new(5, 5, 10, 10), Rect::new(15, 5, 0, 10)]);
    /// # }
    ///```
    pub fn carve_hend(&self, height: u16) -> [Rect; 2] {
        let (h, t) = self.hextent().carve_end(height);
        // We can unwrap, because both extents are within our range by definition.
        [self.hslice(&h).unwrap(), self.hslice(&t).unwrap()]
    }

    /// Carve a rectangle with a fixed width out of the start of the vertical
    /// extent of this rect. Returns a [top, bottom] array. Top is either empty
    /// or has the exact height specified.
    ///
    ///```
    /// use canopy::geom::Rect;
    /// # fn main() {
    /// let r = Rect::new(5, 5, 10, 10);
    /// assert_eq!(r.carve_vstart(2), [Rect::new(5, 5, 10, 2), Rect::new(5, 7, 10, 8)]);
    /// assert_eq!(r.carve_vstart(20), [Rect::new(5, 5, 10, 0), Rect::new(5, 5, 10, 10)]);
    /// # }
    ///```
    pub fn carve_vstart(&self, height: u16) -> [Rect; 2] {
        let (h, t) = self.vextent().carve_start(height);
        // We can unwrap, because both extents are within our range by definition.
        [self.vslice(&h).unwrap(), self.vslice(&t).unwrap()]
    }

    /// Carve a rectangle with a fixed width out of the end of the vertical
    /// extent of this rect. Returns a [top, bottom] array. Bottom is either
    /// empty or has the exact height specified.
    ///
    ///```
    /// use canopy::geom::Rect;
    /// # fn main() {
    /// let r = Rect::new(5, 5, 10, 10);
    /// assert_eq!(r.carve_vend(2), [Rect::new(5, 5, 10, 8), Rect::new(5, 13, 10, 2)]);
    /// assert_eq!(r.carve_vend(20), [Rect::new(5, 5, 10, 10), Rect::new(5, 15, 10, 0)]);
    /// # }
    ///```
    pub fn carve_vend(&self, height: u16) -> [Rect; 2] {
        let (h, t) = self.vextent().carve_end(height);
        // We can unwrap, because both extents are within our range by definition.
        [self.vslice(&h).unwrap(), self.vslice(&t).unwrap()]
    }

    /// Clamp this rectangle, shifting it to lie within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, return an error.
    pub fn clamp_within(&self, rect: impl Into<Rect>) -> Result<Self> {
        let rect = rect.into();
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
    pub fn contains_point(&self, p: impl Into<Point>) -> bool {
        let p = p.into();
        if p.x < self.tl.x || p.x >= self.tl.x + self.w {
            false
        } else {
            !(p.y < self.tl.y || p.y >= self.tl.y + self.h)
        }
    }

    /// Does this rectangle completely enclose the other? If other is
    /// zero-sized but its origin lies within this rect, it's considered
    /// contained.
    pub fn contains_rect(&self, other: &Rect) -> bool {
        // The rectangle is completely contained if both the upper left and the
        // lower right points are inside self. There's a subtlety here: if other
        // is zero-sized, but it's origin lies within this rect, it is
        // considered contained. The saturating_subs below make sure that this
        // doesn't crash.
        if other.is_empty() {
            self.contains_point(other.tl)
        } else {
            self.contains_point(other.tl)
                && self.contains_point(Point {
                    x: (other.tl.x + other.w).saturating_sub(1),
                    y: (other.tl.y + other.h).saturating_sub(1),
                })
        }
    }

    /// Extracts an inner rectangle, given a border width.
    pub fn inner(&self, border: u16) -> Result<Rect> {
        if self.w < (border * 2) || self.h < (border * 2) {
            return Err(Error::Geometry(
                "rectangle too small to calculate inner".into(),
            ));
        }
        Ok(Rect::new(
            self.tl.x + border,
            self.tl.y + border,
            self.w - (border * 2),
            self.h - (border * 2),
        ))
    }

    /// Extract a horizontal section of this rect based on an extent.
    pub fn hslice(&self, e: &LineSegment) -> Result<Self> {
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
    pub fn rebase_point(&self, pt: impl Into<Point>) -> Result<Point> {
        let pt = pt.into();
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
                    tl: (x, y).into(),
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
    pub fn vslice(&self, e: &LineSegment) -> Result<Self> {
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

    pub fn first_line(&self) -> Line {
        Line {
            tl: self.tl,
            w: self.w,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn size(&self) -> Size {
        (*self).into()
    }

    /// Subtract a rectangle from this one, returning a set of rectangles
    /// describing what remains.
    pub fn sub(&self, other: &Rect) -> Vec<Rect> {
        if other == self {
            vec![]
        } else if let Some(isec) = self.intersect(other) {
            let rects = vec![
                // Left
                Rect {
                    tl: self.tl,
                    h: self.h,
                    w: isec.tl.x.saturating_sub(self.tl.x),
                },
                // Right
                Rect {
                    tl: Point {
                        x: isec.tl.x + isec.w,
                        y: self.tl.y,
                    },
                    h: self.h,
                    w: (self.tl.x + self.w).saturating_sub(isec.tl.x + isec.w),
                },
                // Top
                Rect {
                    tl: Point {
                        x: isec.tl.x,
                        y: self.tl.y,
                    },
                    h: isec.tl.y.saturating_sub(self.tl.y),
                    w: isec.w,
                },
                // Bottom
                Rect {
                    tl: Point {
                        x: isec.tl.x,
                        y: isec.tl.y + isec.h,
                    },
                    h: (self.tl.x + self.h).saturating_sub(isec.tl.y + isec.h),
                    w: isec.w,
                },
            ];
            rects.into_iter().filter(|x| !x.is_empty()).collect()
        } else {
            vec![*self]
        }
    }
}

impl From<Size> for Rect {
    fn from(s: Size) -> Rect {
        Rect {
            tl: Point::default(),
            w: s.w,
            h: s.h,
        }
    }
}

impl From<Line> for Rect {
    fn from(l: Line) -> Rect {
        Rect {
            tl: l.tl,
            w: l.w,
            h: 1,
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
    fn rect_sub() -> Result<()> {
        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(20, 20, 20, 20)),
            vec![Rect::new(10, 10, 10, 10)],
        );
        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(0, 0, 0, 0)),
            vec![Rect::new(10, 10, 10, 10)],
        );

        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(10, 10, 5, 5)),
            vec![Rect::new(15, 10, 5, 10), Rect::new(10, 15, 5, 5),],
        );

        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(15, 15, 5, 5)),
            vec![Rect::new(10, 10, 5, 10), Rect::new(15, 10, 5, 5),],
        );

        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(15, 15, 5, 5)),
            vec![Rect::new(10, 10, 5, 10), Rect::new(15, 10, 5, 5),],
        );

        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(12, 12, 6, 6)),
            vec![
                Rect::new(10, 10, 2, 10),
                Rect::new(18, 10, 2, 10),
                Rect::new(12, 10, 6, 2),
                Rect::new(12, 18, 6, 2),
            ],
        );

        assert_eq!(
            Rect::new(10, 10, 10, 10).sub(&Rect::new(10, 10, 10, 5)),
            vec![Rect::new(10, 15, 10, 5),],
        );

        Ok(())
    }

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
        assert!(r.contains_point((10, 10)));
        assert!(!r.contains_point((9, 10)));
        assert!(!r.contains_point((20, 20)));
        assert!(r.contains_point((19, 19)));
        assert!(!r.contains_point((20, 21)));

        assert!(r.contains_rect(&Rect::new(10, 10, 1, 1)));
        assert!(r.contains_rect(&Rect::new(10, 10, 0, 0)));
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
        assert_eq!(r.rebase_point((11, 11))?, Point { x: 1, y: 1 });
        assert_eq!(r.rebase_point((10, 10))?, Point { x: 0, y: 0 });
        if let Ok(_) = r.rebase_point((9, 9)) {
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
            Rect::new(11, 11, 5, 5).clamp_within(Rect::new(10, 10, 10, 10))?,
            Rect::new(11, 11, 5, 5),
        );
        assert_eq!(
            Rect::new(19, 19, 5, 5).clamp_within(Rect::new(10, 10, 10, 10))?,
            Rect::new(15, 15, 5, 5),
        );
        assert_eq!(
            Rect::new(5, 5, 5, 5).clamp_within(Rect::new(10, 10, 10, 10))?,
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
