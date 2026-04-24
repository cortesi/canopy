use super::{Direction, Error, Line, LineSegment, Point, Result, Size};

/// A rectangle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Rect {
    /// Top-left corner
    pub tl: Point,
    /// Width
    pub w: u32,
    /// Height
    pub h: u32,
}

impl Default for Rect {
    fn default() -> Self {
        Self::zero()
    }
}

impl Rect {
    /// Construct a rectangle from coordinates and size.
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            tl: Point { x, y },
            w,
            h,
        }
    }

    /// The width times the height of the rectangle
    pub fn area(&self) -> u32 {
        self.w.saturating_mul(self.h)
    }

    /// Creat a zero-sized `Rect` at the origin.
    pub fn zero() -> Self {
        Self::new(0, 0, 0, 0)
    }

    /// Return a rect with the same size, with the top left at the given point.
    pub fn at(&self, p: impl Into<Point>) -> Self {
        Self {
            tl: p.into(),
            w: self.w,
            h: self.h,
        }
    }

    /// Carve a rectangle with a fixed width out of the start of the horizontal
    /// extent of this rect. Returns a [left, right] array. Left is either
    /// empty or has the extract width specified.
    pub fn carve_hstart(&self, width: u32) -> (Self, Self) {
        let (h, t) = self.hextent().carve_start(width);
        // We can unwrap, because both extents are within our range by definition.
        (self.hslice(&h).unwrap(), self.hslice(&t).unwrap())
    }

    /// Carve a rectangle with a fixed width out of the end of the horizontal
    /// extent of this rect. Returns a [left, right] array. Right is either
    /// empty or has the exact width specified.
    pub fn carve_hend(&self, width: u32) -> (Self, Self) {
        let (h, t) = self.hextent().carve_end(width);
        // We can unwrap, because both extents are within our range by definition.
        (self.hslice(&h).unwrap(), self.hslice(&t).unwrap())
    }

    /// Carve a rectangle with a fixed height out of the start of the vertical
    /// extent of this rect. Returns a [top, bottom] array. Top is either empty
    /// or has the exact height specified.
    pub fn carve_vstart(&self, height: u32) -> (Self, Self) {
        let (h, t) = self.vextent().carve_start(height);
        // We can unwrap, because both extents are within our range by definition.
        (self.vslice(&h).unwrap(), self.vslice(&t).unwrap())
    }

    /// Carve a rectangle with a fixed height out of the end of the vertical
    /// extent of this rect. Returns a [top, bottom] array. Bottom is either
    /// empty or has the exact height specified.
    pub fn carve_vend(&self, height: u32) -> (Self, Self) {
        let (h, t) = self.vextent().carve_end(height);
        // We can unwrap, because both extents are within our range by definition.
        (self.vslice(&h).unwrap(), self.vslice(&t).unwrap())
    }

    /// Clamp this rectangle, shifting it to lie within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, return an error.
    pub fn clamp_within(&self, rect: impl Into<Self>) -> Result<Self> {
        let rect = rect.into();
        if rect.w < self.w || rect.h < self.h {
            Err(Error::Geometry("can't clamp to smaller rectangle".into()))
        } else {
            Ok(Self {
                tl: self.tl.clamp(Self {
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
        if self.is_zero() {
            p == self.tl
        } else {
            let right = self.tl.x.saturating_add(self.w);
            let bottom = self.tl.y.saturating_add(self.h);
            !((p.x < self.tl.x || p.x >= right) || (p.y < self.tl.y || p.y >= bottom))
        }
    }

    /// Does this rectangle completely enclose the other? If other is
    /// zero-sized but its origin lies within this rect, it's considered
    /// contained.
    pub fn contains_rect(&self, other: &Self) -> bool {
        // The rectangle is completely contained if both the upper left and the
        // lower right points are inside self. There's a subtlety here: if other
        // is zero-sized, but it's origin lies within this rect, it is
        // considered contained. The saturating_subs below make sure that this
        // doesn't crash.
        if other.is_zero() {
            self.contains_point(other.tl)
        } else {
            self.contains_point(other.tl)
                && self.contains_point(Point {
                    x: (other.tl.x + other.w).saturating_sub(1),
                    y: (other.tl.y + other.h).saturating_sub(1),
                })
        }
    }

    /// Extracts an inner rectangle, given a border width. If the border width
    /// would exceed the size of the Rect, we return a zero rect.
    pub fn inner(&self, border: u32) -> Self {
        let Some(border_width) = border.checked_mul(2) else {
            return Self::default();
        };
        if self.w < border_width || self.h < border_width {
            Self::default()
        } else {
            Self::new(
                self.tl.x.saturating_add(border),
                self.tl.y.saturating_add(border),
                self.w - border_width,
                self.h - border_width,
            )
        }
    }

    /// Extract a horizontal section of this rect based on an extent.
    pub fn hslice(&self, e: &LineSegment) -> Result<Self> {
        if !self.hextent().contains(e) {
            Err(Error::Geometry("extract extent outside rectangle".into()))
        } else {
            Ok(Self::new(e.off, self.tl.y, e.len, self.h))
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
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let h = self.hextent().intersection(&other.hextent())?;
        let v = self.vextent().intersection(&other.vextent())?;
        Some(Self::new(h.off, v.off, h.len, v.len))
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
            x: pt.x.saturating_sub(self.tl.x),
            y: pt.y.saturating_sub(self.tl.y),
        })
    }

    /// Given a rectangle contained within this rectangle, shift the inner
    /// rectangle to be relative to our origin. If the rect is not entirely
    /// contained, an error is returned.
    pub fn rebase_rect(&self, other: &Self) -> Result<Self> {
        if !self.contains_rect(other) {
            return Err(Error::Geometry(format!(
                "rebase of non-contained rect - outer={self:?} inner={other:?}",
            )));
        }
        Ok(Self {
            tl: self.rebase_point(other.tl)?,
            w: other.w,
            h: other.h,
        })
    }

    /// A safe function for shifting the rectangle by an offset, which won't
    /// under- or overflow.
    pub fn shift(&self, x: i32, y: i32) -> Self {
        Self {
            tl: self.tl.scroll(x, y),
            w: self.w,
            h: self.h,
        }
    }

    /// Shift this rectangle, constrained to be within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, self unchanged.
    pub fn shift_within(&self, x: i32, y: i32, rect: Self) -> Self {
        if rect.w < self.w || rect.h < self.h {
            *self
        } else {
            Self {
                tl: self.tl.scroll_within(
                    x,
                    y,
                    Self {
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
    pub fn split_horizontal(&self, n: u32) -> Result<Vec<Self>> {
        let widths = split(self.w, n)?;
        let mut off: u32 = self.tl.x;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Self::new(off, self.tl.y, widths[i as usize], self.h));
            off = off.saturating_add(widths[i as usize]);
        }
        Ok(ret)
    }

    /// Splits the rectangle vertically into n sections, as close to equally
    /// sized as possible.
    pub fn split_vertical(&self, n: u32) -> Result<Vec<Self>> {
        let heights = split(self.h, n)?;
        let mut off: u32 = self.tl.y;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Self::new(self.tl.x, off, self.w, heights[i as usize]));
            off = off.saturating_add(heights[i as usize]);
        }
        Ok(ret)
    }

    /// Splits the rectangle into columns, with each column split into rows.
    /// Returns a Vec of rects per column.
    pub fn split_panes(&self, spec: &[u32]) -> Result<Vec<Vec<Self>>> {
        let mut ret = vec![];

        let cols = split(self.w, spec.len() as u32)?;
        let mut x = self.tl.x;
        for (ci, width) in cols.iter().enumerate() {
            let mut y = self.tl.y;
            let mut colret = vec![];
            for height in split(self.h, spec[ci])? {
                colret.push(Self {
                    tl: (x, y).into(),
                    w: *width,
                    h: height,
                });
                y = y.saturating_add(height);
            }
            ret.push(colret);
            x = x.saturating_add(*width);
        }
        Ok(ret)
    }

    /// Sweeps upwards from the top of the rectangle. Stops once the closure returns true.
    pub fn search_up(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for y in (0..self.tl.y).rev() {
            for x in self.tl.x..self.tl.x.saturating_add(self.w) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    /// Sweeps downwards from the bottom of the rectangle. Stops once the closure returns true.
    pub fn search_down(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for y in self.tl.y.saturating_add(self.h)..u32::MAX {
            for x in self.tl.x..self.tl.x.saturating_add(self.w) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    /// Sweeps leftwards the left of the rectangle. Stops once the closure returns true.
    pub fn search_left(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for x in (0..self.tl.x).rev() {
            for y in self.tl.y..self.tl.y.saturating_add(self.h) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    /// Sweeps rightwards from the right of the rectangle. Stops once the closure returns true.
    pub fn search_right(&self, f: &mut dyn FnMut(Point) -> Result<bool>) -> Result<()> {
        'outer: for x in self.tl.x.saturating_add(self.w)..u32::MAX {
            for y in self.tl.y..self.tl.y.saturating_add(self.h) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }

    /// Searches in a given direction sweeping to and fro. Stops once the closure returns true.
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
            Ok(Self::new(self.tl.x, e.off, self.w, e.len))
        }
    }

    /// The vertical extent of this rect.
    pub fn vextent(&self) -> LineSegment {
        LineSegment {
            off: self.tl.y,
            len: self.h,
        }
    }

    /// Return a line with a given offset in the rectangle.
    pub fn line(&self, off: u32) -> Line {
        if off >= self.h {
            panic!("offset exceeds rectangle height")
        }
        Line {
            tl: (self.tl.x, self.tl.y.saturating_add(off)).into(),
            w: self.w,
        }
    }

    /// Does this rect have a zero size?
    pub fn is_zero(&self) -> bool {
        self.area() == 0
    }

    /// Return the `Size` of this rectangle, which has the same size as the
    /// `Rect` but no location.
    pub fn expanse(&self) -> Size {
        (*self).into()
    }

    /// Subtract a rectangle from this one, returning a set of rectangles
    /// describing what remains.
    pub fn sub(&self, other: &Self) -> Vec<Self> {
        if other == self {
            vec![]
        } else if let Some(isec) = self.intersect(other) {
            let right = self.tl.x.saturating_add(self.w);
            let bottom = self.tl.y.saturating_add(self.h);
            let isec_right = isec.tl.x.saturating_add(isec.w);
            let isec_bottom = isec.tl.y.saturating_add(isec.h);
            let rects = vec![
                Self::new(
                    self.tl.x,
                    self.tl.y,
                    isec.tl.x.saturating_sub(self.tl.x),
                    self.h,
                ),
                Self::new(
                    isec_right,
                    self.tl.y,
                    right.saturating_sub(isec_right),
                    self.h,
                ),
                Self::new(
                    isec.tl.x,
                    self.tl.y,
                    isec.w,
                    isec.tl.y.saturating_sub(self.tl.y),
                ),
                Self::new(
                    isec.tl.x,
                    isec_bottom,
                    isec.w,
                    bottom.saturating_sub(isec_bottom),
                ),
            ];
            rects.into_iter().filter(|x| !x.is_zero()).collect()
        } else {
            vec![*self]
        }
    }
}

impl From<Size> for Rect {
    fn from(s: Size) -> Self {
        Self {
            tl: Point::default(),
            w: s.w,
            h: s.h,
        }
    }
}

impl From<Line> for Rect {
    fn from(l: Line) -> Self {
        Self {
            tl: l.tl,
            w: l.w,
            h: 1,
        }
    }
}

impl From<(u32, u32, u32, u32)> for Rect {
    fn from(v: (u32, u32, u32, u32)) -> Self {
        let (x_pos, y_pos, width, height) = v;
        Self {
            tl: (x_pos, y_pos).into(),
            w: width,
            h: height,
        }
    }
}

/// Split a length into n sections, as evenly as possible.
fn split(len: u32, n: u32) -> Result<Vec<u32>> {
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
    use proptest::prelude::*;

    use super::*;

    fn rect_strategy() -> impl Strategy<Value = Rect> {
        (0u32..200, 0u32..200, 0u32..100, 0u32..100).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
    }

    #[test]
    fn carve() -> Result<()> {
        let r = Rect::new(5, 5, 10, 10);

        assert_eq!(
            r.carve_hstart(2),
            (Rect::new(5, 5, 2, 10), Rect::new(7, 5, 8, 10))
        );
        assert_eq!(
            r.carve_hstart(20),
            (Rect::new(5, 5, 0, 10), Rect::new(5, 5, 10, 10))
        );

        assert_eq!(
            r.carve_hend(2),
            (Rect::new(5, 5, 8, 10), Rect::new(13, 5, 2, 10))
        );
        assert_eq!(
            r.carve_hend(20),
            (Rect::new(5, 5, 10, 10), Rect::new(15, 5, 0, 10))
        );

        assert_eq!(
            r.carve_vstart(2),
            (Rect::new(5, 5, 10, 2), Rect::new(5, 7, 10, 8))
        );
        assert_eq!(
            r.carve_vstart(20),
            (Rect::new(5, 5, 10, 0), Rect::new(5, 5, 10, 10))
        );

        assert_eq!(
            r.carve_vend(2),
            (Rect::new(5, 5, 10, 8), Rect::new(5, 13, 10, 2))
        );
        assert_eq!(
            r.carve_vend(20),
            (Rect::new(5, 5, 10, 10), Rect::new(5, 15, 10, 0))
        );

        Ok(())
    }

    #[test]
    fn extreme_rect_arithmetic_saturates() {
        let rect = Rect::new(u32::MAX - 1, u32::MAX - 1, 10, 10);
        assert_eq!(rect.area(), 100);
        assert!(rect.contains_point((u32::MAX - 1, u32::MAX - 1)));
        assert!(!rect.contains_point((0, 0)));

        let huge = Rect::new(0, 0, u32::MAX, u32::MAX);
        assert_eq!(huge.area(), u32::MAX);
    }

    proptest! {
        #[test]
        fn intersection_is_commutative_and_contained(a in rect_strategy(), b in rect_strategy()) {
            let ab = a.intersect(&b);
            let ba = b.intersect(&a);
            prop_assert_eq!(ab, ba);
            if let Some(intersection) = ab {
                prop_assert!(a.contains_rect(&intersection));
                prop_assert!(b.contains_rect(&intersection));
            }
        }

        #[test]
        fn sub_fragments_stay_in_source_and_avoid_removed_rect(a in rect_strategy(), b in rect_strategy()) {
            for fragment in a.sub(&b) {
                prop_assert!(a.contains_rect(&fragment));
                prop_assert!(fragment.intersect(&b).is_none());
            }
        }

        #[test]
        fn split_horizontal_covers_original_width(rect in rect_strategy(), n in 1u32..20) {
            let parts = rect.split_horizontal(n).expect("non-zero split count should succeed");
            let total: u32 = parts.iter().map(|part| part.w).sum();
            prop_assert_eq!(total, rect.w);
            prop_assert!(parts.iter().all(|part| part.h == rect.h));
        }

        #[test]
        fn split_vertical_covers_original_height(rect in rect_strategy(), n in 1u32..20) {
            let parts = rect.split_vertical(n).expect("non-zero split count should succeed");
            let total: u32 = parts.iter().map(|part| part.h).sum();
            prop_assert_eq!(total, rect.h);
            prop_assert!(parts.iter().all(|part| part.w == rect.w));
        }
    }

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

        assert_eq!(
            Rect::new(3, 10, 10, 10).sub(&Rect::new(5, 12, 6, 6)),
            vec![
                Rect::new(3, 10, 2, 10),
                Rect::new(11, 10, 2, 10),
                Rect::new(5, 10, 6, 2),
                Rect::new(5, 18, 6, 2),
            ],
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
        assert_eq!(r.inner(1), Rect::new(1, 1, 8, 8),);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "offset exceeds rectangle height")]
    fn line_rejects_bottom_edge() {
        Rect::new(0, 0, 10, 10).line(10);
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

        let r = Rect::new(0, 0, 0, 0);
        assert!(r.contains_point((0, 0)));

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
        assert!(r.rebase_point((9, 9)).is_err());
        Ok(())
    }

    #[test]
    fn tscroll() -> Result<()> {
        assert_eq!(
            Rect::new(5, 5, 10, 10).shift(-10, -10),
            Rect::new(0, 0, 10, 10)
        );
        assert_eq!(
            Rect::new(u32::MAX - 5, u32::MAX - 5, 10, 10).shift(10, 10),
            Rect::new(u32::MAX, u32::MAX, 10, 10)
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
            r.split_panes(&[2, 2])?,
            vec![
                [Rect::new(10, 10, 20, 20), Rect::new(10, 30, 20, 20)],
                [Rect::new(30, 10, 20, 20), Rect::new(30, 30, 20, 20)]
            ],
        );
        assert_eq!(
            r.split_panes(&[2, 1])?,
            vec![
                vec![Rect::new(10, 10, 20, 20), Rect::new(10, 30, 20, 20)],
                vec![Rect::new(30, 10, 20, 40)],
            ],
        );
        Ok(())
    }
}
