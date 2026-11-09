//! Frames for a busy indicator.

use std::time::{Duration, Instant};

/// A sequence of glyphs that turns while work runs.
///
/// A spinner holds no state: a widget that is busy picks the frame for how
/// long it has waited, or for a step it counts itself, and repaints on its
/// own schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spinner {
    /// Glyphs in rotation order.
    frames: &'static [char],
    /// How long each frame shows.
    period: Duration,
}

impl Spinner {
    /// Braille dots, for a spinner inline with text.
    pub const DOTS: Self = Self {
        frames: &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
        period: Duration::from_millis(80),
    };

    /// An ASCII line, for any terminal font.
    pub const LINE: Self = Self {
        frames: &['|', '/', '-', '\\'],
        period: Duration::from_millis(100),
    };

    /// Return the frame for `step`, counting from zero.
    #[must_use]
    pub fn step(&self, step: usize) -> char {
        self.frames[step % self.frames.len()]
    }

    /// Return the frame to show after `elapsed`.
    #[must_use]
    pub fn frame(&self, elapsed: Duration) -> char {
        let steps = elapsed.as_millis() / self.period.as_millis().max(1);
        self.step(usize::try_from(steps).unwrap_or(0))
    }

    /// Return how long each frame shows, which is how often a busy widget
    /// repaints.
    #[must_use]
    pub fn period(&self) -> Duration {
        self.period
    }
}

/// Whether the work behind a spinner runs, and when the spinner started.
///
/// A widget that turns its own spinner sets the run from its owner, wakes
/// its poll when a run starts, and dates the run at the first poll, on the
/// driver clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpinnerRun {
    /// The work does not run.
    #[default]
    Idle,
    /// The work runs.
    Running {
        /// When the spinner started to turn, after the first poll of a run.
        since: Option<Instant>,
    },
}

impl SpinnerRun {
    /// Record whether the work runs, and return whether a run started.
    pub fn set(&mut self, running: bool) -> bool {
        let started = running && *self == Self::Idle;
        match (running, *self) {
            (false, _) => *self = Self::Idle,
            (true, Self::Idle) => *self = Self::Running { since: None },
            (true, Self::Running { .. }) => {}
        }
        started
    }

    /// Start the spinner of a run at `now`, once. Return whether the work
    /// runs.
    pub fn turn(&mut self, now: Instant) -> bool {
        match self {
            Self::Idle => false,
            Self::Running { since } => {
                since.get_or_insert(now);
                true
            }
        }
    }

    /// Return whether the work runs.
    pub fn running(self) -> bool {
        self != Self::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_transitions() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        let turning = SpinnerRun::Running { since: Some(now) };
        let waiting = SpinnerRun::Running { since: None };
        for (run, running, next, started) in [
            (SpinnerRun::Idle, false, SpinnerRun::Idle, false),
            (SpinnerRun::Idle, true, waiting, true),
            (waiting, true, waiting, false),
            (turning, true, turning, false),
            (turning, false, SpinnerRun::Idle, false),
        ] {
            let mut moved = run;
            assert_eq!(moved.set(running), started, "{run:?} set {running}");
            assert_eq!(moved, next, "{run:?} set {running}");
        }
        for (run, next, runs) in [
            (SpinnerRun::Idle, SpinnerRun::Idle, false),
            (waiting, SpinnerRun::Running { since: Some(later) }, true),
            (turning, turning, true),
        ] {
            let mut moved = run;
            assert_eq!(moved.turn(later), runs, "{run:?} turns");
            assert_eq!(moved, next, "{run:?} turns");
        }
    }

    #[test]
    fn frames_turn_with_time_and_wrap() {
        assert_eq!(Spinner::LINE.frame(Duration::ZERO), '|');
        assert_eq!(Spinner::LINE.frame(Duration::from_millis(150)), '/');
        assert_eq!(Spinner::LINE.frame(Duration::from_millis(400)), '|');
        assert_eq!(Spinner::DOTS.step(10), Spinner::DOTS.step(0));
    }
}
