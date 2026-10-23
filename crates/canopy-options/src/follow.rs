//! A widget's link to its options: it applies each change of the store to
//! the values it shows.

use std::{cell::Cell, mem, rc::Rc};

use canopy::{Context, ContextExt, Widget, error::Result, runtime::PollLifetime};

use crate::{
    model::OptionGroup,
    store::{OptionsHandle, Store},
};

/// A widget's link to its group of a store.
///
/// The widget keeps the values it shows in fields of its own, and applies a
/// change of the store to them. The follower remembers the values the widget
/// applied last, so the widget applies only what changed.
///
/// A change reaches the widget through a subscription. A widget can be busy
/// when the store changes, such as when a host calls it and the call writes
/// an option. The change then waits for the widget's next poll, which
/// [`Self::take_pending`] reports. An error of the widget's apply reaches
/// the store, which shows it as a notice.
pub struct Follower<G> {
    /// The group of the store.
    handle: OptionsHandle<G>,
    /// The values the widget applied last.
    applied: G,
    /// Whether the widget is writing, so its own subscriber leaves the change
    /// to the write.
    writing: Rc<Cell<bool>>,
    /// Whether a change waits for the widget's poll.
    pending: Rc<Cell<bool>>,
}

impl<G: Clone + PartialEq + 'static> Follower<G> {
    /// Follow a store of the follower's own, which has no files.
    ///
    /// Each call makes a separate store. A widget uses one until a host
    /// gives it a handle to the host's store.
    #[must_use]
    pub fn standalone() -> Self
    where
        G: OptionGroup + Send + Sync,
    {
        Self::new(Store::<G>::default().handle())
    }

    /// Follow `handle`, with its current values applied.
    #[must_use]
    pub fn new(handle: OptionsHandle<G>) -> Self {
        Self {
            applied: (*handle.get()).clone(),
            handle,
            writing: Rc::default(),
            pending: Rc::default(),
        }
    }

    /// Return the values the widget applied last.
    #[must_use]
    pub const fn applied(&self) -> &G {
        &self.applied
    }

    /// Call `apply` on the widget of `context` after each change of the
    /// group.
    ///
    /// The widget calls this as it mounts. `apply` takes the change with
    /// [`Self::take_change`]. The widget also polls as the options panel
    /// closes, so it can move the focus that the panel held.
    pub fn follow<W: Widget>(
        &self,
        context: &mut dyn Context,
        apply: fn(&mut W, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        let node = context.node_id();
        let writing = Rc::clone(&self.writing);
        let pending = Rc::clone(&self.pending);
        let wake = context.wake_handle(PollLifetime::Node)?;
        self.handle.wake_on_close(wake.clone());
        self.handle.subscribe("", move |context, _, _| {
            if writing.get() {
                return Ok(());
            }
            // A widget that cannot be reached is busy, and applies the change
            // at its poll. An error of the apply itself is the store's to
            // report.
            let mut outcome = Ok(());
            let reached = context.with_widget_mut(node, |widget: &mut W, context| {
                outcome = apply(widget, context);
                Ok(())
            });
            if reached.is_err() {
                pending.set(true);
                wake.wake()?;
            }
            outcome
        });
        Ok(())
    }

    /// Return whether a change waits for the widget's poll, and clear it.
    pub fn take_pending(&self) -> bool {
        self.pending.replace(false)
    }

    /// Change the group with `write`. The widget applies the change itself
    /// afterwards, with [`Self::take_change`].
    pub fn write(
        &self,
        context: &mut dyn Context,
        write: impl FnOnce(&mut G) + 'static,
    ) -> Result<()> {
        self.writing.set(true);
        let written = self.handle.update(context, write);
        self.writing.set(false);
        written
    }

    /// Remove the live values at or below `path`, relative to the group.
    /// The widget applies the change itself afterwards.
    pub fn revert(&self, context: &mut dyn Context, path: &str) -> Result<()> {
        let path = self.handle.path(path);
        self.writing.set(true);
        let reverted = self.handle.backend().revert(context, Some(&path));
        self.writing.set(false);
        reverted
    }

    /// Return whether a leaf at or below `path`, relative to the group, has
    /// a live value.
    #[must_use]
    pub fn live_at(&self, path: &str) -> bool {
        self.handle.backend().has_live_at(&self.handle.path(path))
    }

    /// Return the values the widget applied last and the current values, if
    /// they differ, and remember the current values as applied.
    pub fn take_change(&mut self) -> Option<(G, G)> {
        let next = (*self.handle.get()).clone();
        if next == self.applied {
            return None;
        }
        let previous = mem::replace(&mut self.applied, next.clone());
        Some((previous, next))
    }
}
