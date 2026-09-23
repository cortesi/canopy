//! Frames for a busy indicator.

use std::time::Duration;

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

    /// Return whether `glyph` is one of this spinner's frames.
    #[must_use]
    pub fn contains(&self, glyph: char) -> bool {
        self.frames.contains(&glyph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_turn_with_time_and_wrap() {
        assert_eq!(Spinner::LINE.frame(Duration::ZERO), '|');
        assert_eq!(Spinner::LINE.frame(Duration::from_millis(150)), '/');
        assert_eq!(Spinner::LINE.frame(Duration::from_millis(400)), '|');
        assert_eq!(Spinner::DOTS.step(10), Spinner::DOTS.step(0));
    }
}
