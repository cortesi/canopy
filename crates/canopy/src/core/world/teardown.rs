//! Completion-boundary removal requests.

use std::collections::VecDeque;

use super::Core;
use crate::{
    NodeId,
    core::wake::WorkStamp,
    error::{Error, Result},
};

/// Maximum retained removals across one outer dispatch and its nested
/// callbacks.
const MAX_REMOVAL_REQUESTS: usize = 1024;

/// One queued removal tied to the widget that received the request.
pub(super) struct RemovalRequest {
    /// Arena node to remove.
    node: NodeId,
    /// Widget incarnation when queued.
    incarnation: u64,
}

/// Pending teardown and dispatch nesting state.
#[derive(Default)]
pub(super) struct CompletionBatch {
    /// FIFO requests from active dispatches.
    pub(super) requests: VecDeque<RemovalRequest>,
    /// Active explicit dispatch boundaries.
    depth: usize,
    /// Lifecycle cleanup cannot add work while this is true.
    draining: bool,
}

impl Core {
    /// Test whether scheduled work still belongs to the same live widget or
    /// attachment.
    pub(crate) fn work_stamp_valid(&self, stamp: WorkStamp) -> bool {
        self.nodes.get(stamp.node).is_some_and(|node| {
            node.incarnation == stamp.incarnation
                && stamp
                    .attachment
                    .is_none_or(|generation| node.attachment_generation == Some(generation))
        })
    }

    /// Capture a wake handle for the requested lifetime of a live node.
    pub(crate) fn wake_handle(
        &self,
        node: NodeId,
        lifetime: crate::WorkLifetime,
    ) -> Result<crate::NodeWakeHandle> {
        let entry = self.nodes.get(node).ok_or(Error::NodeNotFound(node))?;
        let attachment = match lifetime {
            crate::WorkLifetime::Node => None,
            crate::WorkLifetime::Attachment => Some(
                entry
                    .attachment_generation
                    .ok_or(Error::NodeDetached(node))?,
            ),
        };
        self.wake_registry.handle(WorkStamp {
            node,
            incarnation: entry.incarnation,
            attachment,
        })
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
            self.drain_removals()?;
        }
        Ok(())
    }

    /// Queue removal of this widget incarnation after callbacks return.
    pub(crate) fn remove_after_dispatch(&mut self, node: NodeId) -> Result<()> {
        if self.completion.draining || self.rolling_back_tree_edit {
            return Err(Error::Invalid(
                "cannot queue removal during lifecycle cleanup".into(),
            ));
        }
        let Some(entry) = self.nodes.get(node) else {
            return Ok(());
        };
        if self.completion.requests.len() >= MAX_REMOVAL_REQUESTS {
            return Err(Error::InvalidOperation(format!(
                "dispatch removal batch exceeds {MAX_REMOVAL_REQUESTS} requests"
            )));
        }
        self.completion.requests.push_back(RemovalRequest {
            node,
            incarnation: entry.incarnation,
        });
        if self.completion.depth == 0 && self.callback_depth == 0 {
            self.drain_removals()?;
        }
        Ok(())
    }

    /// Apply the current FIFO batch, stopping and discarding the tail on
    /// failure.
    fn drain_removals(&mut self) -> Result<()> {
        if self.completion.draining {
            return Ok(());
        }
        self.completion.draining = true;
        let result = self.drain_removals_inner();
        self.completion.requests.clear();
        self.completion.draining = false;
        result
    }

    /// Remove only nodes whose widget incarnation still matches the request.
    fn drain_removals_inner(&mut self) -> Result<()> {
        while let Some(request) = self.completion.requests.pop_front() {
            if self
                .nodes
                .get(request.node)
                .is_some_and(|node| node.incarnation == request.incarnation)
            {
                self.remove_subtree(request.node)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, WakeOutcome, Widget, WorkLifetime};

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
            for _ in 0..=MAX_REMOVAL_REQUESTS {
                ctx.remove_after_dispatch(target)?;
            }
            Ok::<_, Error>(())
        })?;
        assert!(matches!(&result, Err(Error::InvalidOperation(message))
            if message == "dispatch removal batch exceeds 1024 requests"));
        assert_eq!(core.completion.requests.len(), MAX_REMOVAL_REQUESTS);
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
        fn poll_lifetime(&self) -> WorkLifetime {
            WorkLifetime::Attachment
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
        for lifetime in [WorkLifetime::Node, WorkLifetime::Attachment] {
            let mut core = Core::new();
            let node = core.create_detached(Leaf { veto: false })?;
            core.attach(core.root, node)?;
            let original = core.wake_handle(node, lifetime)?;
            let stamp = WorkStamp {
                node,
                incarnation: core.nodes[node].incarnation,
                attachment: match lifetime {
                    WorkLifetime::Node => None,
                    WorkLifetime::Attachment => core.nodes[node].attachment_generation,
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
        let node_handle = core.wake_handle(node, WorkLifetime::Node)?;
        let attached_handle = core.wake_handle(node, WorkLifetime::Attachment)?;
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
        let reattached = core.wake_handle(node, WorkLifetime::Attachment)?;
        core.replace_subtree(node, Leaf { veto: false })?;
        assert_eq!(node_handle.wake()?, WakeOutcome::Expired);
        assert_eq!(reattached.wake()?, WakeOutcome::Expired);
        let replacement = core.wake_handle(node, WorkLifetime::Node)?;
        core.remove_subtree(node)?;
        assert_eq!(replacement.wake()?, WakeOutcome::Expired);
        Ok(())
    }
}
