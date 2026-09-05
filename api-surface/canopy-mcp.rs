// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy-mcp, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_mcp {
    //! MCP and smoke-test helpers for canopy applications.

    /// Errors returned by `canopy-mcp`.
    #[derive(Debug, Error, Display)]
    pub enum Error {
        /// A canopy runtime error.
        Canopy(canopy::error::Error),
        /// A canopy command conversion error.
        Command(canopy::commands::CommandError),
        /// An I/O error.
        Io(io::Error),
        /// An MCP transport or protocol error.
        Tmcp(tmcp::Error),
        /// The application factory failed to build an app instance.
        App(Box<dyn StdError + Send + Sync>),
        /// The UDS listener thread panicked while shutting down.
        ListenerThreadPanicked,
        /// The UDS listener stopped before reporting startup readiness.
        ListenerReadinessClosed,
        /// A smoke suite did not resolve to any Luau scripts.
        NoScripts(std::path::PathBuf),
    }

    impl Error {
        /// Wrap an application-specific setup error.
        pub fn app(error: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {}
    }

    impl From<Error> for Error {
        fn from(source: CanopyError) -> Self {}
    }

    impl From<CommandError> for Error {
        fn from(source: CommandError) -> Self {}
    }

    impl From<Error> for Error {
        fn from(source: io::Error) -> Self {}
    }

    impl From<Error> for Error {
        fn from(source: tmcp::Error) -> Self {}
    }

    /// Result type used by `canopy-mcp`.
    pub type Result<T> = std::result::Result<T, Error>;

    /// Launcher mode for a Canopy application.
    pub enum LaunchMode {
        /// Run the interactive terminal UI.
        Run {
            /// Optional live MCP Unix-domain socket path.
            mcp_socket: Option<std::path::PathBuf>,
        },
        /// Serve the headless MCP automation server over stdio.
        HeadlessMcp,
        /// Print the generated Luau API and exit.
        Api,
    }

    /// Launch a Canopy app in the selected mode.
    ///
    /// The caller owns CLI parsing and app-specific configuration. This function
    /// owns the repeated framework wiring: API output, headless MCP, live MCP, and
    /// the terminal runloop.
    pub fn launch(factory: crate::script::AppFactory, mode: LaunchMode) -> crate::Result<i32> {}

    /// Headless evaluator that creates a fresh canopy app instance for each
    /// request.
    #[derive(Clone)]
    pub struct AppEvaluator {}

    impl AppEvaluator {
        /// Construct an evaluator with a default headless viewport size.
        pub fn new(factory: AppFactory) -> Self {}

        /// Render and return the app's Luau API definition.
        pub fn script_api(&self) -> Result<String> {}

        /// Return the evaluator's registered fixture catalog.
        pub fn fixtures(&self) -> Result<Vec<FixtureInfo>> {}

        /// Return bootstrap information for a fresh headless app instance.
        pub fn bootstrap(&self) -> Result<BootstrapResponse> {}

        /// Evaluate a Luau script against a fresh headless app.
        pub fn evaluate(&self, request: &ScriptEvalRequest) -> ScriptEvalOutcome {}
    }

    /// Shared application factory used by the automation helpers.
    pub type AppFactory = std::sync::Arc<dyn Fn() -> crate::Result<canopy::Canopy> + Send + Sync>;

    /// Compact command availability record returned by bootstrap.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct BootstrapCommand {
        /// Command name relative to its owner.
        pub name: String,
        /// Widget owner name, or empty for free commands.
        pub owner: String,
        /// Whether the command currently resolves.
        pub available: bool,
        /// Eligibility, when an owner resolves.
        pub status: Option<String>,
        /// User-facing reason for disabled eligibility.
        pub disabled_reason: Option<String>,
        /// Missing originating event or row context, separate from eligibility.
        pub missing_requirements: Vec<String>,
        /// Debug token for the current target node, when available.
        pub target: Option<String>,
    }

    impl JsonSchema for BootstrapCommand {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Compact script journal record returned by bootstrap.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct BootstrapJournalEntry {
        /// Monotonic journal id.
        pub id: u64,
        /// Script origin.
        pub origin: String,
        /// Whether the evaluation completed successfully.
        pub ok: bool,
        /// Number of logs emitted by this evaluation.
        pub log_count: usize,
        /// Number of assertions emitted by this evaluation.
        pub assertion_count: usize,
        /// Wall-clock duration in milliseconds.
        pub duration_ms: u64,
    }

    impl JsonSchema for BootstrapJournalEntry {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Bootstrap payload for an agent entering a Canopy app.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct BootstrapResponse {
        /// Operating guide for the automation surface.
        pub guide: String,
        /// Stable FNV-1a digest of the generated API.
        pub api_digest: String,
        /// Compact discovery inventory for the generated API.
        pub api_sources: Vec<ruau_script_api::ScriptApiEntry>,
        /// Registered fixtures.
        pub fixtures: Vec<canopy::FixtureInfo>,
        /// Legacy focus-relative command availability.
        pub commands: Vec<BootstrapCommand>,
        /// Default top-level script target policy.
        pub default_target: String,
        /// Availability from the root used by top-level script evaluation.
        pub default_commands: Vec<BootstrapCommand>,
        /// Availability from the currently focused node.
        pub focus_commands: Vec<BootstrapCommand>,
        /// Recent script journal entries.
        pub journal: Vec<BootstrapJournalEntry>,
    }

    impl JsonSchema for BootstrapResponse {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Error details included in a failed script evaluation.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct ScriptErrorInfo {
        /// Pipeline stage that failed: `build`, `typecheck`, `timeout`, `runtime`,
        /// `cancelled`, or `invalid`.
        pub error_type: String,
        /// Stable host error category such as `no_target` or `unknown_command`,
        /// when the failure carried structured fields.
        pub kind: Option<String>,
        /// Command id when the error came from command dispatch.
        pub command: Option<String>,
        /// Owner name when the error came from node-target resolution.
        pub owner: Option<String>,
        /// Human-readable error message.
        pub message: String,
    }

    impl JsonSchema for ScriptErrorInfo {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Structured response for the `script_eval` tool and smoke runner.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct ScriptEvalOutcome {
        /// Whether the script completed successfully.
        pub success: bool,
        /// Final task state for the evaluation.
        pub state: ScriptTaskState,
        /// Optional JSON-serializable script return value.
        pub value: Option<serde_json::Value>,
        /// Log lines emitted during evaluation.
        pub logs: Vec<String>,
        /// Assertion outcomes recorded during evaluation.
        pub assertions: Vec<canopy::script::ScriptAssertion>,
        /// Typecheck diagnostics captured before execution.
        pub diagnostics: Vec<canopy::script::ScriptCheckDiagnostic>,
        /// Timing information for the request.
        pub timing: ScriptTiming,
        /// Error payload when evaluation fails.
        pub error: Option<ScriptErrorInfo>,
    }

    impl ScriptEvalOutcome {
        /// Encode the outcome as an MCP tool result.
        pub fn to_tool_result(&self) -> CallToolResult {}
    }

    impl JsonSchema for ScriptEvalOutcome {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Request payload for the `script_eval` tool.
    #[derive(Deserialize, Debug, Clone, StructuralPartialEq, PartialEq, Serialize)]
    pub struct ScriptEvalRequest {
        /// Luau source code to execute.
        pub script: String,
        /// Optional named fixture applied before evaluation.
        pub fixture: Option<String>,
        /// Optional evaluation timeout in milliseconds.
        pub timeout_ms: Option<u64>,
    }

    impl JsonSchema for ScriptEvalRequest {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Evaluation task state exposed to automation callers.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ScriptTaskState {
        /// Evaluation completed successfully.
        Completed,
        /// Evaluation failed before completion.
        Failed,
        /// Evaluation stopped at the cooperative timeout boundary.
        TimedOut,
        /// Evaluation was explicitly cancelled.
        Cancelled,
    }

    impl JsonSchema for ScriptTaskState {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Timing information for a script evaluation.
    #[derive(Debug, Clone, Default, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ScriptTiming {
        /// Time spent constructing and rendering the headless app.
        pub build_ms: u64,
        /// Time spent executing the script and final render.
        pub exec_ms: u64,
        /// Total wall-clock time for the request.
        pub total_ms: u64,
    }

    impl JsonSchema for ScriptTiming {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Convert a closure into a shared app factory.
    pub fn app_factory<F>(factory: F) -> AppFactory
    where
        F: Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static, {
    }

    /// Evaluate a Luau script against an existing live canopy app.
    pub fn evaluate_live(
        canopy: &mut canopy::Canopy,
        request: &ScriptEvalRequest,
    ) -> ScriptEvalOutcome {
    }

    /// Request payload for applying a named fixture to a live app.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Deserialize, Serialize)]
    pub struct ApplyFixtureRequest {
        /// Registered fixture name.
        pub name: String,
    }

    impl JsonSchema for ApplyFixtureRequest {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Handle for a running live UDS MCP listener.
    pub struct UdsServerHandle {}

    impl UdsServerHandle {
        /// Stop the listener and remove the socket path.
        pub fn stop(self) -> Result<()> {}
    }

    impl Drop for UdsServerHandle {
        fn drop(&mut self) {}
    }

    /// Build an MCP tool result with structured and text JSON payloads.
    pub fn json_tool_result(value: serde_json::Value) -> tmcp::schema::CallToolResult {}

    /// Serve `bootstrap`, `script_eval`, `script_api`, and `fixtures` over stdio
    /// for an app factory.
    pub fn serve_stdio(factory: crate::script::AppFactory) -> crate::Result<()> {}

    /// Serve live MCP automation for a running canopy app over a Unix-domain
    /// socket.
    pub fn serve_uds(
        socket_path: impl AsRef<std::path::Path>,
        automation: canopy::AutomationHandle,
    ) -> crate::Result<UdsServerHandle> {
    }

    /// Result of running one smoke script.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct ScriptResult {
        /// Script path on disk.
        pub path: std::path::PathBuf,
        /// Fixture derived for this script, if any.
        pub fixture: Option<String>,
        /// Structured script outcome, carrying success, timing, and error details.
        pub outcome: crate::script::ScriptEvalOutcome,
    }

    /// Configuration for a smoke-suite run.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq)]
    pub struct SuiteConfig {
        /// Root directory to scan for `.luau` scripts when no explicit script list
        /// is provided.
        pub suite_dir: std::path::PathBuf,
        /// Optional subset of scripts to run. Relative paths are resolved against
        /// `suite_dir`.
        pub scripts: Vec<std::path::PathBuf>,
    }

    impl SuiteConfig {
        /// Construct a config using a suite directory and default options.
        pub fn new(suite_dir: impl Into<PathBuf>) -> Self {}
    }

    /// Aggregated result for a smoke suite.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct SuiteResult {
        /// Per-script results in execution order.
        pub scripts: Vec<ScriptResult>,
    }

    impl SuiteResult {
        /// Return true when all smoke scripts passed.
        pub fn success(&self) -> bool {}
    }

    /// Recursively collect `.luau` scripts under a directory.
    pub fn collect_luau_scripts(
        root: &std::path::Path,
        output: &mut Vec<std::path::PathBuf>,
    ) -> crate::Result<()> {
    }

    /// Resolve the ordered list of smoke scripts for a suite run.
    ///
    /// An explicit script list keeps its given order, because that order decides
    /// which script a fail-fast run stops on. Discovered files are sorted so a
    /// directory walk is reproducible.
    pub fn discover_scripts(config: &SuiteConfig) -> crate::Result<Vec<std::path::PathBuf>> {}

    /// Derive a fixture name from the first path component under the suite root.
    ///
    /// Only a normal component names a fixture; a root, prefix, or `..` component
    /// does not.
    pub fn fixture_for_script(
        suite_dir: &std::path::Path,
        script: &std::path::Path,
    ) -> Option<String> {
    }

    /// Run a smoke suite against fresh headless app instances.
    pub fn run_suite(
        factory: impl Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static,
        config: &SuiteConfig,
    ) -> crate::Result<SuiteResult> {
    }
}
