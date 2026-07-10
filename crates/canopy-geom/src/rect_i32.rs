use super::{PointI32, Rect};

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
    /// Construct a rectangle from coordinates and size.
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            tl: PointI32 { x, y },
            w,
            h,
        }
    }

    /// Does this rect have a zero size?
    pub fn is_zero(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Check if the rectangle contains a point.
    pub fn contains_point(&self, p: super::Point) -> bool {
        if self.is_zero() {
            return false;
        }
        let px = i64::from(p.x);
        let py = i64::from(p.y);
        let left = i64::from(self.tl.x);
        let top = i64::from(self.tl.y);
        let right = left + i64::from(self.w);
        let bottom = top + i64::from(self.h);

        px >= left && px < right && py >= top && py < bottom
    }

    /// Convert a screen point to local coordinates relative to this rect.
    /// If the point is to the left/top of the rect, the result clamps to 0.
    pub fn to_local_point(&self, p: super::Point) -> super::Point {
        let px = i64::from(p.x);
        let py = i64::from(p.y);
        let left = i64::from(self.tl.x);
        let top = i64::from(self.tl.y);
        super::Point {
            x: u32::try_from((px - left).clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
            y: u32::try_from((py - top).clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
        }
    }

    /// Intersect this signed rect with an unsigned rect in the same coordinate space.
    pub fn intersect_rect(&self, other: Rect) -> Option<Rect> {
        if self.is_zero() || other.is_zero() {
            return None;
        }
        let left = i64::from(self.tl.x);
        let top = i64::from(self.tl.y);
        let right = left + i64::from(self.w);
        let bottom = top + i64::from(self.h);

        let other_left = i64::from(other.tl.x);
        let other_top = i64::from(other.tl.y);
        let other_right = other_left + i64::from(other.w);
        let other_bottom = other_top + i64::from(other.h);

        let inter_left = left.max(other_left);
        let inter_top = top.max(other_top);
        let inter_right = right.min(other_right);
        let inter_bottom = bottom.min(other_bottom);

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

    /// Center point of the rect.
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

impl From<Rect> for RectI32 {
    fn from(r: Rect) -> Self {
        Self {
            tl: PointI32 {
                x: i32::try_from(r.tl.x).unwrap_or(i32::MAX),
                y: i32::try_from(r.tl.y).unwrap_or(i32::MAX),
            },
            w: r.w,
            h: r.h,
        }
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
    fn unsigned_origins_clamp_when_converted() {
        let converted = RectI32::from(Rect::new(u32::MAX, u32::MAX, 1, 1));
        assert_eq!(converted.tl, PointI32::new(i32::MAX, i32::MAX));
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
                prop_assert!(!intersection.is_zero());
                prop_assert!(other.contains_rect(&intersection));
                prop_assert!(signed.contains_point(intersection.tl));
            }
        }
    }
}
