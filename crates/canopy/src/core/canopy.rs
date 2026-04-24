use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::Write,
    path::Path as FsPath,
    sync::mpsc,
};

use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};

use super::{inputmap, poll::Poller, termbuf::TermBuf};
use crate::{
    backend::BackendControl,
    commands::{self, CommandDispatchKind},
    core::{
        Core, NodeId,
        context::CoreViewContext,
        dump::dump_with_focus,
        fixture::{Fixture, FixtureInfo},
        help,
        style::Effect,
        view::View,
    },
    cursor,
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{Point, Rect, RectI32, Size},
    layout::Display,
    path::Path,
    render::{Render, RenderBackend},
    script,
    style::{ResolvedStyle, StyleManager, StyleMap, solarized},
    widget::EventOutcome,
};

/// Application runtime state and renderer coordination.
pub struct Canopy {
    /// Core state.
    pub core: Core,

    /// Stores the focus_gen during the last render.
    last_render_focus_gen: u64,

    /// Last focus path ids, used to detect focus path changes.
    last_focus_path: Vec<NodeId>,

    /// The poller is responsible for tracking nodes that have pending poll events.
    poller: Poller,

    /// Root window size.
    pub(crate) root_size: Option<Size>,

    /// Script execution host.
    pub(crate) script_host: script::ScriptHost,
    /// Cached Luau API definition text.
    script_api_text: Option<String>,
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
    pub(crate) event_tx: mpsc::Sender<Event>,
    /// Event receiver channel.
    pub(crate) event_rx: Option<mpsc::Receiver<Event>>,
    /// Cross-thread automation callback sender.
    automation_tx: mpsc::Sender<AutomationCallback>,
    /// Cross-thread automation callback receiver.
    automation_rx: mpsc::Receiver<AutomationCallback>,

    /// Style map used for rendering.
    pub style: StyleMap,
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

/// Handle for submitting automation work to a live canopy runloop.
#[derive(Clone)]
pub struct AutomationHandle {
    /// Sender for queued UI-thread callbacks.
    callback_tx: mpsc::Sender<AutomationCallback>,
    /// Sender for wake events so the runloop notices queued work.
    wake_tx: mpsc::Sender<Event>,
}

impl AutomationHandle {
    /// Queue a callback to run on the UI thread.
    pub fn submit(&self, callback: AutomationCallback) -> Result<()> {
        self.callback_tx
            .send(callback)
            .map_err(|_| error::Error::RunLoop("automation callback channel closed".into()))?;
        self.wake_tx
            .send(Event::Wake)
            .map_err(|_| error::Error::RunLoop("event loop wake channel closed".into()))?;
        Ok(())
    }

    /// Execute a closure on the UI thread and wait for its result.
    pub fn request<R, F>(&self, callback: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Canopy) -> Result<R> + Send + 'static,
    {
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

/// Rendering traversal scratch state shared across recursion.
struct RenderTraversal<'a> {
    /// Destination buffer for draw operations.
    dest_buf: &'a mut TermBuf,
    /// Style manager stack.
    styl: &'a mut StyleManager,
    /// Accumulated style effects for the current subtree.
    effect_stack: &'a mut Vec<Effect>,
}

/// No-op backend used to refresh the offscreen terminal buffer for inspection.
struct SnapshotBackend;

impl RenderBackend for SnapshotBackend {
    fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, _txt: &str) -> Result<()> {
        Ok(())
    }

    fn supports_char_shift(&self) -> bool {
        false
    }

    fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Input routed through the shared bubbling pipeline.
#[derive(Clone, Copy)]
enum RoutedInput {
    /// Key input.
    Key(key::Key),
    /// Mouse input in screen coordinates.
    Mouse(mouse::MouseEvent),
}

impl RoutedInput {
    /// Return the binding input spec for this routed input.
    fn input_spec(self) -> inputmap::InputSpec {
        match self {
            Self::Key(key) => inputmap::InputSpec::Key(key),
            Self::Mouse(mouse) => inputmap::InputSpec::Mouse(mouse.into()),
        }
    }

    /// Return the event to dispatch to a specific node.
    fn event_for_node(self, core: &Core, node_id: NodeId) -> Event {
        match self {
            Self::Key(key) => Event::Key(key),
            Self::Mouse(mouse) => Event::Mouse(Self::local_mouse(core, node_id, mouse)),
        }
    }

    /// Return true when an anchored binding may run before widget event dispatch.
    fn allows_pre_event_binding(self) -> bool {
        matches!(self, Self::Key(_))
    }

    /// Return a short diagnostic label.
    fn label(self) -> &'static str {
        match self {
            Self::Key(_) => "key",
            Self::Mouse(_) => "mouse",
        }
    }

