//! Dispatch boundaries, posted calls, deferred removals and modal closes, and
//! wake handles.
//!
//! Work that a callback cannot do while widget cells are out waits in one FIFO
//! batch, and the batch drains when the outermost dispatch boundary succeeds.
//! The drain pops one request at a time, and a posted call's dispatch opens its
//! own checkpoint after the pop, so the batch's length checkpoints stay valid
//! under four invariants:
//!
//! - The drain pops the next request only after the current one, and every edit
//!   nested in it, has finished.
//! - A nested `finish_dispatch` neither drains nor clears the shared queue.
//! - No drain starts while a tree edit is open, because a tree edit runs inside
//!   a callback or a dispatch boundary.
//! - The drain restores `draining` on every exit.

use std::{collections::VecDeque, mem};

use super::Core;
use crate::{
    NodeId,
    commands::{self, CommandCall, CommandTarget},
    core::{notice::NoticeSource, wake::PollOwner},
    error::{Error, Result},
    input::{Event, ModalToken},
    runtime::{NodeWakeHandle, PollLifetime},
};

/// Maximum retained completion requests across one outer dispatch and its
/// nested callbacks.
const MAX_COMPLETION_REQUESTS: usize = 1024;

/// Maximum posted calls one drain runs. A posted command that posts itself
/// again would otherwise never let the drain end.
pub const MAX_POSTED_CALLS: usize = 1024;

/// One queued removal tied to the widget that received the request.
pub(super) struct RemovalRequest {
    /// Arena node to remove.
    node: NodeId,
    /// Widget incarnation when queued.
    incarnation: u64,
}

/// One posted call, resolved when it was posted.
pub(super) struct PostRequest {
    /// Node that posted the call, which a failure's notice names.
    origin: NodeId,
    /// Node the call resolved to.
    target: NodeId,
    /// Target widget incarnation when posted.
    incarnation: u64,
    /// The call, bound to exactly the target.
    call: CommandCall,
    /// Event in scope when posted, which the call's injections read.
    event: Option<Event>,
    /// Kind of handler that was running when posted, which a failure's
    /// notice names.
    source: Option<NoticeSource>,
}

/// Where the posted call that stopped a drain came from.
#[derive(Clone, Copy, Debug)]
pub struct PostFailure {
    /// Node that posted the call.
    pub(crate) origin: NodeId,
    /// Kind of handler that posted it, if a known kind was running.
    pub(crate) source: Option<NoticeSource>,
}

/// Mutation deferred until the outer callback boundary completes.
pub(super) enum CompletionRequest {
    /// Run a posted call.
    Post(PostRequest),
    /// Remove a widget incarnation.
    Remove(RemovalRequest),
    /// Close a modal and all its descendants.
    CloseModal(ModalToken),
}

impl PostRequest {
    /// Return where this call came from, for a failure's notice.
    fn failure(&self) -> PostFailure {
        PostFailure {
            origin: self.origin,
            source: self.source,
        }
    }
}

impl CompletionRequest {
    /// Name the deferred work for diagnostics.
    fn label(&self) -> &'static str {
        match self {
            Self::Post(_) => "posted call",
            Self::Remove(_) => "removal",
            Self::CloseModal(_) => "modal close",
        }
    }
}

/// Pending completion requests and dispatch nesting state.
#[derive(Default)]
pub(super) struct CompletionBatch {
    /// FIFO requests from active dispatches.
    pub(super) requests: VecDeque<CompletionRequest>,
    /// Active explicit dispatch boundaries.
    depth: usize,
    /// Whether a drain loop runs, so a boundary that finishes inside it does
    /// not start another.
    draining: bool,
    /// Lifecycle hooks running. No work can be queued while any runs.
    sealed: usize,
    /// The posted call whose failure stopped the last drain.
    failed: Option<PostFailure>,
    /// Kinds of the running handlers, innermost last.
    notice_sources: Vec<NoticeSource>,
}

impl Core {
    /// Test whether scheduled work still belongs to the same live widget or
    /// attachment.
    pub(crate) fn work_stamp_valid(&self, stamp: PollOwner) -> bool {
        self.nodes.get(stamp.node).is_some_and(|node| {
            node.incarnation == stamp.incarnation
                && stamp
                    .attachment
                    .is_none_or(|generation| node.attachment_generation == Some(generation))
        })
    }

