//! Testing-only clock injection for the shared runtime driver.

use std::sync::Arc;

use super::Canopy;
use crate::{
    error::{Error, Result},
    testing::ManualClock,
};

impl Canopy {
    /// Install an explicit test clock before the application starts polling.
    ///
    /// Replacing the clock after initialization, publication, or evaluation
    /// begins returns an error, so existing deadlines retain their original
    /// time base.
    pub fn set_clock_for_testing(&mut self, clock: Arc<ManualClock>) -> Result<()> {
        if self.termbuf.is_some()
            || self.core.nodes.values().any(|node| node.initialized)
            || self.script_host.is_eval_active()
            || self.next_deadline().is_some()
        {
            return Err(Error::InvalidOperation(
                "test clock must be installed before runtime initialization".into(),
            ));
        }
        self.poller.set_clock(clock)
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
        let mut canopy = Canopy::new();
        canopy.set_clock_for_testing(clock.clone())?;
        canopy.replace_root(counter.clone())?;
        canopy.set_root_size(Size::new(10, 2))?;
        canopy.finalize_api()?;
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
