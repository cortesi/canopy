#![deny(unsafe_code)]
//! Command-line entry point for the Hello example application.

use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use canopy::terminal::RunOptions;
use canopy_mcp::{
    AppFactory, AppMetadata, ConfigHome, Error as McpError, LaunchMode, ResetPolicy, launch,
};
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
    #[arg(long)]
    no_config: bool,

    /// Directory holding `init.luau`, in place of `HELLO_CONFIG_HOME` or
    /// `~/.hello`.
    #[arg(long)]
    config_home: Option<PathBuf>,
}

/// Headless command modes.
#[derive(Debug, Subcommand)]
enum Command {
    /// Serve headless MCP automation over stdio.
    Mcp,
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();

    // The API is a property of the application itself, so it is rendered
    // without user state.
    if args.api {
        print!("{}", hello::create_app(None)?.script_api()?);
        return Ok(ExitCode::SUCCESS);
    }

    let mode = match args.command {
        Some(Command::Mcp) => LaunchMode::HeadlessMcp,
        None => LaunchMode::Run {
            mcp_socket: args.mcp,
            options: RunOptions::default(),
        },
    };

    // Headless mode resolves no home, so automation runs stay hermetic. An
    // interactive first run writes the defaults for the user to edit.
    let home = ConfigHome::resolve("hello", args.config_home, args.no_config, &mode)?;
    if let Some(home) = &home {
        home.write_defaults(hello::DEFAULT_CONFIG)
            .with_context(|| format!("write default config to {}", home.path().display()))?;
    }

    let factory = AppFactory::new(
        AppMetadata {
            app: "hello".into(),
            reset: ResetPolicy::Isolated,
        },
        move || hello::create_app(home.as_ref()).map_err(McpError::app),
    );
    Ok(launch(factory, mode)?)
}
