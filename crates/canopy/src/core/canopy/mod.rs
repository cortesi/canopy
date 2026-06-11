#![expect(
    clippy::multiple_inherent_impl,
    reason = "Canopy methods are split by facade, rendering, and routing concerns."
)]

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::Write,
    path::Path as FsPath,
    sync::mpsc,
    time::Duration,
};

use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};

use super::{inputmap, poll::Poller, termbuf::TermBuf};

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

    /// Return the root node ID.
    pub fn root_id(&self) -> NodeId {
        self.core.root_id()
    }

    /// Create a detached widget node.
    pub fn create_detached<W>(&mut self, widget: W) -> TypedId<W>
    where
        W: Widget + 'static,
    {
        TypedId::new(self.core.create_detached(widget))
    }

    /// Replace the root's children with a single node.
    pub fn set_root_child(&mut self, child: impl Into<NodeId>) -> Result<()> {
        let root = self.root_id();
        self.core.set_children(root, vec![child.into()])
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

    /// Return the internal core state.
    #[doc(hidden)]
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Return the internal core state mutably.
    #[doc(hidden)]
    pub fn core_mut(&mut self) -> &mut Core {
        self.render_pending = true;
        &mut self.core
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

    /// Evaluate a Luau source string with a cooperative timeout.
    pub fn eval_script_value_with_timeout(
        &mut self,
        source: &str,
        timeout: Duration,
    ) -> Result<commands::ArgValue> {
        if !self.script_host.is_finalized() {
            self.finalize_api()?;
        }
        let script_id = self.compile_script(source)?;
        let host = self.script_host.clone();
        host.execute_value_with_timeout(self, self.core.root_id(), script_id, timeout)
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

    /// Type-check a Luau source string against the finalized app API.
    pub fn check_script(&mut self, source: &str) -> Result<script::ScriptCheckResult> {
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
