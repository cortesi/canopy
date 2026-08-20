use std::time::{Duration, Instant};

use canopy::geom::Point;

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
pub(crate) struct ClickTracker {
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
        if let Some(state) = self.last.as_mut() {
            if state.location == location && now.duration_since(state.last_click) <= self.threshold
            {
                state.count = state.count.saturating_add(1).min(3);
                state.last_click = now;
                return state.count;
            }
            state.location = location;
            state.count = 1;
            state.last_click = now;
            return 1;
        }
        self.last = Some(ClickRecord {
            location,
            last_click: now,
            count: 1,
        });
        1
    }
}
