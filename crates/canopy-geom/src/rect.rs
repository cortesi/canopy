use super::{Error, Line, LineSegment, Point, Result, Size};

/// A half-open rectangle with an unsigned origin and size.
#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub struct Rect {
    /// Top-left corner
    pub tl: Point,
    /// Width
    pub w: u32,
    /// Height
    pub h: u32,
}

impl Rect {
    /// Empty rectangle at the origin.
    pub const ZERO: Self = Self {
        tl: Point::ZERO,
        w: 0,
        h: 0,
    };

    /// Construct a rectangle from coordinates and size.
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            tl: Point { x, y },
            w,
            h,
        }
    }

    /// Return the exclusive right edge using widened arithmetic.
    fn right(&self) -> u64 {
        u64::from(self.tl.x) + u64::from(self.w)
    }

    /// Return the exclusive bottom edge using widened arithmetic.
    fn bottom(&self) -> u64 {
        u64::from(self.tl.y) + u64::from(self.h)
    }

    /// Does this half-open rectangle contain the point?
    pub fn contains_point(&self, p: Point) -> bool {
        if self.is_empty() {
            false
        } else {
            p.x >= self.tl.x
                && u64::from(p.x) < self.right()
                && p.y >= self.tl.y
                && u64::from(p.y) < self.bottom()
        }
    }

    /// Does this rectangle completely enclose the other's half-open bounds?
    ///
    /// Empty rectangles are treated as anchored bounds. They are contained
    /// when their coincident edges fall within this rectangle's closed edge
    /// bounds, including the far edge.
    pub fn contains_rect(&self, other: Self) -> bool {
        self.tl.x <= other.tl.x
            && self.tl.y <= other.tl.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }

    /// Extract a horizontal section of this rect based on an extent.
    pub fn hslice(&self, e: LineSegment) -> Result<Self> {
        if !self.hextent().contains(e) {
            Err(Error::ExtentOutsideRect {
                extent: e,
                rect: *self,
            })
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
    pub fn intersect(&self, other: Self) -> Option<Self> {
        let h = self.hextent().intersection(other.hextent())?;
        let v = self.vextent().intersection(other.vextent())?;
        Some(Self::new(h.off, v.off, h.len, v.len))
    }

    /// Extract a slice of this rect based on a vertical extent.
    pub fn vslice(&self, e: LineSegment) -> Result<Self> {
        if !self.vextent().contains(e) {
            Err(Error::ExtentOutsideRect {
                extent: e,
                rect: *self,
            })
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
    ///
    /// This remains fallible because an empty rectangle has no valid line and
    /// silently moving an out-of-range render request would hide layout bugs.
    pub fn line(&self, off: u32) -> Result<Line> {
        if off >= self.h {
            return Err(Error::LineOffsetOutside {
                offset: off,
                height: self.h,
            });
        }
        Ok(Line {
            tl: (self.tl.x, self.tl.y.saturating_add(off)).into(),
            w: self.w,
        })
    }

    /// Does this rect have a zero size?
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Return the `Size` of this rectangle, which has the same size as the
    /// `Rect` but no location.
    pub const fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn rect_strategy() -> impl Strategy<Value = Rect> {
        (0u32..200, 0u32..200, 0u32..100, 0u32..100).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
    }

    fn boundary_u32() -> impl Strategy<Value = u32> {
        prop_oneof![
            Just(0),
            Just(1),
            Just(i32::MAX as u32),
            Just(u32::MAX - 1),
            Just(u32::MAX),
            any::<u32>(),
        ]
    }

    fn boundary_rect_strategy() -> impl Strategy<Value = Rect> {
        (
            boundary_u32(),
            boundary_u32(),
            boundary_u32(),
            boundary_u32(),
        )
            .prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
    }

    #[test]
    fn extreme_rect_arithmetic_saturates() {
        let rect = Rect::new(u32::MAX - 1, u32::MAX - 1, 10, 10);
        assert!(rect.contains_point((u32::MAX - 1, u32::MAX - 1).into()));
        assert!(!rect.contains_point((0, 0).into()));
    }

    proptest! {
        #[test]
        fn boundary_intersection_and_containment_agree(
            a in boundary_rect_strategy(),
            b in boundary_rect_strategy(),
        ) {
            let intersection = a.intersect(b);
            prop_assert_eq!(intersection, b.intersect(a));
            if let Some(intersection) = intersection {
                prop_assert!(!intersection.is_empty());
                prop_assert!(a.contains_rect(intersection));
                prop_assert!(b.contains_rect(intersection));
            }
            if a.contains_rect(b) && !b.is_empty() {
                prop_assert_eq!(a.intersect(b), Some(b));
            }
        }

        #[test]
        fn intersection_is_commutative_and_contained(a in rect_strategy(), b in rect_strategy()) {
            let ab = a.intersect(b);
            let ba = b.intersect(a);
            prop_assert_eq!(ab, ba);
            if let Some(intersection) = ab {
                prop_assert!(a.contains_rect(intersection));
                prop_assert!(b.contains_rect(intersection));
            }
        }

    }

    #[test]
    fn intersect() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        let r2 = Rect::new(11, 11, 2, 2);
        assert_eq!(r.intersect(r2), Some(r2));
        assert_eq!(r2.intersect(r), Some(r2));
        assert_eq!(r.intersect(r), Some(r));
        assert_eq!(
            r.intersect(Rect::new(9, 9, 3, 3)),
            Some(Rect::new(10, 10, 2, 2))
        );
        assert_eq!(
            r.intersect(Rect::new(19, 19, 3, 3)),
            Some(Rect::new(19, 19, 1, 1))
        );
        Ok(())
    }

    #[test]
    fn line_rejects_bottom_edge() {
        assert!(Rect::new(0, 0, 10, 10).line(10).is_err());
    }

    #[test]
    fn contains() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        assert!(r.contains_point((10, 10).into()));
        assert!(!r.contains_point((9, 10).into()));
        assert!(!r.contains_point((20, 20).into()));
        assert!(r.contains_point((19, 19).into()));
        assert!(!r.contains_point((20, 21).into()));

        assert!(r.contains_rect(Rect::new(10, 10, 1, 1)));
        assert!(r.contains_rect(Rect::new(10, 10, 0, 0)));
        assert!(r.contains_rect(r));

        let r = Rect::new(0, 0, 0, 0);
        assert!(!r.contains_point((0, 0).into()));
        assert!(r.contains_rect(r));

        Ok(())
    }
}
