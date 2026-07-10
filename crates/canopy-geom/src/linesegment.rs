use super::{Error, Result};

/// A half-open, directionless one-dimensional line segment.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct LineSegment {
    /// The offset of this extent.
    pub off: u32,
    /// The length of this extent.
    pub len: u32,
}

impl LineSegment {
    /// The exclusive far edge of the extent using widened arithmetic.
    pub fn end(&self) -> u64 {
        u64::from(self.off) + u64::from(self.len)
    }

    /// Return a segment starting at the nearer input and extending toward the
    /// farther input.
    ///
    /// The length saturates when the complete span exceeds `u32::MAX`.
    pub fn saturating_enclose(&self, other: &Self) -> Self {
        let off = self.off.min(other.off);
        Self {
            off,
            len: u32::try_from(self.end().max(other.end()) - u64::from(off)).unwrap_or(u32::MAX),
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
                    off: self.off.saturating_add(n),
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
                    off: u32::try_from(self.end()).unwrap_or(u32::MAX),
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
                    off: u32::try_from(s.end()).unwrap_or(u32::MAX),
                    len: n,
                },
            )
        }
    }

    /// Are these two line segments adjacent but non-overlapping?
    pub fn abuts(&self, other: &Self) -> bool {
        self.end() == u64::from(other.off) || other.end() == u64::from(self.off)
    }

    /// Does other lie completely within this extent.
    pub fn contains(&self, other: &Self) -> bool {
        self.off <= other.off && self.end() >= other.end()
    }

    /// Return true if the two segments overlap.
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
        } else if self.off <= other.off && u64::from(other.off) < self.end() {
            Some(Self {
                off: other.off,
                len: u32::try_from(self.end() - u64::from(other.off)).ok()?,
            })
        } else if other.off <= self.off && u64::from(self.off) < other.end() {
            Some(Self {
                off: self.off,
                len: u32::try_from(other.end() - u64::from(self.off)).ok()?,
            })
        } else {
            None
        }
    }

    /// Split this extent into (pre, active, post) extents, based on the
    /// position of a window within a view. The main use for this function is
    /// computation of the active indicator size and position in a scrollbar.
    pub fn split_active(&self, window: Self, view: Self) -> Result<(Self, Self, Self)> {
        if window.len == 0 {
            Err(Error::ZeroLengthWindow)
        } else if !view.contains(&window) {
            Err(Error::WindowOutsideView { window, view })
        } else {
            let track_len = u64::from(self.len);
            let view_len = u64::from(view.len);
            let leading = u64::from(window.off - view.off);
            let pre = track_len * leading / view_len;
            let active_numerator = track_len * u64::from(window.len);
            let active = active_numerator.div_ceil(view_len).min(track_len - pre);
            let post = track_len - pre - active;
            let pre = u32::try_from(pre).unwrap_or(u32::MAX);
            let active = u32::try_from(active).unwrap_or(u32::MAX);
            let post = u32::try_from(post).unwrap_or(u32::MAX);
            let active_off = self.off.saturating_add(pre);
            let post_off = active_off.saturating_add(active);

            Ok((
                Self {
                    off: self.off,
                    len: pre,
                },
                Self {
                    off: active_off,
                    len: active,
                },
                Self {
                    off: post_off,
                    len: post,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn segment_strategy() -> impl Strategy<Value = LineSegment> {
        (0u32..200, 0u32..100).prop_map(|(off, len)| LineSegment { off, len })
    }

    #[test]
    fn end() -> Result<()> {
        let s = LineSegment { off: 5, len: 5 };
        assert_eq!(s.end(), 10);
        Ok(())
    }

    #[test]
    fn end_widens() {
        let s = LineSegment {
            off: u32::MAX - 1,
            len: 10,
        };
        assert_eq!(s.end(), u64::from(u32::MAX) + 9);
    }

    proptest! {
        #[test]
        fn intersection_is_commutative_and_contained(a in segment_strategy(), b in segment_strategy()) {
            let ab = a.intersection(&b);
            let ba = b.intersection(&a);
            prop_assert_eq!(ab, ba);
            if let Some(intersection) = ab {
                prop_assert!(a.contains(&intersection));
                prop_assert!(b.contains(&intersection));
                prop_assert!(intersection.len > 0);
            }
        }

        #[test]
        fn enclose_contains_both_segments(a in segment_strategy(), b in segment_strategy()) {
            let enclosure = a.saturating_enclose(&b);
            prop_assert!(enclosure.contains(&a));
            prop_assert!(enclosure.contains(&b));
        }

        #[test]
        fn carve_start_preserves_original_extent(segment in segment_strategy(), n in 0u32..150) {
            let (head, tail) = segment.carve_start(n);
            prop_assert_eq!(head.off, segment.off);
            prop_assert!(segment.contains(&head));
            prop_assert!(segment.contains(&tail));
            prop_assert_eq!(head.len.saturating_add(tail.len), segment.len);
        }
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
        assert_eq!(a.saturating_enclose(&b), enclosure);
        assert_eq!(b.saturating_enclose(&a), enclosure);
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

    #[test]
    fn split_active_handles_extreme_extents() -> Result<()> {
        let track = LineSegment {
            off: u32::MAX - 1,
            len: u32::MAX,
        };
        let (pre, active, post) = track.split_active(
            LineSegment {
                off: u32::MAX - 1,
                len: 1,
            },
            LineSegment {
                off: 0,
                len: u32::MAX,
            },
        )?;
        assert_eq!(pre.len, u32::MAX - 1);
        assert_eq!(active.len, 1);
        assert_eq!(post.len, 0);
        assert_eq!(
            pre.len.saturating_add(active.len).saturating_add(post.len),
            track.len
        );
        Ok(())
    }
}
