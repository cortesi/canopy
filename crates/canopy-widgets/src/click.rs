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

    /// Return the click count for `location`, saturating at 3.
    pub(crate) fn count(&mut self, location: Point) -> u8 {
        let now = Instant::now();
        let count = match &self.last {
            Some(last)
                if last.location == location
                    && now.duration_since(last.last_click) <= self.threshold =>
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
