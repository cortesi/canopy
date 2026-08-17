#![expect(
    clippy::multiple_inherent_impl,
    reason = "Canopy methods are split by facade, rendering, and routing concerns."
)]

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::Write,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, mpsc},
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ruau::{
    source::{SourceProvider, fs::DirectoryMountsError},
    vm::NativeModule,
};
use serde::{Deserialize, Serialize};

use super::{
    inputmap,
    poll::Poller,
    termbuf::{RenderLimits, TermBuf},
};

mod rendering;
mod routing;
#[cfg(test)]
mod tests;
use crate::{
    backend::BackendControl,
    commands::{self, CommandDispatchKind},
    core::{
        Core, NodeId, TypedId,
        dump::dump_with_focus,
        fixture::{Fixture, FixtureInfo},
        help,
    },
    error::{self, Result},
    event::{Event, key, mouse},
    geom::Size,
    path::Path,
    script,
    style::{StyleMap, solarized},
    widget::Widget,
};

/// Application runtime state and renderer coordination.
pub struct Canopy {
    /// Core state.
    pub(super) core: Core,
    /// Backend controller waiting to be acquired by a terminal session.
    pub(crate) backend: Option<Box<dyn BackendControl>>,

    /// The poller is responsible for tracking nodes that have pending poll events.
    poller: Poller,

    /// Root window size.
    pub(crate) root_size: Option<Size>,
    /// Limits for the materialized visible render target.
    pub(crate) render_limits: RenderLimits,

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
    /// Binding targets whose release is deferred until a startup attempt commits.
    deferred_binding_releases: Option<Vec<inputmap::BindingTarget>>,
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
    /// Input mapping table.
    pub(crate) keymap: inputmap::InputMap,
    /// Trace for the most recent key or mouse routing pass.
    route_trace: Vec<RouteTraceEntry>,

    /// Cached terminal buffer.
    termbuf: Option<TermBuf>,
    /// Whether a render is pending after the most recent event.
    render_pending: bool,

    /// Event sender channel.
    pub(crate) event_tx: UnboundedSender<Event>,
    /// Event receiver channel.
    pub(crate) event_rx: Option<UnboundedReceiver<Event>>,
    /// Cross-thread automation callback sender.
    automation_tx: mpsc::SyncSender<AutomationCallback>,
    /// Cross-thread automation callback receiver.
    automation_rx: mpsc::Receiver<AutomationCallback>,
    /// Thread that exclusively owns this application instance.
    ui_thread: ThreadId,

    /// Style map used for rendering.
    style: StyleMap,
}

/// Script API finalization state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptApiState {
    /// Registrations remain open and no surface is staged.
    Open,
    /// The surface is staged but the runtime has not been published.
    Preparing,
    /// The runtime, definitions, and module source are ready.
    Ready,
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
    callback_tx: mpsc::SyncSender<AutomationCallback>,
    /// Sender for wake events so the runloop notices queued work.
    wake_tx: UnboundedSender<Event>,
    /// Thread that owns the associated Canopy instance.
    ui_thread: ThreadId,
}

impl AutomationHandle {
    /// Queue a callback to run on the UI thread.
    pub fn submit(&self, callback: AutomationCallback) -> Result<()> {
        self.callback_tx
            .try_send(callback)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    error::Error::RunLoop("automation callback queue is full".into())
                }
                mpsc::TrySendError::Disconnected(_) => {
                    error::Error::RunLoop("automation callback channel closed".into())
                }
            })?;
        self.wake_tx
            .unbounded_send(Event::Wake)
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
    /// Input map before the script ran.
    keymap: inputmap::InputMap,
    /// Binding IDs present before the script ran.
    binding_ids: HashSet<inputmap::BindingId>,
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

/// Data needed to run a default-bindings script after dropping the Canopy borrow.
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

