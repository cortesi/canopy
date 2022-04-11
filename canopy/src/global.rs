use std::cell::RefCell;
use std::sync::mpsc;

use crate::{event::Event, poll::Poller};

pub(crate) struct GlobalState {
    // A counter that is incremented every time focus changes. The current focus
    // will have a state `focus_gen` equal to this.
    pub focus_gen: u64,
    // Stores the focus_gen during the last render. Used to detect if focus has
    // changed.
    pub last_focus_gen: u64,
    // A counter that is incremented every time we render. All items that
    // require rendering during the current sweep will have a state `render_gen`
    // equal to this.
    pub render_gen: u64,
    /// The poller is responsible for tracking nodes that have pending poll
    /// events, and scheduling their execution.
    pub poller: Poller,
    /// Has the tree been tainted? This reset to false before every event sweep.
    pub taint: bool,

    pub event_tx: mpsc::Sender<Event>,
    pub event_rx: Option<mpsc::Receiver<Event>>,
}

impl GlobalState {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        GlobalState {
            focus_gen: 1,
            last_focus_gen: 1,
            render_gen: 1,
            taint: false,
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
        }
    }
}

thread_local! {
    pub (crate) static STATE: RefCell<GlobalState> = RefCell::new(GlobalState::new());
}

pub(crate) fn start_poller(tx: mpsc::Sender<Event>) {
    STATE.with(|global_state| {
        global_state.borrow_mut().event_tx = tx;
    });
}
