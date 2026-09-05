#![deny(unsafe_code)]

//! MCP and smoke-test helpers for canopy applications.

/// Error types shared across the automation helpers.
mod error;
/// Shared executable launch harness for app binaries.
mod launch;
/// Application and execution metadata shared by MCP and replay clients.
mod metadata;
/// Headless script-evaluation types and helpers.
mod script;
/// Stdio MCP server wrapper for script automation.
mod server;
/// Smoke-suite discovery and execution helpers.
mod smoke;

pub use error::{Error, Result};
pub use launch::{AutomationPolicy, LaunchMode, LaunchOptions, launch, launch_with_options};
pub use metadata::{
    AppFactory, AppMetadata, ExecutionMetadata, ExecutionMode, LiveContext, ResetPolicy, Viewport,
    app_factory,
};
pub use script::{
    AppEvaluator, BootstrapCommand, BootstrapJournalEntry, BootstrapRequest, BootstrapResponse,
    ScriptErrorInfo, ScriptEvalOutcome, ScriptEvalRequest, ScriptTaskState, ScriptTiming,
    evaluate_live,
};
pub use server::{
    ApplyFixtureRequest, UdsServerHandle, json_tool_result, serve_stdio, serve_uds,
    serve_uds_with_context,
};
pub use smoke::{
    ScriptResult, SuiteConfig, SuiteResult, collect_luau_scripts, discover_scripts,
    fixture_for_script, run_suite,
};
