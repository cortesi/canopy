//! Motion at emission: the clocks, the schedule, and the settings of moving
//! cells.
//!
//! Frames publish at rest. The scheduler evaluates moving cells on the driver
//! clock, and the adapter emits when their colors change, with no layout or
//! render.

use std::time::{Duration, Instant};

use super::Canopy;
use crate::{NodeId, geom::Point, style::MotionClocks};

/// Global motion settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionSettings {
    /// Whether cells move. With `false`, every cell stays at rest.
    pub enabled: bool,
    /// Most samples a second of motion that changes continuously.
    pub max_fps: u32,
    /// Time without input after which repeating motion pauses at rest.
    pub idle_pause: Duration,
}

impl Default for MotionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_fps: 30,
            idle_pause: Duration::from_secs(10),
        }
    }
}

impl MotionSettings {
    /// Return the time between two samples of continuous motion.
    fn sample(&self) -> Duration {
        Duration::from_secs(1) / self.max_fps.max(1)
    }
}

/// The identity of the primary cursor, whose change restarts its motion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PrimaryCursor {
    /// Node that declared it.
    pub node: NodeId,
    /// Screen location.
    pub location: Point,
    /// Role.
    pub role: String,
}

/// Motion state held by the driver.
#[derive(Debug, Default)]
pub(super) struct MotionState {
    /// Global settings.
    pub settings: MotionSettings,
    /// Whether an adapter emits motion. Headless runs stay at rest.
    pub live: bool,
    /// Shared start of epoch animations, set at the first frame.
    epoch: Option<Instant>,
    /// Time of the last input event.
    last_input: Option<Instant>,
    /// Time the primary cursor last changed, or the terminal regained focus.
    cursor_since: Option<Instant>,
    /// Whether the terminal lacks focus.
    unfocused: bool,
    /// Primary cursor of the last published frame.
    primary: Option<PrimaryCursor>,
    /// Next time a moving cell can change.
    next_sample: Option<Instant>,
}

impl MotionState {
    /// Return whether an adapter emits motion.
    pub fn active(&self) -> bool {
        self.live && self.settings.enabled
    }

    /// Return the clocks at `now`.
    pub fn clocks(&mut self, now: Instant) -> MotionClocks {
        let epoch = *self.epoch.get_or_insert(now);
        let last_input = self.last_input.unwrap_or(epoch);
        let cursor = last_input.max(self.cursor_since.unwrap_or(epoch));
        let idle = now.saturating_duration_since(last_input) >= self.settings.idle_pause;
        MotionClocks {
            now,
            epoch,
            last_input,
            cursor,
            paused: self.unfocused || idle,
            focused: !self.unfocused,
        }
    }

    /// Record an input event.
    pub fn input(&mut self, now: Instant) {
        self.last_input = Some(now);
    }

    /// Record a change of terminal focus. Regaining focus restarts the
    /// cursor's motion.
    pub fn focus(&mut self, focused: bool, now: Instant) {
        self.unfocused = !focused;
        if focused {
            self.cursor_since = Some(now);
        }
    }

    /// Record the primary cursor of a published frame, restarting its motion
    /// when it changed.
    pub fn primary(&mut self, primary: Option<PrimaryCursor>, now: Instant) {
        if primary != self.primary {
            self.cursor_since = Some(now);
            self.primary = primary;
        }
    }
}

impl Canopy {
    /// Return the motion settings.
    pub fn motion(&self) -> MotionSettings {
        self.motion.settings
    }

    /// Replace the motion settings.
    pub fn set_motion(&mut self, settings: MotionSettings) {
        self.motion.settings = settings;
        self.motion.next_sample = None;
        self.sync_motion_active();
    }

    /// Emit motion. The terminal adapter enables it. Headless runs and tests
    /// keep every cell at rest unless they enable it.
    pub fn set_motion_live(&mut self, live: bool) {
        self.motion.live = live;
        self.motion.next_sample = None;
        self.sync_motion_active();
    }

    /// Give widgets the motion policy, and repaint when it changes, so a
    /// widget that animates by itself can settle at rest.
    fn sync_motion_active(&mut self) {
        let active = self.motion.active();
        if self.core.motion_active != active {
            self.core.motion_active = active;
            self.core.invalidate(crate::Invalidation::Paint);
        }
    }

    /// Return the clocks of the current driver time.
    pub(super) fn motion_clocks(&mut self) -> MotionClocks {
        let now = self.now();
        self.motion.clocks(now)
    }

    /// Return the next time a moving cell can change, if motion is emitted.
    pub(super) fn motion_deadline(&self) -> Option<Instant> {
        if !self.motion.active() {
            return None;
        }
        self.motion.next_sample
    }

    /// Schedule the next sample after emitting the published frame at `now`.
    pub(super) fn schedule_motion(&mut self, now: Instant) {
        self.motion.next_sample = None;
        if !self.motion.active() {
            return;
        }
        let clocks = self.motion.clocks(now);
        let sample = self.motion.settings.sample();
        let Some(buffer) = self.published_buf().filter(|buf| buf.has_motion()) else {
            return;
        };
        let mut next = buffer.next_motion_change(&clocks, sample);
        // Repeating motion comes to rest when the idle pause begins.
        if !clocks.paused {
            let idle = clocks.last_input + self.motion.settings.idle_pause;
            next = Some(next.map_or(idle, |next| next.min(idle)));
        }
        // A deadline never lands in the past, so the driver cannot spin.
        self.motion.next_sample = next.map(|next| next.max(now + Duration::from_millis(1)));
    }

    /// Return whether a due sample changes the cells emitted last. A sample
    /// that changes nothing schedules the next one.
    pub(super) fn motion_due(&mut self) -> bool {
        if !self.motion.active() {
            return false;
        }
        let now = self.now();
        if self.motion.next_sample.is_none_or(|next| next > now) {
            return false;
        }
        let clocks = self.motion.clocks(now);
        let changed = match (self.published_buf(), self.frame.emitted_buf.as_deref()) {
            (Some(published), Some(emitted)) => published.motion_differs(emitted, &clocks),
            _ => false,
        };
        if !changed {
            self.schedule_motion(now);
        }
        changed
    }
}
