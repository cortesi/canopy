//! Testing-only clock, script-module, journal, layout, and driver access.

use std::sync::Arc;

use futures::{Stream, StreamExt};

use super::Canopy;
#[cfg(test)]
use crate::render::RenderBackend;
use crate::{
    error::{Error, Result},
    testing::ManualClock,
};

impl Canopy {
    /// Invalidate cached exports from persistent script modules.
    ///
    /// Pass a root such as `@user` or `@project` to invalidate one root, or
    /// `None` to invalidate every root. Returns the new source epoch, or
    /// `None` when no module source is configured or the named root is
    /// unknown.
    pub fn invalidate_script_modules(&mut self, root: Option<&str>) -> Result<Option<u64>> {
        if self.script.host.is_eval_active() {
            return Err(Error::ScriptBusy(
                "cannot reload modules while evaluation is active".into(),
            ));
        }
        let Some(source) = self.script.module_source.as_ref() else {
            return Ok(None);
        };
        let epoch = match root {
            Some(root) => match source.invalidate(root) {
                Ok(epoch) => epoch,
                Err(_) => return Ok(None),
            },
            None => source.invalidate_all(),
        };
        self.clear_script_callbacks();
        Ok(Some(epoch))
    }

    /// Set the maximum number of retained script journal entries.
    ///
    /// When the journal exceeds the limit the oldest entries are evicted. A
    /// limit of zero disables retention entirely.
    pub fn set_script_journal_limit(&mut self, limit: usize) {
        self.journal.set_limit(limit);
    }

    /// Remove application bindings and callbacks from the current source epoch.
    fn clear_script_callbacks(&mut self) {
        let removed = self.core.input_map.clear_application();
        self.release_removed_bindings(removed);
        for hook in self.script.host.drain_on_start_hooks() {
            self.script.host.release_function(hook);
        }
    }

    /// Install an explicit test clock before the application starts polling.
    ///
    /// Replacing the clock after initialization, publication, or evaluation
    /// begins returns an error, so existing deadlines retain their original
    /// time base.
    pub fn set_clock_for_testing(&mut self, clock: Arc<ManualClock>) -> Result<()> {
        if self.frame.termbuf.is_some()
            || self.core.nodes.values().any(|node| node.initialized)
            || self.script.host.is_eval_active()
            || self.next_deadline().is_some()
        {
            return Err(Error::Invalid(
                "test clock must be installed before runtime initialization".into(),
            ));
        }
        self.poller.set_clock(clock)
    }

    /// Render the tree only if a render is pending.
    #[cfg(test)]
    pub(crate) fn render_if_pending<R: RenderBackend>(&mut self, be: &mut R) -> Result<bool> {
        if !self.core.changes.is_pending() {
            return Ok(false);
        }
        self.render(be)?;
        Ok(true)
    }

    /// Bring layout up to date without painting, as frame preparation does
    /// before it paints. Benchmarks use this to time layout alone.
    pub fn layout_for_testing(&mut self) -> Result<()> {
        self.core.invalidate(crate::Invalidation::Layout);
        self.settle_layout()
    }

    /// Take the driver's event notifications so a test can step only when work
    /// wakes it.
    ///
    /// The test owns subsequent event delivery. Synchronous evaluation is
    /// unavailable while this receiver is outside the app.
    pub fn take_event_receiver(&mut self) -> Option<impl Stream<Item = ()> + use<>> {
        self.event_rx
            .take()
            .map(|events| events.map(|_notification| ()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::Duration,
    };

    use super::*;
    use crate::{Context, NodeWakeHandle, Widget, Work, WorkLifetime, geom::Size};

    #[derive(Clone)]
    struct PollCounter {
        count: Rc<Cell<usize>>,
        wake: Rc<RefCell<Option<NodeWakeHandle>>>,
        interval: Rc<Cell<Option<Duration>>>,
    }

    impl Widget for PollCounter {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            *self.wake.borrow_mut() = Some(ctx.wake_handle(WorkLifetime::Node)?);
            Ok(())
        }

        fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
            self.count.set(self.count.get() + 1);
            self.interval.get()
        }
    }

    fn polling_app(interval: Duration) -> Result<(Canopy, Arc<ManualClock>, PollCounter)> {
        let clock = Arc::new(ManualClock::new());
        let counter = PollCounter {
            count: Rc::new(Cell::new(0)),
            wake: Rc::new(RefCell::new(None)),
            interval: Rc::new(Cell::new(Some(interval))),
        };
        let mut canopy = crate::CanopyBuilder::new().build()?;
        canopy.set_clock_for_testing(clock.clone())?;
        canopy.replace_root(counter.clone())?;
        canopy.set_root_size(Size::new(10, 2))?;
        canopy.turn(Work::Prepare)?;
        Ok((canopy, clock, counter))
    }

    #[test]
    fn manual_deadline_and_worker_wake_drive_polls_without_sleeping() -> Result<()> {
        let (mut canopy, clock, counter) = polling_app(Duration::from_millis(10))?;
        assert_eq!(counter.count.get(), 1);
        assert_eq!(
            canopy.next_deadline(),
            Some(clock.now() + Duration::from_millis(10))
        );
        clock.advance(Duration::from_millis(9))?;
        canopy.turn(Work::Wake)?;
        assert_eq!(counter.count.get(), 1);
        clock.advance(Duration::from_millis(1))?;
        canopy.turn(Work::Wake)?;
        assert_eq!(counter.count.get(), 2);
        counter.wake.borrow().as_ref().unwrap().wake()?;
        canopy.turn(Work::Wake)?;
        assert_eq!(counter.count.get(), 3);
        assert!(
            canopy
                .set_clock_for_testing(Arc::new(ManualClock::new()))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn wake_poll_can_cancel_its_previous_deadline() -> Result<()> {
        let (mut canopy, clock, counter) = polling_app(Duration::from_millis(10))?;
        clock.advance(Duration::from_millis(2))?;
        counter.interval.set(None);
        counter.wake.borrow().as_ref().unwrap().wake()?;
        canopy.turn(Work::Wake)?;
        assert_eq!(counter.count.get(), 2);
        assert_eq!(canopy.next_deadline(), None);

        clock.advance(Duration::from_millis(8))?;
        canopy.turn(Work::Wake)?;
        assert_eq!(counter.count.get(), 2, "the old timer must stay canceled");

        counter.wake.borrow().as_ref().unwrap().wake()?;
        canopy.turn(Work::Wake)?;
        assert_eq!(
            counter.count.get(),
            3,
            "explicit wakes still poll immediately"
        );
        Ok(())
    }

    #[test]
    fn submillisecond_poll_intervals_allow_the_runtime_to_sleep() -> Result<()> {
        for interval in [Duration::ZERO, Duration::from_micros(500)] {
            let (mut canopy, clock, counter) = polling_app(interval)?;
            assert_eq!(counter.count.get(), 1);
            assert_eq!(
                canopy.next_deadline(),
                Some(clock.now() + Duration::from_millis(1))
            );
            for _ in 0..10 {
                canopy.turn(Work::Wake)?;
            }
            assert_eq!(counter.count.get(), 1);
            clock.advance(Duration::from_millis(1))?;
            canopy.turn(Work::Wake)?;
            assert_eq!(counter.count.get(), 2);
            assert_eq!(
                canopy.next_deadline(),
                Some(clock.now() + Duration::from_millis(1))
            );
        }
        Ok(())
    }
}
