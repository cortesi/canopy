#![deny(unsafe_code)]
//! Command-line entry point for the Hello example application.

use std::{env, path::PathBuf, process};

use anyhow::{Context, Result, bail};
use canopy::terminal::RunOptions;
use canopy_mcp::{AppFactory, AppMetadata, Error as McpError, LaunchMode, ResetPolicy, launch};
use clap::{Parser, Subcommand};

/// Minimal Canopy application.
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Headless operation.
    #[command(subcommand)]
    command: Option<Command>,

    /// Print the generated Luau API and exit.
    #[arg(long)]
    api: bool,

    /// Serve live MCP automation over this Unix-domain socket.
    #[arg(long)]
    mcp: Option<PathBuf>,

    /// Do not read or create the user Luau configuration.
    #[arg(long, global = true)]
    no_config: bool,

    /// Directory containing `init.luau` and `bindings.luau`.
    #[arg(long, global = true)]
    config_home: Option<PathBuf>,
}

/// Headless command modes.
#[derive(Debug, Subcommand)]
enum Command {
    /// Serve headless MCP automation over stdio.
    Mcp,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.no_config && args.config_home.is_some() {
        bail!("--no-config conflicts with --config-home");
    }

    // Headless and API modes must never read or create user state. `--api`
    // opts out here; `.canopyctl.toml` passes `--no-config` to the headless
    // command so automation runs stay hermetic.
    let config_root = if args.no_config || args.api {
        None
    } else {
        let root = args.config_home.map_or_else(default_config_root, Ok)?;
        hello::ensure_user_config(&root)
            .with_context(|| format!("initialize user config at {}", root.display()))?;
        Some(root)
    };

    let mode = match args.command {
        Some(Command::Mcp) => LaunchMode::HeadlessMcp,
        None if args.api => LaunchMode::Api,
        None => LaunchMode::Run {
            mcp_socket: args.mcp,
        },
    };

    let factory = AppFactory::new(
        AppMetadata {
            app: "hello".into(),
            reset: ResetPolicy::Isolated,
        },
        move || hello::create_app(config_root.clone()).map_err(McpError::app),
    );
    let code = launch(factory, mode, RunOptions::default())?;
    if code != 0 {
        process::exit(code);
    }
    Ok(())
}

/// Resolve the default persistent script root.
fn default_config_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("HELLO_CONFIG_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is not set; use --config-home")?;
    Ok(PathBuf::from(home).join(".hello"))
}
