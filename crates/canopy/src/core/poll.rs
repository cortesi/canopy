//! Poll scheduling for widget callbacks.

use std::{
    cmp::Ordering,
    collections::{HashMap, binary_heap::BinaryHeap},
    fmt::Debug,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::{
    NodeId,
    error::{Error, Result},
    event::Event,
};

/// Time source used to calculate poll deadlines.
trait Clock: Debug + Send + Sync {
    /// Return the current monotonic time.
    fn now(&self) -> Instant;
}

/// Production monotonic clock.
#[derive(Debug)]
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// One scheduled node callback.
#[derive(Debug)]
struct PendingNode {
    /// Scheduled time for the callback.
    deadline: Instant,
    /// Node identifier to poll.
    node_id: NodeId,
}

impl PartialEq for PendingNode {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.node_id == other.node_id
    }
}

impl Eq for PendingNode {}

/// Reverse order so the closest deadline is at the top.
impl PartialOrd for PendingNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Reverse order so the closest deadline is at the top.
impl Ord for PendingNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .deadline
            .cmp(&self.deadline)
            .then_with(|| other.node_id.cmp(&self.node_id))
    }
}

/// Pending deadlines, with stale heap entries removed lazily after rescheduling.
#[derive(Default, Debug)]
struct PendingHeap {
    /// Deadline-ordered callback entries.
    nodes: BinaryHeap<PendingNode>,
    /// Authoritative deadline for each scheduled node.
    deadlines: HashMap<NodeId, Instant>,
}

impl PendingHeap {
    /// Schedule or reschedule one node at an absolute deadline.
    fn schedule(&mut self, node_id: NodeId, deadline: Instant) {
        self.deadlines.insert(node_id, deadline);
        self.nodes.push(PendingNode { deadline, node_id });
    }

    /// Cancel a pending node callback.
    fn cancel(&mut self, node_id: NodeId) {
        self.deadlines.remove(&node_id);
    }

    /// Discard heap entries superseded by a reschedule or cancellation.
    fn discard_stale(&mut self) {
        while self
            .nodes
            .peek()
            .is_some_and(|node| self.deadlines.get(&node.node_id).copied() != Some(node.deadline))
        {
            self.nodes.pop();
        }
    }

    /// Calculate how long the worker should wait for the next deadline.
    fn current_wait(&mut self, now: Instant) -> Option<Duration> {
        self.discard_stale();
        self.nodes
            .peek()
            .map(|node| node.deadline.saturating_duration_since(now))
    }

    /// Remove and return every callback due at `now`.
    fn collect(&mut self, now: Instant) -> Vec<NodeId> {
        let mut due = Vec::new();
        loop {
            self.discard_stale();
            let Some(node) = self.nodes.peek() else {
                break;
            };
            if node.deadline > now {
                break;
            }
            let node = self.nodes.pop().expect("pending node disappeared");
            if self.deadlines.remove(&node.node_id) == Some(node.deadline) {
                due.push(node.node_id);
            }
        }
        due
    }
}

/// Commands accepted by the scheduler worker.
#[derive(Clone, Copy, Debug)]
enum SchedulerCommand {
    /// Schedule or reschedule a node.
    Schedule {
        /// Node whose callback should run.
        node_id: NodeId,
        /// Absolute monotonic deadline.
        deadline: Instant,
    },
    /// Cancel a node's pending callback.
    Cancel(NodeId),
    /// Stop the worker.
    Shutdown,
}

/// Apply one scheduler command, returning false on shutdown.
fn apply_command(command: SchedulerCommand, pending: &mut PendingHeap) -> bool {
    match command {
        SchedulerCommand::Schedule { node_id, deadline } => {
            pending.schedule(node_id, deadline);
            true
        }
        SchedulerCommand::Cancel(node_id) => {
            pending.cancel(node_id);
            true
        }
        SchedulerCommand::Shutdown => false,
    }
}

/// Run the scheduler until shutdown or the event receiver closes.
fn scheduler_worker(
    commands: &mpsc::Receiver<SchedulerCommand>,
    event_tx: &mpsc::Sender<Event>,
    clock: &dyn Clock,
) {
    let mut pending = PendingHeap::default();
    loop {
        let now = clock.now();
        let due = pending.collect(now);
        if !due.is_empty() && event_tx.send(Event::Poll(due)).is_err() {
            return;
        }

        let command = match pending.current_wait(now) {
            Some(wait) => match commands.recv_timeout(wait) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            },
            None => match commands.recv() {
                Ok(command) => command,
                Err(mpsc::RecvError) => return,
            },
        };
        if !apply_command(command, &mut pending) {
            return;
        }
    }
}

/// Owned scheduler for widget poll callbacks.
#[derive(Debug)]
pub struct Poller {
    /// Scheduler command sender.
    command_tx: Option<mpsc::Sender<SchedulerCommand>>,
    /// Scheduler worker, joined during shutdown.
    worker: Option<thread::JoinHandle<()>>,
    /// Clock used to calculate checked deadlines.
    clock: Arc<dyn Clock>,
}

