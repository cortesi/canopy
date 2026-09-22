//! Script lifecycle: API finalization, startup and config scripts, default
//! bindings, fixtures, and the script journal.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt, fs,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::Instant,
};

use ruau::{filesystem::DirectoryMountsError, source::SourceProvider, vm::NativeModule};
use serde::{Deserialize, Serialize};

use super::{Canopy, EvalRequest};
use crate::{
    commands::{self, CommandDispatchKind},
    core::{
        NodeId,
        fixture::{Fixture, FixtureInfo},
        inputmap,
    },
    error::{self, Result},
    script,
};

/// Registered default binding script metadata.
struct DefaultBindingsScript {
    /// Source text evaluated for this owner.
    source: String,
    /// Pre-compiled script handle available after `finalize_api()`.
    script_id: Option<script::ScriptId>,
}

/// Registered app startup script metadata.
pub(super) struct StartupScript {
    /// Human-readable startup script name.
    name: String,
    /// Source text evaluated during startup.
    source: String,
    /// Pre-compiled script handle available after `finalize_api()`.
    script_id: Option<script::ScriptId>,
    /// Whether this script completed successfully.
    pub(super) ran: bool,
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

/// The script host and the sources, callbacks, and registrations it runs.
pub struct ScriptState {
    /// Script execution host.
    pub(crate) host: script::LuauHost,
    /// Stack of active script dispatch anchors for the current VM invocation.
    pub(crate) context_stack: Vec<NodeId>,
    /// Cached Luau API definition text.
    api_text: Option<String>,
    /// Configured persistent Luau module roots.
    pub(super) module_roots: script::ScriptModuleRoots,
    /// Finalized persistent Luau module source, if any.
    pub(super) module_source: Option<Arc<script::ScriptModuleSource>>,
    /// Extra audited Ruau native modules registered by the app.
    native_modules: Vec<Arc<dyn NativeModule>>,
    /// App-level startup scripts run before user and project init files.
    pub(super) startup_scripts: Vec<StartupScript>,
    /// Successfully executed filesystem startup modules.
    completed_startup_modules: HashSet<PathBuf>,
    /// Compiled handles retained across filesystem startup retries.
    startup_module_scripts: HashMap<PathBuf, script::ScriptId>,
    /// Binding targets whose release is deferred until a startup attempt
    /// commits.
    pub(super) deferred_binding_releases: Option<Vec<script::LuauFunctionId>>,
    /// Registered default binding scripts keyed by owner name.
    default_bindings: HashMap<String, DefaultBindingsScript>,
    /// Registered named fixtures keyed by fixture name.
    fixtures: HashMap<String, Fixture>,
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            host: script::LuauHost::new(),
            context_stack: Vec::new(),
            api_text: None,
            module_roots: script::ScriptModuleRoots::default(),
            module_source: None,
            native_modules: Vec::new(),
            startup_scripts: Vec::new(),
            completed_startup_modules: HashSet::new(),
            startup_module_scripts: HashMap::new(),
            deferred_binding_releases: None,
            default_bindings: HashMap::new(),
            fixtures: HashMap::new(),
        }
    }
}

/// Bounded in-memory history of script evaluations.
pub struct ScriptJournal {
    /// Retained entries, oldest first.
    entries: Vec<ScriptJournalEntry>,
    /// Next entry id; never reused, even after entries are evicted.
    next_id: u64,
    /// Maximum number of retained entries; the oldest are evicted first.
    limit: usize,
}

impl Default for ScriptJournal {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            limit: DEFAULT_SCRIPT_JOURNAL_LIMIT,
        }
    }
}

impl ScriptJournal {
    /// Allocate the id of the next entry.
    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Append an entry and evict the oldest entries beyond the limit.
    fn push(&mut self, entry: ScriptJournalEntry) {
        self.entries.push(entry);
        self.enforce_limit();
    }

    /// Replace the retention limit and evict the oldest entries beyond it.
    #[cfg(any(test, feature = "testing"))]
    pub(super) fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.enforce_limit();
    }

    /// Evict the oldest entries beyond the retention limit.
    fn enforce_limit(&mut self) {
        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(..excess);
        }
    }
}

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