    /// Return the owner of a live node's work for the requested lifetime.
    ///
    /// Attachment-bound work on a detached node has no owner and returns
    /// `None`.
    pub(crate) fn work_stamp(
        &self,
        node: NodeId,
        lifetime: PollLifetime,
    ) -> Result<Option<PollOwner>> {
        let entry = self.nodes.get(node).ok_or(Error::NodeNotFound(node))?;
        let attachment = match (lifetime, entry.attachment_generation) {
            (PollLifetime::Node, _) => None,
            (PollLifetime::Attachment, Some(generation)) => Some(generation),
            (PollLifetime::Attachment, None) => return Ok(None),
        };
        Ok(Some(PollOwner {
            node,
            incarnation: entry.incarnation,
            attachment,
        }))
    }

    /// Capture a wake handle for the requested lifetime of a live node.
    pub(crate) fn wake_handle(
        &self,
        node: NodeId,
        lifetime: PollLifetime,
    ) -> Result<NodeWakeHandle> {
        let stamp = self
            .work_stamp(node, lifetime)?
            .ok_or(Error::NodeDetached(node))?;
        self.wake_registry.handle(stamp)
    }

    /// Start a dispatch boundary and return its queue checkpoint.
    pub(crate) fn begin_dispatch(&mut self) -> usize {
        self.completion.depth += 1;
        self.completion.requests.len()
    }

    /// Restore a failed dispatch checkpoint or drain the successful outer
    /// boundary.
    pub(crate) fn finish_dispatch(&mut self, checkpoint: usize, success: bool) -> Result<()> {
        if !success {
            self.completion.requests.truncate(checkpoint);
        }
        self.completion.depth = self
            .completion
            .depth
            .checked_sub(1)
            .expect("dispatch completion requires an active boundary");
        if success && self.completion.depth == 0 && self.callback_depth == 0 {
            self.drain_completion()?;
        }
        Ok(())
    }

    /// Queue removal of this widget incarnation after callbacks return.
    ///
    /// A node that no longer exists needs no removal.
    pub(crate) fn remove_after_dispatch(&mut self, node: NodeId) -> Result<()> {
        let Some(entry) = self.nodes.get(node) else {
            return Ok(());
        };
        self.enqueue_completion(CompletionRequest::Remove(RemovalRequest {
            node,
            incarnation: entry.incarnation,
        }))
    }

    /// Close a modal and its nested scopes after callbacks return, with
    /// the same failure checkpoint as node removal.
    pub(crate) fn close_modal_after_dispatch(&mut self, token: ModalToken) -> Result<()> {
        self.enqueue_completion(CompletionRequest::CloseModal(token))
    }

    /// Post `call` from `origin`, to run when the outermost dispatch
    /// completes.
    ///
    /// The target resolves and the arguments are checked now, so a bad call
    /// fails here. The call keeps the event in scope for its injections.
    pub(crate) fn post(&mut self, origin: NodeId, call: &CommandCall) -> Result<()> {
        let target = commands::resolve_posted(self, origin, call)?;
        let incarnation = self.nodes[target].incarnation;
        self.enqueue_completion(CompletionRequest::Post(PostRequest {
            origin,
            target,
            incarnation,
            call: call.clone().with_target(CommandTarget::Exact(target)),
            event: self.current_event().cloned(),
            source: self.completion.notice_sources.last().copied(),
        }))
    }

    /// Run `hook`, a lifecycle hook, with the batch sealed against new work.
    pub(crate) fn sealed<R>(&mut self, hook: impl FnOnce(&mut Self) -> R) -> R {
        self.completion.sealed += 1;
        let result = hook(self);
        self.completion.sealed -= 1;
        result
    }

    /// Return how many requests wait in the batch.
    #[cfg(test)]
    pub(crate) fn queued_completions(&self) -> usize {
        self.completion.requests.len()
    }

    /// Take the posted call whose failure stopped the last drain, if a posted
    /// call stopped it.
    pub(crate) fn take_failed_post(&mut self) -> Option<PostFailure> {
        self.completion.failed.take()
    }

