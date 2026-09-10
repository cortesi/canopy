#![expect(
    clippy::multiple_inherent_impl,
    reason = "Canopy methods are split by facade, rendering, and routing concerns."
)]

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt, fs,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, mpsc},
    thread::{self, ThreadId},
    time::Instant,
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ruau::{filesystem::DirectoryMountsError, source::SourceProvider, vm::NativeModule};
use serde::{Deserialize, Serialize};

use super::{
    inputmap,
    poll::Poller,
    snapshot::FrameSnapshot,
    termbuf::{RenderLimits, TermBuf},
};

mod builder;
pub use builder::{CanopyBuilder, ScriptTrust};
mod rendering;
#[cfg(test)]
mod rendering_tests;
#[cfg(test)]
mod snapshot_tests;
mod turn;
#[cfg(test)]
mod turn_cleanup_tests;
#[cfg(any(test, feature = "testing"))]
mod turn_testing;
pub use turn::{EvalId, EvalOutcome, EvalRequest, EvalTicket, FrameId, TurnOutcome, Work};
mod routing;
#[cfg(test)]
mod tests;
use crate::{
    commands::{self, CommandDispatchKind},
    core::{
        Core, NodeId, TypedId,
        dump::dump,
        fixture::{Fixture, FixtureInfo},
    },
    error::{self, Result},
    event::Event,
    geom::Size,
    script,
    style::{StyleMap, solarized},
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

/// Application runtime state and renderer coordination.
pub struct Canopy {
    /// Core state.
    pub(super) core: Core,
    /// The poller is responsible for tracking nodes that have pending poll
    /// events.
    poller: Poller,

    /// Root window size.
    root_size: Option<Size>,
    /// Limits for the materialized visible render target.
    render_limits: RenderLimits,

    /// Script execution host.
    pub(crate) script_host: script::LuauHost,
    /// Cached Luau API definition text.
    script_api_text: Option<String>,
    /// Configured persistent Luau module roots.
    script_module_roots: script::ScriptModuleRoots,
    /// Finalized persistent Luau module source, if any.
    script_module_source: Option<Arc<script::ScriptModuleSource>>,
    /// Extra audited Ruau native modules registered by the app.
    script_native_modules: Vec<Arc<dyn NativeModule>>,
    /// App-level startup scripts run before user and project init files.
    startup_scripts: Vec<StartupScript>,
    /// Successfully executed filesystem startup modules.
    completed_startup_modules: HashSet<PathBuf>,
    /// Compiled handles retained across filesystem startup retries.
    startup_module_scripts: HashMap<PathBuf, script::ScriptId>,
    /// Binding targets whose release is deferred until a startup attempt
    /// commits.
    deferred_binding_releases: Option<Vec<script::LuauFunctionId>>,
    /// In-memory journal of script evaluations.
    script_journal: Vec<ScriptJournalEntry>,
    /// Stack of active script dispatch anchors for the current VM invocation.
    pub(crate) script_context_stack: Vec<NodeId>,
    /// Next journal entry id; never reused, even after the journal is cleared.
    script_journal_next_id: u64,
    /// Maximum number of retained journal entries; oldest are evicted first.
    script_journal_limit: usize,
    /// Registered default binding scripts keyed by owner name.
    default_bindings: HashMap<String, DefaultBindingsScript>,
    /// Registered named fixtures keyed by fixture name.
    fixtures: HashMap<String, Fixture>,
    /// Trace for the most recent key or mouse routing pass.
    route_trace: Vec<RouteTraceEntry>,

    /// Cached terminal buffer.
    termbuf: Option<TermBuf>,
    /// Last successfully prepared immutable observation.
    snapshot: Option<Arc<FrameSnapshot>>,
    /// Last successfully emitted terminal frame.
    emitted_buf: Option<TermBuf>,
    /// Adapter-independent runtime progress.
    driver: turn::Driver,
    /// Whether a render is pending after the most recent event.
    render_pending: bool,

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

/// A phase in key or mouse event routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePhase {
    /// The initial routing target was selected.
    Target,
    /// A binding matched before the widget received the event.
    PreEventBinding,
    /// The event was offered to a widget.
    WidgetEvent,
    /// A binding matched after the widget ignored the event.
    PostEventBinding,
    /// Routing moved from a node to its parent.
    Bubble,
    /// A resolved binding is being executed.
    BindingExecution,
    /// A widget or binding handled the event.
    Handled,
    /// Routing ended without a handler.
    Unhandled,
}

impl RoutePhase {
    /// Return a stable diagnostic label for this phase.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::PreEventBinding => "pre-event-binding",
            Self::WidgetEvent => "widget-event",
            Self::PostEventBinding => "post-event-binding",
            Self::Bubble => "bubble",
            Self::BindingExecution => "binding-execution",
            Self::Handled => "handled",
            Self::Unhandled => "unhandled",
        }
    }
}

