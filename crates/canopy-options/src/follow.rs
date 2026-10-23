//! A widget's link to its options: it applies each change of the store to
//! the values it shows.

use std::{
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
};

use canopy::{
    Context, ContextExt, Widget,
    error::{Error, Result},
    runtime::{NoticePost, PollLifetime},
};

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
/// an option. The change then waits until the outermost dispatch completes,
/// as [`ContextExt::with_widget_mut_or_post`] describes. An error of an
/// apply that runs at once reaches the store, which shows it as a notice
/// with the errors of the other subscribers. A waiting apply shows its own
/// error as a notice.
pub struct Follower<G> {
    /// The group of the store.
    handle: OptionsHandle<G>,
    /// The values the widget applied last.
    applied: G,
    /// Whether the widget is writing, so its own subscriber leaves the change
    /// to the write.
    writing: Rc<Cell<bool>>,
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
    /// [`Self::take_change`]. A widget that is gone, or that another widget
    /// replaced, hears no more changes. The widget also polls as the options
    /// panel closes, so it can move the focus that the panel held.
    pub fn follow<W: Widget + 'static>(
        &self,
        context: &mut dyn Context,
        apply: fn(&mut W, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        let stamp = context.widget_stamp(context.node_id())?;
        let writing = Rc::clone(&self.writing);
        self.handle
            .wake_on_close(context.wake_handle(PollLifetime::Node)?);
        self.handle.subscribe("", move |context, _, _| {
            if writing.get() {
                return Ok(());
            }
            // The store joins the error of an apply that runs now with those
            // of the other subscribers. An apply that waits runs after the
            // subscriber returns, so it posts its own notice: an error would
            // stop the completion drain, and drop the work of others.
            let failure: Rc<RefCell<Option<Error>>> = Rc::default();
            let waiting = Rc::downgrade(&failure);
            context.with_widget_mut_or_post(stamp, move |widget: &mut W, context| {
                let Err(error) = apply(widget, context) else {
                    return Ok(());
                };
                match waiting.upgrade() {
                    Some(failure) => {
                        *failure.borrow_mut() = Some(error);
                        Ok(())
                    }
                    None => context.notify(NoticePost::error(format!("options: {error}"))),
                }
            })?;
            failure.take().map_or(Ok(()), Err)
        });
        Ok(())
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

#[cfg(test)]
mod tests {
    use std::{mem, time::Duration};

    use canopy::{
        Context, NodeId, NodeName, Register, Setup, ViewContextExt, Widget, derive_commands,
        error::{Error, Result},
        runtime::{NoticeLevel, NoticePost},
        testing::harness::Harness,
    };

    use crate::{Follower, OptionGroup, Options, OptionsHandle, Store, Value, tests::Root};

    /// The path of the option that the tests change.
    const HIDDEN: &str = "listing.hidden";

    /// Return whether `root` hides hidden files.
    fn hidden(root: &Root) -> bool {
        root.get(HIDDEN) == Some(Value::Bool(true))
    }

    /// Set `path` of `root` to `value`.
    fn set(root: &mut Root, path: &str, value: &Value) {
        root.set(path, value).expect("the test paths are valid");
    }

    /// A widget that shows the hidden option through a follower.
    struct Shower {
        /// The widget's link to the store.
        options: Follower<Root>,
        /// A handle to the same store, for writes the follower does not make.
        other: OptionsHandle<Root>,
        /// The hidden option, as the widget shows it.
        hidden: bool,
        /// How many applies found a change.
        applies: usize,
        /// Whether the next apply fails.
        fail: bool,
        /// How many times the widget polled.
        polls: usize,
    }

    #[derive_commands]
    impl Shower {
        /// Follow the options of `store`.
        fn new(store: &Store<Root>) -> Self {
            Self {
                options: Follower::new(store.handle()),
                other: store.handle(),
                hidden: false,
                applies: 0,
                fail: false,
                polls: 0,
            }
        }

        /// Apply the change of the options.
        fn apply(&mut self, _context: &mut dyn Context) -> Result<()> {
            if let Some((_, next)) = self.options.take_change() {
                self.applies += 1;
                self.hidden = hidden(&next);
            }
            if mem::take(&mut self.fail) {
                return Err(Error::Invalid("apply failed".into()));
            }
            Ok(())
        }

        /// Write two options through the other handle while this widget is
        /// busy, then post a notice, and return the hidden option as the
        /// widget shows it meanwhile.
        #[command]
        fn hide_elsewhere(&self, context: &mut dyn Context) -> Result<bool> {
            self.other
                .update(context, |root| set(root, HIDDEN, &Value::Bool(true)))?;
            self.other
                .update(context, |root| set(root, "diff.context", &Value::Int(5)))?;
            context.notify(NoticePost::info("after"))?;
            Ok(self.hidden)
        }
    }

    impl Widget for Shower {
        fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
            self.options.follow(context, Self::apply)
        }

        fn poll(&mut self, _context: &mut dyn Context) -> Result<Option<Duration>> {
            self.polls += 1;
            Ok(None)
        }

        fn name(&self) -> NodeName {
            NodeName::convert("shower")
        }
    }

    impl Register for Shower {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    /// Host a shower of `store` inside the options widget.
    fn harness(store: &Store<Root>) -> Harness {
        Harness::builder(Options::new(store, Shower::new(store)))
            .register::<Options>()
            .register::<Shower>()
            .size(80, 24)
            .build()
            .expect("the harness builds")
    }

    /// Read the shower.
    fn shower<R>(harness: &mut Harness, f: impl FnOnce(&mut Shower) -> R) -> R {
        harness
            .with_unique(|shower: &mut Shower, _| Ok(f(shower)))
            .expect("one shower")
    }

    /// Return the messages of the notices of `level`.
    fn notices(harness: &Harness, level: NoticeLevel) -> Vec<String> {
        harness
            .canopy
            .notices()
            .iter()
            .filter(|notice| notice.level == level)
            .map(|notice| notice.message.clone())
            .collect()
    }

    #[test]
    fn a_busy_widget_applies_the_change_as_its_dispatch_completes() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        let polls = shower(&mut harness, |shower| shower.polls);
        harness
            .script(
                r#"canopy.assert(shower.hide_elsewhere() == false, "the apply waits for the call")"#,
            )
            .unwrap();
        let (hidden, applies, after) = shower(&mut harness, |s| (s.hidden, s.applies, s.polls));
        assert!(hidden, "the change applied as the call completed");
        assert_eq!(applies, 1, "two writes applied once");
        assert_eq!(after, polls, "no poll applied it");
        assert!(notices(&harness, NoticeLevel::Error).is_empty());
    }

    #[test]
    fn a_waiting_apply_reports_its_failure_and_the_drain_goes_on() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        shower(&mut harness, |shower| shower.fail = true);
        harness.script("shower.hide_elsewhere()").unwrap();
        assert_eq!(
            notices(&harness, NoticeLevel::Error),
            ["options: invalid: apply failed"]
        );
        assert_eq!(notices(&harness, NoticeLevel::Info), ["after"]);
        assert!(shower(&mut harness, |shower| shower.hidden));
    }

    #[test]
    fn a_removed_widget_hears_no_changes() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        let handle = store.handle();
        harness
            .canopy
            .with_root_context(|context| {
                let root = context.root_id();
                let node: NodeId = context
                    .unique_descendant::<Shower>(root)?
                    .expect("one shower")
                    .into();
                context.remove_subtree(node)?;
                handle.update(context, |root| set(root, HIDDEN, &Value::Bool(true)))
            })
            .unwrap();
        assert!(harness.canopy.notices().is_empty());
        assert!(hidden(&store.current()));
    }

    #[test]
    fn the_panel_close_polls_the_widget() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        harness.render().unwrap();
        let polls = shower(&mut harness, |shower| shower.polls);
        harness.script("options.open()").unwrap();
        harness.script("options.close()").unwrap();
        harness
            .wait_until(Duration::from_secs(5), |harness| {
                Ok(shower(harness, |shower| shower.polls) > polls)
            })
            .expect("the close polls the shower");
    }
}
