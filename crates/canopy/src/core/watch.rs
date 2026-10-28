//! A shared value with a revision, which wakes the widgets that watch it.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use super::wake::NodeWakeHandle;

/// The value, its revision, and its watchers, under one lock.
struct Inner<T> {
    /// The current value.
    value: T,
    /// Advances on each update.
    revision: u64,
    /// One handle for each watching widget.
    watchers: Vec<NodeWakeHandle>,
}

/// A shared value with a revision. Each update wakes the widgets that watch it.
///
/// A producer, often a background thread, changes the value with
/// [`update`](Self::update). A widget calls [`watch`](Self::watch) in
/// `on_mount`, keeps the revision it returns, and compares it with
/// [`changed`](Self::changed) in `poll`. Only the latest value matters: two
/// updates between polls wake the widget once, and it reads the second value.
pub struct Watch<T> {
    /// State shared by every clone.
    inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Clone for Watch<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Default> Default for Watch<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for Watch<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.lock();
        f.debug_struct("Watch")
            .field("value", &inner.value)
            .field("revision", &inner.revision)
            .field("watchers", &inner.watchers.len())
            .finish()
    }
}

impl<T> Watch<T> {
    /// Hold `value` at revision 0, with no watchers.
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                value,
                revision: 0,
                watchers: Vec::new(),
            })),
        }
    }

    /// Lock the state. A panic in another holder leaves the value as that
    /// holder left it, which is still the latest value.
    fn lock(&self) -> MutexGuard<'_, Inner<T>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Change the value, advance the revision, and wake each watcher.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let (output, watchers) = {
            let mut inner = self.lock();
            let output = f(&mut inner.value);
            inner.revision += 1;
            inner.watchers.retain(|handle| !handle.is_expired());
            (output, inner.watchers.clone())
        };
        for handle in &watchers {
            // Only a poisoned registry fails here, and its application has
            // already panicked, so nobody is left to read the value.
            handle.wake().ok();
        }
        output
    }

    /// Change the value without a new revision or a wake, as a reader that
    /// drains part of it.
    pub fn drain<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.lock().value)
    }

    /// Read the value.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.lock().value)
    }

    /// Return the current revision.
    pub fn revision(&self) -> u64 {
        self.lock().revision
    }

    /// Return whether the revision moved past `seen`, and set `seen` to it.
    pub fn changed(&self, seen: &mut u64) -> bool {
        let revision = self.revision();
        let changed = revision != *seen;
        *seen = revision;
        changed
    }

    /// Wake `handle` on each update, and return the current revision. A second
    /// handle for the same widget replaces the first.
    ///
    /// The revision comes from the same lock as the registration, so an update
    /// either shows in the returned revision or wakes `handle`.
    pub fn watch(&self, handle: NodeWakeHandle) -> u64 {
        let mut inner = self.lock();
        let owner = handle.owner();
        inner
            .watchers
            .retain(|known| known.owner() != owner && !known.is_expired());
        inner.watchers.push(handle);
        inner.revision
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::*;
    use crate::{
        core::{
            id::testing_node_id,
            wake::{PollOwner, WakeRegistry},
        },
        error::Result,
    };

    /// The owner of widget incarnation `incarnation`, with node lifetime.
    fn owner(incarnation: u64) -> PollOwner {
        PollOwner {
            node: testing_node_id(),
            incarnation,
            attachment: None,
        }
    }

    #[test]
    fn an_update_wakes_each_watcher_and_drain_wakes_none() -> Result<()> {
        let registry = WakeRegistry::default();
        let watch = Watch::new(Vec::new());
        watch.watch(registry.handle(owner(1))?);
        watch.watch(registry.handle(owner(2))?);
        watch.update(|items| items.push(1));
        assert_eq!(registry.drain()?, [owner(1), owner(2)]);
        assert_eq!(watch.drain(mem::take), [1]);
        assert!(registry.drain()?.is_empty());
        assert_eq!(watch.revision(), 1);
        Ok(())
    }

    #[test]
    fn changed_follows_the_revision() {
        let watch = Watch::new(0);
        let mut seen = watch.revision();
        assert!(!watch.changed(&mut seen));
        watch.update(|value| *value = 5);
        assert!(watch.changed(&mut seen));
        assert!(!watch.changed(&mut seen));
        assert_eq!(watch.read(|value| *value), 5);
    }

    #[test]
    fn an_update_before_watch_shows_in_its_revision() -> Result<()> {
        let registry = WakeRegistry::default();
        let watch = Watch::new(0);
        let before = watch.revision();
        watch.update(|value| *value += 1);
        let mut seen = watch.watch(registry.handle(owner(1))?);
        assert!(seen > before);
        assert!(!watch.changed(&mut seen));
        Ok(())
    }

    #[test]
    fn a_second_watch_replaces_the_first_and_expired_handles_go() -> Result<()> {
        let registry = WakeRegistry::default();
        let watch = Watch::new(0);
        watch.watch(registry.handle(owner(1))?);
        watch.watch(registry.handle(owner(1))?);
        assert_eq!(watch.lock().watchers.len(), 1);
        watch.watch(registry.handle(owner(2))?);
        registry.sync([owner(2)])?;
        watch.update(|value| *value += 1);
        assert_eq!(watch.lock().watchers.len(), 1);
        assert_eq!(registry.drain()?, [owner(2)]);
        Ok(())
    }
}