    /// Note that a handler of kind `source` runs, and return the depth to
    /// restore when it returns. A call it posts keeps the kind, so the call's
    /// failure reads as the handler's own.
    pub(crate) fn push_notice_source(&mut self, source: NoticeSource) -> usize {
        let depth = self.completion.notice_sources.len();
        self.completion.notice_sources.push(source);
        depth
    }

    /// Restore the running handler kinds to `depth`.
    pub(crate) fn pop_notice_source(&mut self, depth: usize) {
        self.completion.notice_sources.truncate(depth);
    }

    /// Admit a completion request unless lifecycle cleanup forbids new work or
    /// the batch is full, then drain the outer boundary when no dispatch
    /// remains open.
    fn enqueue_completion(&mut self, request: CompletionRequest) -> Result<()> {
        if self.completion.sealed > 0 || self.rolling_back_tree_edit {
            return Err(Error::Invalid(format!(
                "cannot queue {} during lifecycle cleanup",
                request.label()
            )));
        }
        if self.completion.requests.len() >= MAX_COMPLETION_REQUESTS {
            return Err(Error::Invalid(format!(
                "dispatch completion batch exceeds {MAX_COMPLETION_REQUESTS} requests"
            )));
        }
        self.completion.requests.push_back(request);
        if self.completion.depth == 0 && self.callback_depth == 0 {
            self.drain_completion()?;
        }
        Ok(())
    }

    /// Apply the current FIFO batch, stopping and discarding the tail on
    /// failure.
    fn drain_completion(&mut self) -> Result<()> {
        if self.completion.draining {
            return Ok(());
        }
        self.completion.draining = true;
        self.completion.failed = None;
        let result = self.drain_completion_inner();
        self.completion.requests.clear();
        self.completion.draining = false;
        result
    }