/// One entry in the most recent input route trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTraceEntry {
    /// Routing phase.
    pub phase: RoutePhase,
    /// Node associated with this route step.
    pub node: Option<NodeId>,
    /// Path visible to binding resolution at this route step.
    pub path: String,
    /// Human-readable route detail.
    pub detail: String,
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
                    error::Error::RunLoop("automation callback queue is full".into())
                }
                mpsc::TrySendError::Disconnected(_) => {
                    error::Error::RunLoop("automation callback channel closed".into())
                }
            })?;
        self.wake_tx
            .unbounded_send(AdapterEvent::Wake)
            .map_err(|_| error::Error::RunLoop("event loop wake channel closed".into()))?;
        Ok(())
    }

    /// Execute a closure on the UI thread and wait for its result.
    pub fn request<R, F>(&self, callback: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Canopy) -> Result<R> + Send + 'static,
    {
        if thread::current().id() == self.ui_thread {
            return Err(error::Error::RunLoop(
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

/// Registered default binding script metadata.
struct DefaultBindingsScript {
    /// Source text evaluated for this owner.
    source: String,
    /// Pre-compiled script handle available after `finalize_api()`.
    script_id: Option<script::ScriptId>,
}

/// Registered app startup script metadata.
struct StartupScript {
    /// Human-readable startup script name.
    name: String,
    /// Source text evaluated during startup.
    source: String,
    /// Pre-compiled script handle available after `finalize_api()`.
    script_id: Option<script::ScriptId>,
    /// Whether this script completed successfully.
    ran: bool,
}

/// Reversible callback and binding state for one startup script attempt.
struct StartupAttempt {
    /// Application-owned input state before the script ran.
    application_bindings: inputmap::ApplicationBindingSnapshot,
    /// Deferred hook queue before the script ran.
    hooks: Vec<script::LuauFunctionId>,
}

/// Paired implementation and declaration module found under a script root.
struct ScriptDeclarationPair {
    /// Implementation source path.
    implementation_path: PathBuf,
    /// Declaration source path.
    declaration_path: PathBuf,
}

/// Default maximum number of retained script journal entries.
const DEFAULT_SCRIPT_JOURNAL_LIMIT: usize = 1024;

/// Baseline captured when a journaled script evaluation begins.
///
/// Nested evaluations record only the logs and assertions they add on top of
/// the enclosing evaluation's state.
#[derive(Clone, Copy)]
pub struct ScriptJournalBaseline {
    /// Evaluation start time.
    started: Instant,
    /// Log count at evaluation start.
    logs: usize,
    /// Assertion count at evaluation start.
    assertions: usize,
}

/// Data needed to run a default-bindings script after dropping the Canopy
/// borrow.
pub struct DefaultBindingsRun {
    /// Script host that owns the retained runtime.
    pub(crate) host: script::LuauHost,
    /// Node anchor for the nested default-bindings run.
    pub(crate) root_id: NodeId,
    /// Compiled default-bindings script id.
    pub(crate) script_id: script::ScriptId,
    /// Source text recorded in the script journal.
    pub(crate) source: String,
    /// Journal baseline captured before the nested run.
    baseline: ScriptJournalBaseline,
}

/// Typed source of one script-journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ScriptOrigin {
    /// Top-level application evaluation.
    Eval,
    /// Builder configuration loaded from a path.
    Config(String),
    /// Application or mounted startup source.
    Startup(String),
    /// Builder-owned binding source.
    Bindings(String),
    /// Widget default-binding source.
    DefaultBindings(String),
    /// Origin retained from a journal written by another producer.
    Other(String),
}

impl fmt::Display for ScriptOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eval => formatter.write_str("eval"),
            Self::Config(value) => write!(formatter, "config:{value}"),
            Self::Startup(value) => write!(formatter, "startup:{value}"),
            Self::Bindings(value) => write!(formatter, "bindings:{value}"),
            Self::DefaultBindings(value) => write!(formatter, "default-bindings:{value}"),
            Self::Other(value) => formatter.write_str(value),
        }
    }
}

impl From<ScriptOrigin> for String {
    fn from(origin: ScriptOrigin) -> Self {
        origin.to_string()
    }
}

impl From<String> for ScriptOrigin {
    fn from(origin: String) -> Self {
        if origin == "eval" {
            Self::Eval
        } else if let Some(value) = origin.strip_prefix("config:") {
            Self::Config(value.to_owned())
        } else if let Some(value) = origin.strip_prefix("startup:") {
            Self::Startup(value.to_owned())
        } else if let Some(value) = origin.strip_prefix("bindings:") {
            Self::Bindings(value.to_owned())
        } else if let Some(value) = origin.strip_prefix("default-bindings:") {
            Self::DefaultBindings(value.to_owned())
        } else {
            Self::Other(origin)
        }
    }
}

/// Replayable record of one script evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptJournalEntry {
    /// Monotonic journal id.
    pub id: u64,
    /// Typed origin serialized as the established journal string.
    pub origin: ScriptOrigin,
    /// Evaluated source text.
    pub source: String,
    /// Whether the evaluation completed successfully.
    pub ok: bool,
    /// Error message when `ok` is false.
    pub error: Option<String>,
    /// Logs emitted by the script.
    pub logs: Vec<String>,
    /// Assertions emitted by the script.
    pub assertions: Vec<script::ScriptAssertion>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

