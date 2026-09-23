use std::{
    any::type_name,
    time::{Duration, Instant},
};

use futures::{
    future::{Either, select},
    pin_mut,
};
use tokio::time::sleep_until;

use super::buf::BufTest;
use crate::{
    Canopy, CanopyBuilder, Context, ContextExt, NodeId, Register, Setup, TypedId, ViewContextExt,
    Work,
    core::{canopy::WorkSelector, termbuf::TermBuf},
    error::{Error, Result},
    event::{Event, key, mouse},
    geom::Size,
    layout::LayoutOverride,
    render::NopBackend,
    script,
    widget::Widget,
};

/// A simple harness that holds a [`Canopy`], a [`NopBackend`] backend and a
/// root node ID. Tests drive the UI by sending key events and triggering
/// renders and can then inspect the render buffer.
pub struct Harness {
    /// The Canopy instance that manages the node tree and rendering.
    pub canopy: Canopy,
    /// The backend used for rendering. In tests, this is a no-op backend.
    backend: NopBackend,
    /// The root node of the UI under test.
    pub root: NodeId,
}

/// Builder for a test harness, layered on [`CanopyBuilder`].
///
/// The root widget fills the harness view. Registration is explicit: a test
/// registers each type it mounts with [`Self::register`] or
/// [`Self::configure`].
pub struct HarnessBuilder<W> {
    /// Root widget under test.
    root: W,
    /// View size for the harness.
    size: Size,
    /// Application builder the harness completes.
    builder: CanopyBuilder,
}

impl<W: Widget + 'static> HarnessBuilder<W> {
    /// Create a new harness builder with the given root widget.
    fn new(root: W) -> Self {
        Self {
            root,
            size: Size::new(100, 100),
            builder: CanopyBuilder::new(),
        }
    }

    /// Set the size of the harness view.
    #[must_use]
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    /// Register a type's commands, bindings, and resources before the API
    /// finalizes.
    #[must_use]
    pub fn register<R: Register + 'static>(self) -> Self {
        self.configure(R::register)
    }

    /// Run one registration callback before the API finalizes.
    #[must_use]
    pub fn configure(mut self, configure: impl FnOnce(&mut Setup) -> Result<()> + 'static) -> Self {
        self.builder = self.builder.configure(configure);
        self
    }

    /// Evaluate named binding source after finalization, as an application
    /// does.
    ///
    /// A widget's defaults reach a test this way, for instance
    /// `button.default_bindings()`, which registration alone does not run.
    #[must_use]
    pub fn bindings(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.builder = self.builder.bindings(name, source);
        self
    }

    /// Build the application, mount the root widget, and prepare the first
    /// frame.
    pub fn build(self) -> Result<Harness> {
        let root = self.root;
        let canopy = self
            .builder
            .assemble(move |canopy| {
                canopy.replace_root(root)?;
                let root = canopy.core.root;
                canopy.core.set_layout_override(
                    root,
                    LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
                )
            })
            .build()?;
        Harness::from_canopy(canopy, self.size)
    }
}

impl Harness {
    /// Wrap an already configured Canopy application in a test harness.
    pub fn from_canopy(mut canopy: Canopy, size: Size) -> Result<Self> {
        canopy.set_screen_size(size)?;
        canopy.turn(Work::Prepare)?;
        let root = canopy.root_id();
        Ok(Self {
            canopy,
            backend: NopBackend::new(),
            root,
        })
    }

