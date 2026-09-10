//! Incarnation-aware, coalesced notifications for background widget producers.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
};

use futures::task::AtomicWaker;

use crate::{
    NodeId,
    error::{Error, Result},
};

/// Lifetime of runtime-managed widget work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkLifetime {
    /// Continue while the widget exists, including while hidden or detached.
    #[default]
    Node,
    /// End when the widget is detached, replaced, or removed.
    Attachment,
}

/// Result of requesting a poll through a node wake handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeOutcome {
    /// The owner now has pending work.
    Queued,
    /// The owner already had pending work.
    Coalesced,
    /// The owning widget, attachment, or application no longer exists.
    Expired,
}

/// Identity captured by a scheduled callback or wake handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkStamp {
    /// Arena node that owns the work.
    pub(crate) node: NodeId,
    /// Widget identity independent of the arena slot generation.
    pub(crate) incarnation: u64,
    /// Required attachment generation, absent for node lifetime.
    pub(crate) attachment: Option<u64>,
}

impl WorkStamp {
    /// Capture the same widget with node lifetime.
    fn node_lifetime(self) -> Self {
        Self {
            attachment: None,
            ..self
        }
    }
}

/// Shared registrations and pending flags, with no queue of producer results.
#[derive(Default, Debug)]
struct WakeState {
    /// Provisional and committed lifetime registrations, each with a pending
    /// bit.
    registrations: HashMap<WorkStamp, bool>,
}

/// Shared notification state retained by the application registry.
#[derive(Default, Debug)]
struct SharedWake {
    /// Registrations synchronized with lifetime commits and producer wakes.
    state: Mutex<WakeState>,
    /// Single driver task awaiting pending owner work.
    notified: AtomicWaker,
}

/// Thread-safe handle that requests a poll of one widget incarnation.
///
/// Producers keep results in their own bounded channels. This handle never owns
/// the application or widget and expires when its registered lifetime ends.
#[derive(Clone, Debug)]
pub struct NodeWakeHandle {
    /// Lifetime captured when this handle was acquired.
    stamp: WorkStamp,
    /// Non-owning reference that expires when the registry is dropped.
    shared: Weak<SharedWake>,
}

impl NodeWakeHandle {
    /// Queue one owner poll, coalesce with existing work, or report expiration.
    pub fn wake(&self) -> Result<WakeOutcome> {
        let Some(shared) = self.shared.upgrade() else {
            return Ok(WakeOutcome::Expired);
        };
        let mut state = shared.state.lock().map_err(|_| poisoned_registry())?;
        if !state.registrations.contains_key(&self.stamp) {
            return Ok(WakeOutcome::Expired);
        }
        let coalesced = state.registrations.iter().any(|(stamp, pending)| {
            *pending && stamp.node == self.stamp.node && stamp.incarnation == self.stamp.incarnation
        });
        state.registrations.insert(self.stamp, true);
        drop(state);
        if !coalesced {
            shared.notified.wake();
        }
        Ok(if coalesced {
            WakeOutcome::Coalesced
        } else {
            WakeOutcome::Queued
        })
    }
}

/// Driver notification registry, bounded by registered owner lifetimes.
///
/// A pending flag and one registered task waker replace an event queue. During
/// a structural transaction old and provisional registrations coexist; `sync`
/// expires whichever registrations the settled tree no longer owns.
#[derive(Clone, Default, Debug)]
pub struct WakeRegistry {
    /// Shared state owned by the application and adapter subscriptions.
    shared: Arc<SharedWake>,
}

impl WakeRegistry {
    /// Create a handle after the caller validates its current widget identity.
    ///
    /// The registration is provisional until the enclosing tree edit settles.
    pub(crate) fn handle(&self, stamp: WorkStamp) -> Result<NodeWakeHandle> {
        self.shared
            .state
            .lock()
            .map_err(|_| poisoned_registry())?
            .registrations
            .entry(stamp)
            .or_insert(false);
        Ok(NodeWakeHandle {
            stamp,
            shared: Arc::downgrade(&self.shared),
        })
    }

    /// Reconcile registrations with a committed or restored node arena.
    ///
    /// Each supplied stamp represents the node's current incarnation and
    /// optional attachment. Node-lifetime work remains valid when
    /// attachment is absent.
    pub(crate) fn sync(&self, owners: impl IntoIterator<Item = WorkStamp>) -> Result<()> {
        let active: HashSet<_> = owners
            .into_iter()
            .flat_map(|stamp| [stamp.node_lifetime(), stamp])
            .collect();
        let mut state = self.shared.state.lock().map_err(|_| poisoned_registry())?;
        state
            .registrations
            .retain(|stamp, _| active.contains(stamp));
        for stamp in active {
            state.registrations.entry(stamp).or_insert(false);
        }
        Ok(())
    }

    /// Expire every handle when the application is destroyed.
    pub(crate) fn close(&self) -> Result<()> {
        self.shared
            .state
            .lock()
            .map_err(|_| poisoned_registry())?
            .registrations
            .clear();
        self.shared.notified.wake();
        Ok(())
    }

    /// Drain at most one callback per owning widget incarnation.
    pub(crate) fn drain(&self) -> Result<Vec<WorkStamp>> {
        let mut state = self.shared.state.lock().map_err(|_| poisoned_registry())?;
        let mut pending = Vec::new();
        for (stamp, queued) in &mut state.registrations {
            if *queued {
                pending.push(*stamp);
                *queued = false;
            }
        }
        // Node lifetime sorts before attachment lifetime. Prefer it when both
        // producers woke the owner, so attachment expiration cannot discard it.
        pending.sort_unstable();
        pending.dedup_by_key(|stamp| (stamp.node, stamp.incarnation));
        Ok(pending)
    }

