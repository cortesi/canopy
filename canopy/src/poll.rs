use std::{
    cmp::Ordering,
    collections::binary_heap::BinaryHeap,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use crate::event::Event;

/// A node that has a pending callback.
struct PendingNode {
    time: SystemTime,
    node_id: u64,
}

impl PartialEq for PendingNode {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl Eq for PendingNode {}

/// Reverse order so nodes with the closest callback time are at the top.
impl PartialOrd for PendingNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(other.time.cmp(&self.time))
    }
}

/// Reverse order so nodes with the closest callback time are at the top.
impl Ord for PendingNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.time.cmp(&self.time)
    }
}

/// A heap that tracks the current list of pending callbacks.
#[derive(Default)]
struct PendingHeap {
    nodes: BinaryHeap<PendingNode>,
}

impl PendingHeap {
    fn _add(&mut self, now: SystemTime, node_id: u64, duration: Duration) {
        self.nodes.push(PendingNode {
            time: now + duration,
            node_id,
        });
    }

    /// Add a node with a callback duration to the heap.
    fn add(&mut self, node_id: u64, duration: Duration) {
        self._add(SystemTime::now(), node_id, duration);
    }

    fn _current_wait(&self, now: SystemTime) -> Option<Duration> {
        if let Some(top) = self.nodes.peek() {
            Some(top.time.duration_since(now).unwrap_or(Duration::new(0, 0)))
        } else {
            None
        }
    }

    /// Retrieve the current shortest wait time. We return None if no nodes are
    /// waiting, and a duration of 0 if the current top-most node has a
    /// scheduled time in the past.
    fn current_wait(&self) -> Option<Duration> {
        self._current_wait(SystemTime::now())
    }

    fn _collect(&mut self, now: SystemTime) -> Vec<u64> {
        let mut v = vec![];
        while let Some(n) = self.nodes.pop() {
            if n.time <= now {
                v.push(n.node_id);
            } else {
                // Put it back on the heap.
                self.nodes.push(n);
                break;
            }
        }
        v
    }

    /// Remove and return all the pending operations .
    pub fn collect(&mut self) -> Vec<u64> {
        self._collect(SystemTime::now())
    }
}

/// The Poller is responsible for scheduling poll events for nodes.
pub struct Poller {
    /// Handle for the scheduler thread
    handle: Option<thread::JoinHandle<()>>,
    pending: Arc<Mutex<PendingHeap>>,
    event_tx: mpsc::Sender<Event>,
}

impl Poller {
    pub(crate) fn new(event_tx: mpsc::Sender<Event>) -> Self {
        Self {
            handle: None,
            pending: Arc::new(Mutex::new(PendingHeap::default())),
            event_tx,
        }
    }

    /// Schedule a node to be polled. This function requires us to pass in the
    /// tx channel, which means that a lock over the global state must already
    /// be in place.
    pub fn schedule(&mut self, node_id: u64, duration: Duration) {
        let mut l = self.pending.lock().unwrap();
        l.add(node_id, duration);
        if let Some(h) = self.handle.as_mut() {
            // The thread is running, let's wake it up.
            h.thread().unpark();
        } else {
            let pending = self.pending.clone();
            let tx = self.event_tx.clone();
            self.handle = Some(thread::spawn(move || loop {
                // Caution: moving this into the statement below means that we
                // retain the lock over the thread park, causing deadlock.
                let d = pending.lock().unwrap().current_wait();
                if let Some(d) = d {
                    thread::park_timeout(d);
                } else {
                    // We have no current wait time, so we just park the thread.
                    thread::park();
                };
                let ids = pending.lock().unwrap().collect();
                if !ids.is_empty() && tx.send(Event::Poll(ids)).is_err() {
                    break;
                }
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tutils::utils, Result, StatefulNode};
    #[test]
    fn pendingheap() -> Result<()> {
        let now = SystemTime::now();

        let mut ph = PendingHeap::default();
        let n1 = utils::TLeaf::new("foo");
        let n2 = utils::TLeaf::new("bar");

        assert_eq!(ph._current_wait(now), None);
        ph._add(now, n1.id(), Duration::from_secs(10));
        assert_eq!(ph._current_wait(now).unwrap(), Duration::from_secs(10));
        ph._add(now, n2.id(), Duration::from_secs(100));
        assert!(ph._current_wait(now).unwrap() <= Duration::from_secs(10));
        assert_eq!(
            ph._collect(SystemTime::now() + Duration::from_secs(11)),
            vec![n1.id()]
        );
        assert!(ph._current_wait(now).unwrap() <= Duration::from_secs(100));

        Ok(())
    }
}
