use super::{Error, Point, PointI32, Rect, Size};

/// A half-open rectangle with a signed origin and unsigned size.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct RectI32 {
    /// Top-left corner.
    pub tl: PointI32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl RectI32 {
    /// Empty rectangle at the origin.
    pub const ZERO: Self = Self {
        tl: PointI32::ZERO,
        w: 0,
        h: 0,
    };

    /// Construct a rectangle from coordinates and size.
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            tl: PointI32 { x, y },
            w,
            h,
        }
    }

    /// Does this rect have a zero size?
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Return the rectangle's dimensions without its origin.
    pub const fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    /// Translate the origin, rejecting a result outside signed coordinates.
    pub fn translate(self, offset: PointI32) -> Result<Self, Error> {
        let tl = PointI32::try_from_i64(
            i64::from(self.tl.x) + i64::from(offset.x),
            i64::from(self.tl.y) + i64::from(offset.y),
        )?;
        Ok(Self { tl, ..self })
    }

    /// Convert a screen point to local coordinates relative to this rect.
    /// If the point is to the left/top of the rect, the result clamps to 0.
    pub fn to_local_point(&self, p: Point) -> Point {
        let dx = i64::from(p.x) - self.left();
        let dy = i64::from(p.y) - self.top();
        Point {
            x: u32::try_from(dx.clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
            y: u32::try_from(dy.clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
        }
    }

    /// Intersect this signed rect with an unsigned rect in the same coordinate
    /// space.
    pub fn intersect_rect(&self, other: Rect) -> Option<Rect> {
        if self.is_empty() || other.is_empty() {
            return None;
        }
        let other_left = i64::from(other.tl.x);
        let other_top = i64::from(other.tl.y);

        let inter_left = self.left().max(other_left);
        let inter_top = self.top().max(other_top);
        let inter_right = self.right().min(other_left + i64::from(other.w));
        let inter_bottom = self.bottom().min(other_top + i64::from(other.h));

        if inter_right <= inter_left || inter_bottom <= inter_top {
            return None;
        }

        Some(Rect::new(
            u32::try_from(inter_left).ok()?,
            u32::try_from(inter_top).ok()?,
            u32::try_from(inter_right - inter_left).ok()?,
            u32::try_from(inter_bottom - inter_top).ok()?,
        ))
    }

    /// Left edge of the rect.
    pub fn left(&self) -> i64 {
        i64::from(self.tl.x)
    }

    /// Top edge of the rect.
    pub fn top(&self) -> i64 {
        i64::from(self.tl.y)
    }

    /// Right edge of the rect.
    pub fn right(&self) -> i64 {
        i64::from(self.tl.x) + i64::from(self.w)
    }

    /// Bottom edge of the rect.
    pub fn bottom(&self) -> i64 {
        i64::from(self.tl.y) + i64::from(self.h)
    }

    /// Center point as widened coordinates.
    ///
    /// The tuple preserves centers beyond `i32` when a large unsigned size
    /// extends from a signed origin.
    pub fn center(&self) -> (i64, i64) {
        (
            self.left() + i64::from(self.w) / 2,
            self.top() + i64::from(self.h) / 2,
        )
    }

    /// Return true if this rect overlaps another vertically.
    pub fn overlaps_vertical(&self, other: Self) -> bool {
        self.top() < other.bottom() && self.bottom() > other.top()
    }

    /// Return true if this rect overlaps another horizontally.
    pub fn overlaps_horizontal(&self, other: Self) -> bool {
        self.left() < other.right() && self.right() > other.left()
    }
}

impl TryFrom<Rect> for RectI32 {
    type Error = Error;

    fn try_from(rect: Rect) -> Result<Self, Self::Error> {
        Ok(Self {
            tl: PointI32::try_from(rect.tl)?,
            w: rect.w,
            h: rect.h,
        })
    }
}

impl TryFrom<RectI32> for Rect {
    type Error = Error;

    fn try_from(rect: RectI32) -> Result<Self, Self::Error> {
        Ok(Self {
            tl: Point::try_from(rect.tl)?,
            w: rect.w,
            h: rect.h,
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn boundary_i32() -> impl Strategy<Value = i32> {
        prop_oneof![
            Just(i32::MIN),
            Just(i32::MIN + 1),
            Just(-1),
            Just(0),
            Just(1),
            Just(i32::MAX - 1),
            Just(i32::MAX),
            any::<i32>()
        ]
    }

    fn boundary_u32() -> impl Strategy<Value = u32> {
        prop_oneof![
            Just(0),
            Just(1),
            Just(i32::MAX as u32),
            Just(u32::MAX - 1),
            Just(u32::MAX),
            any::<u32>()
        ]
    }

    #[test]
    fn intersection_uses_widened_edges() {
        let signed = RectI32::new(i32::MAX - 2, 0, 10, 1);
        let unsigned = Rect::new(i32::MAX as u32, 0, 8, 1);
        assert_eq!(
            signed.intersect_rect(unsigned),
            Some(Rect::new(i32::MAX as u32, 0, 8, 1))
        );
    }

    #[test]
    fn rectangles_convert_and_translate_without_narrowing_sizes() {
        let unsigned = Rect::new(7, 9, u32::MAX, 11);
        let signed = RectI32::try_from(unsigned).unwrap();
        assert_eq!(signed, RectI32::new(7, 9, u32::MAX, 11));
        assert_eq!(Rect::try_from(signed).unwrap(), unsigned);
        assert_eq!(
            signed.translate(PointI32::new(-10, 5)).unwrap(),
            RectI32::new(-3, 14, u32::MAX, 11)
        );
        assert!(
            RectI32::new(i32::MAX, 0, 1, 1)
                .translate(PointI32::new(1, 0))
                .is_err()
        );
        assert!(RectI32::try_from(Rect::new(u32::MAX, 0, 1, 1)).is_err());
        assert!(Rect::try_from(RectI32::new(-1, 0, 1, 1)).is_err());
    }

    proptest! {
        #[test]
        fn signed_intersection_is_contained_and_never_narrows(
            x in boundary_i32(),
            y in boundary_i32(),
            w in boundary_u32(),
            h in boundary_u32(),
            other_x in boundary_u32(),
            other_y in boundary_u32(),
            other_w in boundary_u32(),
            other_h in boundary_u32(),
        ) {
            let signed = RectI32::new(x, y, w, h);
            let other = Rect::new(other_x, other_y, other_w, other_h);
            if let Some(intersection) = signed.intersect_rect(other) {
                prop_assert!(!intersection.is_empty());
                prop_assert!(other.contains_rect(intersection));
                let x = i64::from(intersection.tl.x);
                let y = i64::from(intersection.tl.y);
                prop_assert!(x >= signed.left() && x < signed.right());
                prop_assert!(y >= signed.top() && y < signed.bottom());
            }
        }
    }
}
