use std::cell::RefCell;
use std::sync::mpsc;

use crate::{event::Event, poll::Poller, KeyMap};

pub(crate) struct GlobalState {
    /// A counter that is incremented every time focus changes. The current focus
    /// will have a state `focus_gen` equal to this.
    pub focus_gen: u64,
    /// Stores the focus_gen during the last render. Used to detect if focus has
    /// changed.
    pub last_render_focus_gen: u64,

    /// A counter that is incremented every time we render. All items that
    /// require rendering during the current sweep will have a state `render_gen`
    /// equal to this.
    pub render_gen: u64,
    /// The poller is responsible for tracking nodes that have pending poll
    /// events, and scheduling their execution.
    pub poller: Poller,
    /// Has the tree been tainted? This reset to false before every event sweep.
    pub taint: bool,

    pub keymap: KeyMap,

    pub event_tx: mpsc::Sender<Event>,
    pub event_rx: Option<mpsc::Receiver<Event>>,
}

impl GlobalState {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        GlobalState {
            focus_gen: 1,
            last_render_focus_gen: 1,
            render_gen: 1,
            taint: false,
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            keymap: KeyMap::new(),
        }
    }
}

thread_local! {
    static STATE: RefCell<Option<GlobalState>> = RefCell::new(Some(GlobalState::new()));
}

/// Operate on the global state. This function will panic if state is uninitialized.
pub(crate) fn with<F, R>(f: F) -> R
where
    F: FnOnce(&mut GlobalState) -> R,
{
    STATE.with(|x| {
        let mut s = x.borrow_mut().take().unwrap();
        let v = f(&mut s);
        x.borrow_mut().replace(s);
        v
    })
}

/// Operate on the global keymap
pub fn keymap<F>(f: F)
where
    F: FnOnce(&mut KeyMap),
{
    with(|global_state| {
        f(&mut global_state.keymap);
    });
}

/// Has the focus changed since the last render sweep?
pub(crate) fn focus_changed() -> bool {
    with(|s| s.focus_gen != s.last_render_focus_gen)
}

pub(crate) fn start_poller(tx: mpsc::Sender<Event>) {
    with(|s| {
        s.event_tx = tx;
    });
}