    /// Convert a screen-space mouse event to a node-local event.
    fn local_mouse(core: &Core, node_id: NodeId, mouse: mouse::MouseEvent) -> mouse::MouseEvent {
        let view = core
            .nodes
            .get(node_id)
            .map(|node| node.view)
            .unwrap_or_default();
        mouse::MouseEvent {
            action: mouse.action,
            button: mouse.button,
            modifiers: mouse.modifiers,
            location: view.content.to_local_point(mouse.location),
        }
    }
}

impl Canopy {
    /// Construct a new Canopy instance.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (automation_tx, automation_rx) = mpsc::channel();
        let core = Core::new();
        Self {
            last_render_focus_gen: core.focus_gen,
            last_focus_path: Vec::new(),
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            automation_tx,
            automation_rx,
            keymap: inputmap::InputMap::new(),
            route_trace: Vec::new(),
            script_host: script::ScriptHost::new(),
            script_api_text: None,
            default_bindings: HashMap::new(),
            fixtures: HashMap::new(),
            style: solarized::solarized_dark(),
            root_size: None,
            termbuf: None,
            render_pending: true,
            core,
        }
    }

    /// Return a handle for submitting automation work to this app's UI thread.
    pub fn automation_handle(&self) -> AutomationHandle {
        AutomationHandle {
            callback_tx: self.automation_tx.clone(),
            wake_tx: self.event_tx.clone(),
        }
    }

