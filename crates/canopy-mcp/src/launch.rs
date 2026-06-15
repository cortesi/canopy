use std::path::{Path, PathBuf};

use canopy::backend::crossterm::{RunloopOptions, runloop_with_options};

use crate::{
    Result, ScriptStatus, SuiteConfig, run_suite,
    script::AppFactory,
    server::{serve_stdio, serve_uds},
};

/// Launcher mode for a Canopy application.
pub enum LaunchMode {
    /// Run the interactive terminal UI.
    Run {
        /// Optional live MCP Unix-domain socket path.
        mcp_socket: Option<PathBuf>,
        /// Crossterm runloop options.
        runloop: RunloopOptions,
    },
    /// Serve the headless MCP automation server over stdio.
    HeadlessMcp,
    /// Run a Luau smoke suite against fresh headless app instances.
    Smoke(SuiteConfig),
    /// Print the generated Luau API and exit.
    Api,
}

impl LaunchMode {
    /// Run the interactive terminal UI with the default Ctrl+C diagnostics.
    pub fn run() -> Self {
        Self::Run {
            mcp_socket: None,
            runloop: RunloopOptions::ctrlc_dump(),
        }
    }

    /// Run the interactive terminal UI with a live MCP socket.
    pub fn run_with_mcp(socket_path: PathBuf) -> Self {
        Self::Run {
            mcp_socket: Some(socket_path),
            runloop: RunloopOptions::ctrlc_dump(),
        }
    }
}

/// Launch a Canopy app in the selected mode.
///
/// The caller owns CLI parsing and app-specific configuration. This function
/// owns the repeated framework wiring: API output, headless MCP, smoke suites,
/// live MCP, and the terminal runloop.
pub fn launch(factory: AppFactory, mode: LaunchMode) -> Result<i32> {
    match mode {
        LaunchMode::Run {
            mcp_socket,
            runloop,
        } => run_interactive(&factory, mcp_socket.as_deref(), runloop),
        LaunchMode::HeadlessMcp => {
            serve_stdio(move || (factory.as_ref())())?;
            Ok(0)
        }
        LaunchMode::Smoke(config) => run_smoke(&factory, &config),
        LaunchMode::Api => {
            let canopy = (factory.as_ref())()?;
            print!("{}", canopy.script_api());
            Ok(0)
        }
    }
}

/// Run the interactive terminal UI, optionally serving live MCP automation.
fn run_interactive(
    factory: &AppFactory,
    mcp_socket: Option<&Path>,
    runloop: RunloopOptions,
) -> Result<i32> {
    let canopy = (factory.as_ref())()?;
    let automation = canopy.automation_handle();
    let live_server = mcp_socket
        .map(|socket_path| serve_uds(socket_path, automation))
        .transpose()?;

    let run_result = runloop_with_options(canopy, runloop);
    if let Some(server) = live_server {
        server.stop()?;
    }
    Ok(run_result?)
}

/// Run a smoke suite and print the standard concise report.
fn run_smoke(factory: &AppFactory, config: &SuiteConfig) -> Result<i32> {
    let factory = factory.clone();
    let result = run_suite(move || (factory.as_ref())(), config)?;
    for script in &result.scripts {
        let status = match script.status {
            ScriptStatus::Passed => "PASS",
            ScriptStatus::Failed => "FAIL",
        };
        println!("{status} {}", script.path.display());
        if let Some(message) = &script.message {
            println!("  {message}");
        }
    }
    Ok(if result.success() { 0 } else { 1 })
}