    /// Apply requests in FIFO order. A posted call or a removal applies only
    /// while its node's widget incarnation still matches the request.
    fn drain_completion_inner(&mut self) -> Result<()> {
        let mut posted = 0;
        while let Some(request) = self.completion.requests.pop_front() {
            match request {
                CompletionRequest::Post(request) => {
                    if posted == MAX_POSTED_CALLS {
                        self.completion.failed = Some(request.failure());
                        return Err(Error::Invalid(format!(
                            "one dispatch posted more than {MAX_POSTED_CALLS} calls; \
                             a posted command may be posting itself"
                        )));
                    }
                    posted += 1;
                    if self.is_incarnation(request.target, request.incarnation) {
                        let failure = request.failure();
                        self.deliver(request).inspect_err(|_| {
                            self.completion.failed = Some(failure);
                        })?;
                    }
                }
                CompletionRequest::CloseModal(token) => self.close_modal_now(token)?,
                CompletionRequest::Remove(request) => {
                    if self.is_incarnation(request.node, request.incarnation) {
                        self.remove_subtree(request.node)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Return whether `node` still holds the widget incarnation `incarnation`.
    fn is_incarnation(&self, node: NodeId, incarnation: u64) -> bool {
        self.nodes
            .get(node)
            .is_some_and(|entry| entry.incarnation == incarnation)
    }

    /// Run a posted call in exactly the event it was posted in.
    ///
    /// The call's dispatch is its own boundary: a failure discards what the
    /// call queued, and a success leaves its work for this drain to pick up.
    fn deliver(&mut self, request: PostRequest) -> Result<()> {
        let outer = mem::replace(&mut self.event_scope, request.event.into_iter().collect());
        let result = commands::dispatch(self, request.target, &request.call);
        self.event_scope = outer;
        result.map(|_| ()).map_err(Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Context, Widget,
        runtime::{PollLifetime, WakeOutcome},
    };

    struct Leaf {
        veto: bool,
    }

    impl Widget for Leaf {
        fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> {
            if self.veto {
                Err(Error::Invalid("removal veto".into()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn admission_overflow_fails_the_callback_without_removing_queued_nodes() -> Result<()> {
        let mut core = Core::new();
        let trigger = core.create_detached(Leaf { veto: false })?;
        let target = core.create_detached(Leaf { veto: false })?;
        let checkpoint = core.begin_dispatch();
        let result = core.with_widget_ctx(trigger, |_widget, ctx| {
            for _ in 0..=MAX_COMPLETION_REQUESTS {
                ctx.remove_after_dispatch(target)?;
            }
            Ok::<_, Error>(())
        })?;
        assert!(matches!(&result, Err(Error::Invalid(message))
            if message == "dispatch completion batch exceeds 1024 requests"));
        assert_eq!(core.completion.requests.len(), MAX_COMPLETION_REQUESTS);
        assert!(core.nodes.contains_key(trigger));
        assert!(core.nodes.contains_key(target));
        core.finish_dispatch(checkpoint, result.is_ok())?;
        assert!(core.completion.requests.is_empty());
        let checkpoint = core.begin_dispatch();
        core.finish_dispatch(checkpoint, true)?;
        assert!(core.nodes.contains_key(trigger));
        assert!(core.nodes.contains_key(target));
        Ok(())
    }

    #[test]
    fn nested_failure_discards_only_its_removal_requests() -> Result<()> {
        let mut core = Core::new();
        let first = core.create_detached(Leaf { veto: false })?;
        let second = core.create_detached(Leaf { veto: false })?;
        let outer = core.begin_dispatch();
        core.remove_after_dispatch(first)?;
        let nested = core.begin_dispatch();
        core.remove_after_dispatch(second)?;
        core.finish_dispatch(nested, false)?;
        assert!(core.nodes.contains_key(first));
        core.finish_dispatch(outer, true)?;
        assert!(!core.nodes.contains_key(first));
        assert!(core.nodes.contains_key(second));
        Ok(())
    }

    #[test]
    fn duplicate_ancestor_child_and_replaced_requests_are_safe() -> Result<()> {
        let mut core = Core::new();
        let parent = core.create_detached(Leaf { veto: false })?;
        let child = core.create_detached(Leaf { veto: false })?;
        let replacement = core.create_detached(Leaf { veto: false })?;
        core.attach(parent, child)?;
        let checkpoint = core.begin_dispatch();
        for node in [parent, child, parent, replacement] {
            core.remove_after_dispatch(node)?;
        }
        core.replace_subtree(replacement, Leaf { veto: false })?;
        core.finish_dispatch(checkpoint, true)?;
        assert!(!core.nodes.contains_key(parent));
        assert!(!core.nodes.contains_key(child));
        assert!(core.nodes.contains_key(replacement));
        Ok(())
    }

    #[test]
    fn completion_veto_retains_prior_removals_and_discards_the_tail() -> Result<()> {
        let mut core = Core::new();
        let first = core.create_detached(Leaf { veto: false })?;
        let veto = core.create_detached(Leaf { veto: true })?;
        let last = core.create_detached(Leaf { veto: false })?;
        let checkpoint = core.begin_dispatch();
        for node in [first, veto, last] {
            core.remove_after_dispatch(node)?;
        }
        assert!(core.finish_dispatch(checkpoint, true).is_err());
        assert!(!core.nodes.contains_key(first));
        assert!(core.nodes.contains_key(veto));
        assert!(core.nodes.contains_key(last));
        let checkpoint = core.begin_dispatch();
        core.finish_dispatch(checkpoint, true)?;
        assert!(core.nodes.contains_key(last));
        Ok(())
    }

    struct AttachmentLeaf;

    impl Widget for AttachmentLeaf {
        fn poll_lifetime(&self) -> PollLifetime {
            PollLifetime::Attachment
        }

        fn pre_remove(&mut self, ctx: &mut dyn Context) -> Result<()> {
            assert!(ctx.remove_after_dispatch(ctx.node_id()).is_err());
            Ok(())
        }
    }

    #[test]
    fn attachment_polling_reinitializes_and_cleanup_cannot_enqueue() -> Result<()> {
        let mut core = Core::new();
        let node = core.create_detached(AttachmentLeaf)?;
        core.attach(core.root, node)?;
        core.nodes[node].initialized = true;
        let failure: Result<()> = core.with_tree_edit("failed detach", |core| {
            core.detach(node)?;
            Err(Error::Invalid("abort edit".into()))
        });
        assert!(failure.is_err());
        assert!(core.nodes[node].initialized);
        core.detach(node)?;
        assert!(!core.nodes[node].initialized);
        core.attach(core.root, node)?;
        assert!(!core.nodes[node].initialized);
        let checkpoint = core.begin_dispatch();
        core.remove_after_dispatch(node)?;
        core.finish_dispatch(checkpoint, true)?;
        assert!(!core.nodes.contains_key(node));
        Ok(())
    }

    #[test]
    fn nested_failure_does_not_expire_committed_wakes_before_outer_rollback() -> Result<()> {
        for lifetime in [PollLifetime::Node, PollLifetime::Attachment] {
            let mut core = Core::new();
            let node = core.create_detached(Leaf { veto: false })?;
            core.attach(core.root, node)?;
            let original = core.wake_handle(node, lifetime)?;
            let stamp = PollOwner {
                node,
                incarnation: core.nodes[node].incarnation,
                attachment: match lifetime {
                    PollLifetime::Node => None,
                    PollLifetime::Attachment => core.nodes[node].attachment_generation,
                },
            };
            assert_eq!(original.wake()?, WakeOutcome::Queued);
            let mut provisional = None;
            let result: Result<()> = core.with_tree_edit("outer replacement", |core| {
                core.replace_subtree(node, Leaf { veto: false })?;
                let handle = core.wake_handle(node, lifetime)?;
                assert_eq!(handle.wake()?, WakeOutcome::Queued);
                provisional = Some(handle);
                let nested: Result<()> = core.with_tree_edit("nested failure", |core| {
                    core.detach(node)?;
                    Err(Error::Invalid("nested edit failed".into()))
                });
                assert!(nested.is_err());
                Err(Error::Invalid("outer edit failed".into()))
            });
            assert!(result.is_err());
            assert_eq!(provisional.unwrap().wake()?, WakeOutcome::Expired);
            assert!(core.work_stamp_valid(stamp));
            assert_eq!(core.wake_registry.drain()?, vec![stamp]);
            assert_eq!(original.wake()?, WakeOutcome::Queued);
        }
        Ok(())
    }

    #[test]
    fn work_lifetimes_follow_committed_structural_changes() -> Result<()> {
        let mut core = Core::new();
        let node = core.create_detached(Leaf { veto: false })?;
        core.attach(core.root, node)?;
        let node_handle = core.wake_handle(node, PollLifetime::Node)?;
        let attached_handle = core.wake_handle(node, PollLifetime::Attachment)?;
        let incarnation = core.nodes[node].incarnation;
        let attachment = core.nodes[node].attachment_generation;
        core.set_hidden(node, true)?;
        assert_eq!(attached_handle.wake()?, WakeOutcome::Queued);
        core.wake_registry.drain()?;
        let failure: Result<()> = core.with_tree_edit("failed detach", |core| {
            core.detach(node)?;
            core.replace_subtree(node, Leaf { veto: false })?;
            Err(Error::Invalid("abort edit".into()))
        });
        assert!(failure.is_err());
        assert_eq!(core.nodes[node].incarnation, incarnation);
        assert_eq!(core.nodes[node].attachment_generation, attachment);
        assert_eq!(attached_handle.wake()?, WakeOutcome::Queued);
        core.wake_registry.drain()?;
        core.detach(node)?;
        assert_eq!(attached_handle.wake()?, WakeOutcome::Expired);
        assert_eq!(node_handle.wake()?, WakeOutcome::Queued);
        core.wake_registry.drain()?;
        core.attach(core.root, node)?;
        assert_ne!(core.nodes[node].attachment_generation, attachment);
        let reattached = core.wake_handle(node, PollLifetime::Attachment)?;
        core.replace_subtree(node, Leaf { veto: false })?;
        assert_eq!(node_handle.wake()?, WakeOutcome::Expired);
        assert_eq!(reattached.wake()?, WakeOutcome::Expired);
        let replacement = core.wake_handle(node, PollLifetime::Node)?;
        core.remove_subtree(node)?;
        assert_eq!(replacement.wake()?, WakeOutcome::Expired);
        Ok(())
    }
}
