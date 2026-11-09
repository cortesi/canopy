use std::time::{Duration, Instant};

use canopy::geom::Point;

/// Default delay between clicks that still count as the same multi-click
/// sequence, shared by every [`ClickTracker`] user.
pub const DEFAULT_THRESHOLD: Duration = Duration::from_millis(500);

/// Last click in a multi-click sequence.
#[derive(Clone, Debug)]
struct ClickRecord {
    /// Location of the last click.
    location: Point,
    /// Time of the last click.
    last_click: Instant,
    /// Saturated click count in this sequence.
    count: u8,
}

/// Counts same-location clicks that arrive within a threshold, up to 3.
#[derive(Clone, Debug)]
pub struct ClickTracker {
    /// Maximum delay between clicks that still increment the count.
    threshold: Duration,
    /// Previous click in the sequence, if any.
    last: Option<ClickRecord>,
}

impl ClickTracker {
    /// Construct a tracker with the provided inter-click threshold.
    pub(crate) fn new(threshold: Duration) -> Self {
        Self {
            threshold,
            last: None,
        }
    }

    /// Return the click count for a click at `location` at `now`, which the
    /// caller reads from the driver clock, saturating at 3.
    pub(crate) fn count(&mut self, location: Point, now: Instant) -> u8 {
        let count = match &self.last {
            Some(last)
                if last.location == location
                    && now.saturating_duration_since(last.last_click) <= self.threshold =>
            {
                (last.count + 1).min(3)
            }
            _ => 1,
        };
        self.last = Some(ClickRecord {
            location,
            last_click: now,
            count,
        });
        count
    }
}

impl Default for ClickTracker {
    /// Construct a tracker using [`DEFAULT_THRESHOLD`].
    fn default() -> Self {
        Self::new(DEFAULT_THRESHOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A location for the clicks of a test.
    const HERE: Point = Point { x: 3, y: 1 };

    #[test]
    fn clicks_within_the_threshold_count_up_to_three() {
        let start = Instant::now();
        let mut tracker = ClickTracker::default();
        let step = DEFAULT_THRESHOLD / 2;
        assert_eq!(tracker.count(HERE, start), 1);
        assert_eq!(tracker.count(HERE, start + step), 2, "a double click");
        assert_eq!(tracker.count(HERE, start + step * 2), 3, "a triple click");
        assert_eq!(
            tracker.count(HERE, start + step * 3),
            3,
            "the count saturates"
        );
    }

    #[test]
    fn a_click_after_the_threshold_starts_again() {
        let start = Instant::now();
        let mut tracker = ClickTracker::default();
        assert_eq!(tracker.count(HERE, start), 1);
        assert_eq!(
            tracker.count(HERE, start + DEFAULT_THRESHOLD),
            2,
            "the threshold counts"
        );
        let late = start + DEFAULT_THRESHOLD * 2 + Duration::from_millis(1);
        assert_eq!(tracker.count(HERE, late), 1);
    }

    #[test]
    fn a_click_at_another_location_starts_again() {
        let start = Instant::now();
        let mut tracker = ClickTracker::default();
        assert_eq!(tracker.count(HERE, start), 1);
        assert_eq!(tracker.count(Point { x: 4, y: 1 }, start), 1);
        assert_eq!(tracker.count(Point { x: 4, y: 1 }, start), 2);
    }
}
