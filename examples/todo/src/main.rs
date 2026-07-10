#![deny(unsafe_code)]
//! Command-line entry point for the Todo example application.

use std::{path::PathBuf, process};

use anyhow::Result;
use canopy_mcp::{
    Error as McpError, LaunchMode, SuiteConfig, app_factory, launch, script::AppFactory,
};
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
    /// Run Luau smoke scripts against fresh headless app instances.
    Smoke {
        /// SQLite database path for the todo app.
        path: String,
        /// Suite directory to scan when no explicit scripts are provided.
        #[clap(long, default_value = "examples/todo/smoke")]
        suite: PathBuf,
        /// Stop after the first failing script.
        #[clap(long)]
        fail_fast: bool,
        /// Optional timeout per script in milliseconds.
        #[clap(long)]
        timeout_ms: Option<u64>,
        /// Optional Luau config file applied before each script.
        #[clap(short, long)]
        config: Option<PathBuf>,
        /// Explicit subset of smoke scripts to run.
        scripts: Vec<PathBuf>,
    },
}

/// Build an application factory for a database and optional config.
fn make_factory(path: String, config: Option<PathBuf>) -> AppFactory {
    app_factory(move || {
        create_app_with_config(&path, config.as_deref())
            .map_err(|error| McpError::app_boxed(error.into_boxed_dyn_error()))
    })
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.api {
        let code = launch(
            app_factory(|| {
                let mut cnpy = canopy::Canopy::new();
                todo::setup_app(&mut cnpy).map_err(McpError::app)?;
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
        Some(Command::Smoke {
            path,
            suite,
            fail_fast,
            timeout_ms,
            config,
            scripts,
        }) => launch(
            make_factory(path, config),
            LaunchMode::Smoke(SuiteConfig {
                suite_dir: suite,
                scripts,
                timeout_ms,
                fail_fast,
            }),
        )?,
        None => {
            if let Some(path) = args.path {
                let mode = args
                    .mcp
                    .map_or_else(LaunchMode::run, LaunchMode::run_with_mcp);
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