    /// Register a backend controller.
    pub fn register_backend<T: BackendControl + 'static>(&mut self, be: T) {
        self.core.backend = Some(Box::new(be))
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
        let script_id = self.compile_script(source)?;
        self.run_script(self.core.root_id(), script_id)
    }

    /// Evaluate a Luau source string and return its value.
    pub fn eval_script_value(&mut self, source: &str) -> Result<commands::ArgValue> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
        let script_id = self.compile_script(source)?;
        let host = self.script_host.clone();
        host.execute_value(self, self.core.root_id(), script_id)
    }

    /// Evaluate the app's built-in default bindings script.
    pub fn run_default_script(&mut self, source: &str) -> Result<()> {
        self.eval_script(source)
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
        let root_id = self.core.root_id();
        let mut ctx = crate::core::context::CoreContext::new(&mut self.core, root_id);
        f(&mut ctx)
    }

    #[cfg(all(feature = "typecheck", not(target_os = "macos")))]
    /// Type-check a Luau source string against the finalized app API.
    pub fn check_script(&mut self, source: &str) -> Result<luau_analyze::CheckResult> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
        self.script_host.check_script(source)
    }

    /// Drain and return log lines recorded by the most recent script evaluation.
    pub fn take_script_logs(&self) -> Vec<String> {
        self.script_host.take_logs()
    }

    /// Drain and return assertion outcomes from the most recent script evaluation.
    pub fn take_script_assertions(&self) -> Vec<script::ScriptAssertion> {
        self.script_host.take_assertions()
    }

    /// Evaluate a Luau config file from disk.
    pub fn run_config(&mut self, path: &FsPath) -> Result<()> {
        let source = fs::read_to_string(path)
            .map_err(|err| error::Error::Invalid(format!("config read failed: {err}")))?;
        self.eval_script(&source)
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

    /// Set the active input mode.
    pub fn set_input_mode(&mut self, mode: &str) -> Result<()> {
        self.keymap.set_mode(mode)
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
        if cmds
            .iter()
            .all(|spec| self.core.commands.get(spec.id.0).is_some())
        {
            return Ok(());
        }
        self.core.commands.add(cmds)?;
        Ok(())
    }

    /// Finalize the script API surface for this app.
    pub fn finalize_api(&mut self) -> Result<()> {
        if self.script_host.is_finalized() {
            return Ok(());
        }
        let default_binding_owners = self.default_binding_owners();
        let definitions = script::defs::render_definitions(
            &self.core.commands,
            &default_binding_owners,
            &self.fixture_infos(),
        );
        self.script_host.finalize(
            &self.core.commands,
            &default_binding_owners,
            definitions.clone(),
        )?;
        self.compile_registered_default_bindings()?;
        self.script_api_text = Some(definitions);
        Ok(())
    }

    /// Return the rendered Luau definition file for this app.
    pub fn script_api(&self) -> &str {
        self.script_api_text
            .as_deref()
            .expect("script API requested before finalize_api()")
    }

    /// Run a registered default binding script by owner name.
    pub(crate) fn run_registered_default_bindings(&mut self, owner: &str) -> Result<()> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
        let script_id = self
            .default_bindings
            .get(owner)
            .and_then(|script| script.script_id)
            .ok_or_else(|| {
                error::Error::NotFound(format!("default bindings not registered for owner {owner}"))
            })?;
        let host = self.script_host.clone();
        host.execute(self, self.core.root_id(), script_id)
    }

    /// Return true if the named owner already exports a `default_bindings` command.
    fn owner_has_default_bindings_command(&self, owner: &str) -> bool {
        self.core.commands.iter().any(|(_, spec)| {
            matches!(spec.dispatch, CommandDispatchKind::Node { owner: spec_owner } if spec_owner == owner)
                && spec.name == "default_bindings"
        })
    }

    /// Return the set of owners with registered default binding scripts.
    fn default_binding_owners(&self) -> BTreeSet<String> {
        self.default_bindings.keys().cloned().collect()
    }

    /// Compile any registered default binding scripts after finalization.
    fn compile_registered_default_bindings(&mut self) -> Result<()> {
        let host = self.script_host.clone();
        for script in self.default_bindings.values_mut() {
            if script.script_id.is_none() {
                script.script_id = Some(host.compile(&script.source)?);
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
            for hook in hooks {
                let root_id = self.core.root_id();
                let result = host.call_function(self, root_id, hook);
                host.release_function(hook);
                result?;
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

    /// Has the focus changed since the last render sweep?
    pub(crate) fn focus_changed(&self) -> bool {
        self.core.focus_gen != self.last_render_focus_gen
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

        let matched_bindings = self.keymap.bindings_matching_path(input_mode, &focus_path);
        let help_bindings: Vec<super::help::HelpBinding<'_>> = matched_bindings
            .into_iter()
            .map(|mb| {
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
                    mode: input_mode,
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

        let bindings = self.keymap.bindings_matching_path(input_mode, &target_path);
        if bindings.is_empty() {
            out.push_str("bindings: (none)\n");
        } else {
            out.push_str("bindings:\n");
            for mb in bindings {
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
                    "  [{:?}] {} {} ({kind}) -> {label}\n",
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

    /// Render the tree only if a render is pending.
    pub(crate) fn render_if_pending<R: RenderBackend>(&mut self, be: &mut R) -> Result<bool> {
        if !self.render_pending {
            return Ok(false);
        }
        self.render(be)?;
        Ok(true)
    }

    /// Refresh the cached terminal buffer without producing user-visible output.
    pub(crate) fn refresh_snapshot(&mut self) -> Result<()> {
        let mut backend = SnapshotBackend;
        let _ignored = self.render_if_pending(&mut backend)?;
        Ok(())
    }

    /// Has the focus path status of this node changed since the last render sweep?
    pub fn node_focus_path_changed(&self, node_id: impl Into<NodeId>) -> bool {
        let node_id = node_id.into();
        if self.focus_changed() {
            self.core.is_on_focus_path(node_id) || self.last_focus_path.contains(&node_id)
        } else {
            false
        }
    }

    /// Register the poller channel.
    pub(crate) fn start_poller(&mut self, tx: mpsc::Sender<Event>) {
        self.event_tx = tx;
    }

    /// Pre-render sweep of the tree.
    pub(crate) fn pre_render(&mut self) -> Result<bool> {
        let root = self.core.root;
        let mut focus_seen = false;
        let mut layout_dirty = false;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let hidden = self.core.nodes.get(id).map(|n| n.hidden).unwrap_or(false);
            if hidden {
                continue;
            }

            if self.core.is_focused(id) {
                focus_seen = true;
            }

            let mounted = self.core.nodes.get(id).map(|n| n.mounted).unwrap_or(false);
            if !mounted {
                layout_dirty = true;
                self.core.mount_node(id)?;
            }

            let initialized = self
                .core
                .nodes
                .get(id)
                .map(|n| n.initialized)
                .unwrap_or(false);
            if !initialized {
                layout_dirty = true;
                let next = self.core.with_widget_mut(id, |w, core| {
                    let mut ctx = crate::core::context::CoreContext::new(core, id);
                    w.poll(&mut ctx)
                })?;
                if let Some(d) = next {
                    self.poller.schedule(id, d);
                }
                if let Some(node) = self.core.nodes.get_mut(id) {
                    node.initialized = true;
                }
            }

            let children = self.core.nodes[id].children.clone();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        if !focus_seen {
            self.core.focus_first(root);
        }

        Ok(layout_dirty)
    }

    /// Render a single node (without children).
    fn render_node(
        &self,
        dest_buf: &mut TermBuf,
        styl: &mut StyleManager,
        node_id: NodeId,
        view: View,
        screen_clip: Rect,
        effect_slice: &[Effect],
    ) -> Result<()> {
        let local_clip = Self::outer_clip_to_local(view.outer, screen_clip);
        let screen_origin = screen_clip.tl;

        let mut rndr = Render::new_shared(&self.style, styl, dest_buf, local_clip, screen_origin)
            .with_effects(effect_slice);

        self.core.with_widget_view(node_id, |widget, core| {
            let ctx = CoreViewContext::new(core, node_id);
            widget.render(&mut rndr, &ctx)
        })?
    }

    /// Recursively render a node subtree.
    fn render_recursive(
        &mut self,
        traversal: &mut RenderTraversal<'_>,
        node_id: NodeId,
        parent_clip: Rect,
        active_start: usize,
        active_len: usize,
    ) -> Result<()> {
        let (hidden, layout, view, children, clear_inherited) = {
            let node = &self.core.nodes[node_id];
            (
                node.hidden,
                node.layout,
                node.view,
                node.children.clone(),
                node.clear_inherited_effects,
            )
        };

        if hidden || layout.display == Display::None {
            return Ok(());
        }

        let Some(screen_clip) = view.outer.intersect_rect(parent_clip) else {
            return Ok(());
        };

        let saved_len = traversal.effect_stack.len();
        let (base_start, base_len) = if clear_inherited {
            (saved_len, 0)
        } else {
            (active_start, active_len)
        };

        if let Some(local) = self.core.nodes[node_id].effects.as_ref() {
            traversal.effect_stack.extend(local.iter().cloned());
        }

        let current_len = base_len + traversal.effect_stack.len() - saved_len;

        traversal.styl.push();

        {
            let effect_slice = &traversal.effect_stack[base_start..base_start + current_len];
            self.render_node(
                traversal.dest_buf,
                traversal.styl,
                node_id,
                view,
                screen_clip,
                effect_slice,
            )?;
        }

        if let Some(children_clip) = view.content.intersect_rect(parent_clip) {
            for child in children {
                self.render_recursive(traversal, child, children_clip, base_start, current_len)?;
            }
        }

        traversal.styl.pop();
        traversal.effect_stack.truncate(saved_len);

        Ok(())
    }

    /// Render the tree into an offscreen buffer.
    fn render_pass(&mut self, root_size: Size) -> Result<TermBuf> {
        let mut styl = StyleManager::default();
        styl.reset();

        let def_style = styl
            .get(&self.style, "")
            .resolve_solid()
            .expect("default style resolves to solid colors");
        let mut next = TermBuf::new(root_size, ' ', def_style);

        let screen_clip = Rect::new(0, 0, root_size.w, root_size.h);
        let mut effect_stack: Vec<Effect> = Vec::new();
        let mut traversal = RenderTraversal {
            dest_buf: &mut next,
            styl: &mut styl,
            effect_stack: &mut effect_stack,
        };
        self.render_recursive(&mut traversal, self.core.root, screen_clip, 0, 0)?;
        self.post_render(&mut next)?;

        Ok(next)
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&self, buf: &mut TermBuf) -> Result<()> {
        let mut current = self.core.focus;
        let mut cursor_spec: Option<(NodeId, View, cursor::Cursor)> = None;
        while let Some(id) = current {
            let cursor = self.core.with_widget_read(id, |w, _| w.cursor())?;
            if let Some(node_cursor) = cursor
                && let Some(node) = self.core.nodes.get(id)
            {
                cursor_spec = Some((id, node.view, node_cursor));
                break;
            }
            current = self.core.nodes.get(id).and_then(|n| n.parent);
        }

        if let Some((_nid, view, c)) = cursor_spec {
            let view_rect = Rect::new(0, 0, view.content.w, view.content.h);
            if view_rect.contains_point(c.location) {
                let screen_x = view.content.tl.x + c.location.x as i32;
                let screen_y = view.content.tl.y + c.location.y as i32;
                if screen_x >= 0 && screen_y >= 0 {
                    let screen_pos = Point {
                        x: screen_x as u32,
                        y: screen_y as u32,
                    };
                    buf.overlay_cursor(screen_pos, c.shape);
                }
            }
        }

        Ok(())
    }

    /// Render the widget tree. All visible nodes are rendered.
    pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        let first_render = self.termbuf.is_none();

        // Apply pending style change from Context::set_style
        if let Some(new_style) = self.core.pending_style.take() {
            self.style = new_style;
        }

        if let Some(root_size) = self.root_size {
            self.core.update_layout(root_size)?;

            let layout_dirty = self.pre_render()?;
            if layout_dirty {
                self.core.update_layout(root_size)?;
            }

            let _ = self.core.take_help_snapshot_observed();
            let mut next = self.render_pass(root_size)?;
            if self.core.take_help_snapshot_observed() {
                self.core.pending_help_snapshot = None;
                self.core.update_layout(root_size)?;
                if layout_dirty {
                    self.core.update_layout(root_size)?;
                }
                next = self.render_pass(root_size)?;
            }

            be.reset()?;

            if let Some(prev) = &self.termbuf {
                next.diff(prev, be)?;
            } else {
                next.render(be)?;
            }
            self.termbuf = Some(next);

            if let Some(target) = self.core.take_diagnostic_dump_request() {
                eprintln!("{}", self.diagnostic_dump(target));
            }

            self.last_render_focus_gen = self.core.focus_gen;
            self.last_focus_path = self.core.focus_path_ids();

            if first_render && self.run_on_start_hooks()? {
                return self.render(be);
            }

            self.render_pending = false;
        }

        Ok(())
    }

    /// Convert a screen-space clip rect into local outer coordinates.
    fn outer_clip_to_local(outer: RectI32, clip: Rect) -> Rect {
        let dx = (clip.tl.x as i64 - outer.tl.x as i64).max(0) as u32;
        let dy = (clip.tl.y as i64 - outer.tl.y as i64).max(0) as u32;
        Rect::new(dx, dy, clip.w, clip.h)
    }

    /// Return the starting target and binding path for a mouse event.
    fn mouse_route_start(&mut self, location: Point) -> Result<(Option<NodeId>, Path)> {
        if let Some(capture) = self.core.mouse_capture {
            if self.core.nodes.contains_key(capture) {
                return Ok((Some(capture), self.core.node_path(self.core.root, capture)));
            } else {
                self.core.mouse_capture = None;
            }
        }

        let target = self.core.locate_node(self.core.root, location)?;
        let path = target
            .map(|id| self.core.node_path(self.core.root, id))
            .unwrap_or_else(Path::empty);
        Ok((target, path))
    }

    /// Add one entry to the current route trace.
    fn trace_route(
        &mut self,
        phase: RoutePhase,
        node: Option<NodeId>,
        path: &Path,
        detail: impl Into<String>,
    ) {
        self.route_trace.push(RouteTraceEntry {
            phase,
            node,
            path: path.to_string(),
            detail: detail.into(),
        });
    }

    /// Propagate a key or mouse event through one bubbling route.
    fn route_input(
        &mut self,
        start: Option<NodeId>,
        mut path: Path,
        input: RoutedInput,
    ) -> Result<bool> {
        self.route_trace.clear();
        self.trace_route(
            RoutePhase::Target,
            start,
            &path,
            format!("{} route selected", input.label()),
        );

        let mut target = start;
        while let Some(id) = target {
            if !self.core.nodes.contains_key(id) {
                self.trace_route(
                    RoutePhase::Unhandled,
                    Some(id),
                    &path,
                    "target node disappeared",
                );
                return Ok(false);
            }

            let mut fallback_binding = None;
            if let Some((binding, path_match)) =
                self.keymap.resolve_match(&path, &input.input_spec())
            {
                if input.allows_pre_event_binding()
                    && path_match.anchored_end
                    && path_match.depth > 0
                {
                    self.trace_route(
                        RoutePhase::PreEventBinding,
                        Some(id),
                        &path,
                        "matched before widget event",
                    );
                    return self.execute_routed_binding(id, &path, input, binding);
                }
                fallback_binding = Some(binding);
            }

            let event = input.event_for_node(&self.core, id);
            self.trace_route(
                RoutePhase::WidgetEvent,
                Some(id),
                &path,
                format!("{event:?}"),
            );
            let outcome = self.core.dispatch_event_on_node(id, &event)?;

            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => {
                    self.trace_route(RoutePhase::Handled, Some(id), &path, format!("{outcome:?}"));
                    return Ok(true);
                }
                EventOutcome::Ignore => {
                    if let Some(binding) = fallback_binding {
                        self.trace_route(
                            RoutePhase::PostEventBinding,
                            Some(id),
                            &path,
                            "matched after widget ignored event",
                        );
                        return self.execute_routed_binding(id, &path, input, binding);
                    }
                    self.trace_route(RoutePhase::Bubble, Some(id), &path, "ignored");
                    target = self.core.nodes.get(id).and_then(|node| node.parent);
                    path.pop();
                }
            }
        }

        self.trace_route(RoutePhase::Unhandled, None, &path, "no handler");
        Ok(false)
    }

    /// Execute a binding after route resolution.
    fn execute_routed_binding(
        &mut self,
        node_id: NodeId,
        path: &Path,
        input: RoutedInput,
        binding: inputmap::BindingTarget,
    ) -> Result<bool> {
        let label = help::binding_label(
            &binding,
            &self.core.commands,
            |sid| self.script_host.script_source(sid),
            |id| self.script_host.function_label(id),
        );
        self.trace_route(RoutePhase::BindingExecution, Some(node_id), path, label);

        let event = input.event_for_node(&self.core, node_id);
        let frame = self.core.command_scope_for_event(&event);
        let depth = self.core.push_command_scope(frame);
        let result = self.execute_binding(node_id, binding);
        self.core.pop_command_scope(depth);
        self.fulfill_pending_help_request();
        result?;

        self.trace_route(
            RoutePhase::Handled,
            Some(node_id),
            path,
            "binding completed",
        );
        Ok(true)
    }

    /// Propagate a mouse event through the node under the event and all its ancestors.
    pub(crate) fn mouse(&mut self, m: mouse::MouseEvent) -> Result<()> {
        let (target, path) = self.mouse_route_start(m.location)?;
        let changed = self.route_input(target, path, RoutedInput::Mouse(m))?;
        if changed {
            self.render_pending = true;
        }
        Ok(())
    }

    /// Propagate a key event through the focus and all its ancestors.
    pub(crate) fn key<T>(&mut self, tk: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let key = tk.into();
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root);
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        let path = self.core.node_path(self.core.root, start);
        let changed = self.route_input(Some(start), path, RoutedInput::Key(key))?;
        if changed {
            self.render_pending = true;
        }

        Ok(())
    }

    /// Dispatch a focus-related event to the focused node, bubbling as needed.
    fn dispatch_focus_event(&mut self, event: &Event) -> Result<()> {
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root);
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        self.core.dispatch_event(start, event)?;
        Ok(())
    }

    /// Handle poll events by executing callbacks on each node in the list.
    fn poll(&mut self, ids: &[NodeId]) -> Result<()> {
        for id in ids {
            if self.core.nodes.contains_key(*id) {
                let next = self.core.with_widget_mut(*id, |w, core| {
                    let mut ctx = crate::core::context::CoreContext::new(core, *id);
                    w.poll(&mut ctx)
                })?;
                if let Some(d) = next {
                    self.poller.schedule(*id, d);
                }
            }
        }
        Ok(())
    }

    /// Drain queued automation callbacks that were marshalled onto the UI thread.
    pub(crate) fn service_automation(&mut self) {
        while let Ok(callback) = self.automation_rx.try_recv() {
            callback(self);
        }
    }

    /// Propagate an event through the tree.
    pub(crate) fn event(&mut self, e: Event) -> Result<()> {
        match e {
            Event::Key(k) => self.key(k),
            Event::Mouse(m) => self.mouse(m),
            Event::Resize(s) => {
                self.render_pending = true;
                self.set_root_size(s)
            }
            Event::Poll(ids) => {
                self.render_pending = true;
                self.poll(&ids)
            }
            Event::Paste(content) => {
                self.render_pending = true;
                let event = Event::Paste(content);
                self.dispatch_focus_event(&event)
            }
            Event::Wake => Ok(()),
            Event::FocusGained => {
                self.render_pending = true;
                self.dispatch_focus_event(&Event::FocusGained)
            }
            Event::FocusLost => {
                self.render_pending = true;
                self.dispatch_focus_event(&Event::FocusLost)
            }
        }
    }

    /// Set the size on the root node.
    pub fn set_root_size(&mut self, size: Size) -> Result<()> {
        self.root_size = Some(size);
        self.render_pending = true;
        self.core.update_layout(size)?;
        Ok(())
    }

    /// Execute a resolved binding target on a node.
    fn execute_binding(&mut self, node_id: NodeId, binding: inputmap::BindingTarget) -> Result<()> {
        match binding {
            inputmap::BindingTarget::Script(sid) => self.run_script(node_id, sid),
            inputmap::BindingTarget::Command(cmd) => {
                commands::dispatch(&mut self.core, node_id, &cmd)
                    .map(|_| ())
                    .map_err(Into::into)
            }
            inputmap::BindingTarget::CommandSequence(sequence) => {
                for command in sequence {
                    commands::dispatch(&mut self.core, node_id, &command)?;
                }
                Ok(())
            }
            inputmap::BindingTarget::SetInputMode(mode) => self.set_input_mode(&mode),
            inputmap::BindingTarget::LuauFunction(id) => {
                let host = self.script_host.clone();
                host.call_function(self, node_id, id)
            }
        }
    }

    /// Release any Luau closures referenced by removed bindings.
    pub(crate) fn release_removed_bindings(
        &self,
        removed: Vec<(inputmap::BindingId, inputmap::BindingTarget)>,
    ) -> usize {
        let released = removed.len();
        for (_, binding) in removed {
            self.release_binding_target(&binding);
        }
        released
    }

    /// Release script-host resources held by a binding target.
    pub(crate) fn release_binding_target(&self, binding: &inputmap::BindingTarget) {
        if let inputmap::BindingTarget::LuauFunction(id) = binding {
            self.script_host.release_function(*id);
        }
    }
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

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use super::*;
    use crate::{
        Context, ReadContext,
        commands::{CommandNode, CommandSpec},
        derive_commands,
        error::Result,
        geom::{Direction, Point, RectI32},
        layout::Layout,
        path::Path,
        state::NodeName,
        testing::{
            backend::{CanvasRender, TestRender},
            ttree::{Ba, BaLa, BaLb, OutcomeTarget, R, get_state, reset_state, run_ttree},
        },
        widget::{EventOutcome, Widget},
    };

    static POLL_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub struct PollWidget;

    #[derive_commands]
    impl PollWidget {
        pub fn new() -> Self {
            Self
        }
    }

    impl Widget for PollWidget {
        fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
            POLL_COUNT.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    pub struct StaticWidget;

    #[derive_commands]
    impl StaticWidget {
        pub fn new() -> Self {
            Self
        }
    }

    impl Widget for StaticWidget {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }
    }

    pub struct CaptureWidget {
        drags: usize,
    }

    #[derive_commands]
    impl CaptureWidget {
        pub fn new() -> Self {
            Self { drags: 0 }
        }
    }

    impl Widget for CaptureWidget {
        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
            if let Event::Mouse(mouse_event) = event {
                match mouse_event.action {
                    mouse::Action::Down if mouse_event.button == mouse::Button::Left => {
                        ctx.capture_mouse();
                        return Ok(EventOutcome::Handle);
                    }
                    mouse::Action::Drag if mouse_event.button == mouse::Button::Left => {
                        self.drags = self.drags.saturating_add(1);
                        return Ok(EventOutcome::Handle);
                    }
                    mouse::Action::Up if mouse_event.button == mouse::Button::Left => {
                        ctx.release_mouse();
                        return Ok(EventOutcome::Handle);
                    }
                    _ => {}
                }
            }
            Ok(EventOutcome::Ignore)
        }
    }

    fn set_outcome<T: Any + OutcomeTarget>(core: &mut Core, id: NodeId, outcome: EventOutcome) {
        let _ignored = core.with_widget_mut(id, |w, _| {
            let any = w as &mut dyn Any;
            if let Some(node) = any.downcast_mut::<T>() {
                node.set_outcome(outcome);
            }
        });
    }

    fn capture_drag_count(core: &mut Core, id: NodeId) -> usize {
        core.with_widget_mut(id, |w, _| {
            let any = w as &mut dyn Any;
            any.downcast_mut::<CaptureWidget>()
                .map(|widget| widget.drags)
                .unwrap_or(0)
        })
        .unwrap_or(0)
    }

    fn make_mouse_event(core: &Core, node_id: NodeId) -> mouse::MouseEvent {
        let loc = core
            .nodes
            .get(node_id)
            .map(|n| {
                let tl = n.view.outer.tl;
                Point {
                    x: tl.x.max(0) as u32,
                    y: tl.y.max(0) as u32,
                }
            })
            .unwrap_or_default();
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: loc,
        }
    }

    #[test]
    fn mouse_move_does_not_request_render() -> Result<()> {
        let mut canopy = Canopy::new();
        let app_id = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(StaticWidget::new()))?;
        canopy.core.set_layout_of(app_id, Layout::fill())?;
        canopy.set_root_size(Size::new(10, 6))?;

        let (_, mut render) = TestRender::create();
        canopy.render(&mut render)?;
        assert!(!canopy.render_if_pending(&mut render)?);

        let event = mouse::MouseEvent {
            action: mouse::Action::Moved,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: Point { x: 1, y: 1 },
        };
        canopy.event(Event::Mouse(event))?;
        assert!(!canopy.render_if_pending(&mut render)?);
        Ok(())
    }

    #[test]
    fn mouse_capture_routes_drag_outside() -> Result<()> {
        let mut canopy = Canopy::new();
        let app_id = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(CaptureWidget::new()))?;
        canopy.core.set_layout_of(app_id, Layout::fill())?;
        canopy.set_root_size(Size::new(10, 6))?;

        let (_, mut render) = TestRender::create();
        canopy.render(&mut render)?;

        let down = make_mouse_event(&canopy.core, app_id);
        canopy.event(Event::Mouse(down))?;

        let drag = mouse::MouseEvent {
            action: mouse::Action::Drag,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: Point { x: 50, y: 50 },
        };
        canopy.event(Event::Mouse(drag))?;

        assert_eq!(capture_drag_count(&mut canopy.core, app_id), 1);

        let up = mouse::MouseEvent {
            action: mouse::Action::Up,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: Point { x: 50, y: 50 },
        };
        canopy.event(Event::Mouse(up))?;

        Ok(())
    }

    #[test]
    fn set_widget_resets_initialization() -> Result<()> {
        POLL_COUNT.store(0, Ordering::SeqCst);
        let mut canopy = Canopy::new();
        let node_id = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(PollWidget::new()))?;
        canopy.set_root_size(Size::new(10, 10))?;

        let (_, mut render) = TestRender::create();
        render.render(&mut canopy)?;
        assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 1);

        canopy
            .core
            .replace_widget_keep_children(node_id, PollWidget::new())?;
        render.render(&mut canopy)?;
        assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[test]
    fn tbindings() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('a'.into()),
                "",
                c.script_host.compile(r#"ba_la.c_leaf()"#)?,
            )?;
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('r'.into()),
                "",
                c.script_host.compile(r#"r.c_root()"#)?,
            )?;
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('x'.into()),
                "ba/",
                c.script_host.compile(r#"r.c_root()"#)?,
            )?;

            c.core.set_focus(tree.a_a);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba_la.c_leaf()"]);

            reset_state();
            c.key('r')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

            reset_state();
            c.core.set_focus(tree.a);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "ba_la.c_leaf()"]);

            reset_state();
            c.core.set_focus(tree.a_a);
            c.key('x')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

            reset_state();
            c.core.set_focus(tree.root);
            c.key('x')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->ignore"]);

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn input_mode_binding_target_switches_modes() -> Result<()> {
        let mut canopy = Canopy::new();
        canopy.bind_input_mode("", inputmap::InputSpec::Key('i'.into()), "", "insert")?;

        canopy.key('i')?;

        assert_eq!(canopy.input_mode(), "insert");
        assert!(
            canopy
                .route_trace()
                .iter()
                .any(|entry| entry.phase == RoutePhase::BindingExecution)
        );
        Ok(())
    }

    #[test]
    fn route_trace_records_unhandled_key_pipeline() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            c.key('z')?;
            let phases = c
                .route_trace()
                .iter()
                .map(|entry| entry.phase)
                .collect::<Vec<_>>();

            assert!(phases.contains(&RoutePhase::Target));
            assert!(phases.contains(&RoutePhase::WidgetEvent));
            assert!(phases.contains(&RoutePhase::Bubble));
            assert!(phases.contains(&RoutePhase::Unhandled));
            assert!(c.diagnostic_dump(tree.a_a).contains("route trace:"));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn register_default_bindings_is_idempotent_for_identical_scripts() -> Result<()> {
        run_ttree(|c, _, _| {
            c.register_default_bindings("r", "canopy.log(\"once\")")?;
            c.register_default_bindings("r", "canopy.log(\"once\")")?;

            let err = c
                .register_default_bindings("r", "canopy.log(\"twice\")")
                .unwrap_err();
            assert!(matches!(err, error::Error::Invalid(_)));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tkey() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.root);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            c.key('a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec![
                    "ba@key->ignore",
                    "r@key->handle",
                    "ba@key->ignore",
                    "r@key->ignore"
                ]
            );
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Ignore);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_lb@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<BaLb>(&mut c.core, tree.a_b, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<BaLb>(&mut c.core, tree.a_b, EventOutcome::Handle);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            c.core.set_focus(tree.root);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_ttree(|c, mut tr, tree| {
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_ttree(|c, mut tr, tree| {
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_ttree(|c, mut tr, tree| {
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            let size: u32 = 100;
            let half = i32::try_from(size / 2).expect("size fits i32");
            tr.render(c)?;
            assert_eq!(
                c.core.nodes[tree.root].view.outer,
                RectI32::new(0, 0, size, size)
            );
            assert_eq!(
                c.core.nodes[tree.a].view.outer,
                RectI32::new(0, 0, size / 2, size)
            );
            assert_eq!(
                c.core.nodes[tree.b].view.outer,
                RectI32::new(half, 0, size / 2, size)
            );

            c.set_root_size(Size::new(50, 50))?;
            tr.render(c)?;
            assert_eq!(c.core.nodes[tree.b].view.outer, RectI32::new(25, 0, 25, 50));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn trender() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            tr.render(c)?;
            assert!(!tr.buf_empty());

            tr.render(c)?;
            assert!(tr.buf_empty());
            tr.render(c)?;
            tr.render(c)?;
            tr.render(c)?;

            tr.render(c)?;
            assert!(tr.buf_empty());

            c.core.set_focus(tree.a_a);
            tr.render(c)?;
            assert!(tr.buf_empty());

            c.core.focus_next(c.core.root);
            tr.render(c)?;
            assert!(tr.buf_empty());

            c.core.focus_prev(c.core.root);
            tr.render(c)?;
            assert!(tr.buf_empty());

            tr.render(c)?;
            assert!(tr.buf_empty());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn focus_path() -> Result<()> {
        run_ttree(|c, _, _tree| {
            assert_eq!(c.core.focus_path(c.core.root), Path::empty());
            c.core.focus_next(c.core.root);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r"]));
            c.core.focus_next(c.core.root);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r", "ba"]));
            c.core.focus_next(c.core.root);
            assert_eq!(
                c.core.focus_path(c.core.root),
                Path::new(&["r", "ba", "ba_la"])
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_next() -> Result<()> {
        run_ttree(|c, _, tree| {
            assert!(!c.core.is_focused(tree.root));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.root));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.a));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.a_a));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.a_b));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.b));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.b_a));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.b_b));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_prev() -> Result<()> {
        run_ttree(|c, _, tree| {
            assert!(!c.core.is_focused(tree.root));
            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b_b));

            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b_a));

            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b));

            c.core.set_focus(tree.root);
            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b_b));

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tshift_right() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            tr.render(c)?;
            c.core.set_focus(tree.a_a);
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_a));
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_a));

            c.core.set_focus(tree.a_b);
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_b));
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_b));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run_ttree(|c, _, tree| {
            assert_eq!(c.core.focus_path(c.core.root), Path::empty());

            assert!(!c.core.is_on_focus_path(tree.root));
            assert!(!c.core.is_on_focus_path(tree.a));

            c.core.set_focus(tree.a_a);
            assert!(c.core.is_on_focus_path(tree.root));
            assert!(c.core.is_on_focus_path(tree.a));
            assert!(!c.core.is_on_focus_path(tree.b));
            assert_eq!(
                c.core.focus_path(c.core.root),
                Path::new(&["r", "ba", "ba_la"])
            );

            c.core.set_focus(tree.a);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r", "ba"]));

            c.core.set_focus(tree.root);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r"]));

            c.core.set_focus(tree.b_a);
            assert_eq!(
                c.core.focus_path(c.core.root),
                Path::new(&["r", "bb", "bb_la"])
            );
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tkey_no_render() -> Result<()> {
        struct N;

        impl CommandNode for N {
            fn commands() -> &'static [&'static CommandSpec] {
                &[]
            }
        }

        impl Widget for N {
            fn layout(&self) -> Layout {
                Layout::fill()
            }

            fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
                true
            }

            fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
                r.text("any", ctx.view().outer_rect_local().line(0), "<n>")
            }

            fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
                let outcome = match event {
                    Event::Key(_) => EventOutcome::Consume,
                    _ => EventOutcome::Ignore,
                };
                Ok(outcome)
            }

            fn name(&self) -> NodeName {
                NodeName::convert("n")
            }
        }

        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        canopy.add_commands::<N>()?;
        canopy.core.replace_subtree(canopy.core.root, N)?;

        canopy.set_root_size(Size::new(10, 1))?;
        canopy.core.set_focus(canopy.core.root);
        canopy.render(&mut tr)?;
        assert!(!tr.buf_empty());
        let prev_buf = canopy.termbuf.clone().expect("missing termbuf");
        tr.text.lock().unwrap().text.clear();

        canopy.key('a')?;
        canopy.render(&mut tr)?;
        let next_buf = canopy.termbuf.clone().expect("missing termbuf");
        assert_eq!(prev_buf.cells, next_buf.cells);
        Ok(())
    }

    #[test]
    fn zero_size_child_ok() -> Result<()> {
        struct Child;

        #[derive_commands]
        impl Child {}

        impl Widget for Child {
            fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
                Ok(())
            }

            fn name(&self) -> NodeName {
                NodeName::convert("child")
            }
        }

        struct Parent;

        #[derive_commands]
        impl Parent {
            fn new() -> Self {
                Self
            }
        }

        impl Widget for Parent {
            fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
                Ok(())
            }

            fn name(&self) -> NodeName {
                NodeName::convert("parent")
            }
        }

        let size = Size::new(5, 1);
        let (_, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        canopy
            .core
            .replace_subtree(canopy.core.root, Parent::new())?;
        let child = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(Child))?;
        canopy
            .core
            .set_layout_of(child, Layout::column().fixed_width(0).fixed_height(0))?;

        canopy.set_root_size(size)?;
        canopy.render(&mut cr)?;
        Ok(())
    }
}
