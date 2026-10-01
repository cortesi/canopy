//! Colors that change over time.
//!
//! An [`Animation`] is to time what a gradient is to space: colors at stops
//! over one run. Canopy renders every animation at rest, and evaluates it only
//! when it emits cells to the terminal.

use std::{
    fmt,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use super::{Color, GradientStop, Mix};

/// What follows one run of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Repeat {
    /// One run, which then holds its last color.
    Once,
    /// Runs one after another, each from the first stop.
    Loop,
    /// Runs forward, then backward, then forward again.
    Alternate,
}

/// How time within a run maps to the stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Easing {
    /// Colors change at a constant rate.
    Linear,
    /// Colors change slowly at the ends of a run and quickly in the middle.
    InOut,
    /// Each stop holds until the next one, with no mixing.
    Hold,
}

/// When the first run of an animation starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationStart {
    /// The shared motion epoch, which keeps loops in step with each other.
    Epoch,
    /// A fixed time on the driver clock.
    At(Instant),
    /// The last input event, so the motion restarts whenever the operator acts.
    LastInput,
    /// The first published frame that shows the animation. Clones share the
    /// bound time, so a paint that renders again keeps its start.
    Shown,
}

/// When repeating motion holds at rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Pause {
    /// While the operator is idle or the terminal lacks focus. Decoration
    /// pauses so, and costs nothing while nobody watches it.
    #[default]
    Idle,
    /// Never. A busy indicator moves for as long as it shows, so that the
    /// operator sees work run while they wait.
    Never,
}

impl Pause {
    /// Return whether repeating motion holds at rest at the clocks' time.
    pub(crate) fn holds(self, clocks: &MotionClocks) -> bool {
        self == Self::Idle && clocks.paused
    }
}

/// The time that binds an [`AnimationStart::Shown`] animation, shared by its
/// clones.
#[derive(Clone, Default)]
pub(crate) struct ShownAt(Arc<OnceLock<Instant>>);

impl ShownAt {
    /// Return the bound time.
    fn get(&self) -> Option<Instant> {
        self.0.get().copied()
    }

    /// Bind the time, unless it is bound already.
    fn bind(&self, now: Instant) {
        self.0.get_or_init(|| now);
    }
}

impl fmt::Debug for ShownAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ShownAt").field(&self.get()).finish()
    }
}

/// The bound time is runtime state, not part of an animation's value.
impl PartialEq for ShownAt {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// The clocks that motion evaluates against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MotionClocks {
    /// Current driver time.
    pub now: Instant,
    /// Shared start of [`AnimationStart::Epoch`] animations.
    pub epoch: Instant,
    /// Time of the last input event.
    pub last_input: Instant,
    /// Start of the primary cursor's motion: its last change, or the last
    /// input event if that is later.
    pub cursor: Instant,
    /// Whether repeating motion holds at rest, while idle or unfocused.
    pub paused: bool,
    /// Whether the terminal has focus.
    pub focused: bool,
}

impl MotionClocks {
    /// Clocks at `now` with every start at `now`, unpaused.
    #[cfg(test)]
    pub(crate) fn at(now: Instant) -> Self {
        Self {
            now,
            epoch: now,
            last_input: now,
            cursor: now,
            paused: false,
            focused: true,
        }
    }
}

/// Colors over time: stops over one run, a run length, and a repeat.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// Colors over one run, as gradient stops are colors over space.
    pub stops: Vec<GradientStop>,
    /// Length of one run.
    pub duration: Duration,
    /// What follows one run.
    pub repeat: Repeat,
    /// How time maps to the stops.
    pub easing: Easing,
    /// When the first run starts.
    pub start: AnimationStart,
    /// The space that mixes colors between stops.
    pub mix: Mix,
    /// When a repeating animation holds at rest.
    pub pause: Pause,
    /// Bound start of a [`AnimationStart::Shown`] animation.
    shown: ShownAt,
}

impl Animation {
    /// Construct a linear loop over `stops`, in step with the motion epoch.
    pub fn new(mut stops: Vec<GradientStop>, duration: Duration) -> Self {
        if stops.is_empty() {
            stops.push(GradientStop::new(0.0, Color::White));
        }
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Self {
            stops,
            duration,
            repeat: Repeat::Loop,
            easing: Easing::Linear,
            start: AnimationStart::Epoch,
            mix: Mix::Oklab,
            pause: Pause::Idle,
            shown: ShownAt::default(),
        }
    }

