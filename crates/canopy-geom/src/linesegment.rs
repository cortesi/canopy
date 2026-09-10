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
    /// Construct a line segment from its offset and length.
    pub const fn new(off: u32, len: u32) -> Self {
        Self { off, len }
    }

    /// The exclusive far edge of the extent using widened arithmetic.
    pub(crate) fn end(&self) -> u64 {
        u64::from(self.off) + u64::from(self.len)
    }

    /// Does other lie completely within this extent.
    pub fn contains(&self, other: Self) -> bool {
        self.off <= other.off && self.end() >= other.end()
    }

    /// Return the intersection between this line segment and other. The line
    /// segment returned will always have a non-zero length.
    pub(crate) fn intersection(&self, other: Self) -> Option<Self> {
        if self.len == 0 || other.len == 0 {
            None
        } else if self.contains(other) {
            Some(other)
        } else if other.contains(*self) {
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
        } else if !view.contains(window) {
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
            let ab = a.intersection(b);
            let ba = b.intersection(a);
            prop_assert_eq!(ab, ba);
            if let Some(intersection) = ab {
                prop_assert!(a.contains(intersection));
                prop_assert!(b.contains(intersection));
                prop_assert!(intersection.len > 0);
            }
        }

    }

    #[test]
    fn intersect() -> Result<()> {
        let l = LineSegment { off: 5, len: 5 };

        assert_eq!(
            l.intersection(LineSegment { off: 6, len: 2 }),
            Some(LineSegment { off: 6, len: 2 })
        );
        assert_eq!(l.intersection(LineSegment { off: 1, len: 10 }), Some(l));
        assert_eq!(
            l.intersection(LineSegment { off: 6, len: 8 }),
            Some(LineSegment { off: 6, len: 4 })
        );
        assert_eq!(
            l.intersection(LineSegment { off: 0, len: 8 }),
            Some(LineSegment { off: 5, len: 3 })
        );
        assert_eq!(l.intersection(l), Some(l));
        assert_eq!(l.intersection(LineSegment { off: 0, len: 2 }), None);
        assert_eq!(l.intersection(LineSegment { off: 10, len: 2 }), None);
        assert_eq!(l.intersection(LineSegment { off: 5, len: 0 }), None);
        assert_eq!(l.intersection(LineSegment { off: 0, len: 5 }), None);
        Ok(())
    }

    #[test]
    fn contains() -> Result<()> {
        let v = LineSegment { off: 1, len: 3 };
        assert!(v.contains(LineSegment { off: 1, len: 3 }));
        assert!(!v.contains(LineSegment { off: 1, len: 4 }));
        assert!(!v.contains(LineSegment { off: 2, len: 3 }));
        assert!(!v.contains(LineSegment { off: 0, len: 2 }));

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
