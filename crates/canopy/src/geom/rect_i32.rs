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

    /// Check if the rectangle contains a point.
    pub fn contains_point(&self, p: super::Point) -> bool {
        let px = p.x as i64;
        let py = p.y as i64;
        let left = self.tl.x as i64;
        let top = self.tl.y as i64;
        let right = left + self.w as i64;
        let bottom = top + self.h as i64;

        px >= left && px < right && py >= top && py < bottom
    }

    /// Convert a screen point to local coordinates relative to this rect.
    /// If the point is to the left/top of the rect, the result clamps to 0.
    pub fn to_local_point(&self, p: super::Point) -> super::Point {
        let px = p.x as i64;
        let py = p.y as i64;
        let left = self.tl.x as i64;
        let top = self.tl.y as i64;
        super::Point {
            x: (px - left).max(0) as u32,
            y: (py - top).max(0) as u32,
        }
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

    /// Left edge of the rect.
    pub fn left(&self) -> i64 {
        self.tl.x as i64
    }

    /// Top edge of the rect.
    pub fn top(&self) -> i64 {
        self.tl.y as i64
    }

    /// Right edge of the rect.
    pub fn right(&self) -> i64 {
        self.tl.x as i64 + self.w as i64
    }

    /// Bottom edge of the rect.
    pub fn bottom(&self) -> i64 {
        self.tl.y as i64 + self.h as i64
    }

    /// Center point of the rect.
    pub fn center(&self) -> (i64, i64) {
        (
            self.left() + self.w as i64 / 2,
            self.top() + self.h as i64 / 2,
        )
    }

    /// Return true if this rect overlaps another vertically.
    pub fn overlaps_vertical(&self, other: RectI32) -> bool {
        self.top() < other.bottom() && self.bottom() > other.top()
    }

    /// Return true if this rect overlaps another horizontally.
    pub fn overlaps_horizontal(&self, other: RectI32) -> bool {
        self.left() < other.right() && self.right() > other.left()
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
