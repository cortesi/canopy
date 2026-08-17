use std::path::{Path, PathBuf};

use canopy::terminal::runloop;

use crate::{
    Result,
    script::AppFactory,
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

impl LaunchMode {
    /// Run the interactive terminal UI.
    pub fn run() -> Self {
        Self::Run { mcp_socket: None }
    }

    /// Run the interactive terminal UI with a live MCP socket.
    pub fn run_with_mcp(socket_path: PathBuf) -> Self {
        Self::Run {
            mcp_socket: Some(socket_path),
        }
    }
}

/// Launch a Canopy app in the selected mode.
///
/// The caller owns CLI parsing and app-specific configuration. This function
/// owns the repeated framework wiring: API output, headless MCP, live MCP, and
/// the terminal runloop.
pub fn launch(factory: AppFactory, mode: LaunchMode) -> Result<i32> {
    match mode {
        LaunchMode::Run { mcp_socket } => run_interactive(&factory, mcp_socket.as_deref()),
        LaunchMode::HeadlessMcp => {
            serve_stdio(move || (factory.as_ref())())?;
            Ok(0)
        }
        LaunchMode::Api => {
            let canopy = (factory.as_ref())()?;
            print!("{}", canopy.script_api()?);
            Ok(0)
        }
    }
}

/// Run the interactive terminal UI, optionally serving live MCP automation.
fn run_interactive(factory: &AppFactory, mcp_socket: Option<&Path>) -> Result<i32> {
    let canopy = (factory.as_ref())()?;
    let automation = canopy.automation_handle();
    let live_server = mcp_socket
        .map(|socket_path| serve_uds(socket_path, automation))
        .transpose()?;

    let run_result = runloop(canopy);
    if let Some(server) = live_server {
        server.stop()?;
    }
    Ok(run_result?)
}