    /// Fade once from `from` to `to`, from the first frame that shows it.
    pub fn fade(from: Color, to: Color, duration: Duration) -> Self {
        Self::new(
            vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            duration,
        )
        .with_repeat(Repeat::Once)
        .with_easing(Easing::InOut)
        .with_start(AnimationStart::Shown)
    }

    /// Fade from `from` to `to` and back, once each period.
    pub fn pulse(from: Color, to: Color, period: Duration) -> Self {
        Self::new(
            vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            period / 2,
        )
        .with_repeat(Repeat::Alternate)
        .with_easing(Easing::InOut)
    }

    /// Show `on` for `on_time`, then `off` for `off_time`, in a loop.
    pub fn blink(on: Color, off: Color, on_time: Duration, off_time: Duration) -> Self {
        let duration = on_time + off_time;
        let split = if duration.is_zero() {
            1.0
        } else {
            on_time.as_secs_f32() / duration.as_secs_f32()
        };
        Self::new(
            vec![GradientStop::new(0.0, on), GradientStop::new(split, off)],
            duration,
        )
        .with_easing(Easing::Hold)
    }

    /// Replace the repeat.
    #[must_use]
    pub fn with_repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }

    /// Replace the easing.
    #[must_use]
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Replace the start.
    #[must_use]
    pub fn with_start(mut self, start: AnimationStart) -> Self {
        self.start = start;
        self
    }

    /// Replace the mixing space.
    #[must_use]
    pub fn with_mix(mut self, mix: Mix) -> Self {
        self.mix = mix;
        self
    }

    /// Replace the pause.
    #[must_use]
    pub fn with_pause(mut self, pause: Pause) -> Self {
        self.pause = pause;
        self
    }

    /// Return the color that frames show at rest: the last stop of a single
    /// run, and the first stop of a repeating one.
    pub fn rest(&self) -> Color {
        match self.repeat {
            Repeat::Once => self.color_at(self.duration.as_nanos()),
            Repeat::Loop | Repeat::Alternate => self.color_at(0),
        }
    }

    /// Map every stop color, keeping the timing and the bound start.
    #[must_use]
    pub fn map_colors(&self, f: impl Fn(Color) -> Color) -> Self {
        Self {
            stops: self
                .stops
                .iter()
                .map(|stop| GradientStop::new(stop.offset, f(stop.color)))
                .collect(),
            ..self.clone()
        }
    }

    /// Share the bound start of another animation.
    pub(crate) fn share_start(mut self, shown: &ShownAt) -> Self {
        self.shown = shown.clone();
        self
    }

    /// Bind a [`AnimationStart::Shown`] start to `now` if it is unbound.
    pub(crate) fn bind_shown(&self, now: Instant) {
        if self.start == AnimationStart::Shown {
            self.shown.bind(now);
        }
    }

    /// Return whether the animation repeats.
    fn repeats(&self) -> bool {
        self.repeat != Repeat::Once
    }

    /// Return the start of the first run.
    fn start_time(&self, clocks: &MotionClocks) -> Instant {
        match self.start {
            AnimationStart::Epoch => clocks.epoch,
            AnimationStart::At(at) => at,
            AnimationStart::LastInput => clocks.last_input,
            AnimationStart::Shown => self.shown.get().unwrap_or(clocks.now),
        }
    }

    /// Return whether a single run has ended by `now`.
    pub(crate) fn finished(&self, clocks: &MotionClocks) -> bool {
        !self.repeats()
            && clocks
                .now
                .saturating_duration_since(self.start_time(clocks))
                >= self.duration
    }

    /// Return the color at the clocks' time.
    pub(crate) fn color(&self, clocks: &MotionClocks) -> Color {
        if self.repeats() && self.pause.holds(clocks) {
            return self.rest();
        }
        let elapsed = clocks
            .now
            .saturating_duration_since(self.start_time(clocks));
        self.color_at(self.run_position(elapsed.as_nanos()))
    }

    /// Return the position within a run, in nanoseconds from its start, after
    /// `elapsed` nanoseconds.
    fn run_position(&self, elapsed: u128) -> u128 {
        let duration = self.duration.as_nanos();
        if duration == 0 {
            return 0;
        }
        match self.repeat {
            Repeat::Once => elapsed.min(duration),
            Repeat::Loop => elapsed % duration,
            Repeat::Alternate => {
                let local = elapsed % (2 * duration);
                if local <= duration {
                    local
                } else {
                    2 * duration - local
                }
            }
        }
    }

    /// Return the position of a stop within a run, in nanoseconds, rounded to
    /// whole microseconds so that an `f32` offset lands on its intended edge.
    fn edge(&self, offset: f32) -> u128 {
        let micros = f64::from(offset) * self.duration.as_nanos() as f64 / 1000.0;
        micros.round() as u128 * 1000
    }

    /// Return the color at a position within a run, in nanoseconds.
    fn color_at(&self, run: u128) -> Color {
        let first = &self.stops[0];
        if self.easing == Easing::Hold {
            return self
                .stops
                .iter()
                .rev()
                .find(|stop| self.edge(stop.offset) <= run)
                .unwrap_or(first)
                .color;
        }
        let duration = self.duration.as_nanos();
        let position = if duration == 0 {
            1.0
        } else {
            (run as f64 / duration as f64) as f32
        };
        let position = match self.easing {
            Easing::InOut => ease_in_out(position),
            Easing::Linear | Easing::Hold => position,
        };
        let mut prev = first;
        if position <= prev.offset {
            return prev.color;
        }
        for stop in &self.stops[1..] {
            if position <= stop.offset {
                let span = (stop.offset - prev.offset).max(f32::EPSILON);
                return prev
                    .color
                    .mix(stop.color, (position - prev.offset) / span, self.mix);
            }
            prev = stop;
        }
        prev.color
    }

    /// Return the next time the color can change after the clocks' time.
    ///
    /// A held animation changes at its stop edges. A continuous one changes
    /// at every sample, `sample` apart, until a single run ends.
    pub(crate) fn next_change(&self, clocks: &MotionClocks, sample: Duration) -> Option<Instant> {
        if self.repeats() && self.pause.holds(clocks) {
            return None;
        }
        let start = self.start_time(clocks);
        if clocks.now < start {
            return Some(start);
        }
        if self.finished(clocks) || self.duration.is_zero() {
            return None;
        }
        let elapsed = clocks.now.duration_since(start).as_nanos();
        if self.easing != Easing::Hold {
            let next = clocks.now + sample;
            return Some(match self.repeat {
                Repeat::Once => next.min(start + self.duration),
                Repeat::Loop | Repeat::Alternate => next,
            });
        }
        let duration = self.duration.as_nanos();
        let offsets = self
            .stops
            .iter()
            .map(|stop| self.edge(stop.offset))
            .filter(|&at| at > 0 && at <= duration);
        let (cycle, mut edges): (u128, Vec<u128>) = match self.repeat {
            Repeat::Once => (duration, offsets.collect()),
            // Each run returns to the first stop.
            Repeat::Loop => (
                duration,
                offsets
                    .filter(|&at| at < duration)
                    .chain([duration])
                    .collect(),
            ),
            // The backward run leaves each stop just after it crosses it.
            Repeat::Alternate => (
                2 * duration,
                offsets.flat_map(|at| [at, 2 * duration - at + 1]).collect(),
            ),
        };
        edges.sort_unstable();
        let run = elapsed / cycle;
        let local = elapsed % cycle;
        let next = match edges.iter().find(|&&edge| edge > local) {
            Some(edge) => run * cycle + edge,
            None if self.repeat == Repeat::Once => return None,
            None => (run + 1) * cycle + edges.first().copied()?,
        };
        Some(start + nanos(next))
    }
}

