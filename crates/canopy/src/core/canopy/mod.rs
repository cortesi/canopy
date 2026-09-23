#![expect(
    clippy::multiple_inherent_impl,
    reason = "Canopy methods are split by facade, scripting, rendering, routing, turn, and \
              testing concerns."
)]

use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    thread::{self, ThreadId},
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use super::{
    inputmap,
    notice::{Notice, NoticeSource},
    poll::Poller,
    snapshot::FrameSnapshot,
    termbuf::{RenderLimits, TermBuf},
};

mod builder;
pub use builder::{CanopyBuilder, ScriptTrust};
mod rendering;
#[cfg(test)]
mod rendering_tests;
mod routing;
pub use routing::{RouteTraceEntry, RouteTraceKind};
mod scripting;
use scripting::{ScriptJournal, ScriptState};
pub use scripting::{ScriptJournalEntry, ScriptOrigin};
mod setup;
pub use setup::{Register, Setup};
#[cfg(test)]
mod snapshot_tests;
#[cfg(any(test, feature = "testing"))]
mod testing;
#[cfg(test)]
mod tests;
mod turn;
pub use turn::{
    EvalId, EvalOutcome, EvalRequest, EvalTicket, FrameId, TurnInput, TurnOutcome, TurnSelector,
};
#[cfg(test)]
mod turn_cleanup_tests;
use crate::{
    NodeId, TypedId, commands,
    core::{Core, dump::dump},
    error,
    error::Result,
    geom::Size,
    input::{Event, key::Key},
    style::{StyleMap, default::default_dark},
    widget::Widget,
};

/// Input and wake notifications carried by an application adapter channel.
#[derive(Debug, Clone)]
pub enum AdapterEvent {
    /// Route widget-facing input.
    Input(Event),
    /// Service queued runtime work.
    Wake,
}

/// Hook run against the root context after runtime state it follows changes.
type Hook = fn(&mut dyn crate::Context) -> Result<()>;

/// Named hooks that follow one piece of runtime state through its change
/// counter.
#[derive(Default)]
struct Hooks {
    /// Hooks keyed by name. They run in name order.
    hooks: BTreeMap<&'static str, Hook>,
    /// State generation the hooks last saw.
    synced: u64,
}

impl Hooks {
    /// Register a hook, replacing any hook with the same name.
    fn insert(&mut self, name: &'static str, hook: Hook) {
        self.hooks.insert(name, hook);
    }

    /// Return whether the state changed since the hooks last ran.
    fn pending(&self, generation: u64) -> bool {
        !self.hooks.is_empty() && self.synced != generation
    }

    /// Mark `generation` seen and return the hooks to run for it.
    fn take_due(&mut self, generation: u64) -> Vec<Hook> {
        if !self.pending(generation) {
            return Vec::new();
        }
        self.synced = generation;
        self.hooks.values().copied().collect()
    }
}

/// Application runtime state and renderer coordination.
pub struct Canopy {
    /// Core state.
    pub(super) core: Core,
    /// The poller is responsible for tracking nodes that have pending poll
    /// events.
    poller: Poller,
    /// Frame geometry and the buffers the frame pipeline owns.
    frame: FrameState,
    /// The script host and the sources, callbacks, and registrations it runs.
    pub(crate) script: ScriptState,
    /// Bounded history of script evaluations.
    journal: ScriptJournal,
    /// Hooks run before a frame after the mode stack changes.
    mode_hooks: Hooks,
    /// Hooks run after the shown notice changes.
    notice_hooks: Hooks,
    /// Trace for the most recent key or mouse routing pass.
    route_trace: Vec<RouteTraceEntry>,
    /// Adapter-independent runtime progress.
    driver: turn::Driver,

    /// Event sender channel.
    event_tx: UnboundedSender<AdapterEvent>,
    /// Event receiver channel.
    pub(crate) event_rx: Option<UnboundedReceiver<AdapterEvent>>,
    /// Cross-thread automation callback sender.
    automation_tx: mpsc::SyncSender<turn::AutomationMessage>,
    /// Cross-thread automation callback receiver.
    automation_rx: mpsc::Receiver<turn::AutomationMessage>,
    /// Thread that exclusively owns this application instance.
    ui_thread: ThreadId,

    /// Style map used for rendering.
    style: StyleMap,
}

/// Frame geometry and the buffers the frame pipeline owns.
#[derive(Default)]
struct FrameState {
    /// Root window size.
    screen_size: Option<Size>,
    /// Limits for the materialized visible render target.
    render_limits: RenderLimits,
    /// Last successfully prepared immutable snapshot, which owns its buffer.
    snapshot: Option<Arc<FrameSnapshot>>,
    /// Last successfully emitted terminal frame, invalidated by output failure.
    emitted_buf: Option<Arc<TermBuf>>,
}