impl Canopy {
    /// Construct an empty Canopy instance for the consuming builder.
    fn empty() -> Self {
        let (tx, rx) = unbounded();
        let (automation_tx, automation_rx) = mpsc::sync_channel(AUTOMATION_QUEUE_CAPACITY);
        let core = Core::new();
        Self {
            poller: Poller::new(),
            driver: turn::Driver::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            automation_tx,
            automation_rx,
            ui_thread: thread::current().id(),
            route_trace: Vec::new(),
            script_host: script::LuauHost::new(),
            script_api_text: None,
            script_module_roots: script::ScriptModuleRoots::default(),
            script_module_source: None,
            script_native_modules: Vec::new(),
            startup_scripts: Vec::new(),
            completed_startup_modules: HashSet::new(),
            startup_module_scripts: HashMap::new(),
            deferred_binding_releases: None,
            script_journal: Vec::new(),
            script_context_stack: Vec::new(),
            script_journal_next_id: 1,
            script_journal_limit: DEFAULT_SCRIPT_JOURNAL_LIMIT,
            default_bindings: HashMap::new(),
            fixtures: HashMap::new(),
            style: solarized::solarized_dark(),
            root_size: None,
            render_limits: RenderLimits::default(),
            termbuf: None,
            snapshot: None,
            emitted_buf: None,
            render_pending: true,
            core,
        }
    }

    /// Construct an unbuilt Canopy instance for low-level tests.
    #[cfg(any(test, feature = "testing"))]
    pub fn new() -> Self {
        Self::empty()
    }

    /// Return whether the application API has been finalized by its builder.
    pub fn is_api_finalized(&self) -> bool {
        self.script_host.is_finalized()
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

    /// Replace the visible render-target limits.
    pub fn set_render_limits(&mut self, limits: RenderLimits) -> Result<()> {
        if let Some(size) = self.root_size {
            limits.cell_count(size)?;
        }
        self.render_limits = limits;
        Ok(())
    }

    /// Create a detached widget node.
    pub fn create_detached<W>(&mut self, widget: W) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        Ok(TypedId::new(self.core.create_detached(widget)?))
    }

