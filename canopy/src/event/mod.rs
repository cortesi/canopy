pub mod key;
pub mod mouse;

use crate::geom::Size;

use std::sync::mpsc;

/// This enum represents all the event types that drive the application.
#[derive(Debug, PartialEq)]
pub(crate) enum Event {
    /// A keystroke
    Key(key::Key),
    /// A mouse action
    Mouse(mouse::Mouse),
    /// Terminal resize
    Resize(Size),
    Poll(Vec<u64>),
    Render,
}

/// An emitter that is polled by the application to retrieve events.
pub(crate) struct EventSource {
    rx: mpsc::Receiver<Event>,
    tx: mpsc::Sender<Event>,
}

impl Default for EventSource {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        EventSource { rx, tx }
    }
}

impl EventSource {
    /// Get a channel to pump events into the app. In practice, this will
    /// usually be user-defined Event::Action events.
    pub fn tx(&self) -> mpsc::Sender<Event> {
        self.tx.clone()
    }

    /// Retrieve the next event, blocking until an event is recieved or the
    /// underlying channel closes..
    pub fn next(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