impl ScriptJournalBaseline {
    /// Return the baseline of a top-level evaluation that started at
    /// `started`.
    pub(super) fn top_level(started: Instant) -> Self {
        Self {
            started,
            logs: 0,
            assertions: 0,
        }
    }
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
    /// Evaluate a Luau source string at the root and return its value.
    ///
    /// The synchronous caller restrictions of [`Self::eval`] apply.
    pub fn eval_script(&mut self, source: &str) -> Result<commands::ArgValue> {
        let outcome = self.eval(EvalRequest {
            source: source.to_owned(),
            timeout: None,
            anchor: self.root_id(),
        })?;
        outcome.into_result()
    }

    /// Finalize the script API surface if an evaluation needs it.
    pub(super) fn ensure_finalized(&mut self) -> Result<()> {
        if self.script.host.is_finalized() {
            return Ok(());
        }
        self.finalize_api_inner()
    }

    /// Configure the `@user` persistent script root for the builder.
    pub(super) fn set_user_script_root_inner(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.ensure_api_unfinalized("script module roots")?;
        self.script.module_roots.set_user_root(root);
        Ok(())
    }

    /// Configure the `@project` persistent script root for the builder.
    pub(super) fn set_project_script_root_inner(&mut self, root: impl Into<PathBuf>) -> Result<()> {
        self.ensure_api_unfinalized("script module roots")?;
        self.script.module_roots.set_project_root(root);
        Ok(())
    }

    /// Register an audited Ruau native module on the same surface as Canopy
    /// commands.
    #[cfg(test)]
    pub(crate) fn register_script_module(&mut self, module: Arc<dyn NativeModule>) -> Result<()> {
        self.ensure_api_unfinalized("script native module registration")?;
        self.script.native_modules.push(module);
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
            .script
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
        self.script.startup_scripts.push(StartupScript {
            name: name.to_string(),
            source: source.to_string(),
            script_id: None,
            ran: false,
        });
        Ok(())
    }