    /// Replace the root widget while preserving its stable node ID.
    pub fn replace_root<W>(&mut self, widget: W) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        let root = self.root_id();
        self.core.replace_subtree(root, widget)?;
        self.render_pending = true;
        Ok(TypedId::new(root))
    }

    /// Return the active style map.
    pub fn style(&self) -> &StyleMap {
        &self.style
    }

    /// Mutate the active style map before the next render.
    pub fn style_mut(&mut self) -> &mut StyleMap {
        self.render_pending = true;
        &mut self.style
    }

    /// Get a reference to the current render buffer, if any.
    pub fn buf(&self) -> Option<&TermBuf> {
        self.termbuf.as_ref()
    }

    /// Read the last publication without running widget hooks or refreshing
    /// state.
    pub fn snapshot(&self) -> Option<Arc<FrameSnapshot>> {
        self.snapshot.clone()
    }

    /// Prepare pending changes after widget mutation callbacks have returned.
    ///
    /// This is the synchronous boundary for native callers that need snapshots
    /// or geometry before the next driver turn. `turn(Work::Prepare)` also
    /// services queued automation and advances the driver lifecycle.
    pub fn flush(&mut self) -> Result<()> {
        if self.core.callback_depth != 0 {
            return Err(error::Error::InvalidPhase { operation: "flush" });
        }
        self.prepare_frame(false).map(|_| ())
    }

    /// Evaluate a Luau source string at the root and return its value.
    pub fn eval_script(&mut self, source: &str) -> Result<commands::ArgValue> {
        let outcome = self.eval(EvalRequest {
            source: source.to_owned(),
            timeout: None,
            anchor: self.root_id(),
        })?;
        outcome.into_result()
    }

    /// Finalize the script API surface if an evaluation needs it.
    fn ensure_finalized(&mut self) -> Result<()> {
        if self.script_host.is_finalized() {
            return Ok(());
        }
        self.finalize_api_inner()
    }

    /// Configure the `@user` persistent script root for the builder.
    fn set_user_script_root_inner(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.ensure_api_unfinalized("script module roots")?;
        self.script_module_roots.set_user_root(root);
        Ok(())
    }

    /// Configure the `@project` persistent script root for the builder.
    fn set_project_script_root_inner(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.ensure_api_unfinalized("script module roots")?;
        self.script_module_roots.set_project_root(root);
        Ok(())
    }

    /// Configure the `@user` persistent script root in a low-level test.
    #[cfg(any(test, feature = "testing"))]
    pub fn set_user_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.set_user_script_root_inner(root)
    }

    /// Configure the `@project` persistent script root in a low-level test.
    #[cfg(any(test, feature = "testing"))]
    pub fn set_project_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.set_project_script_root_inner(root)
    }

    /// Invalidate cached exports from persistent script modules.
    ///
    /// Pass a root such as `@user` or `@project` to invalidate one root, or
    /// `None` to invalidate every root. Returns the new source epoch, or
    /// `None` when no module source is configured or the named root is
    /// unknown.
    pub fn invalidate_script_modules(&mut self, root: Option<&str>) -> Result<Option<u64>> {
        if self.script_host.is_eval_active() {
            return Err(error::Error::script_structured(
                error::ScriptErrorKind::ScriptBusy,
                "cannot reload modules while evaluation is active",
            ));
        }
        let Some(source) = self.script_module_source.as_ref() else {
            return Ok(None);
        };
        let epoch = match root {
            Some(root) => match source.invalidate(root) {
                Ok(epoch) => epoch,
                Err(_) => return Ok(None),
            },
            None => source.invalidate_all(),
        };
        self.clear_script_callbacks();
        Ok(Some(epoch))
    }

    /// Register an audited Ruau native module on the same surface as Canopy
    /// commands.
    pub fn register_script_module(&mut self, module: Arc<dyn NativeModule>) -> Result<()> {
        self.ensure_api_unfinalized("script native module registration")?;
        self.script_native_modules.push(module);
        Ok(())
    }

    /// Register an app-level startup script.
    pub fn register_startup_script(&mut self, name: &str, source: &str) -> Result<()> {
        self.ensure_api_unfinalized("startup script registration")?;
        if name.trim().is_empty() {
            return Err(error::Error::Invalid(
                "startup script name cannot be empty".into(),
            ));
        }
        if let Some(existing) = self
            .startup_scripts
            .iter()
            .find(|script| script.name == name)
        {
            if existing.source == source {
                return Ok(());
            }
            return Err(error::Error::Invalid(format!(
                "conflicting startup script already registered for {name}"
            )));
        }
        self.startup_scripts.push(StartupScript {
            name: name.to_string(),
            source: source.to_string(),
            script_id: None,
            ran: false,
        });
        Ok(())
    }

    /// Require every startup script root to define a typed global.
    pub fn require_startup_global(&mut self, name: &str, type_text: &str) -> Result<()> {
        self.ensure_api_unfinalized("startup global requirement")?;
        self.script_host.require_startup_global(name, type_text)
    }

    /// Run app, user, and project startup scripts during preparation.
    fn run_startup_scripts_inner(&mut self) -> Result<usize> {
        self.driver.startup_attempted = true;
        self.ensure_finalized()?;
        let host = self.script_host.clone();
        let mut ran = 0;
        let startup_scripts = self
            .startup_scripts
            .iter()
            .enumerate()
            .filter(|(_, script)| !script.ran)
            .map(|script| {
                let (index, script) = script;
                let script_id = script
                    .script_id
                    .expect("startup scripts are compiled during finalize_api()");
                (index, script.name.clone(), script.source.clone(), script_id)
            })
            .collect::<Vec<_>>();
        for (index, name, source, script_id) in startup_scripts {
            self.run_startup_attempt(ScriptOrigin::Startup(name), &source, script_id)?;
            self.startup_scripts[index].ran = true;
            ran += 1;
        }
        for module in self.script_module_roots.startup_modules() {
            if self.completed_startup_modules.contains(&module.path) {
                continue;
            }
            let mounted_source = self
                .script_module_source
                .as_ref()
                .expect("startup modules require a finalized filesystem source")
                .source_for_path(&module.path)
                .map_err(|err| {
                    error::Error::Invalid(format!(
                        "{} startup script read failed: {err}",
                        module.namespace.name()
                    ))
                })?;
            let mounted_source = mounted_source.source();
            let source = mounted_source
                .as_str()
                .expect("filesystem sources are validated as UTF-8")
                .to_string();
            let module_id = mounted_source.id().clone();
            let script_id = match self.startup_module_scripts.get(&module.path).copied() {
                Some(script_id) => script_id,
                None => {
                    let script_id = host.compile_startup_source(mounted_source)?;
                    self.startup_module_scripts
                        .insert(module.path.clone(), script_id);
                    script_id
                }
            };
            self.run_startup_attempt(
                ScriptOrigin::Startup(module_id.to_string()),
                &source,
                script_id,
            )?;
            self.completed_startup_modules.insert(module.path);
            ran += 1;
        }
        Ok(ran)
    }

    /// Run startup scripts directly in a low-level test.
    #[cfg(any(test, feature = "testing"))]
    pub fn run_startup_scripts(&mut self) -> Result<usize> {
        self.run_startup_scripts_inner()
    }

    /// Execute one startup script with callback and binding rollback.
    fn run_startup_attempt(
        &mut self,
        origin: ScriptOrigin,
        source: &str,
        script_id: script::ScriptId,
    ) -> Result<()> {
        let attempt = self.begin_startup_attempt();
        let baseline = self.begin_script_journal();
        let host = self.script_host.clone();
        let result = host
            .execute(self, self.core.root_id(), script_id, None)
            .map(|_| ());
        self.record_script_journal(origin, source, baseline, &result);
        if result.is_ok() {
            self.commit_startup_attempt();
        } else {
            self.rollback_startup_attempt(attempt);
        }
        result
    }

    /// Snapshot registries and begin deferring callback releases.
    fn begin_startup_attempt(&mut self) -> StartupAttempt {
        debug_assert!(self.deferred_binding_releases.is_none());
        let attempt = StartupAttempt {
            application_bindings: self.core.input_map.snapshot_application(),
            hooks: self.script_host.on_start_hooks(),
        };
        self.deferred_binding_releases = Some(Vec::new());
        attempt
    }

    /// Commit a startup attempt and release targets it replaced or removed.
    fn commit_startup_attempt(&mut self) {
        let releases = self.deferred_binding_releases.take().unwrap_or_default();
        for id in releases {
            self.script_host.release_function(id);
        }
    }

    /// Restore registries after a failed startup attempt and release only its
    /// callbacks.
    fn rollback_startup_attempt(&mut self, attempt: StartupAttempt) {
        let new_targets = self
            .core
            .input_map
            .targets_not_in(&attempt.application_bindings);
        self.core
            .input_map
            .restore_application(attempt.application_bindings);
        self.deferred_binding_releases = None;
        for id in new_targets {
            self.script_host.release_function(id);
        }

        let baseline_hooks = attempt.hooks.iter().copied().collect::<HashSet<_>>();
        let current_hooks = self.script_host.replace_on_start_hooks(attempt.hooks);
        for hook in current_hooks {
            if !baseline_hooks.contains(&hook) {
                self.script_host.release_function(hook);
            }
        }
    }

    /// Register a Luau script as the default bindings for a widget namespace.
    pub fn register_default_bindings(&mut self, name: &str, script: &str) -> Result<()> {
        self.ensure_api_unfinalized("default binding registration")?;
        if name.trim().is_empty() {
            return Err(error::Error::Invalid(
                "default binding owner name cannot be empty".into(),
            ));
        }
        if self.owner_has_default_bindings_command(name) {
            return Err(error::Error::Invalid(format!(
                "owner {name} already defines a command named default_bindings"
            )));
        }
        if let Some(existing) = self.default_bindings.get(name) {
            if existing.source == script {
                return Ok(());
            }
            return Err(error::Error::Invalid(format!(
                "conflicting default bindings already registered for owner {name}"
            )));
        }
        self.default_bindings.insert(
            name.to_string(),
            DefaultBindingsScript {
                source: script.to_string(),
                script_id: None,
            },
        );
        Ok(())
    }

    /// Register a named fixture available to headless and live automation.
    pub fn register_fixture(&mut self, fixture: Fixture) -> Result<()> {
        self.ensure_api_unfinalized("fixture registration")?;
        if fixture.name.trim().is_empty() {
            return Err(error::Error::Invalid("fixture name cannot be empty".into()));
        }
        if let Some(existing) = self.fixtures.get(&fixture.name) {
            if existing.description == fixture.description {
                return Ok(());
            }
            return Err(error::Error::Invalid(format!(
                "conflicting fixture already registered for {}",
                fixture.name
            )));
        }
        self.fixtures.insert(fixture.name.clone(), fixture);
        Ok(())
    }

    /// Return registered fixture metadata in stable name order.
    pub fn fixture_infos(&self) -> Vec<FixtureInfo> {
        let mut fixtures = self
            .fixtures
            .values()
            .map(Fixture::info)
            .collect::<Vec<_>>();
        fixtures.sort_by(|left, right| left.name.cmp(&right.name));
        fixtures
    }

    /// Apply a named fixture to the current app instance.
    pub fn apply_fixture(&mut self, name: &str) -> Result<()> {
        let setup = self
            .fixtures
            .get(name)
            .map(|fixture| Arc::clone(&fixture.setup))
            .ok_or_else(|| error::Error::NotFound(format!("fixture {name}")))?;
        setup(self)?;
        self.render_pending = true;
        Ok(())
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

    /// Type-check a named Luau source against the finalized app API.
    pub fn check_script(
        &mut self,
        source_name: &str,
        source: &str,
    ) -> Result<script::ScriptCheckResult> {
        self.ensure_finalized()?;
        self.script_host.check_script(source_name, source)
    }

    /// Drain and return log lines recorded by the most recent script
    /// evaluation.
    pub fn take_script_logs(&mut self) -> Vec<String> {
        self.script_host.take_logs()
    }

    /// Drain and return assertion outcomes from the most recent script
    /// evaluation.
    pub fn take_script_assertions(&mut self) -> Vec<script::ScriptAssertion> {
        self.script_host.take_assertions()
    }

    /// Return the in-memory script evaluation journal.
    ///
    /// The journal retains the most recent entries up to the configured limit.
    /// Entry ids are monotonic and never reused, so a first id greater than
    /// one indicates that older entries were evicted or cleared.
    pub fn script_journal(&self) -> &[ScriptJournalEntry] {
        &self.script_journal
    }

    /// Set the maximum number of retained script journal entries.
    ///
    /// When the journal exceeds the limit the oldest entries are evicted. A
    /// limit of zero disables retention entirely.
    pub fn set_script_journal_limit(&mut self, limit: usize) {
        self.script_journal_limit = limit;
        self.enforce_script_journal_limit();
    }

    /// Evaluate a Luau config file during builder setup.
    fn run_config_inner(&mut self, path: &FsPath) -> Result<()> {
        let baseline = self.begin_script_journal();
        let source = fs::read_to_string(path)
            .map_err(|err| error::Error::Invalid(format!("config read failed: {err}")))?;
        let result = (|| {
            self.ensure_finalized()?;
            let mounted_source = match &self.script_module_source {
                Some(mounts) => match mounts.source_for_path(path) {
                    Ok(source) => Some(source),
                    Err(DirectoryMountsError::OutsideRoots { .. }) => None,
                    Err(error) => {
                        return Err(error::Error::Invalid(format!(
                            "config path is invalid for script module roots: {error}"
                        )));
                    }
                },
                None => None,
            };
            let script_id = match mounted_source {
                Some(source) => self.script_host.compile_source(source.source())?,
                None => self.script_host.compile(&source)?,
            };
            let host = self.script_host.clone();
            host.execute(self, self.core.root_id(), script_id, None)
                .map(|_| ())
        })();
        self.record_script_journal(
            ScriptOrigin::Config(path.display().to_string()),
            &source,
            baseline,
            &result,
        );
        result
    }

    /// Evaluate a Luau config file directly in a low-level test.
    #[cfg(any(test, feature = "testing"))]
    pub fn run_config(&mut self, path: &FsPath) -> Result<()> {
        self.run_config_inner(path)
    }

    /// Install an idempotent framework-owned command binding.
    pub fn bind_framework(
        &mut self,
        group: inputmap::FrameworkBindingGroup,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        command: commands::CommandCall,
    ) -> Result<inputmap::BindingId> {
        self.core
            .input_map
            .bind_framework(group, input, options, command.action())
    }

    /// Install or replace an application command binding.
    ///
    /// An omitted command target resolves from the node where the binding wins.
    pub fn bind_command(
        &mut self,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        command: commands::CommandCall,
    ) -> Result<inputmap::BindingId> {
        let (id, removed) = self.core.input_map.replace_application_action(
            input.into(),
            options,
            inputmap::BindingTarget::Command(command.action()),
        )?;
        self.release_removed_bindings(removed);
        Ok(id)
    }

    /// Remove an application binding by ID.
    ///
    /// A framework-owned ID returns an error.
    pub fn unbind(&mut self, id: inputmap::BindingId) -> Result<bool> {
        let Some(target) = self.core.input_map.unbind(id)? else {
            return Ok(false);
        };
        if let inputmap::BindingTarget::Script(target) = target {
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

    /// Remove all bindings from all modes.
    pub fn clear_bindings(&mut self) -> usize {
        let removed = self.core.input_map.clear_application();
        self.release_removed_bindings(removed)
    }

    /// Remove application bindings and callbacks from the current source epoch.
    fn clear_script_callbacks(&mut self) {
        let removed = self.core.input_map.clear_application();
        self.release_removed_bindings(removed);
        for hook in self.script_host.drain_on_start_hooks() {
            self.script_host.release_function(hook);
        }
    }

    /// Return the active input mode.
    pub fn input_mode(&self) -> &str {
        self.core.input_map.current_mode()
    }

    /// Set the active input mode.
    pub fn set_input_mode(&mut self, mode: &str) {
        self.core.input_map.set_mode(mode);
    }

    /// Push an input mode above the current mode.
    pub fn push_input_mode(&mut self, mode: &str) {
        self.core.input_map.push_mode(mode);
    }

    /// Pop the top input mode and return the new active mode.
    pub fn pop_input_mode(&mut self) -> &str {
        self.core.input_map.pop_mode()
    }

    /// Return the most recent key or mouse route trace.
    pub fn route_trace(&self) -> &[RouteTraceEntry] {
        &self.route_trace
    }

    /// Load the commands from a command node using the default node name.
    /// Returns an error if any command id is already registered.
    pub fn add_commands<T: commands::CommandNode>(&mut self) -> Result<()> {
        self.ensure_api_unfinalized("command registration")?;
        let cmds = <T>::commands();
        self.core.commands.add(cmds)?;
        Ok(())
    }

    /// Finalize the script API surface for the consuming builder.
    fn finalize_api_inner(&mut self) -> Result<()> {
        if self.script_host.is_finalized() {
            return Ok(());
        }
        let module_source = self.script_module_roots.module_source().map_err(|error| {
            error::Error::Invalid(format!("script module roots are invalid: {error}"))
        })?;
        let surface_source = module_source
            .as_ref()
            .map(|source| Arc::clone(source) as Arc<dyn SourceProvider>);
        let default_binding_owners = self.default_binding_owners();
        let existing_scripts = self.script_host.script_ids();
        let default_script_ids = self
            .default_bindings
            .iter()
            .map(|(owner, script)| (owner.clone(), script.script_id))
            .collect::<HashMap<_, _>>();
        let startup_script_ids = self
            .startup_scripts
            .iter()
            .map(|script| script.script_id)
            .collect::<Vec<_>>();
        let definitions = self.script_host.prepare_finalize(
            &self.core.commands,
            &default_binding_owners,
            &self.script_native_modules,
            surface_source,
            &self.fixture_infos(),
        )?;
        let prepared = (|| {
            self.validate_script_module_declarations(module_source.as_ref())?;
            self.script_host
                .finalize_checkpoint(script::FinalizeStep::DeclarationsValidated)?;
            self.compile_registered_default_bindings()?;
            self.script_host
                .finalize_checkpoint(script::FinalizeStep::DefaultBindingsCompiled)?;
            self.compile_registered_startup_scripts()?;
            self.script_host
                .finalize_checkpoint(script::FinalizeStep::StartupScriptsCompiled)?;
            self.script_host.publish_finalize()
        })();
        if let Err(error) = prepared {
            self.script_host.abort_finalize(&existing_scripts);
            for (owner, script) in &mut self.default_bindings {
                script.script_id = default_script_ids.get(owner).copied().flatten();
            }
            for (script, previous) in self.startup_scripts.iter_mut().zip(startup_script_ids) {
                script.script_id = previous;
            }
            return Err(error);
        }
        self.script_module_source = module_source;
        self.script_api_text = Some(definitions);
        Ok(())
    }

    /// Finalize the script API surface for a low-level test application.
    #[cfg(any(test, feature = "testing"))]
    pub fn finalize_api(&mut self) -> Result<()> {
        self.finalize_api_inner()
    }

    /// Return the rendered Luau definition file for a ready app.
    pub fn script_api(&self) -> Result<&str> {
        self.script_api_text.as_deref().ok_or_else(|| {
            error::Error::InvalidOperation("script API is not finalized".to_string())
        })
    }

    /// Prepare a registered default binding script for a nested scoped run.
    pub(crate) fn prepare_registered_default_bindings(
        &self,
        owner: &str,
    ) -> Result<DefaultBindingsRun> {
        let script = self.default_bindings.get(owner).ok_or_else(|| {
            error::Error::NotFound(format!("default bindings not registered for owner {owner}"))
        })?;
        let script_id = script.script_id.ok_or_else(|| {
            error::Error::NotFound(format!("default bindings not compiled for owner {owner}"))
        })?;
        let source = script.source.clone();
        let host = self.script_host.clone();
        let baseline = self.begin_script_journal();
        Ok(DefaultBindingsRun {
            host,
            root_id: self.core.root_id(),
            script_id,
            source,
            baseline,
        })
    }

    /// Record a nested default-bindings run after it completes.
    pub(crate) fn record_registered_default_bindings(
        &mut self,
        owner: &str,
        run: &DefaultBindingsRun,
        result: &Result<()>,
    ) {
        self.record_script_journal(
            ScriptOrigin::DefaultBindings(owner.to_owned()),
            &run.source,
            run.baseline,
            result,
        );
    }

    /// Return true if the named owner already exports a `default_bindings`
    /// command.
    fn owner_has_default_bindings_command(&self, owner: &str) -> bool {
        self.core.commands.iter().any(|(_, spec)| {
            matches!(spec.dispatch, CommandDispatchKind::Node { owner: spec_owner } if spec_owner == owner)
                && spec.name == "default_bindings"
        })
    }

    /// Ensure the script surface can still be extended.
    fn ensure_api_unfinalized(&self, subject: &str) -> Result<()> {
        if self.script_host.is_finalized() {
            return Err(error::Error::InvalidOperation(format!(
                "{subject} is sealed after finalize_api()"
            )));
        }
        Ok(())
    }

    /// Validate paired `.luau`/`.d.luau` modules under persistent roots.
    fn validate_script_module_declarations(
        &self,
        module_source: Option<&Arc<script::ScriptModuleSource>>,
    ) -> Result<()> {
        let Some(surface) = self.script_host.surface() else {
            return Ok(());
        };
        let mut failures = Vec::new();
        for pair in self.script_declaration_pairs(module_source)? {
            let implementation_source =
                fs::read_to_string(&pair.implementation_path).map_err(|err| {
                    error::Error::Invalid(format!(
                        "script implementation read failed for {}: {err}",
                        pair.implementation_path.display()
                    ))
                })?;
            let declaration_source = fs::read_to_string(&pair.declaration_path).map_err(|err| {
                error::Error::Invalid(format!(
                    "script declaration read failed for {}: {err}",
                    pair.declaration_path.display()
                ))
            })?;
            let check = surface.check_conformance(&implementation_source, &declaration_source);
            if check.is_ok() {
                continue;
            }
            let result = script::ScriptCheckResult::from_diagnostics(
                check
                    .diagnostics()
                    .records()
                    .map(|diagnostic| {
                        script::diagnostic_record_to_script(
                            Some(pair.implementation_path.display().to_string()),
                            diagnostic,
                        )
                    })
                    .collect(),
            );
            failures.push(format!(
                "{}:\n{}",
                pair.declaration_path.display(),
                result.format_diagnostics()
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(error::Error::Parse(error::ParseError::new(
                failures.join("\n"),
            )))
        }
    }

    /// Return paired implementation/declaration modules under configured roots.
    fn script_declaration_pairs(
        &self,
        module_source: Option<&Arc<script::ScriptModuleSource>>,
    ) -> Result<Vec<ScriptDeclarationPair>> {
        let mut pairs = Vec::new();
        for root in [
            self.script_module_roots.user_root(),
            self.script_module_roots.project_root(),
        ]
        .into_iter()
        .flatten()
        {
            let source =
                module_source.expect("configured roots have a finalized filesystem source");
            collect_script_declaration_pairs(source, root, &mut pairs)?;
        }
        Ok(pairs)
    }

    /// Begin a journaled script evaluation and capture its diagnostics
    /// baseline.
    fn begin_script_journal(&self) -> ScriptJournalBaseline {
        // Top-level evaluations clear diagnostics on entry, so their baseline
        // is empty; nested evaluations record only what they add.
        let (logs, assertions) = if script::in_live_scope(self) {
            self.script_host.diagnostics_counts()
        } else {
            (0, 0)
        };
        ScriptJournalBaseline {
            started: self.now(),
            logs,
            assertions,
        }
    }

    /// Append a script evaluation to the in-memory journal.
    fn record_script_journal<T>(
        &mut self,
        origin: ScriptOrigin,
        source: &str,
        baseline: ScriptJournalBaseline,
        result: &Result<T>,
    ) {
        let duration_ms = u64::try_from(
            self.now()
                .saturating_duration_since(baseline.started)
                .as_millis(),
        )
        .unwrap_or(u64::MAX);
        let mut logs = self.script_host.logs();
        let logs = logs.split_off(baseline.logs.min(logs.len()));
        let mut assertions = self.script_host.assertions();
        let assertions = assertions.split_off(baseline.assertions.min(assertions.len()));
        let id = self.script_journal_next_id;
        self.script_journal_next_id += 1;
        self.script_journal.push(ScriptJournalEntry {
            id,
            origin,
            source: source.to_string(),
            ok: result.is_ok(),
            error: result.as_ref().err().map(ToString::to_string),
            logs,
            assertions,
            duration_ms,
        });
        self.enforce_script_journal_limit();
    }

    /// Evict the oldest journal entries beyond the retention limit.
    fn enforce_script_journal_limit(&mut self) {
        if self.script_journal.len() > self.script_journal_limit {
            let excess = self.script_journal.len() - self.script_journal_limit;
            self.script_journal.drain(..excess);
        }
    }

    /// Return the set of owners with registered default binding scripts.
    fn default_binding_owners(&self) -> BTreeSet<String> {
        self.default_bindings.keys().cloned().collect()
    }

    /// Compile any registered default binding scripts after finalization.
    fn compile_registered_default_bindings(&mut self) -> Result<()> {
        let host = self.script_host.clone();
        let mut scripts = self.default_bindings.iter_mut().collect::<Vec<_>>();
        scripts.sort_by_key(|(a, _)| a.as_str());
        for (_, script) in scripts {
            if script.script_id.is_none() {
                script.script_id = Some(host.compile(&script.source)?);
            }
        }
        Ok(())
    }

    /// Compile any registered startup scripts after finalization.
    fn compile_registered_startup_scripts(&mut self) -> Result<()> {
        let host = self.script_host.clone();
        for script in &mut self.startup_scripts {
            if script.script_id.is_none() {
                script.script_id = Some(host.compile_startup_named(
                    &script.source,
                    format!("startup/{}", script.name).as_bytes(),
                )?);
            }
        }
        Ok(())
    }

    /// Execute and release all queued startup hooks.
    fn run_on_start_hooks(&mut self) -> Result<bool> {
        let host = self.script_host.clone();
        let mut ran = false;
        while host.has_on_start_hooks() {
            let hooks = host.drain_on_start_hooks();
            ran |= !hooks.is_empty();
            let mut hooks = hooks.into_iter();
            while let Some(hook) = hooks.next() {
                let root_id = self.core.root_id();
                let result = host.call_function(self, root_id, hook);
                host.release_function(hook);
                if let Err(error) = result {
                    for pending in hooks {
                        host.release_function(pending);
                    }
                    for queued in host.drain_on_start_hooks() {
                        host.release_function(queued);
                    }
                    return Err(error);
                }
            }
        }
        Ok(ran)
    }

    /// Return command availability using an explicit target policy.
    pub fn command_availability(
        &self,
        target: commands::CommandTarget,
    ) -> Result<Vec<commands::CommandAvailability<'_>>> {
        commands::CommandResolver::for_target(&self.core, target).availability()
    }

    /// Return the effective key bindings for a node or the current focus.
    pub fn available_bindings(
        &self,
        focus: Option<NodeId>,
    ) -> Result<super::help::BindingSnapshot> {
        self.core.available_bindings(focus)
    }

    /// Build a diagnostic dump with tree, focus, and binding details.
    pub fn diagnostic_dump(&self, target: NodeId) -> String {
        let mut out = String::new();
        let input_mode = self.core.input_map.current_mode();
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
        out.push_str(&format!("input mode: {input_mode}\n"));
        let exclusive = self
            .core
            .input_map
            .active_exclusive_group()
            .map_or("(none)", inputmap::FrameworkBindingGroup::as_str);
        out.push_str(&format!("exclusive group: {exclusive}\n"));

        let mut route = Vec::new();
        let mut route_path = target_path;
        let mut route_node = Some(target);
        while let Some(node) = route_node {
            route.push(route_path.clone());
            route_node = self.core.nodes.get(node).and_then(|entry| entry.parent);
            route_path.pop();
        }
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
                let state = self.core.input_map.diagnostic_state(binding.id, &route);
                out.push_str(&format!(
                    "  [{:?}] owner={:?} scope={:?} {} {} -> {} ({state})\n",
                    binding.id,
                    binding.owner,
                    binding.scope,
                    binding.input,
                    binding.path_filter(),
                    binding.description,
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
                    entry.phase.as_str(),
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

/// Recursively collect adjacent `.luau` and `.d.luau` module pairs.
fn collect_script_declaration_pairs(
    source: &script::ScriptModuleSource,
    dir: &FsPath,
    pairs: &mut Vec<ScriptDeclarationPair>,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)
        .map_err(|err| error::Error::Invalid(format!("script root scan failed: {err}")))?
    {
        let entry = entry
            .map_err(|err| error::Error::Invalid(format!("script root scan failed: {err}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_script_declaration_pairs(source, &path, pairs)?;
            continue;
        }
        let Some(implementation_path) = implementation_path_for_declaration(&path) else {
            continue;
        };
        if !implementation_path.is_file() {
            return Err(error::Error::Invalid(format!(
                "script declaration {} has no implementation sibling",
                path.display()
            )));
        }
        if source.module_id_for_path(&implementation_path).is_err() {
            return Err(error::Error::Invalid(format!(
                "script implementation {} is outside configured roots",
                implementation_path.display()
            )));
        }
        pairs.push(ScriptDeclarationPair {
            implementation_path,
            declaration_path: path,
        });
    }
    Ok(())
}

/// Return the implementation sibling for a declaration path.
fn implementation_path_for_declaration(path: &FsPath) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_str()?;
    let stem = file_name.strip_suffix(".d.luau")?;
    Some(path.with_file_name(format!("{stem}.luau")))
}

/// A trait that allows widgets to perform recursive initialization of
/// themselves and their children.
pub trait Loader {
    /// Load commands or resources into the canopy instance.
    /// Returns an error if loading fails.
    fn load(_: &mut Canopy) -> Result<()> {
        Ok(())
    }
}
