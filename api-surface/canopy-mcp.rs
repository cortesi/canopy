// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy-mcp, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_mcp {
    //! MCP and smoke-test helpers for canopy applications.

    pub mod error {
        //! Error types shared across the automation helpers.

        /// Result type used by `canopy-mcp`.
        pub type Result<T> = std::result::Result<T, Error>;

        /// Errors returned by `canopy-mcp`.
        #[derive(Debug, Error, Display)]
        pub enum Error {
            /// A canopy runtime error.
            Canopy(canopy::error::Error),
            /// A canopy command conversion error.
            Command(canopy::commands::CommandError),
            /// An I/O error.
            Io(io::Error),
            /// A JSON encoding or decoding error.
            Json(serde_json::Error),
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
            pub fn app(error: impl StdError + Send + Sync + 'static) -> Self {}

            /// Wrap an already type-erased application setup error.
            pub fn app_boxed(error: Box<dyn StdError + Send + Sync>) -> Self {}
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
            fn from(source: serde_json::Error) -> Self {}
        }

        impl From<Error> for Error {
            fn from(source: tmcp::Error) -> Self {}
        }
    }

    pub mod launch {
        //! Shared executable launch harness for app binaries.

        /// Launcher mode for a Canopy application.
        pub enum LaunchMode {
            /// Run the interactive terminal UI.
            Run {
                /// Optional live MCP Unix-domain socket path.
                mcp_socket: Option<std::path::PathBuf>,
            },
            /// Serve the headless MCP automation server over stdio.
            HeadlessMcp,
            /// Run a Luau smoke suite against fresh headless app instances.
            Smoke(crate::SuiteConfig),
            /// Print the generated Luau API and exit.
            Api,
        }

        impl LaunchMode {
            /// Run the interactive terminal UI.
            pub fn run() -> Self {}

            /// Run the interactive terminal UI with a live MCP socket.
            pub fn run_with_mcp(socket_path: PathBuf) -> Self {}
        }

        /// Launch a Canopy app in the selected mode.
        ///
        /// The caller owns CLI parsing and app-specific configuration. This function
        /// owns the repeated framework wiring: API output, headless MCP, smoke suites,
        /// live MCP, and the terminal runloop.
        pub fn launch(factory: crate::script::AppFactory, mode: LaunchMode) -> crate::Result<i32> {}
    }

    pub mod script {
        //! Headless script-evaluation types and helpers.

        /// Shared application factory used by the automation helpers.
        pub type AppFactory =
            std::sync::Arc<dyn Fn() -> crate::Result<canopy::Canopy> + Send + Sync>;

        /// Convert a closure into a shared app factory.
        pub fn app_factory<F>(factory: F) -> AppFactory
        where
            F: Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static, {
        }

        /// Request payload for the `script_eval` tool.
        #[derive(Deserialize, Debug, Clone, StructuralPartialEq, PartialEq)]
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

        /// Structured typecheck diagnostic returned by `script_eval`.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
        pub struct ScriptDiagnostic {
            /// Diagnostic severity such as `error` or `warning`.
            pub severity: String,
            /// One-based line number, or zero when the diagnostic is not source-bound.
            pub line: usize,
            /// One-based column number, or zero when the diagnostic is not source-bound.
            pub column: usize,
            /// Human-readable diagnostic message.
            pub message: String,
        }

        impl JsonSchema for ScriptDiagnostic {
            fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

            fn inline_schema() -> bool {}
        }

        /// Assertion outcome recorded during script execution.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
        pub struct ScriptAssertion {
            /// Whether the assertion passed.
            pub passed: bool,
            /// Assertion message emitted by the runtime.
            pub message: String,
        }

        impl JsonSchema for ScriptAssertion {
            fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

            fn inline_schema() -> bool {}
        }

        /// Timing information for a script evaluation.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
        pub struct ScriptTiming {
            /// Time spent constructing and rendering the headless app.
            pub build_ms: u64,
            /// Time spent executing the script and final render.
            pub exec_ms: u64,
            /// Total wall-clock time for the request.
            pub total_ms: u64,
        }

        impl ScriptTiming {
            /// Zeroed timing information for early errors.
            pub fn zero() -> Self {}
        }

        impl JsonSchema for ScriptTiming {
            fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

            fn inline_schema() -> bool {}
        }

        /// Evaluation task state exposed to automation callers.
        #[derive(
            Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize,
        )]
        pub enum ScriptTaskState {
            /// Evaluation completed successfully.
            Completed,
            /// Evaluation failed before completion.
            Failed,
            /// Evaluation stopped at the cooperative timeout boundary.
            TimedOut,
        }

        impl JsonSchema for ScriptTaskState {
            fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

            fn inline_schema() -> bool {}
        }

        /// Error details included in a failed script evaluation.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
        pub struct ScriptErrorInfo {
            /// Pipeline stage that failed: `build`, `typecheck`, `timeout`, or `runtime`.
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
            pub assertions: Vec<ScriptAssertion>,
            /// Typecheck diagnostics captured before execution.
            pub diagnostics: Vec<ScriptDiagnostic>,
            /// Timing information for the request.
            pub timing: ScriptTiming,
            /// Error payload when evaluation fails.
            pub error: Option<ScriptErrorInfo>,
        }

        impl ScriptEvalOutcome {
            /// Encode the outcome as an MCP tool result.
            pub fn to_tool_result(&self) -> CallToolResult {}

            /// Build a failure payload with no result value.
            pub fn error_only(
                error_type: impl Into<String>,
                message: impl Into<String>,
                diagnostics: Vec<ScriptDiagnostic>,
                timing: ScriptTiming,
            ) -> Self {
            }
        }

        impl JsonSchema for ScriptEvalOutcome {
            fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

            fn inline_schema() -> bool {}
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
            /// Full generated Luau API definition.
            pub api: String,
            /// Stable FNV-1a digest of `api`.
            pub api_digest: String,
            /// Registered fixtures.
            pub fixtures: Vec<canopy::FixtureInfo>,
            /// Current command availability.
            pub commands: Vec<BootstrapCommand>,
            /// Recent script journal entries.
            pub journal: Vec<BootstrapJournalEntry>,
        }

        impl JsonSchema for BootstrapResponse {
            fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

            fn inline_schema() -> bool {}
        }

        /// Headless evaluator that creates a fresh canopy app instance for each request.
        #[derive(Clone)]
        pub struct AppEvaluator {}

        impl AppEvaluator {
            /// Construct an evaluator with a default headless viewport size.
            pub fn new(factory: AppFactory) -> Self {}

            /// Override the headless viewport size used for evaluations.
            pub fn with_view_size(self, width: u32, height: u32) -> Self {}

            /// Render and return the app's Luau API definition.
            pub fn script_api(&self) -> Result<String> {}

            /// Return the evaluator's registered fixture catalog.
            pub fn fixtures(&self) -> Result<Vec<FixtureInfo>> {}

            /// Return bootstrap information for a fresh headless app instance.
            pub fn bootstrap(&self) -> Result<BootstrapResponse> {}

            /// Evaluate a Luau script against a fresh headless app.
            pub fn evaluate(&self, request: &ScriptEvalRequest) -> ScriptEvalOutcome {}
        }

        /// Build a bootstrap payload from a finalized app.
        pub fn bootstrap_for_canopy(
            canopy: &mut canopy::Canopy,
        ) -> crate::Result<BootstrapResponse> {
        }

        /// Evaluate a Luau script against an existing live canopy app.
        pub fn evaluate_live(
            canopy: &mut canopy::Canopy,
            request: &ScriptEvalRequest,
        ) -> ScriptEvalOutcome {
        }
    }

    pub mod server {
        //! Stdio MCP server wrapper for script automation.

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

        /// Serve `script_eval` and `script_api` over stdio for an app factory.
        pub fn serve_stdio(
            factory: impl Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static,
        ) -> crate::Result<()> {
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

        /// Serve live MCP automation for a running canopy app over a Unix-domain socket.
        pub fn serve_uds(
            socket_path: impl AsRef<std::path::Path>,
            automation: canopy::AutomationHandle,
        ) -> crate::Result<UdsServerHandle> {
        }
    }

    pub mod smoke {
        //! Smoke-suite discovery and execution helpers.

        /// Configuration for a smoke-suite run.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq)]
        pub struct SuiteConfig {
            /// Root directory to scan for `.luau` scripts when no explicit script list is provided.
            pub suite_dir: std::path::PathBuf,
            /// Optional subset of scripts to run. Relative paths are resolved against `suite_dir`.
            pub scripts: Vec<std::path::PathBuf>,
            /// Optional timeout per script in milliseconds.
            pub timeout_ms: Option<u64>,
            /// Stop after the first failing script when true.
            pub fail_fast: bool,
        }

        impl SuiteConfig {
            /// Construct a config using a suite directory and default options.
            pub fn new(suite_dir: impl Into<PathBuf>) -> Self {}
        }

        /// Final status for a smoke script.
        #[derive(
            Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize,
        )]
        pub enum ScriptStatus {
            /// The script passed.
            Passed,
            /// The script failed.
            Failed,
        }

        /// Result of running one smoke script.
        #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
        pub struct ScriptResult {
            /// Script path on disk.
            pub path: std::path::PathBuf,
            /// Fixture derived for this script, if any.
            pub fixture: Option<String>,
            /// Pass or fail status.
            pub status: ScriptStatus,
            /// Total script duration in milliseconds.
            pub elapsed_ms: u64,
            /// Optional summary message.
            pub message: Option<String>,
            /// Structured script outcome.
            pub outcome: crate::script::ScriptEvalOutcome,
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

        /// Run a smoke suite against fresh headless app instances.
        pub fn run_suite(
            factory: impl Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static,
            config: &SuiteConfig,
        ) -> crate::Result<SuiteResult> {
        }
    }

    /// Errors returned by `canopy-mcp`.
    #[derive(Debug, Error, Display)]
    pub enum Error {
        /// A canopy runtime error.
        Canopy(canopy::error::Error),
        /// A canopy command conversion error.
        Command(canopy::commands::CommandError),
        /// An I/O error.
        Io(io::Error),
        /// A JSON encoding or decoding error.
        Json(serde_json::Error),
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
        pub fn app(error: impl StdError + Send + Sync + 'static) -> Self {}

        /// Wrap an already type-erased application setup error.
        pub fn app_boxed(error: Box<dyn StdError + Send + Sync>) -> Self {}
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
        fn from(source: serde_json::Error) -> Self {}
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
        /// Run a Luau smoke suite against fresh headless app instances.
        Smoke(crate::SuiteConfig),
        /// Print the generated Luau API and exit.
        Api,
    }

    impl LaunchMode {
        /// Run the interactive terminal UI.
        pub fn run() -> Self {}

        /// Run the interactive terminal UI with a live MCP socket.
        pub fn run_with_mcp(socket_path: PathBuf) -> Self {}
    }

    /// Launch a Canopy app in the selected mode.
    ///
    /// The caller owns CLI parsing and app-specific configuration. This function
    /// owns the repeated framework wiring: API output, headless MCP, smoke suites,
    /// live MCP, and the terminal runloop.
    pub fn launch(factory: crate::script::AppFactory, mode: LaunchMode) -> crate::Result<i32> {}

    /// Headless evaluator that creates a fresh canopy app instance for each request.
    #[derive(Clone)]
    pub struct AppEvaluator {}

    impl AppEvaluator {
        /// Construct an evaluator with a default headless viewport size.
        pub fn new(factory: AppFactory) -> Self {}

        /// Override the headless viewport size used for evaluations.
        pub fn with_view_size(self, width: u32, height: u32) -> Self {}

        /// Render and return the app's Luau API definition.
        pub fn script_api(&self) -> Result<String> {}

        /// Return the evaluator's registered fixture catalog.
        pub fn fixtures(&self) -> Result<Vec<FixtureInfo>> {}

        /// Return bootstrap information for a fresh headless app instance.
        pub fn bootstrap(&self) -> Result<BootstrapResponse> {}

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
        /// Full generated Luau API definition.
        pub api: String,
        /// Stable FNV-1a digest of `api`.
        pub api_digest: String,
        /// Registered fixtures.
        pub fixtures: Vec<canopy::FixtureInfo>,
        /// Current command availability.
        pub commands: Vec<BootstrapCommand>,
        /// Recent script journal entries.
        pub journal: Vec<BootstrapJournalEntry>,
    }

    impl JsonSchema for BootstrapResponse {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Assertion outcome recorded during script execution.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ScriptAssertion {
        /// Whether the assertion passed.
        pub passed: bool,
        /// Assertion message emitted by the runtime.
        pub message: String,
    }

    impl JsonSchema for ScriptAssertion {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Structured typecheck diagnostic returned by `script_eval`.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ScriptDiagnostic {
        /// Diagnostic severity such as `error` or `warning`.
        pub severity: String,
        /// One-based line number, or zero when the diagnostic is not source-bound.
        pub line: usize,
        /// One-based column number, or zero when the diagnostic is not source-bound.
        pub column: usize,
        /// Human-readable diagnostic message.
        pub message: String,
    }

    impl JsonSchema for ScriptDiagnostic {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Error details included in a failed script evaluation.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Serialize, Deserialize)]
    pub struct ScriptErrorInfo {
        /// Pipeline stage that failed: `build`, `typecheck`, `timeout`, or `runtime`.
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
        pub assertions: Vec<ScriptAssertion>,
        /// Typecheck diagnostics captured before execution.
        pub diagnostics: Vec<ScriptDiagnostic>,
        /// Timing information for the request.
        pub timing: ScriptTiming,
        /// Error payload when evaluation fails.
        pub error: Option<ScriptErrorInfo>,
    }

    impl ScriptEvalOutcome {
        /// Encode the outcome as an MCP tool result.
        pub fn to_tool_result(&self) -> CallToolResult {}

        /// Build a failure payload with no result value.
        pub fn error_only(
            error_type: impl Into<String>,
            message: impl Into<String>,
            diagnostics: Vec<ScriptDiagnostic>,
            timing: ScriptTiming,
        ) -> Self {
        }
    }

    impl JsonSchema for ScriptEvalOutcome {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Request payload for the `script_eval` tool.
    #[derive(Deserialize, Debug, Clone, StructuralPartialEq, PartialEq)]
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
    }

    impl JsonSchema for ScriptTaskState {
        fn schema_name() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn schema_id() -> schemars::_private::alloc::borrow::Cow<'static, str> {}

        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {}

        fn inline_schema() -> bool {}
    }

    /// Timing information for a script evaluation.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ScriptTiming {
        /// Time spent constructing and rendering the headless app.
        pub build_ms: u64,
        /// Time spent executing the script and final render.
        pub exec_ms: u64,
        /// Total wall-clock time for the request.
        pub total_ms: u64,
    }

    impl ScriptTiming {
        /// Zeroed timing information for early errors.
        pub fn zero() -> Self {}
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

    /// Serve `script_eval` and `script_api` over stdio for an app factory.
    pub fn serve_stdio(
        factory: impl Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static,
    ) -> crate::Result<()> {
    }

    /// Serve live MCP automation for a running canopy app over a Unix-domain socket.
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
        /// Pass or fail status.
        pub status: ScriptStatus,
        /// Total script duration in milliseconds.
        pub elapsed_ms: u64,
        /// Optional summary message.
        pub message: Option<String>,
        /// Structured script outcome.
        pub outcome: crate::script::ScriptEvalOutcome,
    }

    /// Final status for a smoke script.
    #[derive(Debug, Clone, Copy, StructuralPartialEq, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ScriptStatus {
        /// The script passed.
        Passed,
        /// The script failed.
        Failed,
    }

    /// Configuration for a smoke-suite run.
    #[derive(Debug, Clone, StructuralPartialEq, PartialEq)]
    pub struct SuiteConfig {
        /// Root directory to scan for `.luau` scripts when no explicit script list is provided.
        pub suite_dir: std::path::PathBuf,
        /// Optional subset of scripts to run. Relative paths are resolved against `suite_dir`.
        pub scripts: Vec<std::path::PathBuf>,
        /// Optional timeout per script in milliseconds.
        pub timeout_ms: Option<u64>,
        /// Stop after the first failing script when true.
        pub fail_fast: bool,
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

    /// Run a smoke suite against fresh headless app instances.
    pub fn run_suite(
        factory: impl Fn() -> crate::Result<canopy::Canopy> + Send + Sync + 'static,
        config: &SuiteConfig,
    ) -> crate::Result<SuiteResult> {
    }
}