/// Callback marshalled onto the UI thread for live automation.
pub type AutomationCallback = Box<dyn FnOnce(&mut Canopy) + Send + 'static>;

/// Maximum queued automation callbacks before producers receive backpressure.
const AUTOMATION_QUEUE_CAPACITY: usize = 256;
/// Maximum automation callbacks serviced during one event-loop turn.
const AUTOMATION_SERVICE_BUDGET: usize = 64;

/// Handle for submitting automation work to a live canopy runloop.
#[derive(Clone)]
pub struct AutomationHandle {
    /// Sender for queued UI-thread callbacks.
    callback_tx: mpsc::SyncSender<turn::AutomationMessage>,
    /// Sender for wake events so the runloop notices queued work.
    wake_tx: UnboundedSender<AdapterEvent>,
    /// Thread that owns the associated Canopy instance.
    ui_thread: ThreadId,
}

impl AutomationHandle {
    /// Queue a callback to run on the UI thread.
    pub fn submit(&self, callback: AutomationCallback) -> Result<()> {
        self.submit_message(turn::AutomationMessage::Callback(callback))
    }

    /// Enqueue a typed driver message and wake its adapter.
    fn submit_message(&self, message: turn::AutomationMessage) -> Result<()> {
        self.callback_tx
            .try_send(message)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    error::Error::Driver("automation callback queue is full".into())
                }
                mpsc::TrySendError::Disconnected(_) => {
                    error::Error::Driver("automation callback channel closed".into())
                }
            })?;
        self.wake_tx
            .unbounded_send(AdapterEvent::Wake)
            .map_err(|_| error::Error::Driver("event loop wake channel closed".into()))?;
        Ok(())
    }

    /// Execute a closure on the UI thread and wait for its result.
    pub fn request<R, F>(&self, callback: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Canopy) -> Result<R> + Send + 'static,
    {
        if thread::current().id() == self.ui_thread {
            return Err(error::Error::Driver(
                "synchronous automation request from the UI thread".into(),
            ));
        }
        let (tx, rx) = mpsc::channel();
        self.submit(Box::new(move |canopy| {
            let _ignored = tx.send(callback(canopy));
        }))?;
        rx.recv()?
    }
}

impl Canopy {
    /// Construct an empty Canopy instance for the consuming builder.
    fn empty() -> Self {
        let (tx, rx) = unbounded();
        let (automation_tx, automation_rx) = mpsc::sync_channel(AUTOMATION_QUEUE_CAPACITY);
        let mut core = Core::new();
        core.invalidate(crate::Invalidation::Paint);
        Self {
            poller: Poller::new(),
            driver: turn::Driver::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            automation_tx,
            automation_rx,
            ui_thread: thread::current().id(),
            route_trace: Vec::new(),
            frame: FrameState::default(),
            script: ScriptState::default(),
            journal: ScriptJournal::default(),
            mode_hooks: Hooks::default(),
            notice_hooks: Hooks::default(),
            style: default_dark(),
            core,
        }
    }

    /// Return a handle for submitting automation work to this app's UI thread.
    pub fn automation_handle(&self) -> AutomationHandle {
        AutomationHandle {
            callback_tx: self.automation_tx.clone(),
            wake_tx: self.event_tx.clone(),
            ui_thread: self.ui_thread,
        }
    }

    /// Return the root node ID.
    pub fn root_id(&self) -> NodeId {
        self.core.root_id()
    }

