// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy-mcp, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_mcp {
    //! MCP and smoke-test helpers for canopy applications.

    /// Errors returned by `canopy-mcp`.
    #[derive(Debug, Error, Display)]
    pub enum Error {
        /// The requested transport conflicts with explicit disabled automation.
        AutomationDisabled,
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

    /// Authority granted to local automation transports.
    #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
    pub enum AutomationPolicy {
        /// Do not start an automation listener.
        Disabled,
        /// Allow local clients to exercise the application's exposed native
        /// actions.
        TrustedLocal,
    }

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

    /// Explicit terminal and automation policy for an application launch.
    #[derive(Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq)]
    pub struct LaunchOptions {
        /// Terminal interrupt behavior.
        pub run_options: canopy::terminal::RunOptions,
        /// Trust granted to a requested automation listener.
        pub automation: AutomationPolicy,
    }

    /// Launch a Canopy app in the selected mode.
    ///
    /// The caller owns CLI parsing and app-specific configuration. This function
    /// owns the repeated framework wiring: API output, headless MCP, live MCP, and
    /// the terminal runloop.
    /// Explicit `HeadlessMcp` and socket modes grant `TrustedLocal` automation.
    /// Use `launch_with_options` when the application must deny that authority.
    pub fn launch(factory: crate::AppFactory, mode: LaunchMode) -> crate::Result<i32> {}

    /// Launch with explicit policies; disabled automation rejects listener modes
    /// before constructing the application or opening any transport.
    pub fn launch_with_options(
        factory: crate::AppFactory,
        mode: LaunchMode,
        options: LaunchOptions,
    ) -> crate::Result<i32> {
    }

    /// Shared application constructor with an explicit domain-state declaration.
    #[derive(Clone)]
    pub struct AppFactory {}

    impl AppFactory {
        /// Declare application identity and reset behavior.
        pub fn with_metadata(self, metadata: AppMetadata) -> Self {}

        /// Read the application declaration without constructing its UI.
        pub fn metadata(&self) -> &AppMetadata {}
    }

    impl Deref for AppFactory {
        type Target = dyn Fn() -> Result<Canopy, Error> + Send + Sync;
        fn deref(&self) -> &Self::Target {}
    }

    impl AsRef<dyn Fn() -> Result<Canopy, Error> + Send + Sync> for AppFactory {
        fn as_ref(&self) -> &(dyn Fn() -> crate::Result<Canopy> + Send + Sync + 'static) {}
    }

    /// Identity and domain reset behavior declared by an application factory.
    #[derive(Clone, Debug, StructuralPartialEq, PartialEq, Eq, Default)]
    pub struct AppMetadata {
        /// Stable application name used by replay compatibility checks.
        pub app: String,
        /// Domain reset behavior, independent of UI reconstruction.
        pub reset: ResetPolicy,
    }

    /// Owned execution metadata returned even when an evaluation fails.
    #[derive(Clone, Debug, Default, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ExecutionMetadata {
        /// Stable application identity.
        pub app: String,
        /// UI lifetime used by this evaluation.
        pub execution: ExecutionMode,
        /// Opaque identity of the application session, not a durable node
        /// reference.
        pub session_id: String,
        /// Evaluated or requested viewport.
        pub viewport: Viewport,
        /// Effective domain reset contract.
        pub reset: ResetPolicy,
        /// Generated API identity, absent only when preparation could not obtain
        /// it.
        pub api_digest: Option<String>,
    }

    impl JsonSchema for ExecutionMetadata {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Whether evaluation constructs a new UI or uses the running application.
    #[derive(
        Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize,
    )]
    pub enum ExecutionMode {
        /// Each evaluation constructs a fresh application UI.
        FreshAppPerEval,
        /// Evaluations share one running application.
        LiveSession,
    }

    impl JsonSchema for ExecutionMode {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Shared identity and fixture state for one live listener or direct session.
    #[derive(Clone, Debug)]
    pub struct LiveContext {}

    impl LiveContext {
        /// Create one context per running app session, then clone it for requests.
        pub fn new(app: AppMetadata) -> Self {}

        /// Record a successful explicit domain fixture application.
        pub fn fixture_applied(&self) {}
    }

    /// Application-declared reset behavior for domain data.
    #[derive(
        Clone, Copy, Debug, Default, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize,
    )]
    pub enum ResetPolicy {
        /// Domain state can persist outside the UI instance.
        External,
        /// An explicitly applied fixture resets the relevant domain state.
        Fixture,
        /// Each factory invocation owns independent domain state.
        Isolated,
    }

    impl JsonSchema for ResetPolicy {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Terminal dimensions in cells, serialized independently of internal geometry.
    #[derive(
        Clone, Copy, Debug, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize, Default,
    )]
    pub struct Viewport {
        /// Number of columns.
        pub width: u32,
        /// Number of rows.
        pub height: u32,
    }

    impl Viewport {
        /// Reject empty or excessive headless dimensions before constructing an
        /// app.
        pub fn validate(self) -> CanopyResult<()> {}
    }

    impl JsonSchema for Viewport {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    impl From<Viewport> for canopy::geom::Size {
        fn from(viewport: Viewport) -> Self {}
    }

    impl From<Size> for Viewport {
        fn from(size: Size) -> Self {}
    }

    /// Wrap a constructor with a conservative external-state declaration.
    pub fn app_factory<F>(factory: F) -> AppFactory
    where
        F: Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static, {
    }

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

        /// Bootstrap at requested dimensions, validated before factory invocation.
        pub fn bootstrap_with_request(
            &self,
            request: &BootstrapRequest,
        ) -> Result<BootstrapResponse> {
        }

        /// Evaluate a Luau script against a fresh headless app.
        pub fn evaluate(&self, request: &ScriptEvalRequest) -> ScriptEvalOutcome {}
    }

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

    /// Optional dimensions for bootstrap replay preflight.
    #[derive(Deserialize, Debug, Clone, Default, StructuralPartialEq, PartialEq, Serialize)]
    pub struct BootstrapRequest {
        /// Requested headless viewport, or a live viewport compatibility check.
        pub viewport: Option<crate::Viewport>,
    }

    impl JsonSchema for BootstrapRequest {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Bootstrap payload for an agent entering a Canopy app.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct BootstrapResponse {
        /// Execution identity and domain reset behavior for this bootstrap
        /// instance.
        pub metadata: crate::ExecutionMetadata,
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
        /// Execution identity and domain reset behavior for this request.
        pub metadata: crate::ExecutionMetadata,
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
        /// Optional headless dimensions; live requests may only confirm current
        /// dimensions.
        pub viewport: Option<crate::Viewport>,
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

    /// Evaluate a Luau script against an existing live canopy app.
    pub fn evaluate_live(
        canopy: &mut canopy::Canopy,
        request: &ScriptEvalRequest,
        context: &crate::LiveContext,
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
    /// This low-level entry point grants trusted-local access to all exposed native
    /// actions. Use `launch_with_options` to enforce an application launch policy.
    pub fn serve_stdio(factory: crate::AppFactory) -> crate::Result<()> {}

    /// Serve live MCP automation for a running canopy app over a Unix-domain
    /// socket.
    /// This low-level entry point grants trusted-local access. The application must
    /// choose a private socket directory and enforce suitable host filesystem
    /// permissions.
    pub fn serve_uds(
        socket_path: impl AsRef<std::path::Path>,
        automation: canopy::AutomationHandle,
    ) -> crate::Result<UdsServerHandle> {
    }

    /// Serve a live application with an explicit identity retained across
    /// reconnects.
    pub fn serve_uds_with_context(
        socket_path: impl AsRef<std::path::Path>,
        automation: canopy::AutomationHandle,
        context: crate::LiveContext,
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
        factory: crate::AppFactory,
        config: &SuiteConfig,
    ) -> crate::Result<SuiteResult> {
    }
}