    /// Run app, user, and project startup scripts during preparation.
    pub(super) fn run_startup_scripts_inner(&mut self) -> Result<usize> {
        self.driver.startup_attempted = true;
        self.ensure_finalized()?;
        let host = self.script.host.clone();
        let mut ran = 0;
        let startup_scripts = self
            .script
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
            self.script.startup_scripts[index].ran = true;
            ran += 1;
        }
        for module in self.script.module_roots.startup_modules() {
            if self.script.completed_startup_modules.contains(&module.path) {
                continue;
            }
            let mounted_source = self
                .script
                .module_source
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
            let script_id = match self
                .script
                .startup_module_scripts
                .get(&module.path)
                .copied()
            {
                Some(script_id) => script_id,
                None => {
                    let script_id = host.compile_startup_source(mounted_source)?;
                    self.script
                        .startup_module_scripts
                        .insert(module.path.clone(), script_id);
                    script_id
                }
            };
            self.run_startup_attempt(
                ScriptOrigin::Startup(module_id.to_string()),
                &source,
                script_id,
            )?;
            self.script.completed_startup_modules.insert(module.path);
            ran += 1;
        }
        Ok(ran)
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
        let host = self.script.host.clone();
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
        debug_assert!(self.script.deferred_binding_releases.is_none());
        let attempt = StartupAttempt {
            application_bindings: self.core.input_map.snapshot_application(),
            hooks: self.script.host.on_start_hooks(),
        };
        self.script.deferred_binding_releases = Some(Vec::new());
        attempt
    }

    /// Commit a startup attempt and release targets it replaced or removed.
    fn commit_startup_attempt(&mut self) {
        let releases = self
            .script
            .deferred_binding_releases
            .take()
            .unwrap_or_default();
        for id in releases {
            self.script.host.release_function(id);
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
        self.script.deferred_binding_releases = None;
        for id in new_targets {
            self.script.host.release_function(id);
        }

        let baseline_hooks = attempt.hooks.iter().copied().collect::<HashSet<_>>();
        let current_hooks = self.script.host.replace_on_start_hooks(attempt.hooks);
        for hook in current_hooks {
            if !baseline_hooks.contains(&hook) {
                self.script.host.release_function(hook);
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
        if let Some(existing) = self.script.default_bindings.get(name) {
            if existing.source == script {
                return Ok(());
            }
            return Err(error::Error::Invalid(format!(
                "conflicting default bindings already registered for owner {name}"
            )));
        }
        self.script.default_bindings.insert(
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
        if let Some(existing) = self.script.fixtures.get(&fixture.name) {
            if Arc::ptr_eq(&existing.setup, &fixture.setup) {
                return Ok(());
            }
            return Err(error::Error::Invalid(format!(
                "conflicting fixture already registered for {}",
                fixture.name
            )));
        }
        self.script.fixtures.insert(fixture.name.clone(), fixture);
        Ok(())
    }

    /// Return registered fixture metadata in stable name order.
    pub fn fixture_infos(&self) -> Vec<FixtureInfo> {
        let mut fixtures = self
            .script
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
            .script
            .fixtures
            .get(name)
            .map(|fixture| Arc::clone(&fixture.setup))
            .ok_or_else(|| error::Error::NotFound(format!("fixture {name}")))?;
        setup(self)?;
        self.core.invalidate(crate::Invalidation::Paint);
        Ok(())
    }

    /// Type-check a named Luau source against the finalized app API.
    pub fn check_script(
        &mut self,
        source_name: &str,
        source: &str,
    ) -> Result<script::ScriptCheckResult> {
        self.ensure_finalized()?;
        self.script.host.check_script(source_name, source)
    }

    /// Drain and return log lines recorded by the most recent script
    /// evaluation.
    pub fn take_script_logs(&mut self) -> Vec<String> {
        self.script.host.take_logs()
    }

    /// Drain and return assertion outcomes from the most recent script
    /// evaluation.
    pub fn take_script_assertions(&mut self) -> Vec<script::ScriptAssertion> {
        self.script.host.take_assertions()
    }

    /// Return the in-memory script evaluation journal.
    ///
    /// The journal retains the most recent entries up to the configured limit.
    /// Entry ids are monotonic and never reused, so a first id greater than
    /// one indicates that older entries were evicted or cleared.
    pub fn script_journal(&self) -> &[ScriptJournalEntry] {
        &self.journal.entries
    }

    /// Evaluate a Luau config file during builder setup.
    pub(super) fn run_config_inner(&mut self, path: &FsPath) -> Result<()> {
        let baseline = self.begin_script_journal();
        let source = fs::read_to_string(path)
            .map_err(|err| error::Error::Invalid(format!("config read failed: {err}")))?;
        let result = (|| {
            self.ensure_finalized()?;
            let mounted_source = match &self.script.module_source {
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
                Some(source) => self.script.host.compile_source(source.source())?,
                None => self.script.host.compile(&source)?,
            };
            let host = self.script.host.clone();
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

    /// Finalize the script API surface for the consuming builder.
    pub(super) fn finalize_api_inner(&mut self) -> Result<()> {
        if self.script.host.is_finalized() {
            return Ok(());
        }
        let module_source = self.script.module_roots.module_source().map_err(|error| {
            error::Error::Invalid(format!("script module roots are invalid: {error}"))
        })?;
        let surface_source = module_source
            .as_ref()
            .map(|source| Arc::clone(source) as Arc<dyn SourceProvider>);
        let default_binding_owners = self.default_binding_owners();
        let existing_scripts = self.script.host.script_ids();
        let default_script_ids = self
            .script
            .default_bindings
            .iter()
            .map(|(owner, script)| (owner.clone(), script.script_id))
            .collect::<HashMap<_, _>>();
        let startup_script_ids = self
            .script
            .startup_scripts
            .iter()
            .map(|script| script.script_id)
            .collect::<Vec<_>>();
        let definitions = self.script.host.prepare_finalize(
            &self.core.commands,
            &default_binding_owners,
            &self.script.native_modules,
            surface_source,
            &self.fixture_infos(),
            self.core.input_map.widget_actions(),
        )?;
        let prepared = (|| {
            self.validate_script_module_declarations(module_source.as_ref())?;
            self.script
                .host
                .finalize_checkpoint(script::FinalizeStep::DeclarationsValidated)?;
            self.compile_registered_default_bindings()?;
            self.script
                .host
                .finalize_checkpoint(script::FinalizeStep::DefaultBindingsCompiled)?;
            self.compile_registered_startup_scripts()?;
            self.script
                .host
                .finalize_checkpoint(script::FinalizeStep::StartupScriptsCompiled)?;
            self.script.host.publish_finalize()
        })();
        if let Err(error) = prepared {
            self.script.host.abort_finalize(&existing_scripts);
            for (owner, script) in &mut self.script.default_bindings {
                script.script_id = default_script_ids.get(owner).copied().flatten();
            }
            for (script, previous) in self
                .script
                .startup_scripts
                .iter_mut()
                .zip(startup_script_ids)
            {
                script.script_id = previous;
            }
            return Err(error);
        }
        self.script.module_source = module_source;
        self.script.api_text = Some(definitions);
        self.core.input_map.freeze_widget_actions();
        Ok(())
    }

    /// Return the rendered Luau definition file for a ready app.
    pub fn script_api(&self) -> Result<&str> {
        self.script.api_text.as_deref().ok_or_else(|| {
            error::Error::InvalidOperation("script API is not finalized".to_string())
        })
    }

    /// Prepare a registered default binding script for a nested scoped run.
    pub(crate) fn prepare_registered_default_bindings(
        &self,
        owner: &str,
    ) -> Result<DefaultBindingsRun> {
        let script = self.script.default_bindings.get(owner).ok_or_else(|| {
            error::Error::NotFound(format!("default bindings not registered for owner {owner}"))
        })?;
        let script_id = script.script_id.ok_or_else(|| {
            error::Error::NotFound(format!("default bindings not compiled for owner {owner}"))
        })?;
        let source = script.source.clone();
        let host = self.script.host.clone();
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
    pub(super) fn ensure_api_unfinalized(&self, subject: &str) -> Result<()> {
        if self.script.host.is_finalized() {
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
        let Some(surface) = self.script.host.surface() else {
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
            self.script.module_roots.user_root(),
            self.script.module_roots.project_root(),
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
    pub(super) fn begin_script_journal(&self) -> ScriptJournalBaseline {
        // Top-level evaluations clear diagnostics on entry, so their baseline
        // is empty; nested evaluations record only what they add.
        let (logs, assertions) = if script::in_live_scope(self) {
            self.script.host.diagnostics_counts()
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
    pub(super) fn record_script_journal<T>(
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
        let mut logs = self.script.host.logs();
        let logs = logs.split_off(baseline.logs.min(logs.len()));
        let mut assertions = self.script.host.assertions();
        let assertions = assertions.split_off(baseline.assertions.min(assertions.len()));
        let id = self.journal.allocate_id();
        self.journal.push(ScriptJournalEntry {
            id,
            origin,
            source: source.to_string(),
            ok: result.is_ok(),
            error: result.as_ref().err().map(ToString::to_string),
            logs,
            assertions,
            duration_ms,
        });
    }

    /// Return the set of owners with registered default binding scripts.
    fn default_binding_owners(&self) -> BTreeSet<String> {
        self.script.default_bindings.keys().cloned().collect()
    }

    /// Compile any registered default binding scripts after finalization.
    fn compile_registered_default_bindings(&mut self) -> Result<()> {
        let host = self.script.host.clone();
        let mut scripts = self.script.default_bindings.iter_mut().collect::<Vec<_>>();
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
        let host = self.script.host.clone();
        for script in &mut self.script.startup_scripts {
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
    pub(super) fn run_on_start_hooks(&mut self) -> Result<bool> {
        let host = self.script.host.clone();
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
