use crate::{Error, Result};

/// An exctent is a directionless one-dimensional line segment.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct LineSegment {
    /// The offset of this extent.
    pub off: u32,
    /// The length of this extent.
    pub len: u32,
}

impl LineSegment {
    /// The far limit of the extent.
    pub fn far(&self) -> u32 {
        self.off + self.len
    }

    /// Return a line segment that encloses this line segment and another. If
    /// the lines overlap or abut, this is equivalent to joining the segments.
    pub fn enclose(&self, other: &Self) -> Self {
        let off = self.off.min(other.off);
        Self {
            off,
            len: self.far().max(other.far()) - off,
        }
    }

    /// Carve off a fixed-size portion from the start of this LineSegment,
    /// returning a (head, tail) tuple. If the segment is too short to carve out
    /// the width specified, the length of the head will be zero.
    pub fn carve_start(&self, n: u32) -> (Self, Self) {
        if self.len < n {
            (
                Self {
                    off: self.off,
                    len: 0,
                },
                *self,
            )
        } else {
            (
                Self {
                    off: self.off,
                    len: n,
                },
                Self {
                    off: self.off + n,
                    len: self.len - n,
                },
            )
        }
    }

    /// Carve off a fixed-size portion from the end of this LineSegment,
    /// returning a (head, tail) tuple. If the segment is too short to carve out
    /// the width specified, the length of the tail will be zero.
    pub fn carve_end(&self, n: u32) -> (Self, Self) {
        if self.len < n {
            (
                *self,
                Self {
                    off: self.far(),
                    len: 0,
                },
            )
        } else {
            let s = Self {
                off: self.off,
                len: self.len - n,
            };
            (
                s,
                Self {
                    off: s.far(),
                    len: n,
                },
            )
        }
    }

    /// Are these two line segments adjacent but non-overlapping?
    pub fn abuts(&self, other: &Self) -> bool {
        self.far() == other.off || other.far() == self.off
    }

    /// Does other lie completely within this extent.
    pub fn contains(&self, other: &Self) -> bool {
        self.off <= other.off && self.far() >= other.far()
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.intersection(other).is_some()
    }