    /// Replace the root widget while preserving its stable node ID.
    pub fn replace_root<W>(&mut self, widget: W) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        let root = self.root_id();
        self.core.replace_subtree(root, widget)?;
        self.core.invalidate(crate::Invalidation::Paint);
        Ok(TypedId::new(root))
    }

    /// Return the active style map.
    pub fn style(&self) -> &StyleMap {
        &self.style
    }

    /// Mutate the active style map before the next render.
    pub fn style_mut(&mut self) -> &mut StyleMap {
        self.core.invalidate(crate::Invalidation::Paint);
        &mut self.style
    }

    /// Borrow the buffer of the last published frame, if any.
    pub(crate) fn published_buf(&self) -> Option<&TermBuf> {
        self.frame
            .snapshot
            .as_ref()
            .map(|snapshot| &*snapshot.buffer)
    }

    /// Read the last publication without running widget hooks or refreshing
    /// state.
    pub fn snapshot(&self) -> Option<Arc<FrameSnapshot>> {
        self.frame.snapshot.clone()
    }

    /// Prepare pending changes after widget mutation callbacks have returned.
    ///
    /// This is the synchronous boundary for native callers that need snapshots
    /// or geometry before the next driver turn. `turn(TurnInput::Prepare)` also
    /// services queued automation and advances the driver lifecycle.
    pub(crate) fn prepare(&mut self) -> Result<()> {
        if self.core.callback_depth != 0 {
            return Err(error::Error::InvalidPhase {
                operation: "prepare",
            });
        }
        self.prepare_frame(false).map(|_| ())
    }

    /// Run a closure against the root context.
    pub fn with_root_context<R>(
        &mut self,
        f: impl FnOnce(&mut dyn crate::Context) -> Result<R>,
    ) -> Result<R> {
        self.with_context(self.core.root_id(), f)
    }

    /// Run a closure against a mutable context bound to a node.
    pub fn with_context<R>(
        &mut self,
        node: impl Into<NodeId>,
        f: impl FnOnce(&mut dyn crate::Context) -> Result<R>,
    ) -> Result<R> {
        let node = node.into();
        if !self.core.nodes.contains_key(node) {
            return Err(error::Error::NodeNotFound(node));
        }
        self.with_dispatch_boundary(|canopy| {
            canopy.core.invalidate(crate::Invalidation::Layout);
            let mut context = crate::core::context::CoreContext::new(&mut canopy.core, node);
            f(&mut context)
        })
    }

    /// Run work inside a completion boundary so queued teardown drains only
    /// after the outermost callback returns.
    pub(crate) fn with_dispatch_boundary<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<R>,
    ) -> Result<R> {
        let checkpoint = self.core.begin_dispatch();
        let result = f(self);
        let completion = self.core.finish_dispatch(checkpoint, result.is_ok());
        result.and_then(|value| completion.map(|()| value))
    }

    /// Run a closure against an immutable view of the root context.
    pub fn with_root_view<R>(&self, f: impl FnOnce(&dyn crate::ViewContext) -> R) -> R {
        let context = crate::core::context::CoreViewContext::new(&self.core, self.core.root_id());
        f(&context)
    }

    /// Remove an application binding by ID.
    ///
    /// A framework-owned ID returns an error.
    pub(crate) fn unbind(&mut self, id: inputmap::BindingId) -> Result<bool> {
        let Some(target) = self.core.input_map.unbind(id)? else {
            return Ok(false);
        };
        if let inputmap::BindingAction::Script(target) = target {
            self.release_binding_target(target);
        }
        Ok(true)
    }

    /// Remove bindings for an input, optionally filtered by mode and path.
    pub(crate) fn unbind_input(
        &mut self,
        input: inputmap::InputSpec,
        selector: &inputmap::BindingSelector<'_>,
    ) -> usize {
        let removed = self.core.input_map.unbind_input(input, selector);
        self.release_removed_bindings(removed)
    }

    /// Remove every application binding and clear the mode stack.
    ///
    /// Framework bindings stay registered. Returns the count removed.
    pub(crate) fn clear_bindings(&mut self) -> usize {
        let removed = self.core.input_map.clear_application();
        self.release_removed_bindings(removed)
    }

    /// Return the newest active mode, or the empty string for the default
    /// mode.
    pub fn mode(&self) -> &str {
        self.core.input_map.mode()
    }

    /// Replace the active modes with one mode. The empty string returns to
    /// the default mode.
    pub fn set_mode(&mut self, mode: &str) {
        self.core.input_map.set_mode(mode);
    }

    /// Push a mode above the active modes.
    pub fn push_mode(&mut self, mode: &str) {
        self.core.input_map.push_mode(mode);
    }

    /// Push a mode that takes only the next key.
    ///
    /// The next key pops the mode. When the mode binds that key, the binding
    /// runs after the pop and before any widget sees the key. Any other key
    /// only pops the mode.
    pub fn push_transient_mode(&mut self, mode: &str) {
        self.core.input_map.push_transient_mode(mode);
    }

    /// Return whether the mode stack or the shown notice changed since their
    /// hooks last ran.
    pub(super) fn state_hooks_pending(&self) -> bool {
        self.mode_hooks
            .pending(self.core.input_map.mode_generation())
            || self.notice_hooks.pending(self.core.notices.generation())
    }

    /// Run the mode hooks and the notice hooks once for the current state.
    pub(super) fn run_state_hooks(&mut self) -> Result<()> {
        let mut hooks = self
            .mode_hooks
            .take_due(self.core.input_map.mode_generation());
        hooks.extend(self.notice_hooks.take_due(self.core.notices.generation()));
        for hook in hooks {
            self.with_root_context(hook)?;
        }
        Ok(())
    }

    /// Return the retained notices, oldest first.
    ///
    /// A failure from an input binding, a widget handler, or a poll that
    /// [`Error::is_notice`](crate::error::Error::is_notice) classifies as a
    /// notice is recorded here while the application keeps running. The
    /// newest notice is shown until the next input event. The queue keeps the
    /// newest 32.
    pub fn notices(&self) -> &[Notice] {
        self.core.notices.entries()
    }

    /// Record a notice-class failure, or return any other failure.
    fn notice_or_fail(
        &mut self,
        error: error::Error,
        source: NoticeSource,
        node: Option<NodeId>,
    ) -> Result<()> {
        if !error.is_notice() {
            return Err(error);
        }
        self.core.notices.record(Notice::new(&error, source, node));
        Ok(())
    }

    /// Stop showing the newest notice because input arrived, and sync its
    /// hooks at once, so an overlay that showed it hides before hit testing.
    fn dismiss_notice(&mut self) -> Result<()> {
        if !self.core.notices.dismiss() {
            return Ok(());
        }
        for hook in self.notice_hooks.take_due(self.core.notices.generation()) {
            self.with_root_context(hook)?;
        }
        Ok(())
    }

    /// Pop the newest mode and return the newest active mode after the pop.
    pub fn pop_mode(&mut self) -> &str {
        self.core.input_map.pop_mode()
    }

    /// Return the most recent key or mouse route trace.
    pub fn route_trace(&self) -> &[RouteTraceEntry] {
        &self.route_trace
    }

    /// Return command availability using an explicit target policy.
    pub fn command_availability(
        &self,
        target: commands::CommandTarget,
    ) -> Result<Vec<commands::CommandAvailability>> {
        commands::CommandResolver::for_target(&self.core, target).availability()
    }

    /// Return the effective key bindings for a node or the current focus.
    pub fn available_bindings(
        &self,
        focus: Option<NodeId>,
    ) -> Result<super::help::BindingSnapshot> {
        self.core.available_bindings(focus)
    }

    /// Explain where `key` would go for a node or the current focus.
    ///
    /// The result is advisory: it predicts the route from the same resolver
    /// and widget predictions as dispatch, but routing still acts one node at
    /// a time. Use [`Canopy::route_trace`] for what actually happened.
    pub fn explain_key(
        &self,
        focus: Option<NodeId>,
        key: Key,
    ) -> Result<super::keyroute::KeyRouteExplanation> {
        self.core.explain_key(focus, key)
    }

    /// Build a diagnostic dump with tree, focus, and binding details.
    pub(crate) fn diagnostic_dump(&self, target: NodeId) -> String {
        let mut out = String::new();
        let mode = self.core.input_map.mode();
        let target = if self.core.nodes.contains_key(target) {
            target
        } else {
            self.core.root
        };
        let focus_path = self.core.focus_path(self.core.root);
        let target_path = self.core.path_of(self.core.root, target);

        out.push_str("Canopy diagnostics\n");
        out.push_str(&format!("focus: {:?}\n", self.core.focus));
        out.push_str(&format!("focus path: {focus_path}\n"));
        out.push_str(&format!("target: {target:?}\n"));
        out.push_str(&format!("target path: {target_path}\n"));
        out.push_str(&format!("mode: {mode}\n"));
        let group = self
            .core
            .input_map
            .active_framework_group()
            .map_or("(none)", inputmap::FrameworkBindingGroup::as_str);
        out.push_str(&format!("framework group: {group}\n"));

        let mut bindings = self.core.input_map.bindings().iter().collect::<Vec<_>>();
        bindings.sort_by(|left, right| {
            left.input
                .to_string()
                .cmp(&right.input.to_string())
                .then_with(|| left.insertion_id.cmp(&right.insertion_id))
        });
        if bindings.is_empty() {
            out.push_str("bindings: (none)\n");
        } else {
            out.push_str("bindings:\n");
            for binding in bindings {
                let verdict = self.core.binding_verdict(binding.id, target);
                out.push_str(&format!(
                    "  [{:?}] tier={:?} {} {} -> {} ({})\n",
                    binding.id,
                    binding.tier,
                    binding.input,
                    binding.path_filter(),
                    binding.description,
                    verdict.label(),
                ));
            }
        }

        if self.route_trace.is_empty() {
            out.push_str("route trace: (none)\n");
        } else {
            out.push_str("route trace:\n");
            for entry in &self.route_trace {
                out.push_str(&format!(
                    "  {} node={:?} path={} {}\n",
                    entry.kind.label(),
                    entry.node,
                    entry.path,
                    entry.detail
                ));
            }
        }

        out.push_str("\nnode tree:\n");
        match dump(&self.core) {
            Ok(tree) => {
                out.push_str(&tree);
                if !tree.ends_with('\n') {
                    out.push('\n');
                }
            }
            Err(err) => {
                out.push_str(&format!("failed to dump node tree: {err}\n"));
            }
        }

        out
    }
}