    /// Create a harness builder for constructing a test harness with a fluent
    /// API.
    pub fn builder<W: Widget + 'static>(root: W) -> HarnessBuilder<W> {
        HarnessBuilder::new(root)
    }

    /// Create a harness with the builder's default screen size and no
    /// registration.
    pub fn new<W: Widget + 'static>(root: W) -> Result<Self> {
        Self::builder(root).build()
    }

    /// Access the current render buffer. Panics if a render has not yet been
    /// performed.
    pub fn buf(&self) -> &TermBuf {
        self.canopy.buf().expect("render buffer not initialized")
    }

    /// Send a key event and render.
    pub fn key<T>(&mut self, k: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.canopy
            .turn(Work::Input(vec![Event::Key(k.into())]))
            .map(|_| ())
    }

    /// Send a mouse event and render.
    pub fn mouse(&mut self, m: mouse::MouseEvent) -> Result<()> {
        self.canopy
            .turn(Work::Input(vec![Event::Mouse(m)]))
            .map(|_| ())
    }

    /// Send a sequence of key events and render after each.
    pub fn keys<I, K>(&mut self, keys: I) -> Result<()>
    where
        I: IntoIterator<Item = K>,
        K: Into<key::Key>,
    {
        for key in keys {
            self.key(key)?;
        }
        Ok(())
    }

    /// Type a string as a sequence of key events.
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        self.keys(text.chars())
    }

    /// Render the root node into the harness backend.
    pub fn render(&mut self) -> Result<()> {
        self.canopy.render(&mut self.backend)
    }

    /// Execute a script on the app under test.
    pub fn script(&mut self, script: &str) -> Result<()> {
        self.canopy.eval_script(script).map(|_| ())
    }

    /// Execute a closure with mutable access to a widget by node id.
    pub fn with_widget<W, R>(
        &mut self,
        node_id: impl Into<NodeId>,
        f: impl FnOnce(&mut W) -> R,
    ) -> R
    where
        W: Widget + 'static,
    {
        let node_id = node_id.into();
        self.canopy
            .with_context(node_id, |ctx| {
                ctx.with_widget_mut(node_id, |widget, _| Ok(f(widget)))
            })
            .expect("with_widget failed")
    }

    /// Execute a closure with mutable access to the root widget.
    pub fn with_root_widget<W, R>(&mut self, f: impl FnOnce(&mut W) -> R) -> R
    where
        W: Widget + 'static,
    {
        let root = self.root;
        self.with_widget(root, f)
    }

    /// Execute a closure with mutable access to the root widget and its
    /// context.
    pub fn with_root_widget_context<W, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        let root = self.root;
        self.canopy
            .with_root_context(|ctx| ctx.with_widget_mut(root, f))
    }

    /// Execute a closure with the only widget of type `W` in the tree, and its
    /// context.
    ///
    /// The search includes the root. It fails when no widget or more than one
    /// widget of type `W` is mounted.
    pub fn with_unique<W, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        self.canopy.with_root_context(|ctx| {
            let root = ctx.root_id();
            let found: Vec<TypedId<W>> = ctx
                .typed_id::<W>(root)
                .ok()
                .into_iter()
                .chain(ctx.descendants::<W>(root))
                .collect();
            let node = match found.as_slice() {
                [node] => *node,
                [] => return Err(Error::NotFound(type_name::<W>().to_string())),
                _ => return Err(Error::MultipleMatches),
            };
            ctx.with_widget_mut(node, f)
        })
    }

    /// Run real turns until `predicate` holds, failing after `timeout` of
    /// real time.
    ///
    /// This is the native counterpart of Luau `canopy.wait_for`. The first
    /// turn prepares pending changes. Each later turn services what the
    /// runtime would service: worker wakes, queued automation, and polls due
    /// under the installed clock. A manual clock advances only when the test
    /// advances it. The predicate runs before every wait, so it sees the frame
    /// the last turn prepared.
    pub fn wait_until(
        &mut self,
        timeout: Duration,
        mut predicate: impl FnMut(&mut Self) -> Result<bool>,
    ) -> Result<()> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| Error::Invalid("wait timeout exceeds the clock range".into()))?;
        let mut selector = WorkSelector::default();
        self.canopy.turn(Work::Prepare)?;
        loop {
            if predicate(self)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::Invalid(format!(
                    "the condition did not hold within {timeout:?}"
                )));
            }
            if let Some(work) = self.next_work(&mut selector, deadline)? {
                self.canopy.turn(work)?;
            }
        }
    }

    /// Wait for the next turn input, or return `None` at `deadline`.
    fn next_work(
        &mut self,
        selector: &mut WorkSelector,
        deadline: Instant,
    ) -> Result<Option<Work>> {
        let mut events = self
            .canopy
            .event_rx
            .take()
            .ok_or_else(|| Error::Driver("the harness event channel is taken".into()))?;
        let canopy = &mut self.canopy;
        let work = script::block_on(async {
            let next = selector.next_from(canopy, &mut events);
            let expiry = sleep_until(deadline.into());
            pin_mut!(next, expiry);
            match select(next, expiry).await {
                Either::Left((work, _)) => work.map(Some),
                Either::Right(((), _)) => Ok(None),
            }
        });
        self.canopy.event_rx = Some(events);
        work
    }

    /// Get a BufTest instance that references the current buffer.
    pub fn tbuf(&self) -> BufTest<'_> {
        BufTest::new(self.buf())
    }

    /// Find all nodes whose paths match the filter, relative to the root.
    pub fn find_nodes(&self, path_filter: &str) -> Result<Vec<NodeId>> {
        self.canopy
            .with_root_view(|context| context.find_nodes(path_filter))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    use super::*;
    use crate::{
        NodeWakeHandle, ViewContext, WorkLifetime, error::Result, geom::Line, layout::Layout,
        render::Render, state::NodeName, testing::ManualClock, widget::Widget,
    };

    struct TestNode;

    impl TestNode {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for TestNode {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            r.text("base", Line::new(0, 0, 5), "test")?;
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("test_node")
        }
    }

    #[test]
    fn test_harness_builder() {
        let mut h = Harness::builder(TestNode::new())
            .size(20, 5)
            .build()
            .unwrap();

        h.render().unwrap();
        assert!(h.tbuf().contains_text("test"));
    }

    /// Counts its polls, and repeats on an interval until it reaches a limit.
    struct Poller {
        /// Polls so far, shared with the test.
        polls: Arc<AtomicUsize>,
        /// Interval between polls, if the widget schedules itself.
        interval: Option<Duration>,
        /// Poll count after which the widget stops scheduling itself.
        limit: usize,
        /// Handle a worker uses to wake the widget.
        wake: Arc<Mutex<Option<NodeWakeHandle>>>,
    }

    impl Poller {
        fn new(interval: Option<Duration>, limit: usize) -> Self {
            Self {
                polls: Arc::new(AtomicUsize::new(0)),
                interval,
                limit,
                wake: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl Widget for Poller {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            *self.wake.lock().expect("wake lock") = Some(ctx.wake_handle(WorkLifetime::Node)?);
            Ok(())
        }

        fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
            let polls = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(self.interval.filter(|_| polls < self.limit))
        }
    }

    #[test]
    fn wait_until_services_a_worker_wake() -> Result<()> {
        let poller = Poller::new(None, 0);
        let (polls, wake) = (Arc::clone(&poller.polls), Arc::clone(&poller.wake));
        let mut harness = Harness::new(poller)?;
        let baseline = polls.load(Ordering::SeqCst);
        let handle = wake.lock().expect("wake lock").clone().expect("mounted");
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            handle.wake()
        });
        harness.wait_until(Duration::from_secs(10), |_| {
            Ok(polls.load(Ordering::SeqCst) > baseline)
        })?;
        worker.join().expect("worker")?;
        Ok(())
    }

    #[test]
    fn wait_until_services_polls_due_in_real_time() -> Result<()> {
        let poller = Poller::new(Some(Duration::from_millis(5)), 3);
        let polls = Arc::clone(&poller.polls);
        let mut harness = Harness::new(poller)?;
        harness.wait_until(Duration::from_secs(10), |_| {
            Ok(polls.load(Ordering::SeqCst) >= 3)
        })
    }

    #[test]
    fn wait_until_services_polls_due_under_a_manual_clock() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let poller = Poller::new(Some(Duration::from_secs(60)), 3);
        let polls = Arc::clone(&poller.polls);
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.set_clock_for_testing(Arc::clone(&clock))?;
        canopy.replace_root(poller)?;
        let mut harness = Harness::from_canopy(canopy, Size::new(10, 2))?;
        // An hour of polls passes in a moment, because the test moves the
        // clock while it waits.
        harness.wait_until(Duration::from_secs(10), |_| {
            clock.advance(Duration::from_secs(60))?;
            Ok(polls.load(Ordering::SeqCst) >= 3)
        })
    }

    #[test]
    fn wait_until_fails_when_the_condition_never_holds() -> Result<()> {
        let mut harness = Harness::new(TestNode::new())?;
        let error = harness
            .wait_until(Duration::from_millis(20), |_| Ok(false))
            .expect_err("the wait times out");
        assert!(error.to_string().contains("did not hold"), "{error}");
        // The harness keeps its event channel, so evaluation still works.
        harness.script("canopy.log('after')")?;
        Ok(())
    }

    /// A leaf that carries a value, mounted beside others of its kind.
    struct Leaf(u32);

    impl Widget for Leaf {}

    /// A root that mounts the leaves it is given.
    struct Parent(Vec<u32>);

    impl Widget for Parent {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            for value in self.0.drain(..) {
                ctx.add_child(ctx.node_id(), Leaf(value))?;
            }
            Ok(())
        }
    }

    #[test]
    fn with_unique_finds_the_only_widget_of_a_type() -> Result<()> {
        let mut harness = Harness::new(Parent(vec![7]))?;
        assert_eq!(harness.with_unique(|leaf: &mut Leaf, _| Ok(leaf.0))?, 7);
        assert!(harness.with_unique(|_: &mut Parent, _| Ok(())).is_ok());

        let mut crowded = Harness::new(Parent(vec![1, 2]))?;
        assert!(matches!(
            crowded.with_unique(|_: &mut Leaf, _| Ok(())),
            Err(Error::MultipleMatches)
        ));
        let mut empty = Harness::new(Parent(Vec::new()))?;
        assert!(matches!(
            empty.with_unique(|_: &mut Leaf, _| Ok(())),
            Err(Error::NotFound(_))
        ));
        Ok(())
    }
}
