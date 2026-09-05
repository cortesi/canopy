//! Explicit monotonic time for runtime tests.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::{
    core::poll::Clock,
    error::{Error, Result},
};

/// Monotonic clock advanced by the test instead of elapsed wall time.
///
/// Share it through an `Arc` and install it before preparing the application.
/// Advancing time does not run callbacks; deliver `Work::Wake` to service them.
#[derive(Debug)]
pub struct ManualClock {
    /// Current test instant, changed only by explicit advancement.
    now: Mutex<Instant>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualClock {
    /// Start a clock at the current monotonic instant.
    pub fn new() -> Self {
        Self {
            now: Mutex::new(Instant::now()),
        }
    }

    /// Return the explicitly controlled current instant.
    pub fn now(&self) -> Instant {
        *self.now.lock().expect("manual clock lock poisoned")
    }

    /// Advance time, rejecting a duration outside the platform's instant range.
    pub fn advance(&self, duration: Duration) -> Result<()> {
        let mut now = self
            .now
            .lock()
            .map_err(|_| Error::InvalidOperation("manual clock lock poisoned".into()))?;
        *now = now
            .checked_add(duration)
            .ok_or_else(|| Error::InvalidOperation("manual clock overflow".into()))?;
        Ok(())
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.now()
    }
}
