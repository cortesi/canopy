use std::{
    env,
    io::Write,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::ExitCode,
};

use canopy::terminal::{RunOptions, runloop};

use crate::{
    AppFactory, Result,
    server::{serve_stdio, serve_uds},
};

/// Environment variable naming a Unix-domain socket that a host listens on.
/// Once the live automation socket accepts connections, the launcher
/// connects to it and writes one line, so the host waits on that signal
/// rather than retrying the automation socket.
pub const READY_SOCKET_ENV: &str = "CANOPY_READY_SOCKET";

/// Launcher mode for a Canopy application.
pub enum LaunchMode {
    /// Run the interactive terminal UI.
    Run {
        /// Optional live MCP Unix-domain socket path.
        mcp_socket: Option<PathBuf>,
        /// Terminal adapter policy for the run loop.
        options: RunOptions,
    },
    /// Serve the headless MCP automation server over stdio.
    HeadlessMcp,
}

/// Launch a Canopy app in the selected mode, and return the process exit
/// code.
///
/// The caller owns CLI parsing and app-specific configuration, including
/// printing the Luau API. This function owns the repeated framework wiring:
/// headless MCP, live MCP, and the terminal runloop.
pub fn launch(factory: AppFactory, mode: LaunchMode) -> Result<ExitCode> {
    let code = match mode {
        LaunchMode::Run {
            mcp_socket,
            options,
        } => run_interactive(&factory, mcp_socket.as_deref(), options)?,
        LaunchMode::HeadlessMcp => {
            serve_stdio(factory)?;
            0
        }
    };
    // Status codes outside a byte report plain failure.
    Ok(u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from))
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
    if live_server.is_some() {
        announce_ready();
    }

    let run_result = runloop(canopy, options);
    if let Some(server) = live_server {
        server.stop()?;
    }
    Ok(run_result?)
}

/// Tell a host that waits on [`READY_SOCKET_ENV`] that the live automation
/// socket accepts connections. A host that stopped waiting is no failure of
/// the app, so a failed connection or write is ignored.
fn announce_ready() {
    let Some(path) = env::var_os(READY_SOCKET_ENV) else {
        return;
    };
    if let Ok(mut stream) = UnixStream::connect(path) {
        drop(stream.write_all(b"ready\n"));
    }
}
