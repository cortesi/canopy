pub mod key;
pub mod mouse;

use std::sync::mpsc;

use crate::{NodeId, geom::Expanse};

/// This enum represents all the event types that drive the application.
#[derive(Debug)]
pub enum Event {
    /// A keystroke
    Key(key::Key),
    /// A mouse action
    Mouse(mouse::MouseEvent),
    /// Terminal resize
    Resize(Expanse),
    /// A poll event
    Poll(Vec<NodeId>),
    /// Terminal has gained focus
    FocusGained,
    /// Terminal has lost focus
    FocusLost,
    /// Cut and paste
    #[allow(dead_code)]
    Paste(String),
}

/// An emitter that is polled by the application to retrieve events.
#[allow(dead_code)]
pub(crate) struct EventSource {
    rx: mpsc::Receiver<Event>,
}

#[allow(dead_code)]
impl EventSource {
    pub fn new(rx: mpsc::Receiver<Event>) -> Self {
        Self { rx }
    }
}

#[allow(dead_code)]
impl EventSource {
    /// Retrieve the next event, blocking until an event is recieved or the
    /// underlying channel closes..
    pub fn next(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
