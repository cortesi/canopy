#![deny(unsafe_code)]

//! MCP and smoke-test helpers for canopy applications.

/// Error types shared across the automation helpers.
mod error;
/// Shared executable launch harness for app binaries.
mod launch;
/// Headless script-evaluation types and helpers.
mod script;
/// Stdio MCP server wrapper for script automation.
mod server;
/// Smoke-suite discovery and execution helpers.
mod smoke;

pub use error::{Error, Result};
pub use launch::{LaunchMode, launch};
pub use script::{
    AppEvaluator, AppFactory, BootstrapCommand, BootstrapJournalEntry, BootstrapResponse,
    ScriptErrorInfo, ScriptEvalOutcome, ScriptEvalRequest, ScriptTaskState, ScriptTiming,
    app_factory, evaluate_live,
};
pub use server::{ApplyFixtureRequest, UdsServerHandle, json_tool_result, serve_stdio, serve_uds};
pub use smoke::{
    ScriptResult, SuiteConfig, SuiteResult, collect_luau_scripts, discover_scripts,
    fixture_for_script, run_suite,
};
