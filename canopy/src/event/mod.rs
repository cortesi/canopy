pub mod key;
pub mod mouse;
use crate::geom;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event;

#[derive(Debug, PartialEq)]
pub enum Event {
    Key(key::Key),
    Mouse(mouse::Mouse),
    Resize(geom::Rect),
    Tick(),
}

pub struct EventSource {
    rx: mpsc::Receiver<Event>,
    tx: mpsc::Sender<Event>,
}

static TICKRATE: AtomicU32 = AtomicU32::new(200);

impl EventSource {
    pub fn new(millis: u32) -> Self {
        let (tx, rx) = mpsc::channel();
        let es = EventSource { rx, tx };
        es.set_tickrate(millis);
        es.spawn();
        es
    }

    pub fn spawn(&self) {
        let tick_tx = self.tx.clone();
        let evt_tx = self.tx.clone();
        thread::spawn(move || loop {
            match event::read() {
                Ok(evt) => {
                    let oevt = match evt {
                        event::Event::Key(e) => Event::Key(e.into()),
                        event::Event::Mouse(e) => Event::Mouse(e.into()),
                        event::Event::Resize(x, y) => Event::Resize(geom::Rect {
                            tl: geom::Point { x: 0, y: 0 },
                            w: x,
                            h: y,
                        }),
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
        thread::spawn(move || loop {
            if tick_tx.send(Event::Tick()).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(
                TICKRATE.load(Ordering::Relaxed).into(),
            ));
        });
    }

    pub fn set_tickrate(&self, millis: u32) {
        TICKRATE.store(millis, Ordering::Relaxed)
    }

    pub fn next(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
