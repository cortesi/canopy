//! Driver-owned deadlines for widget poll callbacks.

use std::{
    cmp::Ordering,
    collections::{HashMap, binary_heap::BinaryHeap},
    fmt::Debug,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    NodeId,
    core::wake::WorkStamp,
    error::{Error, Result},
};

/// Monotonic time source shared by driver scheduling and deterministic tests.
pub trait Clock: Debug + Send + Sync {
    /// Return the current monotonic time.
    fn now(&self) -> Instant;
}

/// Production monotonic clock.
#[derive(Debug)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// One scheduled incarnation of a node callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingNode {
    /// Monotonic instant when this callback becomes due.
    deadline: Instant,
    /// Widget incarnation and optional attachment that own this callback.
    stamp: WorkStamp,
}

impl PartialOrd for PendingNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PendingNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .deadline
            .cmp(&self.deadline)
            .then_with(|| other.stamp.cmp(&self.stamp))
    }
}

/// Deadline heap with one authoritative entry per node and bounded stale
/// entries.
#[derive(Default, Debug)]
struct PendingHeap {
    /// Deadline-ordered entries, including bounded stale entries.
    nodes: BinaryHeap<PendingNode>,
    /// Latest accepted deadline for each node.
    deadlines: HashMap<NodeId, PendingNode>,
}

impl PendingHeap {
    /// Replace one node deadline and bound obsolete heap entries.
    fn schedule(&mut self, stamp: WorkStamp, deadline: Instant) {
        let pending = PendingNode { deadline, stamp };
        self.deadlines.insert(stamp.node, pending);
        self.nodes.push(pending);
        self.compact();
    }

    /// Rebuild when stale entries outnumber current deadlines.
    fn compact(&mut self) {
        if self.nodes.len() > self.deadlines.len().saturating_mul(2) {
            self.nodes = self.deadlines.values().copied().collect();
        }
    }

    /// Remove obsolete entries before inspecting the next callback.
    fn discard_stale(&mut self) {
        while self
            .nodes
            .peek()
            .is_some_and(|pending| self.deadlines.get(&pending.stamp.node) != Some(pending))
        {
            self.nodes.pop();
        }
    }

    /// Return the earliest authoritative deadline.
    fn next_deadline(&mut self) -> Option<Instant> {
        self.discard_stale();
        self.nodes.peek().map(|pending| pending.deadline)
    }

    /// Remove and return every callback due at the supplied instant.
    fn collect(&mut self, now: Instant) -> Vec<WorkStamp> {
        let mut due = Vec::new();
        loop {
            self.discard_stale();
            let Some(pending) = self.nodes.peek() else {
                break;
            };
            if pending.deadline > now {
                break;
            }
            let pending = self.nodes.pop().expect("pending deadline exists");
            if self.deadlines.remove(&pending.stamp.node) == Some(pending) {
                due.push(pending.stamp);
            }
        }
        self.compact();
        due
    }
}

/// Poll deadlines serviced by the application driver without a worker thread.
#[derive(Debug)]
pub struct Poller {
    /// Scheduled callbacks owned by this application.
    pending: PendingHeap,
    /// Time source shared with driver deadline calculations.
    clock: Arc<dyn Clock>,
}

impl Poller {
    /// Construct a scheduler using the system monotonic clock.
    pub(crate) fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// Construct a scheduler using an explicit monotonic clock.
    pub(crate) fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            pending: PendingHeap::default(),
            clock,
        }
    }

    /// Install a testing clock before any pending deadline exists.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn set_clock(&mut self, clock: Arc<dyn Clock>) -> Result<()> {
        if !self.pending.deadlines.is_empty() {
            return Err(Error::InvalidOperation(
                "cannot replace a clock with pending poll deadlines".into(),
            ));
        }
        self.clock = clock;
        Ok(())
    }

    /// Return the scheduler's current time.
    pub(crate) fn now(&self) -> Instant {
        self.clock.now()
    }

    /// Replace the pending callback for this owner.
    pub(crate) fn schedule(&mut self, stamp: WorkStamp, duration: Duration) -> Result<()> {
        let deadline = self
            .now()
            .checked_add(duration)
            .ok_or_else(|| Error::RunLoop("poll deadline overflow".into()))?;
        self.pending.schedule(stamp, deadline);
        Ok(())
    }

    /// Cancel work belonging to the specified widget incarnation.
    pub(crate) fn cancel_owner(&mut self, node: NodeId, incarnation: u64) {
        if self
            .pending
            .deadlines
            .get(&node)
            .is_some_and(|pending| pending.stamp.incarnation == incarnation)
        {
            self.pending.deadlines.remove(&node);
            self.pending.compact();
        }
    }

    /// Cancel work whose lifetime is no longer valid after a structural commit.
    pub(crate) fn retain(&mut self, mut valid: impl FnMut(WorkStamp) -> bool) {
        let expired: Vec<_> = self
            .pending
            .deadlines
            .values()
            .filter(|pending| !valid(pending.stamp))
            .map(|pending| (pending.stamp.node, pending.stamp.incarnation))
            .collect();
        for (node, incarnation) in expired {
            self.cancel_owner(node, incarnation);
        }
    }

    /// Return the earliest pending deadline for adapter waiting.
    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        self.pending.next_deadline()
    }

    /// Remove callbacks due under the injected clock.
    pub(crate) fn collect_due(&mut self) -> Vec<WorkStamp> {
        let now = self.now();
        self.pending.collect(now)
    }
}

#[cfg(test)]
mod tests {
    use slotmap::SlotMap;

    use super::*;
    use crate::testing::ManualClock;

    fn stamp() -> WorkStamp {
        let mut nodes: SlotMap<NodeId, ()> = SlotMap::with_key();
        WorkStamp {
            node: nodes.insert(()),
            incarnation: 1,
            attachment: None,
        }
    }

    #[test]
    fn rescheduling_and_cancellation_use_latest_incarnation() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut poller = Poller::with_clock(clock.clone());
        let old = stamp();
        poller.schedule(old, Duration::from_secs(10))?;
        let current = WorkStamp {
            incarnation: 2,
            ..old
        };
        poller.schedule(current, Duration::from_secs(20))?;
        poller.cancel_owner(old.node, old.incarnation);
        clock.advance(Duration::from_secs(15))?;
        assert!(poller.collect_due().is_empty());
        clock.advance(Duration::from_secs(5))?;
        assert_eq!(poller.collect_due(), [current]);
        assert_eq!(poller.next_deadline(), None);
        Ok(())
    }

    #[test]
    fn repeated_rescheduling_bounds_heap_storage() -> Result<()> {
        let mut poller = Poller::new();
        let owner = stamp();
        for delay in 0..10_000 {
            poller.schedule(owner, Duration::from_secs(delay))?;
            assert!(poller.pending.nodes.len() <= 2);
        }
        poller.cancel_owner(owner.node, owner.incarnation);
        assert!(poller.pending.nodes.is_empty());
        assert_eq!(poller.next_deadline(), None);
        Ok(())
    }

    #[test]
    fn lifetime_cancellation_discards_due_work() -> Result<()> {
        let mut poller = Poller::new();
        let owner = WorkStamp {
            attachment: Some(4),
            ..stamp()
        };
        poller.schedule(owner, Duration::ZERO)?;
        poller.retain(|stamp| stamp.attachment.is_none());
        assert!(poller.collect_due().is_empty());
        Ok(())
    }
}
