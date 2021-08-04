pub mod key;
pub mod mouse;
pub mod tick;

use crate::{geom::Size, Actions};

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event;

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

impl<A: 'static + Actions> EventSource<A> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let es = EventSource { rx, tx };
        es.spawn();
        es
    }

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

    pub fn spawn(&self) {
        let evt_tx = self.tx.clone();
        thread::spawn(move || loop {
            match event::read() {
                Ok(evt) => {
                    let oevt = match evt {
                        event::Event::Key(e) => Event::Key(e.into()),
                        event::Event::Mouse(e) => Event::Mouse(e.into()),
                        event::Event::Resize(x, y) => Event::Resize(Size::new(x, y)),
                    };
                    let ret = evt_tx.send(oevt);
                    if ret.is_err() {
                        // FIXME: Do a bit more work here. Restore context,
                        // exit.
                        return;
                    }
                }
                Err(_) => {
                    // FIXME: Do a bit more work here. Restore context,
                    // exit.
                    return;
                }
            }
        });
    }

    pub fn next(&self) -> Result<Event<A>, mpsc::RecvError> {
        self.rx.recv()
    }
}
