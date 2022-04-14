pub mod key;
pub mod mouse;

use crate::geom::Expanse;

use std::sync::mpsc;

/// This enum represents all the event types that drive the application.
#[derive(Debug, PartialEq)]
pub(crate) enum Event {
    /// A keystroke
    Key(key::Key),
    /// A mouse action
    Mouse(mouse::Mouse),
    /// Terminal resize
    Resize(Expanse),
    Poll(Vec<u64>),
    Render,
}

/// An emitter that is polled by the application to retrieve events.
pub(crate) struct EventSource {
    rx: mpsc::Receiver<Event>,
}

impl EventSource {
    pub fn new(rx: mpsc::Receiver<Event>) -> Self {
        EventSource { rx }
    }
}

impl EventSource {
    /// Retrieve the next event, blocking until an event is recieved or the
    /// underlying channel closes..
    pub fn next(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
