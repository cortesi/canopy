pub mod key;
pub mod mouse;
pub mod tick;

use crate::{geom::Size, Actions};

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// This enum represents all the event types that drive the application.
#[derive(Debug, PartialEq)]
pub enum Event<A> {
    /// A keystroke
    Key(key::Key),
    /// A mouse action
    Mouse(mouse::Mouse),
    /// Terminal resize
    Resize(Size),
    /// User-definable actions
    Action(A),
}

/// An emitter that is polled by the application to retrieve events.
pub struct EventSource<A> {
    rx: mpsc::Receiver<Event<A>>,
    tx: mpsc::Sender<Event<A>>,
}

impl<A: 'static + Actions> Default for EventSource<A> {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        EventSource { rx, tx }
    }
}

impl<A: 'static + Actions> EventSource<A> {
    /// Convenience function that spawns a thread that periodically pumps the
    /// specified event into the app.
    pub fn periodic(&self, millis: u64, action: A) {
        let tick_tx = self.tx.clone();
        thread::spawn(move || loop {
            if tick_tx.send(Event::Action(action)).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(millis));
        });
    }

    /// Get a channel to pump events into the app. In practice, this will
    /// usually be user-defined Event::Action events.
    pub fn tx(&self) -> mpsc::Sender<Event<A>> {
        self.tx.clone()
    }

    /// Retrieve the next event, blocking until an event is recieved or the
    /// underlying channel closes..
    pub fn next(&self) -> Result<Event<A>, mpsc::RecvError> {
        self.rx.recv()
    }
}
