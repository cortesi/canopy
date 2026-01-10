use std::{
    cmp::Ordering,
    collections::binary_heap::BinaryHeap,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::{NodeId, event::Event};

/// A node that has a pending callback.
#[derive(Debug)]
struct PendingNode {
    /// Scheduled time for the callback.
    time: Instant,
    /// Node identifier to poll.
    node_id: NodeId,
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
        Some(self.cmp(other))
    }
}

/// Reverse order so nodes with the closest callback time are at the top.
impl Ord for PendingNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.time.cmp(&self.time)
    }
}

/// A heap that tracks the current list of pending callbacks.
#[derive(Default, Debug)]
struct PendingHeap {
    /// Pending node heap.
    nodes: BinaryHeap<PendingNode>,
}

impl PendingHeap {
    /// Add a node with an explicit time base.
    fn _add(&mut self, now: Instant, node_id: NodeId, duration: Duration) {
        self.nodes.push(PendingNode {
            time: now + duration,
            node_id,
        });
    }

    /// Add a node with a callback duration to the heap.
    fn add(&mut self, node_id: NodeId, duration: Duration) {
        self._add(Instant::now(), node_id, duration);
    }

    /// Calculate the wait time relative to a given timestamp.
    fn _current_wait(&self, now: Instant) -> Option<Duration> {
        self.nodes.peek().map(|top| {
            top.time
                .checked_duration_since(now)
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Retrieve the current shortest wait time. We return None if no nodes are
    /// waiting, and a duration of 0 if the current top-most node has a
    /// scheduled time in the past.
    fn current_wait(&self) -> Option<Duration> {
        self._current_wait(Instant::now())
    }

    /// Collect due node IDs relative to a given timestamp.
    fn _collect(&mut self, now: Instant) -> Vec<NodeId> {
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
    pub fn collect(&mut self) -> Vec<NodeId> {
        self._collect(Instant::now())
    }
}

/// The Poller is responsible for scheduling poll events for nodes.
#[derive(Debug)]
pub struct Poller {
    /// Handle for the scheduler thread
    handle: Option<thread::JoinHandle<()>>,
    /// Pending heap shared with the scheduler thread.
    pending: Arc<Mutex<PendingHeap>>,
    /// Event sender for poll notifications.
    event_tx: mpsc::Sender<Event>,
}

impl Poller {
    /// Construct a new poller.
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
    pub fn schedule(&mut self, node_id: impl Into<NodeId>, duration: Duration) {
        let mut l = self.pending.lock().unwrap();
        l.add(node_id.into(), duration);
        if let Some(h) = self.handle.as_mut() {
            // The thread is running, let's wake it up.
            h.thread().unpark();
        } else {
            let pending = self.pending.clone();
            let tx = self.event_tx.clone();
            self.handle = Some(thread::spawn(move || {
                loop {
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
                }
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use slotmap::SlotMap;

    use super::*;
    use crate::error::Result;
    #[test]
    fn pendingheap() -> Result<()> {
        let now = Instant::now();

        let mut ph = PendingHeap::default();
        let mut map: SlotMap<NodeId, ()> = SlotMap::with_key();
        let n1 = map.insert(());
        let n2 = map.insert(());

        assert_eq!(ph._current_wait(now), None);
        ph._add(now, n1, Duration::from_secs(10));
        assert_eq!(ph._current_wait(now).unwrap(), Duration::from_secs(10));
        ph._add(now, n2, Duration::from_secs(100));
        assert!(ph._current_wait(now).unwrap() <= Duration::from_secs(10));
        assert_eq!(ph._collect(now + Duration::from_secs(11)), vec![n1]);
        assert!(ph._current_wait(now).unwrap() <= Duration::from_secs(100));

        Ok(())
    }
}