impl Poller {
    /// Construct a scheduler using the system monotonic clock.
    pub(crate) fn new(event_tx: mpsc::Sender<Event>) -> Self {
        Self::with_clock(event_tx, Arc::new(SystemClock))
    }

    /// Construct a scheduler with an explicit clock.
    fn with_clock(event_tx: mpsc::Sender<Event>, clock: Arc<dyn Clock>) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let worker_clock = Arc::clone(&clock);
        let worker = thread::spawn(move || {
            scheduler_worker(&command_rx, &event_tx, worker_clock.as_ref());
        });
        Self {
            command_tx: Some(command_tx),
            worker: Some(worker),
            clock,
        }
    }

    /// Send a command unless the scheduler has already stopped.
    fn send(&self, command: SchedulerCommand) -> Result<()> {
        if self
            .worker
            .as_ref()
            .is_none_or(thread::JoinHandle::is_finished)
        {
            return Err(Error::RunLoop("poll scheduler is not running".into()));
        }
        self.command_tx
            .as_ref()
            .ok_or_else(|| Error::RunLoop("poll scheduler is shut down".into()))?
            .send(command)
            .map_err(|_| Error::RunLoop("poll scheduler command channel closed".into()))
    }

    /// Schedule or reschedule a node callback.
    pub(crate) fn schedule(&self, node_id: impl Into<NodeId>, duration: Duration) -> Result<()> {
        let node_id = node_id.into();
        let deadline = self
            .clock
            .now()
            .checked_add(duration)
            .ok_or_else(|| Error::RunLoop("poll deadline overflow".into()))?;
        self.cancel(node_id)?;
        self.send(SchedulerCommand::Schedule { node_id, deadline })
    }

    /// Cancel a node's pending callback.
    fn cancel(&self, node_id: impl Into<NodeId>) -> Result<()> {
        self.send(SchedulerCommand::Cancel(node_id.into()))
    }

    /// Stop and join the scheduler worker.
    fn shutdown(&mut self) -> Result<()> {
        if let Some(tx) = self.command_tx.take() {
            let _worker_already_stopped = tx.send(SchedulerCommand::Shutdown);
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::RunLoop("poll scheduler worker panicked".into()))?;
        }
        Ok(())
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        drop(self.shutdown());
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use slotmap::SlotMap;

    use super::*;

    /// Deterministic clock advanced explicitly by tests.
    #[derive(Debug)]
    struct ManualClock {
        now: Mutex<Instant>,
    }

    impl ManualClock {
        /// Construct a clock at an explicit instant.
        fn new(now: Instant) -> Self {
            Self {
                now: Mutex::new(now),
            }
        }

        /// Advance the clock without waiting for wall time.
        fn advance(&self, duration: Duration) {
            let mut now = self.now.lock();
            *now = now.checked_add(duration).expect("test clock overflow");
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> Instant {
            *self.now.lock()
        }
    }

    fn node_ids() -> (NodeId, NodeId) {
        let mut map: SlotMap<NodeId, ()> = SlotMap::with_key();
        (map.insert(()), map.insert(()))
    }

    #[test]
    fn pending_heap_reschedules_and_cancels_deterministically() {
        let now = Instant::now();
        let (first, second) = node_ids();
        let mut pending = PendingHeap::default();

        pending.schedule(first, now + Duration::from_secs(10));
        pending.schedule(first, now + Duration::from_secs(20));
        pending.schedule(second, now + Duration::from_secs(15));
        assert_eq!(pending.collect(now + Duration::from_secs(11)), Vec::new());
        assert_eq!(pending.collect(now + Duration::from_secs(16)), vec![second]);
        pending.cancel(first);
        assert_eq!(pending.current_wait(now), None);
    }

    #[test]
    fn worker_uses_injected_clock_and_emits_due_nodes() {
        let now = Instant::now();
        let clock = Arc::new(ManualClock::new(now));
        let (event_tx, event_rx) = mpsc::channel();
        let poller = Poller::with_clock(event_tx, clock.clone());
        let (node, _) = node_ids();

        clock.advance(Duration::from_secs(5));
        poller
            .schedule(node, Duration::ZERO)
            .expect("scheduler should accept work");
        let event = event_rx.recv().expect("scheduler should emit an event");
        assert!(matches!(event, Event::Poll(nodes) if nodes == vec![node]));
    }

    #[test]
    fn shutdown_joins_worker_and_rejects_more_work() {
        let (event_tx, _event_rx) = mpsc::channel();
        let mut poller = Poller::new(event_tx);
        let (node, _) = node_ids();

        poller.shutdown().expect("scheduler should join cleanly");
        assert!(matches!(
            poller.schedule(node, Duration::ZERO),
            Err(Error::RunLoop(_))
        ));
    }

    #[test]
    fn repeated_construct_drop_joins_every_worker() {
        for _ in 0..64 {
            let (event_tx, _event_rx) = mpsc::channel();
            drop(Poller::new(event_tx));
        }
    }
}
