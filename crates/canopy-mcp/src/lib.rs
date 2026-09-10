#![deny(unsafe_code)]
#![expect(
    clippy::multiple_inherent_impl,
    reason = "AppFactory construction and evaluation live in their owning modules."
)]

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
pub use launch::{LaunchMode, launch};
pub use metadata::{
    AppFactory, AppMetadata, ExecutionMetadata, ExecutionMode, ResetPolicy, Viewport,
};
pub use script::{
    BootstrapCommand, BootstrapJournalEntry, BootstrapRequest, BootstrapResponse, ScriptErrorInfo,
    ScriptErrorType, ScriptEvalOutcome, ScriptEvalRequest, ScriptTaskState, ScriptTiming,
};
pub use server::{
    ApplyFixtureRequest, ApplyFixtureResponse, UdsServerHandle, serve_stdio, serve_uds,
};
pub use smoke::{ScriptOutcome, SuiteConfig, SuiteOutcome, SuiteScript, plan_suite, run_suite};
