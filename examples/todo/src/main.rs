#![deny(unsafe_code)]
//! Command-line entry point for the Todo example application.

use std::{path::PathBuf, process::ExitCode};

use anyhow::{Result, bail};
use canopy::terminal::RunOptions;
use canopy_mcp::{ConfigHome, LaunchMode, launch};
use clap::{Parser, Subcommand};

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

    /// Do not read the user Luau configuration.
    #[clap(long)]
    no_config: bool,

    /// Directory holding `init.luau`, in place of `TODO_CONFIG_HOME` or
    /// `~/.todo`.
    #[clap(long)]
    config_home: Option<PathBuf>,

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

fn main() -> Result<ExitCode> {
    let args = Args::parse();

    if args.api {
        print!("{}", todo::api_app()?.script_api()?);
        return Ok(ExitCode::SUCCESS);
    }
    let (path, mode) = match args.command {
        Some(Command::Mcp { path }) => (path, LaunchMode::HeadlessMcp),
        None => {
            let Some(path) = args.path else {
                bail!("specify a SQLite database path, --api, or the mcp subcommand");
            };
            let mode = LaunchMode::Run {
                mcp_socket: args.mcp,
                options: RunOptions::default(),
            };
            (path, mode)
        }
    };
    let home = ConfigHome::resolve("todo", args.config_home, args.no_config, &mode)?;
    let code = launch(todo::factory(path, home), mode)?;
    Ok(code)
}
