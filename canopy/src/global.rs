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
    pub tx: Option<mpsc::Sender<Event>>,
}

thread_local! {
    pub (crate) static STATE: RefCell<GlobalState> = RefCell::new(GlobalState {
        focus_gen: 1,
        last_focus_gen: 1,
        render_gen: 1,
        poller: Poller::new(),
        tx: None,
    });
}

pub(crate) fn start_poller(tx: mpsc::Sender<Event>) {
    STATE.with(|global_state| {
        global_state.borrow_mut().tx = Some(tx);
    });
}
