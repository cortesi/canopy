use std::path::{Path, PathBuf};

use canopy::terminal::{RunOptions, runloop_with_options};

use crate::{
    AppFactory, Error, LiveContext, Result,
    script::AppEvaluator,
    server::{serve_stdio, serve_uds_with_context},
};

/// Authority granted to local automation transports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutomationPolicy {
    /// Do not start an automation listener.
    #[default]
    Disabled,
    /// Allow local clients to exercise the application's exposed native
    /// actions.
    TrustedLocal,
}

/// Explicit terminal and automation policy for an application launch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Terminal interrupt behavior.
    pub run_options: RunOptions,
    /// Trust granted to a requested automation listener.
    pub automation: AutomationPolicy,
}

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
/// Explicit `HeadlessMcp` and socket modes grant `TrustedLocal` automation.
/// Use `launch_with_options` when the application must deny that authority.
pub fn launch(factory: AppFactory, mode: LaunchMode) -> Result<i32> {
    let options = LaunchOptions {
        automation: if requests_automation(&mode) {
            AutomationPolicy::TrustedLocal
        } else {
            AutomationPolicy::Disabled
        },
        ..LaunchOptions::default()
    };
    launch_with_options(factory, mode, options)
}

/// Launch with explicit policies; disabled automation rejects listener modes
/// before constructing the application or opening any transport.
pub fn launch_with_options(
    factory: AppFactory,
    mode: LaunchMode,
    options: LaunchOptions,
) -> Result<i32> {
    if requests_automation(&mode) && options.automation == AutomationPolicy::Disabled {
        return Err(Error::AutomationDisabled);
    }
    match mode {
        LaunchMode::Run { mcp_socket } => {
            run_interactive(&factory, mcp_socket.as_deref(), options.run_options)
        }
        LaunchMode::HeadlessMcp => {
            serve_stdio(factory)?;
            Ok(0)
        }
        LaunchMode::Api => {
            print!("{}", AppEvaluator::new(factory).script_api()?);
            Ok(0)
        }
    }
}

/// Whether the selected mode requests a local automation transport.
fn requests_automation(mode: &LaunchMode) -> bool {
    matches!(
        mode,
        LaunchMode::HeadlessMcp
            | LaunchMode::Run {
                mcp_socket: Some(_)
            }
    )
}

/// Run the interactive terminal UI, optionally serving live MCP automation.
fn run_interactive(
    factory: &AppFactory,
    mcp_socket: Option<&Path>,
    options: RunOptions,
) -> Result<i32> {
    let canopy = (factory.as_ref())()?;
    let automation = canopy.automation_handle();
    let live_server = mcp_socket
        .map(|socket_path| {
            serve_uds_with_context(
                socket_path,
                automation,
                LiveContext::new(factory.metadata().clone()),
            )
        })
        .transpose()?;

    let run_result = runloop_with_options(canopy, options);
    if let Some(server) = live_server {
        server.stop()?;
    }
    Ok(run_result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_automation_rejects_listener_before_app_setup() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("automation.sock");
        let factory = crate::app_factory(|| panic!("disabled listener must not construct an app"));
        for mode in [
            LaunchMode::HeadlessMcp,
            LaunchMode::Run {
                mcp_socket: Some(socket.clone()),
            },
        ] {
            assert!(matches!(
                launch_with_options(factory.clone(), mode, LaunchOptions::default()),
                Err(Error::AutomationDisabled)
            ));
            assert!(!socket.exists());
        }
        Ok(())
    }

    #[test]
    fn only_explicit_legacy_listener_modes_request_trust() {
        assert!(requests_automation(&LaunchMode::HeadlessMcp));
        assert!(requests_automation(&LaunchMode::Run {
            mcp_socket: Some("local.sock".into())
        }));
        assert!(!requests_automation(&LaunchMode::Api));
        assert!(!requests_automation(&LaunchMode::Run { mcp_socket: None }));
        assert_eq!(
            LaunchOptions::default().automation,
            AutomationPolicy::Disabled
        );
    }
}
