#![deny(unsafe_code)]
//! Command-line entry point for the Todo example application.

use std::{path::PathBuf, process};

use anyhow::{Result, bail};
use canopy::terminal::RunOptions;
use canopy_mcp::{AppFactory, AppMetadata, Error as McpError, LaunchMode, ResetPolicy, launch};
use clap::{Parser, Subcommand};
use todo::{create_app, store::Store};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
/// Todo command-line arguments.
struct Args {
    /// Optional headless operation.
    #[command(subcommand)]
    command: Option<Command>,

    /// Print the Luau API definition and exit
    #[clap(long)]
    api: bool,

    /// Path to a Luau config file
    #[clap(short, long, global = true)]
    config: Option<PathBuf>,

    /// Serve live MCP automation over the given Unix-domain socket path.
    #[clap(long)]
    mcp: Option<PathBuf>,

    /// SQLite database path for interactive mode.
    path: Option<String>,
}

#[derive(Subcommand, Debug)]
/// Headless Todo operations.
enum Command {
    /// Serve headless MCP automation over stdio.
    Mcp {
        /// SQLite database path for the todo app.
        path: String,
    },
}

/// Build an application factory for a database and optional config.
fn make_factory(path: String, config: Option<PathBuf>) -> AppFactory {
    let reset = if path == ":memory:" {
        ResetPolicy::Isolated
    } else {
        ResetPolicy::External
    };
    AppFactory::new(
        AppMetadata {
            app: "todo".into(),
            reset,
        },
        move || {
            let store = Store::open(&path).map_err(McpError::app)?;
            create_app(store, config.as_deref()).map_err(McpError::app)
        },
    )
}

fn main() -> Result<()> {
    let args = Args::parse();

    let code = match args.command {
        Some(Command::Mcp { path }) => launch(
            make_factory(path, args.config),
            LaunchMode::HeadlessMcp,
            RunOptions::default(),
        )?,
        None if args.api => launch(
            AppFactory::new(
                AppMetadata {
                    app: "todo".into(),
                    reset: ResetPolicy::Isolated,
                },
                || Ok(todo::api_app()?),
            ),
            LaunchMode::Api,
            RunOptions::default(),
        )?,
        None => {
            let Some(path) = args.path else {
                bail!("specify a SQLite database path, --api, or the mcp subcommand");
            };
            let mode = LaunchMode::Run {
                mcp_socket: args.mcp,
            };
            launch(make_factory(path, args.config), mode, RunOptions::default())?
        }
    };
    if code != 0 {
        process::exit(code);
    }

    Ok(())
}
