//! Guards for the core state that a call holds for its length.
//!
//! A callback's depth, a dispatch boundary, the handler kind and event in
//! scope, a drain, and lifecycle cleanup are set when a call starts and
//! restored when it ends. Each is owned by a [`Scope`], so an early return, a
//! `?`, or an unwind that an embedder catches restores it.

use std::{
    mem,
    num::NonZeroU32,
    ops::{Deref, DerefMut},
};

use super::{Core, TreeEditJournal, TreeStateSnapshot};
use crate::{
    core::{canopy::Canopy, notice::NoticeSource},
    error::{Error, Result},
    input::Event,
};

/// An owner of the core whose state a [`Scope`] restores.
pub trait CoreOwner {
    /// Return the core.
    fn core_mut(&mut self) -> &mut Core;

    /// Count a mutable callback as running until the scope ends.
    fn enter_callback(&mut self) -> Scope<'_, Self>
    where
        Self: Sized,
    {
        self.core_mut().callback_depth += 1;
        Scope::new(self, Exit::Callback)
    }

    /// Open a dispatch boundary. [`Scope::finish`] closes it and drains the
    /// batch; a scope that ends without it discards the work it queued.
    fn enter_dispatch(&mut self) -> Scope<'_, Self>
    where
        Self: Sized,
    {
        let core = self.core_mut();
        core.completion.depth += 1;
        let checkpoint = core.completion.requests.len();
        Scope::new(self, Exit::Dispatch(checkpoint))
    }

    /// Note that a handler of kind `source` runs until the scope ends. A call
    /// it posts keeps the kind, so the call's failure reads as the handler's
    /// own.
    fn enter_notice(&mut self, source: NoticeSource) -> Scope<'_, Self>
    where
        Self: Sized,
    {
        let sources = &mut self.core_mut().completion.notice_sources;
        let depth = sources.len();
        sources.push(source);
        Scope::new(self, Exit::Notice(depth))
    }

    /// Put `event` in scope for command injection until the scope ends.
    fn enter_event(&mut self, event: &Event) -> Scope<'_, Self>
    where
        Self: Sized,
    {
        let scope = &mut self.core_mut().event_scope;
        let depth = scope.len();
        scope.push(event.clone());
        Scope::new(self, Exit::Event(depth))
    }
}

impl CoreOwner for Core {
    fn core_mut(&mut self) -> &mut Core {
        self
    }
}

impl CoreOwner for Canopy {
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
}

impl<T: CoreOwner> CoreOwner for Scope<'_, T> {
    fn core_mut(&mut self) -> &mut Core {
        self.owner.core_mut()
    }
}

/// The lifecycle cleanup that runs, which limits the work it may start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Cleanup {
    /// No cleanup runs.
    #[default]
    None,
    /// Removal and unmount hooks run, nested this deep. They may edit the
    /// tree, but not queue work.
    Hooks(NonZeroU32),
    /// A failed tree edit rolls back. Its unmount hooks may neither edit the
    /// tree nor queue work.
    Rollback,
}