/// Replayable record of one script evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptJournalEntry {
    /// Monotonic journal id.
    pub id: u64,
    /// Script origin such as `eval`, `config:<path>`, or `startup:app`.
    pub origin: String,
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
    /// Construct a new Canopy instance.
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        let (automation_tx, automation_rx) = mpsc::sync_channel(AUTOMATION_QUEUE_CAPACITY);
        let core = Core::new();
        Self {
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            automation_tx,
            automation_rx,
            ui_thread: thread::current().id(),
            keymap: inputmap::InputMap::new(),
            route_trace: Vec::new(),
            script_host: script::LuauHost::new(),
            script_api_text: None,
            script_module_roots: script::ScriptModuleRoots::new(),
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
            render_pending: true,
            backend: None,
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

    /// Mark the visible application state for redraw.
    pub fn request_redraw(&mut self) {
        self.render_pending = true;
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

    /// Replace the root's children with a single node.
    pub fn set_root_child(&mut self, child: impl Into<NodeId>) -> Result<()> {
        let root = self.root_id();
        self.core.set_children(root, vec![child.into()])
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

    /// Replace the active style map before the next render.
    pub fn set_style(&mut self, style: StyleMap) {
        self.style = style;
        self.render_pending = true;
    }

    /// Register a backend controller.
    pub(crate) fn register_backend<T: BackendControl + 'static>(&mut self, be: T) {
        self.backend = Some(Box::new(be));
    }

    /// Get a reference to the current render buffer, if any.
    pub fn buf(&self) -> Option<&TermBuf> {
        self.termbuf.as_ref()
    }

    /// Run a compiled script by id on the target node.
    pub fn run_script(&mut self, node_id: impl Into<NodeId>, sid: script::ScriptId) -> Result<()> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
        let host = self.script_host.clone();
        host.execute(self, node_id.into(), sid)
    }

    /// Compile a script and return its identifier.
    pub fn compile_script(&mut self, source: &str) -> Result<script::ScriptId> {
        self.script_host.compile(source)
    }

    /// Evaluate a Luau source string in the current app context.
    pub fn eval_script(&mut self, source: &str) -> Result<()> {
        let baseline = self.begin_script_journal();
        let result = (|| {
            if !self.script_host.is_finalized() {
                self.finalize_api()?;
            }
            let script_id = self.compile_script(source)?;
            self.run_script(self.core.root_id(), script_id)
        })();
        self.record_script_journal("eval", source, baseline, &result);
        result
    }

    /// Evaluate a Luau source string and return its value.
    pub fn eval_script_value(&mut self, source: &str) -> Result<commands::ArgValue> {
        let baseline = self.begin_script_journal();
        let result = (|| {
            if !self.script_host.is_finalized() {
                self.finalize_api()?;
            }
            let script_id = self.compile_script(source)?;
            let host = self.script_host.clone();
            host.execute_value(self, self.core.root_id(), script_id)
        })();
        self.record_script_journal("eval", source, baseline, &result);
        result
    }

    /// Evaluate a Luau source string with a cooperative timeout.
    pub fn eval_script_value_with_timeout(
        &mut self,
        source: &str,
        timeout: Duration,
    ) -> Result<commands::ArgValue> {
        let baseline = self.begin_script_journal();
        let result = (|| {
            if !self.script_host.is_finalized() {
                self.finalize_api()?;
            }
            let script_id = self.compile_script(source)?;
            let host = self.script_host.clone();
            host.execute_value_with_timeout(self, self.core.root_id(), script_id, timeout)
        })();
        self.record_script_journal("eval", source, baseline, &result);
        result
    }

    /// Evaluate the app's built-in default bindings script.
    pub fn run_default_script(&mut self, source: &str) -> Result<()> {
        self.eval_script(source)
    }

    /// Return the configured persistent script module roots.
    pub fn script_module_roots(&self) -> &script::ScriptModuleRoots {
        &self.script_module_roots
    }

    /// Configure the `@user` persistent script root.
    pub fn set_user_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.ensure_api_unfinalized("script module roots")?;
        self.script_module_roots.set_user_root(root);
        Ok(())
    }

    /// Configure the `@project` persistent script root.
    pub fn set_project_script_root(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.ensure_api_unfinalized("script module roots")?;
        self.script_module_roots.set_project_root(root);
        Ok(())
    }

    /// Discover and configure the nearest `.canopy` project script root.
    pub fn discover_project_script_root_from(&mut self, start: impl AsRef<FsPath>) -> Result<bool> {
        self.ensure_api_unfinalized("script module roots")?;
        let Some(root) = script::ScriptModuleRoots::discover_project_root(start) else {
            return Ok(false);
        };
        self.script_module_roots.set_project_root(root);
        Ok(true)
    }

    /// Invalidate cached exports from persistent script modules.
    pub fn invalidate_script_modules(&mut self) -> Option<u64> {
        let epoch = self
            .script_module_source
            .as_ref()
            .map(|source| source.invalidate_all());
        if epoch.is_some() {
            self.clear_script_callbacks();
        }
        epoch
    }

    /// Invalidate cached exports from the `@user` persistent script root.
    pub fn invalidate_user_script_modules(&mut self) -> Option<u64> {
        let epoch = self
            .script_module_source
            .as_ref()
            .and_then(|source| source.invalidate("@user").ok());
        if epoch.is_some() {
            self.clear_script_callbacks();
        }
        epoch
    }

    /// Invalidate cached exports from the `@project` persistent script root.
    pub fn invalidate_project_script_modules(&mut self) -> Option<u64> {
        let epoch = self
            .script_module_source
            .as_ref()
            .and_then(|source| source.invalidate("@project").ok());
        if epoch.is_some() {
            self.clear_script_callbacks();
        }
        epoch
    }

    /// Register an audited Ruau native module on the same surface as Canopy commands.
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

    /// Run app, user, and project startup scripts once.
    pub fn run_startup_scripts(&mut self) -> Result<usize> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
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
            self.run_startup_attempt(format!("startup:{name}"), &source, script_id)?;
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
                    let script_id = host.compile_startup_source(&mounted_source)?;
                    self.startup_module_scripts
                        .insert(module.path.clone(), script_id);
                    script_id
                }
            };
            self.run_startup_attempt(format!("startup:{module_id}"), &source, script_id)?;
            self.completed_startup_modules.insert(module.path);
            ran += 1;
        }
        Ok(ran)
    }

    /// Execute one startup script with callback and binding rollback.
    fn run_startup_attempt(
        &mut self,
        origin: String,
        source: &str,
        script_id: script::ScriptId,
    ) -> Result<()> {
        let attempt = self.begin_startup_attempt();
        let baseline = self.begin_script_journal();
        let host = self.script_host.clone();
        let result = host.execute(self, self.core.root_id(), script_id);
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
            keymap: self.keymap.clone(),
            binding_ids: self.keymap.binding_ids(),
            hooks: self.script_host.on_start_hooks(),
        };
        self.deferred_binding_releases = Some(Vec::new());
        attempt
    }

    /// Commit a startup attempt and release targets it replaced or removed.
    fn commit_startup_attempt(&mut self) {
        let releases = self.deferred_binding_releases.take().unwrap_or_default();
        for target in releases {
            if let inputmap::BindingTarget::LuauFunction(id) = target {
                self.script_host.release_function(id);
            }
        }
    }

    /// Restore registries after a failed startup attempt and release only its callbacks.
    fn rollback_startup_attempt(&mut self, attempt: StartupAttempt) {
        let new_targets = self.keymap.targets_not_in(&attempt.binding_ids);
        self.keymap = attempt.keymap;
        self.deferred_binding_releases = None;
        for target in new_targets {
            if let inputmap::BindingTarget::LuauFunction(id) = target {
                self.script_host.release_function(id);
            }
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
        if self.script_host.is_finalized() {
            return Err(error::Error::InvalidOperation(
                "default binding registration is sealed after finalize_api()".into(),
            ));
        }
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
        if self.script_host.is_finalized() {
            return Err(error::Error::InvalidOperation(
                "fixture registration is sealed after finalize_api()".into(),
            ));
        }
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
        let fixture = self
            .fixtures
            .get(name)
            .cloned()
            .ok_or_else(|| error::Error::NotFound(format!("fixture {name}")))?;
        (fixture.setup)(self)?;
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
        let mut context = crate::core::context::CoreContext::new(&mut self.core, node);
        f(&mut context)
    }

    /// Run a closure against an immutable view of the root context.
    pub fn with_root_view<R>(&self, f: impl FnOnce(&dyn crate::ViewContext) -> R) -> R {
        self.with_view(self.core.root_id(), f)
            .expect("root context should always exist")
    }

    /// Run a closure against an immutable view context bound to a node.
    pub fn with_view<R>(
        &self,
        node: impl Into<NodeId>,
        f: impl FnOnce(&dyn crate::ViewContext) -> R,
    ) -> Result<R> {
        let node = node.into();
        if !self.core.nodes.contains_key(node) {
            return Err(error::Error::NodeNotFound(node));
        }
        let context = crate::core::context::CoreViewContext::new(&self.core, node);
        Ok(f(&context))
    }

    /// Type-check a named Luau source against the finalized app API.
    pub fn check_script(
        &mut self,
        source_name: &str,
        source: &str,
    ) -> Result<script::ScriptCheckResult> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
        self.script_host.check_script(source_name, source)
    }

    /// Drain and return log lines recorded by the most recent script evaluation.
    pub fn take_script_logs(&self) -> Vec<String> {
        self.script_host.take_logs()
    }

    /// Drain and return assertion outcomes from the most recent script evaluation.
    pub fn take_script_assertions(&self) -> Vec<script::ScriptAssertion> {
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

    /// Clear the in-memory script evaluation journal.
    pub fn clear_script_journal(&mut self) {
        self.script_journal.clear();
    }

    /// Evaluate a Luau config file from disk.
    pub fn run_config(&mut self, path: &FsPath) -> Result<()> {
        let baseline = self.begin_script_journal();
        let source = fs::read_to_string(path)
            .map_err(|err| error::Error::Invalid(format!("config read failed: {err}")))?;
        let result = (|| {
            if !self.script_host.is_finalized() {
                self.finalize_api()?;
            }
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
                None => self.script_host.compile_named(&source, b"canopy")?,
            };
            let host = self.script_host.clone();
            host.execute(self, self.core.root_id(), script_id)
        })();
        self.record_script_journal(
            format!("config:{}", path.display()),
            &source,
            baseline,
            &result,
        );
        result
    }

    /// Remove a binding by ID. Returns true if a binding was removed.
    pub fn unbind(&mut self, id: inputmap::BindingId) -> bool {
        let removed = self.keymap.unbind_with_targets(id);
        if removed.is_empty() {
            return false;
        }
        for binding in removed {
            self.release_binding_target(&binding);
        }
        true
    }

    /// Remove bindings for a key input, optionally filtered by mode and path.
    pub fn unbind_key_input<K>(
        &mut self,
        key: K,
        mode: Option<&str>,
        path_filter: Option<&str>,
    ) -> usize
    where
        key::Key: From<K>,
    {
        let removed = self.keymap.unbind_input(
            inputmap::InputSpec::Key(key.into()),
            inputmap::BindingFilter { mode, path_filter },
        );
        self.release_removed_bindings(removed)
    }

    /// Remove bindings for a mouse input, optionally filtered by mode and path.
    pub fn unbind_mouse_input<K>(
        &mut self,
        mouse: K,
        mode: Option<&str>,
        path_filter: Option<&str>,
    ) -> usize
    where
        mouse::Mouse: From<K>,
    {
        let removed = self.keymap.unbind_input(
            inputmap::InputSpec::Mouse(mouse.into()),
            inputmap::BindingFilter { mode, path_filter },
        );
        self.release_removed_bindings(removed)
    }

    /// Remove all bindings from all modes.
    pub fn clear_bindings(&mut self) -> usize {
        let removed = self.keymap.clear();
        self.release_removed_bindings(removed)
    }

    /// Remove all callbacks whose VM ownership is tied to the current source epoch.
    fn clear_script_callbacks(&mut self) {
        let removed = self.keymap.remove_luau_functions();
        self.release_removed_bindings(removed);
        for hook in self.script_host.drain_on_start_hooks() {
            self.script_host.release_function(hook);
        }
    }

    /// Return all bindings defined for a mode.
    pub fn bindings_for_mode(&self, mode: &str) -> Vec<inputmap::BindingInfo<'_>> {
        self.keymap.bindings_for_mode(mode)
    }

    /// Return bindings in a mode that match a specific path.
    pub fn bindings_matching_path(
        &self,
        mode: &str,
        path: &Path,
    ) -> Vec<inputmap::MatchedBindingInfo<'_>> {
        self.keymap.bindings_matching_path(mode, path)
    }

    /// Return the active input mode.
    pub fn input_mode(&self) -> &str {
        self.keymap.current_mode()
    }

    /// Return active non-default input modes from oldest to newest.
    pub fn input_mode_stack(&self) -> &[String] {
        self.keymap.mode_stack()
    }

    /// Set the active input mode.
    pub fn set_input_mode(&mut self, mode: &str) -> Result<()> {
        self.keymap.set_mode(mode)
    }

    /// Push an input mode above the current mode.
    pub fn push_input_mode(&mut self, mode: &str) -> Result<()> {
        self.keymap.push_mode(mode)
    }

    /// Pop the top input mode and return the new active mode.
    pub fn pop_input_mode(&mut self) -> &str {
        self.keymap.pop_mode()
    }

    /// Bind a key or mouse input to switch the active input mode.
    pub fn bind_input_mode(
        &mut self,
        mode: &str,
        input: inputmap::InputSpec,
        path_filter: &str,
        next_mode: &str,
    ) -> Result<inputmap::BindingId> {
        self.keymap
            .bind_input_mode(mode, input, path_filter, next_mode)
    }

    /// Return the most recent key or mouse route trace.
    pub fn route_trace(&self) -> &[RouteTraceEntry] {
        &self.route_trace
    }

    /// Load the commands from a command node using the default node name.
    /// Returns an error if any command id is already registered.
    pub fn add_commands<T: commands::CommandNode>(&mut self) -> Result<()> {
        if self.script_host.is_finalized() {
            return Err(error::Error::InvalidOperation(
                "command registration is sealed after finalize_api()".into(),
            ));
        }
        let cmds = <T>::commands();
        self.core.commands.add(cmds)?;
        Ok(())
    }

    /// Finalize the script API surface for this app.
    pub fn finalize_api(&mut self) -> Result<()> {
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
        let definitions = script::defs::render_definitions(
            &self.core.commands,
            &default_binding_owners,
            &self.fixture_infos(),
        );
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
        self.script_host.prepare_finalize(
            &self.core.commands,
            &default_binding_owners,
            &self.script_native_modules,
            surface_source,
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
            self.script_host.publish_finalize(definitions.clone())
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

    /// Return the current script API finalization state.
    pub fn script_api_state(&self) -> ScriptApiState {
        if self.script_host.is_finalized() {
            ScriptApiState::Ready
        } else if self.script_host.surface().is_some() {
            ScriptApiState::Preparing
        } else {
            ScriptApiState::Open
        }
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
        let script_id = self
            .default_bindings
            .get(owner)
            .and_then(|script| script.script_id)
            .ok_or_else(|| {
                error::Error::NotFound(format!("default bindings not registered for owner {owner}"))
            })?;
        let host = self.script_host.clone();
        let source = host.script_source(script_id).unwrap_or_default();
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
            format!("default-bindings:{owner}"),
            &run.source,
            run.baseline,
            result,
        );
    }

    /// Return true if the named owner already exports a `default_bindings` command.
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
            let diagnostics = check
                .diagnostics()
                .records()
                .map(|diagnostic| {
                    script::diagnostic_record_to_script(
                        Some(pair.implementation_path.display().to_string()),
                        diagnostic,
                    )
                })
                .collect::<Vec<_>>();
            failures.push(format!(
                "{}:\n{}",
                pair.declaration_path.display(),
                format_script_diagnostics(&diagnostics)
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

    /// Begin a journaled script evaluation and capture its diagnostics baseline.
    fn begin_script_journal(&self) -> ScriptJournalBaseline {
        // Top-level evaluations clear diagnostics on entry, so their baseline
        // is empty; nested evaluations record only what they add.
        let (logs, assertions) = if script::in_live_scope(self) {
            self.script_host.diagnostics_counts()
        } else {
            (0, 0)
        };
        ScriptJournalBaseline {
            started: Instant::now(),
            logs,
            assertions,
        }
    }

    /// Append a script evaluation to the in-memory journal.
    fn record_script_journal<T>(
        &mut self,
        origin: impl Into<String>,
        source: &str,
        baseline: ScriptJournalBaseline,
        result: &Result<T>,
    ) {
        let duration_ms = u64::try_from(baseline.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut logs = self.script_host.logs();
        let logs = logs.split_off(baseline.logs.min(logs.len()));
        let mut assertions = self.script_host.assertions();
        let assertions = assertions.split_off(baseline.assertions.min(assertions.len()));
        let id = self.script_journal_next_id;
        self.script_journal_next_id += 1;
        self.script_journal.push(ScriptJournalEntry {
            id,
            origin: origin.into(),
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
        let mut owners = self.default_bindings.keys().cloned().collect::<Vec<_>>();
        owners.sort();
        for owner in owners {
            let script = self
                .default_bindings
                .get_mut(&owner)
                .ok_or_else(|| error::Error::Internal("default binding disappeared".into()))?;
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

    /// Output a formatted table of commands to a writer.
    ///
    /// If `include_hidden` is false, commands with `doc.hidden = true` are excluded.
    pub fn print_command_table(&self, w: &mut dyn Write, include_hidden: bool) -> Result<()> {
        let mut cmds: Vec<&commands::CommandSpec> = self
            .core
            .commands
            .iter()
            .map(|(_, v)| v)
            .filter(|c| include_hidden || !c.doc.hidden)
            .collect();

        cmds.sort_by_key(|a| a.id.0);

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(UTF8_FULL);
        for i in cmds {
            let desc = i.doc.short.unwrap_or("");
            table.add_row(vec![
                comfy_table::Cell::new(i.id.0).fg(comfy_table::Color::Green),
                comfy_table::Cell::new(i.signature()),
                comfy_table::Cell::new(desc).fg(comfy_table::Color::Cyan),
            ]);
        }
        writeln!(w, "{table}").map_err(|x| error::Error::Internal(x.to_string()))
    }

    /// Return command availability from the current focus position.
    ///
    /// This computes which commands would resolve to a target if dispatched from the current
    /// focus. For each command:
    /// - Free commands always have `resolution = Some(Free)`
    /// - Node-routed commands have `resolution = Some(Subtree{..})` or `Some(Ancestor{..})`
    ///   if a matching node exists, `None` otherwise
    pub fn command_availability_from_focus(&self) -> Vec<commands::CommandAvailability<'_>> {
        let start = self.core.focus.unwrap_or(self.core.root);
        self.command_availability_from_node(start)
    }

    /// Return command availability from a specific node.
    ///
    /// Computes which commands would dispatch to a target, using the same resolution logic
    /// as `commands::dispatch`:
    /// 1. First search the subtree rooted at `start` in pre-order
    /// 2. Then walk ancestors
    pub fn command_availability_from_node(
        &self,
        start: NodeId,
    ) -> Vec<commands::CommandAvailability<'_>> {
        commands::CommandResolver::new(&self.core, start).availability()
    }

    /// Generate a contextual help snapshot for the current focus.
    ///
    /// The snapshot includes:
    /// - Bindings that would match from the focus path
    /// - Commands with their availability status
    pub fn help_snapshot(&self) -> super::help::HelpSnapshot<'_> {
        self.help_snapshot_for_focus(self.core.focus)
    }

    /// Fulfill any pending help snapshot request.
    ///
    /// If `pending_help_request` is set, capture the help snapshot using the
    /// pre-request focus and store it in `pending_help_snapshot`.
    fn fulfill_pending_help_request(&mut self) {
        if let Some((_target, pre_focus)) = self.core.pending_help_request.take() {
            let snapshot = self.help_snapshot_for_focus(pre_focus).to_owned();
            self.core.pending_help_snapshot = Some(snapshot);
        }
    }

    /// Generate a help snapshot for a specific focus node.
    ///
    /// This is like `help_snapshot` but uses the specified focus instead of
    /// the current focus. Used to capture pre-help context.
    fn help_snapshot_for_focus(&self, focus: Option<NodeId>) -> super::help::HelpSnapshot<'_> {
        let focus = focus.unwrap_or(self.core.root);
        let focus_path = self.core.node_path(self.core.root, focus);
        let input_mode = self.keymap.current_mode();

        let command_avail = self.command_availability_from_node(focus);
        let help_commands: Vec<super::help::HelpCommand<'_>> = command_avail
            .into_iter()
            .map(|avail| super::help::HelpCommand {
                owner: match avail.spec.dispatch {
                    commands::CommandDispatchKind::Free => None,
                    commands::CommandDispatchKind::Node { owner } => Some(owner),
                },
                spec: avail.spec,
                resolution: avail.resolution,
            })
            .collect();

        let mut matched_bindings = Vec::new();
        for mode in self.keymap.active_modes() {
            for binding in self.keymap.bindings_matching_path(mode, &focus_path) {
                matched_bindings.push((mode, binding));
            }
        }
        let help_bindings: Vec<super::help::HelpBinding<'_>> = matched_bindings
            .into_iter()
            .map(|(mode, mb)| {
                let kind = if mb.m.anchored_end && mb.m.depth > 0 {
                    super::help::BindingKind::PreEventOverride
                } else {
                    super::help::BindingKind::PostEventFallback
                };

                let label = super::help::binding_label(
                    mb.info.target,
                    &self.core.commands,
                    |sid| self.script_host.script_source(sid),
                    |id| self.script_host.function_label(id),
                );

                super::help::HelpBinding {
                    input: mb.info.input,
                    mode,
                    path_filter: mb.info.path_filter,
                    target: mb.info.target,
                    kind,
                    label,
                }
            })
            .collect();

        super::help::HelpSnapshot {
            focus,
            focus_path,
            input_mode,
            bindings: help_bindings,
            commands: help_commands,
        }
    }

    /// Build a diagnostic dump with tree, focus, and binding details.
    pub fn diagnostic_dump(&self, target: NodeId) -> String {
        let mut out = String::new();
        let focus = self.core.focus;
        let input_mode = self.keymap.current_mode();
        let target = if self.core.nodes.contains_key(target) {
            target
        } else {
            self.core.root
        };
        let focus_path = self.core.focus_path(self.core.root);
        let target_path = self.core.node_path(self.core.root, target);

        out.push_str("Canopy diagnostics\n");
        out.push_str(&format!("focus: {focus:?}\n"));
        out.push_str(&format!("focus path: {focus_path}\n"));
        out.push_str(&format!("target: {target:?}\n"));
        out.push_str(&format!("target path: {target_path}\n"));
        out.push_str(&format!("input mode: {input_mode}\n"));

        let mut bindings = Vec::new();
        for mode in self.keymap.active_modes() {
            for binding in self.keymap.bindings_matching_path(mode, &target_path) {
                bindings.push((mode, binding));
            }
        }
        if bindings.is_empty() {
            out.push_str("bindings: (none)\n");
        } else {
            out.push_str("bindings:\n");
            for (mode, mb) in bindings {
                let kind = if mb.m.anchored_end && mb.m.depth > 0 {
                    "pre"
                } else {
                    "post"
                };
                let label = help::binding_label(
                    mb.info.target,
                    &self.core.commands,
                    |sid| self.script_host.script_source(sid),
                    |id| self.script_host.function_label(id),
                );
                out.push_str(&format!(
                    "  [{:?}] mode={mode:?} {} {} ({kind}) -> {label}\n",
                    mb.info.id, mb.info.input, mb.info.path_filter
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
        match dump_with_focus(&self.core, self.core.root, focus) {
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

/// Render script diagnostics for an error message.
fn format_script_diagnostics(diagnostics: &[script::ScriptCheckDiagnostic]) -> String {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Validate a child view position against the parent canvas bounds.
/// A trait that allows widgets to perform recursive initialization of themselves and their
/// children.
pub trait Loader {
    /// Load commands or resources into the canopy instance.
    /// Returns an error if loading fails.
    fn load(_: &mut Canopy) -> Result<()> {
        Ok(())
    }
}