/// Convert nanoseconds to a duration, saturating.
fn nanos(n: u128) -> Duration {
    Duration::from_nanos(u64::try_from(n).unwrap_or(u64::MAX))
}

/// Ease a position with a cubic in-out curve.
pub(crate) fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn clocks(start: Instant, ms: u64) -> MotionClocks {
        MotionClocks {
            now: start + MS(ms),
            ..MotionClocks::at(start)
        }
    }

    #[test]
    fn a_blink_holds_each_color_until_its_edge() {
        let t0 = Instant::now();
        let blink = Animation::blink(Color::White, Color::Black, MS(600), MS(400));
        assert_eq!(blink.rest(), Color::White);
        assert_eq!(blink.color(&clocks(t0, 0)), Color::White);
        assert_eq!(blink.color(&clocks(t0, 599)), Color::White);
        assert_eq!(blink.color(&clocks(t0, 600)), Color::Black);
        assert_eq!(blink.color(&clocks(t0, 999)), Color::Black);
        assert_eq!(blink.color(&clocks(t0, 1000)), Color::White);
        let sample = MS(33);
        assert_eq!(
            blink.next_change(&clocks(t0, 0), sample),
            Some(t0 + MS(600))
        );
        assert_eq!(
            blink.next_change(&clocks(t0, 600), sample),
            Some(t0 + MS(1000))
        );
        assert_eq!(
            blink.next_change(&clocks(t0, 1000), sample),
            Some(t0 + MS(1600))
        );
    }

    #[test]
    fn a_pulse_alternates_and_samples_continuously() {
        let t0 = Instant::now();
        let pulse = Animation::pulse(Color::White, Color::Black, MS(1000));
        assert_eq!(pulse.rest(), Color::White);
        assert_eq!(pulse.color(&clocks(t0, 0)).rgb(), (255, 255, 255));
        assert_eq!(pulse.color(&clocks(t0, 500)).rgb(), (0, 0, 0));
        assert_eq!(pulse.color(&clocks(t0, 1000)).rgb(), (255, 255, 255));
        let (quarter, _, _) = pulse.color(&clocks(t0, 250)).rgb();
        assert!((80..160).contains(&quarter), "midway shade {quarter}");
        assert_eq!(
            pulse.next_change(&clocks(t0, 250), MS(33)),
            Some(t0 + MS(283))
        );
    }

    #[test]
    fn a_single_run_ends_at_its_last_stop() {
        let t0 = Instant::now();
        let fade = Animation::fade(Color::White, Color::Black, MS(100));
        assert_eq!(fade.rest(), Color::Rgb { r: 0, g: 0, b: 0 });
        fade.bind_shown(t0);
        fade.bind_shown(t0 + MS(50));
        let late = clocks(t0, 90);
        assert_eq!(fade.next_change(&late, MS(33)), Some(t0 + MS(100)));
        let done = clocks(t0, 100);
        assert!(fade.finished(&done));
        assert_eq!(fade.color(&done), Color::Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(fade.next_change(&done, MS(33)), None);
    }

    #[test]
    fn clones_and_mapped_copies_share_the_shown_start() {
        let t0 = Instant::now();
        let fade = Animation::fade(Color::White, Color::Black, MS(100));
        let dimmed = fade.map_colors(|c| c.scale_brightness(0.5));
        fade.bind_shown(t0);
        dimmed.bind_shown(t0 + MS(60));
        assert!(dimmed.finished(&clocks(t0, 100)));
    }

    #[test]
    fn paused_loops_hold_at_rest() {
        let t0 = Instant::now();
        let blink = Animation::blink(Color::White, Color::Black, MS(600), MS(400));
        let paused = MotionClocks {
            paused: true,
            ..clocks(t0, 700)
        };
        assert_eq!(blink.color(&paused), Color::White);
        assert_eq!(blink.next_change(&paused, MS(33)), None);
    }

    #[test]
    fn a_busy_loop_moves_while_paused() {
        let t0 = Instant::now();
        let blink =
            Animation::blink(Color::White, Color::Black, MS(600), MS(400)).with_pause(Pause::Never);
        let paused = MotionClocks {
            paused: true,
            ..clocks(t0, 700)
        };
        assert_eq!(blink.color(&paused), Color::Black);
        assert_eq!(blink.next_change(&paused, MS(33)), Some(t0 + MS(1000)));
    }

    #[test]
    fn an_alternating_hold_mirrors_its_edges() {
        let t0 = Instant::now();
        let held = Animation::blink(Color::White, Color::Black, MS(250), MS(750))
            .with_repeat(Repeat::Alternate);
        let sample = MS(33);
        assert_eq!(held.next_change(&clocks(t0, 0), sample), Some(t0 + MS(250)));
        let back = t0 + MS(1750) + Duration::from_nanos(1);
        assert_eq!(held.next_change(&clocks(t0, 250), sample), Some(back));
        assert_eq!(held.color(&clocks(t0, 1750)), Color::Black);
        let after = MotionClocks {
            now: back,
            ..MotionClocks::at(t0)
        };
        assert_eq!(held.color(&after), Color::White);
        assert_eq!(held.next_change(&after, sample), Some(t0 + MS(2250)));
    }
}
