pub mod key;
pub mod mouse;
pub mod tick;

use crate::{geom::Size, Actions};

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum Event<A> {
    Key(key::Key),
    Mouse(mouse::Mouse),
    Resize(Size),
    Action(A),
}

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

    /// Get a channel to pump events into the app
    pub fn tx(&self) -> mpsc::Sender<Event<A>> {
        self.tx.clone()
    }

    pub fn next(&self) -> Result<Event<A>, mpsc::RecvError> {
        self.rx.recv()
    }
}
