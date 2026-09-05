//! Event-driven helpers for adapter integration tests.

use futures::channel::mpsc::UnboundedReceiver;

use crate::{Canopy, event::Event};

impl Canopy {
    /// Take the driver's event notifications so a test can step only when work
    /// wakes it.
    ///
    /// The test owns subsequent event delivery. Synchronous evaluation is
    /// unavailable while this receiver is outside the app.
    pub fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<Event>> {
        self.event_rx.take()
    }
}