    /// Register the driver's task and report whether owner work is pending.
    pub(crate) fn poll_notified(&self, cx: &Context<'_>) -> Poll<Result<()>> {
        self.shared.notified.register(cx.waker());
        match self.shared.state.lock() {
            Ok(state) if state.registrations.values().any(|pending| *pending) => {
                Poll::Ready(Ok(()))
            }
            Ok(_) => Poll::Pending,
            Err(_) => Poll::Ready(Err(poisoned_registry())),
        }
    }
}

/// Preserve a synchronization failure as a runtime error.
fn poisoned_registry() -> Error {
    Error::RunLoop("node wake registry lock poisoned".to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc::sync_channel,
        },
        thread,
    };

    use futures::task::{ArcWake, waker};

    use super::*;
    use crate::core::id::testing_node_id;

    fn owner() -> WorkStamp {
        WorkStamp {
            node: testing_node_id(),
            incarnation: 1,
            attachment: Some(1),
        }
    }

    #[test]
    fn repeated_wakes_coalesce_without_losing_node_lifetime_work() -> Result<()> {
        let registry = WakeRegistry::default();
        let owner = owner();
        let attachment = registry.handle(owner)?;
        let node = registry.handle(owner.node_lifetime())?;
        assert_eq!(attachment.wake()?, WakeOutcome::Queued);
        for _ in 0..10_000 {
            assert_eq!(node.wake()?, WakeOutcome::Coalesced);
        }
        registry.sync([owner.node_lifetime()])?;
        assert_eq!(attachment.wake()?, WakeOutcome::Expired);
        assert_eq!(registry.drain()?, [owner.node_lifetime()]);
        assert!(registry.drain()?.is_empty());
        Ok(())
    }

    #[test]
    fn provisional_replacement_preserves_concurrent_old_wake_on_rollback() -> Result<()> {
        let registry = WakeRegistry::default();
        let old = owner();
        let original = registry.handle(old)?;
        let replacement = registry.handle(WorkStamp {
            incarnation: 2,
            ..old
        })?;
        assert_eq!(replacement.wake()?, WakeOutcome::Queued);
        assert_eq!(original.wake()?, WakeOutcome::Queued);
        registry.sync([old])?;
        assert_eq!(replacement.wake()?, WakeOutcome::Expired);
        assert_eq!(registry.drain()?, [old]);
        Ok(())
    }

    #[test]
    fn committed_replacement_expires_old_wake_and_keeps_mount_wake() -> Result<()> {
        let registry = WakeRegistry::default();
        let old = owner();
        let new = WorkStamp {
            incarnation: 2,
            attachment: Some(2),
            ..old
        };
        let original = registry.handle(old)?;
        let replacement = registry.handle(new)?;
        original.wake()?;
        replacement.wake()?;
        registry.sync([new])?;
        assert_eq!(original.wake()?, WakeOutcome::Expired);
        assert_eq!(registry.drain()?, [new]);
        drop(registry);
        assert_eq!(replacement.wake()?, WakeOutcome::Expired);
        Ok(())
    }

    #[test]
    fn producer_owns_bounded_results_and_expired_handle_does_not_queue() -> Result<()> {
        let registry = WakeRegistry::default();
        let handle = registry.handle(owner())?;
        let (sender, receiver) = sync_channel(1);
        sender.try_send("result").unwrap();
        assert!(sender.try_send("extra").is_err());
        registry.sync([])?;
        assert_eq!(handle.wake()?, WakeOutcome::Expired);
        assert!(registry.drain()?.is_empty());
        assert_eq!(receiver.try_recv().unwrap(), "result");
        Ok(())
    }

    #[test]
    fn detached_owner_expires_a_worker_before_result_delivery() -> Result<()> {
        let registry = WakeRegistry::default();
        let owner = owner();
        let handle = registry.handle(owner)?;
        let (start, ready) = sync_channel(0);
        let (result, receiver) = sync_channel(1);
        let worker = thread::spawn(move || {
            ready.recv().expect("start signal");
            result
                .send("worker result")
                .expect("bounded result receiver");
            handle.wake().expect("worker wake result")
        });
        registry.sync([owner.node_lifetime()])?;
        start.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), WakeOutcome::Expired);
        assert_eq!(receiver.try_recv().unwrap(), "worker result");
        assert!(registry.drain()?.is_empty());
        Ok(())
    }

    struct NotificationCount(AtomicUsize);

    impl ArcWake for NotificationCount {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            arc_self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn driver_registration_observes_wakes_before_and_after_parking() -> Result<()> {
        let registry = WakeRegistry::default();
        let handle = registry.handle(owner())?;
        let count = Arc::new(NotificationCount(AtomicUsize::new(0)));
        let waker = waker(count.clone());
        let cx = Context::from_waker(&waker);
        assert!(registry.poll_notified(&cx).is_pending());
        handle.wake()?;
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert!(matches!(registry.poll_notified(&cx), Poll::Ready(Ok(()))));
        registry.drain()?;
        assert!(registry.poll_notified(&cx).is_pending());
        Ok(())
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn wake_handle_is_send_and_sync() {
        assert_send_sync::<NodeWakeHandle>();
    }
}