/// Work that lifecycle cleanup may refuse.
#[derive(Clone, Copy, Debug)]
pub(super) enum CleanupOp {
    /// Queue completion work, named for diagnostics.
    Queue(&'static str),
    /// Edit the tree, named for diagnostics.
    TreeEdit(&'static str),
}

impl Cleanup {
    /// Return whether cleanup in this state admits `op`.
    pub(super) fn admits(self, op: CleanupOp) -> Result<()> {
        match (self, op) {
            (Self::None, _) | (Self::Hooks(_), CleanupOp::TreeEdit(_)) => Ok(()),
            (Self::Hooks(_) | Self::Rollback, CleanupOp::Queue(label)) => Err(Error::Invalid(
                format!("cannot queue {label} during lifecycle cleanup"),
            )),
            (Self::Rollback, CleanupOp::TreeEdit(operation)) => {
                Err(Error::TreeEditDuringRollback { operation })
            }
        }
    }

    /// Return the state inside one more level of hooks. A rollback stays a
    /// rollback, which admits less.
    fn with_hooks(self) -> Self {
        match self {
            Self::None => Self::Hooks(NonZeroU32::MIN),
            Self::Hooks(depth) => Self::Hooks(depth.saturating_add(1)),
            Self::Rollback => Self::Rollback,
        }
    }
}

/// What a [`Scope`] restores when it ends.
enum Exit {
    /// A mutable callback ends.
    Callback,
    /// A dispatch boundary ends, with the batch length at its start. A
    /// boundary that ends without [`Scope::finish`] fails.
    Dispatch(usize),
    /// A handler kind leaves scope, with the depth to restore.
    Notice(usize),
    /// An event leaves scope, with the depth to restore.
    Event(usize),
    /// A delivery's events leave scope, with the outer events to restore.
    Events(Vec<Event>),
    /// A drain ends, and discards what it did not apply.
    Drain,
    /// Lifecycle cleanup ends, with the state to restore.
    Cleanup(Cleanup),
    /// A rollback ends: the tree returns to its snapshot, and cleanup to the
    /// state before it.
    Rollback(Box<TreeStateSnapshot>, Cleanup),
    /// An outermost tree edit ends. An edit whose journal is still open
    /// unwound, and rolls back.
    TreeEdit,
}

impl Exit {
    /// Restore the state this exit names.
    fn restore(self, core: &mut Core) {
        match self {
            Self::Callback => core.callback_depth -= 1,
            Self::Dispatch(checkpoint) => core.close_dispatch(checkpoint, false),
            Self::Notice(depth) => core.completion.notice_sources.truncate(depth),
            Self::Event(depth) => core.event_scope.truncate(depth),
            Self::Events(outer) => core.event_scope = outer,
            Self::Drain => {
                core.completion.requests.clear();
                core.completion.draining = false;
            }
            Self::Cleanup(previous) => core.cleanup = previous,
            Self::Rollback(before, previous) => {
                before.restore(core);
                core.cleanup = previous;
            }
            Self::TreeEdit => {
                if let Some(journal) = core.tree_edit.take()
                    && let Err(error) = core.rollback_tree_edit(journal)
                {
                    tracing::error!(%error, "rolling back an unwound tree edit");
                }
            }
        }
    }
}

/// A guard that restores core state when it ends, by return or by unwind.
///
/// The scope borrows its owner, a [`Core`] or a [`Canopy`], and dereferences
/// to it, so the call inside the scope works through the guard.
pub struct Scope<'a, T: CoreOwner> {
    /// Owner whose core state the scope restores.
    owner: &'a mut T,
    /// State to restore, until the scope ends.
    exit: Option<Exit>,
}

impl<'a, T: CoreOwner> Scope<'a, T> {
    /// Guard `owner` until `exit` restores it.
    fn new(owner: &'a mut T, exit: Exit) -> Self {
        Self {
            owner,
            exit: Some(exit),
        }
    }

    /// Close a dispatch boundary: a failure discards the work it queued, and
    /// the success of the outermost boundary drains the batch.
    pub fn finish(mut self, success: bool) -> Result<()> {
        let Some(Exit::Dispatch(checkpoint)) = self.exit.take() else {
            unreachable!("only a dispatch scope finishes");
        };
        let core = self.owner.core_mut();
        core.close_dispatch(checkpoint, success);
        if success && core.completion_ready() {
            core.drain_completion()?;
        }
        Ok(())
    }
}

impl<T: CoreOwner> Deref for Scope<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.owner
    }
}

impl<T: CoreOwner> DerefMut for Scope<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.owner
    }
}

impl<T: CoreOwner> Drop for Scope<'_, T> {
    fn drop(&mut self) {
        if let Some(exit) = self.exit.take() {
            exit.restore(self.owner.core_mut());
        }
    }
}

