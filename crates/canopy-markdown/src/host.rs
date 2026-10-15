//! Immutable, explicitly activated host actions and bounded reply tracking.

use std::collections::{BTreeMap, BTreeSet};

/// An effect the embedding application may perform once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAction {
    /// Copy this complete payload to the host clipboard.
    CopyText(String),
    /// Open this validated, resolved URL through the host browser service.
    OpenUrl(String),
}

/// A unique host action, frozen at the displayed revision of its activation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostRequest {
    /// Monotonic identifier, not reused on document replacement.
    pub id: u64,
    /// The document epoch at activation.
    pub epoch: u64,
    /// The displayed source revision at activation.
    pub revision: u64,
    /// Selected object identity, if this came from an object.
    pub object: Option<u64>,
    /// One complete immutable effect.
    pub action: HostAction,
}

/// The result of one host request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostOutcome {
    /// The host performed the action.
    Success,
    /// Host policy declined the action.
    Rejected(String),
    /// The host attempted the action but it failed.
    Failed(String),
}

/// A reply to a request that the host has already drained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostReply {
    /// Request being acknowledged.
    pub id: u64,
    /// Success, rejection, or failure.
    pub outcome: HostOutcome,
}

/// Immutable context reserved before asynchronous copy serialization.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ActionContext {
    /// Request identity.
    pub id: u64,
    /// Epoch at activation.
    pub epoch: u64,
    /// Revision at activation.
    pub revision: u64,
    /// Object at activation.
    pub object: Option<u64>,
}

/// A bounded queue with exactly-once draining and duplicate-reply rejection.
#[derive(Default)]
pub(crate) struct Requests {
    /// Last allocated identity.
    next: u64,
    /// Requests ready for the host.
    ready: BTreeMap<u64, HostRequest>,
    /// Drained requests awaiting host acknowledgement.
    delivered: BTreeSet<u64>,
    /// Reserved, ready, and delivered requests that still await completion.
    pending: BTreeMap<u64, ActionContext>,
}

impl Requests {
    /// Reserve a slot before performing copy work.
    pub fn reserve(
        &mut self,
        epoch: u64,
        revision: u64,
        object: Option<u64>,
        limit: usize,
    ) -> Option<ActionContext> {
        if self.pending.len() >= limit {
            return None;
        }
        self.next = self.next.checked_add(1)?;
        let context = ActionContext {
            id: self.next,
            epoch,
            revision,
            object,
        };
        self.pending.insert(context.id, context);
        Some(context)
    }

    /// Finish a reserved request. Returns false after replacement or
    /// cancellation.
    pub fn publish(&mut self, id: u64, action: HostAction) -> bool {
        let Some(context) = self.pending.get(&id) else {
            return false;
        };
        if self.delivered.contains(&id) || self.ready.contains_key(&id) {
            return false;
        }
        self.ready.insert(
            id,
            HostRequest {
                id,
                epoch: context.epoch,
                revision: context.revision,
                object: context.object,
                action,
            },
        );
        true
    }

    /// Forget failed serialization without reporting a successful copy.
    pub fn cancel(&mut self, id: u64) {
        self.pending.remove(&id);
        self.ready.remove(&id);
        self.delivered.remove(&id);
    }

    /// Remove each action at most once for the host to execute.
    pub fn take(&mut self, limit: usize) -> Vec<HostRequest> {
        let mut output = Vec::new();
        for id in self
            .pending
            .keys()
            .filter(|id| !self.delivered.contains(id))
            .copied()
            .take(limit)
            .collect::<Vec<_>>()
        {
            let Some(request) = self.ready.remove(&id) else {
                break;
            };
            self.delivered.insert(id);
            output.push(request);
        }
        output
    }

    /// Match one reply; duplicate and canceled replies are ignored.
    pub fn complete(&mut self, id: u64) -> Option<ActionContext> {
        if !self.delivered.remove(&id) {
            return None;
        }
        self.pending.remove(&id)
    }

    /// Cancel undelivered work at document replacement, preserving the
    /// identifier sequence.
    pub fn replace(&mut self) {
        self.ready.clear();
        self.pending.clear();
        self.delivered.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_preserve_activation_order_across_async_copy() {
        let mut requests = Requests::default();
        let copy = requests.reserve(1, 1, None, 3).unwrap();
        let open = requests.reserve(1, 1, None, 3).unwrap();
        requests.publish(open.id, HostAction::OpenUrl("https://example.com".into()));
        assert!(requests.take(3).is_empty());
        assert!(requests.complete(copy.id).is_none());
        requests.publish(copy.id, HostAction::CopyText("frozen".into()));
        assert_eq!(
            requests.take(3).iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![copy.id, open.id]
        );
        assert!(!requests.publish(copy.id, HostAction::CopyText("duplicate".into())));
        let cancelled = requests.reserve(1, 2, None, 3).unwrap();
        requests.cancel(cancelled.id);
        assert!(requests.take(3).is_empty());
    }

    #[test]
    fn queue_bounds_draining_and_late_replies() {
        let mut requests = Requests::default();
        let first = requests.reserve(1, 2, None, 1).unwrap();
        assert!(requests.reserve(1, 3, None, 1).is_none());
        assert!(requests.publish(first.id, HostAction::CopyText("exact\r\n".into())));
        assert_eq!(requests.take(1).len(), 1);
        assert!(requests.take(1).is_empty());
        assert!(requests.complete(first.id).is_some());
        assert!(requests.complete(first.id).is_none());
        let second = requests.reserve(1, 3, None, 1).unwrap();
        requests.replace();
        assert!(!requests.publish(second.id, HostAction::CopyText("stale".into())));
        assert!(requests.complete(second.id).is_none());
        assert!(requests.reserve(2, 1, None, 1).unwrap().id > second.id);
    }
}
