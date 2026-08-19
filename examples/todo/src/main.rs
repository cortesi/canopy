#![deny(unsafe_code)]
//! Command-line entry point for the Todo example application.

use std::{path::PathBuf, process};

use anyhow::Result;
use canopy_mcp::{AppFactory, Error as McpError, LaunchMode, app_factory, launch};
use clap::{Parser, Subcommand};
use todo::create_app_with_config;

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
    #[clap(short, long)]
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
        /// Optional Luau config file applied before each request.
        #[clap(short, long)]
        config: Option<PathBuf>,
    },
}

/// Build an application factory for a database and optional config.
fn make_factory(path: String, config: Option<PathBuf>) -> AppFactory {
    app_factory(move || create_app_with_config(&path, config.as_deref()).map_err(McpError::app))
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.api {
        let code = launch(
            app_factory(|| {
                let mut cnpy = canopy::Canopy::new();
                todo::setup_app(&mut cnpy)?;
                Ok(cnpy)
            }),
            LaunchMode::Api,
        )?;
        if code != 0 {
            process::exit(code);
        }
        return Ok(());
    }

    let code = match args.command {
        Some(Command::Mcp { path, config }) => {
            launch(make_factory(path, config), LaunchMode::HeadlessMcp)?
        }
        None => {
            if let Some(path) = args.path {
                let mode = LaunchMode::Run {
                    mcp_socket: args.mcp,
                };
                launch(make_factory(path, args.config), mode)?
            } else {
                println!("Specify a file path");
                0
            }
        }
    };
    if code != 0 {
        process::exit(code);
    }

    Ok(())
}
