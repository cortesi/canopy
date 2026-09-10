use std::path::{Path, PathBuf};

use canopy::terminal::{RunOptions, runloop_with_options};

use crate::{
    AppFactory, Result,
    server::{serve_stdio, serve_uds},
};

/// Launcher mode for a Canopy application.
pub enum LaunchMode {
    /// Run the interactive terminal UI.
    Run {
        /// Optional live MCP Unix-domain socket path.
        mcp_socket: Option<PathBuf>,
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
pub fn launch(factory: AppFactory, mode: LaunchMode, run_options: RunOptions) -> Result<i32> {
    match mode {
        LaunchMode::Run { mcp_socket } => {
            run_interactive(&factory, mcp_socket.as_deref(), run_options)
        }
        LaunchMode::HeadlessMcp => {
            serve_stdio(factory)?;
            Ok(0)
        }
        LaunchMode::Api => {
            print!("{}", factory.script_api()?);
            Ok(0)
        }
    }
}

/// Run the interactive terminal UI, optionally serving live MCP automation.
fn run_interactive(
    factory: &AppFactory,
    mcp_socket: Option<&Path>,
    options: RunOptions,
) -> Result<i32> {
    let canopy = factory.build()?;
    let automation = canopy.automation_handle();
    let live_server = mcp_socket
        .map(|socket_path| serve_uds(socket_path, automation, factory.metadata().clone()))
        .transpose()?;

    let run_result = runloop_with_options(canopy, options);
    if let Some(server) = live_server {
        server.stop()?;
    }
    Ok(run_result?)
}
