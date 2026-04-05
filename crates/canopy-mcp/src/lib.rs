#![warn(missing_docs)]

//! MCP and smoke-test helpers for canopy applications.

/// Error types shared across the automation helpers.
pub mod error;
/// Headless script-evaluation types and helpers.
pub mod script;
/// Stdio MCP server wrapper for script automation.
pub mod server;
/// Smoke-suite discovery and execution helpers.
pub mod smoke;

pub use error::{Error, Result};
pub use script::{
    AppEvaluator, ScriptAssertion, ScriptDiagnostic, ScriptErrorInfo, ScriptEvalOutcome,
    ScriptEvalRequest, ScriptTiming, app_factory,
};
pub use server::serve_stdio;
pub use smoke::{ScriptResult, ScriptStatus, SuiteConfig, SuiteResult, run_suite};