    /// Return the intersection between this line segment and other. The line
    /// segment returned will always have a non-zero length.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        if self.len == 0 || other.len == 0 {
            None
        } else if self.contains(other) {
            Some(*other)
        } else if other.contains(self) {
            Some(*self)
        } else if self.off <= other.off && other.off < self.far() {
            Some(Self {
                off: other.off,
                len: self.far() - other.off,
            })
        } else if other.off <= self.off && self.off < other.far() {
            Some(Self {
                off: self.off,
                len: other.far() - self.off,
            })
        } else {
            None
        }
    }

    /// Split this extent into (pre, active, post) extents, based on the
    /// position of a window within a view. The main use for this funtion is
    /// computation of the active indicator size and position in a scrollbar.
    pub fn split_active(&self, window: Self, view: Self) -> Result<(Self, Self, Self)> {
        if window.len == 0 {
            Err(Error::Geometry("window cannot be zero length".into()))
        } else if !view.contains(&window) {
            Err(Error::Geometry(format!(
                "view {view:?} does not contain window {window:?}",
            )))
        } else {
            // Compute the fraction each section occupies of the view.
            let pref = (window.off - view.off) as f64 / view.len as f64;
            let postf = (view.far() - window.far()) as f64 / view.len as f64;
            let lenf = self.len as f64;

            // Now compute the true true length in terms of the space. It's
            // important for the active portion to remain the same length
            // regardless of position in the face of rounding, so we compute it
            // first, then compute the other values in terms of it.
            let active = (lenf - (pref * lenf) - (postf * lenf)).ceil();
            let pre = (pref * self.len as f64).floor();
            let post = lenf - active - pre;

            Ok((
                Self {
                    off: self.off,
                    len: pre as u32,
                },
                Self {
                    off: self.off + pre as u32,
                    len: active as u32,
                },
                Self {
                    off: self.off + pre as u32 + active as u32,
                    len: post as u32,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn far() -> Result<()> {
        let s = LineSegment { off: 5, len: 5 };
        assert_eq!(s.far(), 10);
        Ok(())
    }

    #[test]
    fn carve() -> Result<()> {
        let s = LineSegment { off: 5, len: 5 };
        assert_eq!(
            s.carve_start(2),
            (
                LineSegment { off: 5, len: 2 },
                LineSegment { off: 7, len: 3 }
            )
        );
        assert_eq!(
            s.carve_start(10),
            (
                LineSegment { off: 5, len: 0 },
                LineSegment { off: 5, len: 5 }
            )
        );

        assert_eq!(
            s.carve_end(2),
            (
                LineSegment { off: 5, len: 3 },
                LineSegment { off: 8, len: 2 }
            )
        );
        assert_eq!(
            s.carve_end(10),
            (
                LineSegment { off: 5, len: 5 },
                LineSegment { off: 10, len: 0 }
            )
        );

        Ok(())
    }

    #[test]
    fn intersect() -> Result<()> {
        let l = LineSegment { off: 5, len: 5 };

        assert_eq!(
            l.intersection(&LineSegment { off: 6, len: 2 }),
            Some(LineSegment { off: 6, len: 2 })
        );
        assert_eq!(l.intersection(&LineSegment { off: 1, len: 10 }), Some(l));
        assert_eq!(
            l.intersection(&LineSegment { off: 6, len: 8 }),
            Some(LineSegment { off: 6, len: 4 })
        );
        assert_eq!(
            l.intersection(&LineSegment { off: 0, len: 8 }),
            Some(LineSegment { off: 5, len: 3 })
        );
        assert_eq!(l.intersection(&l), Some(l));
        assert_eq!(l.intersection(&LineSegment { off: 0, len: 2 }), None);
        assert_eq!(l.intersection(&LineSegment { off: 10, len: 2 }), None);
        assert_eq!(l.intersection(&LineSegment { off: 5, len: 0 }), None);
        assert_eq!(l.intersection(&LineSegment { off: 0, len: 5 }), None);
        Ok(())
    }

    #[test]
    fn contains() -> Result<()> {
        let v = LineSegment { off: 1, len: 3 };
        assert!(v.contains(&LineSegment { off: 1, len: 3 }));
        assert!(!v.contains(&LineSegment { off: 1, len: 4 }));
        assert!(!v.contains(&LineSegment { off: 2, len: 3 }));
        assert!(!v.contains(&LineSegment { off: 0, len: 2 }));

        Ok(())
    }

    #[test]
    fn abuts() -> Result<()> {
        let v = LineSegment { off: 1, len: 3 };
        assert!(!v.abuts(&LineSegment { off: 1, len: 3 }));
        assert!(v.abuts(&LineSegment { off: 0, len: 1 }));
        assert!(v.abuts(&LineSegment { off: 4, len: 4 }));
        assert!(!v.abuts(&LineSegment { off: 3, len: 4 }));
        Ok(())
    }

    fn check_enclosure(a: LineSegment, b: LineSegment, enclosure: LineSegment) {
        assert_eq!(a.enclose(&b), enclosure);
        assert_eq!(b.enclose(&a), enclosure);
    }

    #[test]
    fn enclose() -> Result<()> {
        check_enclosure(
            LineSegment { off: 1, len: 3 },
            LineSegment { off: 1, len: 3 },
            LineSegment { off: 1, len: 3 },
        );
        check_enclosure(
            LineSegment { off: 1, len: 3 },
            LineSegment { off: 0, len: 3 },
            LineSegment { off: 0, len: 4 },
        );
        check_enclosure(
            LineSegment { off: 1, len: 3 },
            LineSegment { off: 4, len: 3 },
            LineSegment { off: 1, len: 6 },
        );
        check_enclosure(
            LineSegment { off: 1, len: 3 },
            LineSegment { off: 5, len: 3 },
            LineSegment { off: 1, len: 7 },
        );
        Ok(())
    }

    #[test]
    fn split_active() -> Result<()> {
        let v = LineSegment { off: 10, len: 10 };
        assert_eq!(
            v.split_active(
                LineSegment { off: 100, len: 50 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 0 },
                LineSegment { off: 10, len: 5 },
                LineSegment { off: 15, len: 5 },
            )
        );
        assert_eq!(
            v.split_active(
                LineSegment { off: 150, len: 50 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 5 },
                LineSegment { off: 15, len: 5 },
                LineSegment { off: 20, len: 0 },
            )
        );
        assert_eq!(
            v.split_active(
                LineSegment { off: 130, len: 40 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 3 },
                LineSegment { off: 13, len: 4 },
                LineSegment { off: 17, len: 3 },
            )
        );
        assert_eq!(
            v.split_active(
                LineSegment { off: 100, len: 100 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 0 },
                LineSegment { off: 10, len: 10 },
                LineSegment { off: 20, len: 0 },
            )
        );
        Ok(())
    }
}
