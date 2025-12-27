use super::{PointI32, Rect};

/// A rectangle with a signed origin and unsigned size.
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

    /// Intersect this signed rect with an unsigned rect in the same coordinate space.
    pub fn intersect_rect(&self, other: Rect) -> Option<Rect> {
        let left = self.tl.x;
        let top = self.tl.y;
        let right =
            (self.tl.x as i64 + self.w as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let bottom =
            (self.tl.y as i64 + self.h as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;

        let other_left = other.tl.x as i32;
        let other_top = other.tl.y as i32;
        let other_right =
            (other_left as i64 + other.w as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let other_bottom =
            (other_top as i64 + other.h as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;

        let inter_left = left.max(other_left);
        let inter_top = top.max(other_top);
        let inter_right = right.min(other_right);
        let inter_bottom = bottom.min(other_bottom);

        if inter_right <= inter_left || inter_bottom <= inter_top {
            return None;
        }

        Some(Rect::new(
            inter_left as u32,
            inter_top as u32,
            (inter_right - inter_left) as u32,
            (inter_bottom - inter_top) as u32,
        ))
    }
}

impl From<Rect> for RectI32 {
    fn from(r: Rect) -> Self {
        Self {
            tl: PointI32 {
                x: r.tl.x as i32,
                y: r.tl.y as i32,
            },
            w: r.w,
            h: r.h,
        }
    }
}
