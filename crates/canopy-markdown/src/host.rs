//! Immutable, explicitly activated host actions and bounded reply tracking.

use std::{collections::BTreeMap, mem, sync::Arc};

use image::RgbaImage;

use crate::document::LocalLink;

/// An effect the embedding application may perform once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAction {
    /// Copy this complete payload to the host clipboard.
    CopyText(String),
    /// Open this validated, resolved URL through the host browser service.
    OpenUrl(String),
    /// Follow a link to a document of the host's own, which the host finds.
    OpenLocal(LocalLink),
}

/// What a resource request asks the host for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// An image an image block shows.
    Image,
    /// The text of a note an embed shows; the reader takes the section its
    /// anchor names.
    Note,
    /// Whether a local link names anything, so a link that names nothing can
    /// show faint.
    Link,
}

/// A resource the document needs from the host. Unlike a [`HostAction`], a
/// reader may need a resource again, so resources travel on their own
/// requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRequest {
    /// Identifier the reply names; never reused.
    pub id: u64,
    /// The document epoch that needs it.
    pub epoch: u64,
    /// The document of the host's own that holds it.
    pub target: LocalLink,
    /// The link destination as the document writes it. A host names it to
    /// [`Markdown::recheck_destinations`](crate::Markdown::recheck_destinations).
    pub destination: String,
    /// What the reader asks for.
    pub kind: ResourceKind,
}

/// The host's answer to a resource request.
#[derive(Clone, Debug)]
pub enum Resource {
    /// The image, which the host bounds in size.
    Image(Arc<RgbaImage>),
    /// The note's text, which the host bounds in size.
    Text(String),
    /// What a link check names exists.
    Found,
    /// Nothing the request names exists.
    Missing,
    /// The host could not supply it, and why.
    Failed(String),
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

/// Where a reserved request stands.
#[derive(Debug)]
enum Stage {
    /// Reserved while its action, such as a copy, prepares.
    Reserved,
    /// Ready for the host.
    Ready(HostRequest),
    /// Drained by the host, and awaiting its reply.
    Delivered,
}

/// A bounded queue with exactly-once draining and duplicate-reply rejection.
#[derive(Default)]
pub(crate) struct Requests {
    /// Last allocated identity.
    next: u64,
    /// Each request that awaits completion, in activation order, with where
    /// it stands.
    pending: BTreeMap<u64, (ActionContext, Stage)>,
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
        self.pending.insert(context.id, (context, Stage::Reserved));
        Some(context)
    }

    /// Finish a reserved request. Returns false after replacement or
    /// cancellation, or for a request already finished.
    pub fn publish(&mut self, id: u64, action: HostAction) -> bool {
        let Some((context, stage @ Stage::Reserved)) = self.pending.get_mut(&id) else {
            return false;
        };
        *stage = Stage::Ready(HostRequest {
            id,
            epoch: context.epoch,
            revision: context.revision,
            object: context.object,
            action,
        });
        true
    }

    /// Forget failed serialization without reporting a successful copy.
    pub fn cancel(&mut self, id: u64) {
        self.pending.remove(&id);
    }

    /// Remove each action at most once for the host to execute, in
    /// activation order: a request still reserved holds back those after it.
    pub fn take(&mut self, limit: usize) -> Vec<HostRequest> {
        let mut output = Vec::new();
        for (_, stage) in self
            .pending
            .values_mut()
            .filter(|(_, stage)| !matches!(stage, Stage::Delivered))
            .take(limit)
        {
            match mem::replace(stage, Stage::Delivered) {
                Stage::Ready(request) => output.push(request),
                held => {
                    *stage = held;
                    break;
                }
            }
        }
        output
    }

    /// Match one reply; duplicate and canceled replies are ignored.
    pub fn complete(&mut self, id: u64) -> Option<ActionContext> {
        if !matches!(self.pending.get(&id), Some((_, Stage::Delivered))) {
            return None;
        }
        self.pending.remove(&id).map(|(context, _)| context)
    }

    /// Cancel undelivered work at document replacement, preserving the
    /// identifier sequence.
    pub fn replace(&mut self) {
        self.pending.clear();
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
    fn each_event_moves_each_stage() {
        let action = || HostAction::CopyText("text".into());
        // A reserved request publishes once, and is taken once, and its reply
        // completes it once.
        let mut requests = Requests::default();
        let id = requests.reserve(1, 1, None, 4).unwrap().id;
        assert!(
            requests.complete(id).is_none(),
            "a reserved request has no reply"
        );
        assert!(requests.publish(id, action()));
        assert!(
            !requests.publish(id, action()),
            "a ready request publishes once"
        );
        assert!(
            requests.complete(id).is_none(),
            "a ready request has no reply"
        );
        assert_eq!(requests.take(4).len(), 1);
        assert!(
            !requests.publish(id, action()),
            "a delivered request publishes once"
        );
        assert!(
            requests.take(4).is_empty(),
            "a delivered request is taken once"
        );
        assert_eq!(requests.complete(id).map(|context| context.id), Some(id));
        assert!(requests.complete(id).is_none(), "a reply completes once");
        // A cancel in any stage forgets the request.
        for stage in 0..3 {
            let id = requests.reserve(1, 1, None, 4).unwrap().id;
            if stage > 0 {
                requests.publish(id, action());
            }
            if stage > 1 {
                requests.take(4);
            }
            requests.cancel(id);
            assert!(!requests.publish(id, action()), "stage {stage}");
            assert!(requests.take(4).is_empty(), "stage {stage}");
            assert!(requests.complete(id).is_none(), "stage {stage}");
        }
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