impl Core {
    /// Return whether no dispatch boundary or callback is open, so queued
    /// completion work can drain.
    pub(super) fn completion_ready(&self) -> bool {
        self.completion.depth == 0 && self.callback_depth == 0
    }

    /// End a dispatch boundary that began at `checkpoint`, discarding what it
    /// queued unless it succeeded.
    fn close_dispatch(&mut self, checkpoint: usize, success: bool) {
        if !success {
            self.completion.requests.truncate(checkpoint);
        }
        self.completion.depth = self
            .completion
            .depth
            .checked_sub(1)
            .expect("dispatch completion requires an active boundary");
    }

    /// Run lifecycle hooks, with the batch sealed against new work, until the
    /// scope ends.
    pub(super) fn enter_hooks(&mut self) -> Scope<'_, Self> {
        let hooks = self.cleanup.with_hooks();
        let previous = mem::replace(&mut self.cleanup, hooks);
        Scope::new(self, Exit::Cleanup(previous))
    }

    /// Roll a failed edit back until the scope ends, when the tree returns to
    /// `before`, even if an unmount hook unwinds.
    pub(super) fn enter_rollback(&mut self, before: TreeStateSnapshot) -> Scope<'_, Self> {
        let previous = mem::replace(&mut self.cleanup, Cleanup::Rollback);
        Scope::new(self, Exit::Rollback(Box::new(before), previous))
    }

    /// Open the outermost tree edit with `journal`. The edit takes the journal
    /// back to commit it or to roll it back; one that unwinds with the
    /// journal open rolls back as the scope ends.
    pub(super) fn enter_tree_edit(&mut self, journal: TreeEditJournal) -> Scope<'_, Self> {
        self.tree_edit = Some(journal);
        Scope::new(self, Exit::TreeEdit)
    }

    /// Mark a drain loop as running until the scope ends, which discards the
    /// requests the drain did not apply.
    pub(super) fn enter_drain(&mut self) -> Scope<'_, Self> {
        self.completion.draining = true;
        Scope::new(self, Exit::Drain)
    }

    /// Replace the events in scope with `events` until the scope ends.
    pub(super) fn enter_events(&mut self, events: Vec<Event>) -> Scope<'_, Self> {
        let outer = mem::replace(&mut self.event_scope, events);
        Scope::new(self, Exit::Events(outer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_admits_by_state_and_operation() {
        let hooks = Cleanup::None.with_hooks();
        let queue = CleanupOp::Queue("removal");
        let edit = CleanupOp::TreeEdit("attach");
        assert!(Cleanup::None.admits(queue).is_ok());
        assert!(Cleanup::None.admits(edit).is_ok());
        assert!(hooks.admits(queue).is_err());
        assert!(hooks.admits(edit).is_ok());
        assert!(Cleanup::Rollback.admits(queue).is_err());
        assert!(matches!(
            Cleanup::Rollback.admits(edit),
            Err(Error::TreeEditDuringRollback {
                operation: "attach"
            })
        ));
        assert_eq!(
            hooks.with_hooks(),
            Cleanup::Hooks(NonZeroU32::new(2).unwrap())
        );
        assert_eq!(Cleanup::Rollback.with_hooks(), Cleanup::Rollback);
    }

    #[test]
    fn scopes_restore_their_state_on_drop() {
        let mut core = Core::new();
        {
            let mut callback = core.enter_callback();
            let mut dispatch = callback.enter_dispatch();
            let mut notice = dispatch.enter_notice(NoticeSource::Widget);
            let _event = notice.enter_event(&Event::FocusGained);
        }
        assert_eq!(core.callback_depth, 0);
        assert_eq!(core.completion.depth, 0);
        assert!(core.completion.notice_sources.is_empty());
        assert!(core.event_scope.is_empty());
        {
            let mut hooks = core.enter_hooks();
            let _inner = hooks.enter_hooks();
        }
        assert_eq!(core.cleanup, Cleanup::None);
    }
}
