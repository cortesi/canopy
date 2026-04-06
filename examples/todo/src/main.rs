use std::{path::PathBuf, process};

use anyhow::Result;
use canopy::backend::crossterm::{RunloopOptions, runloop_with_options};
use canopy_mcp::{Error as McpError, SuiteConfig, app_factory, run_suite, serve_stdio, serve_uds};
use clap::{Parser, Subcommand};
use todo::create_app_with_config;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
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

    path: Option<String>,
}

#[derive(Subcommand, Debug)]
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

fn make_factory(path: String, config: Option<PathBuf>) -> canopy_mcp::script::AppFactory {
    app_factory(move || create_app_with_config(&path, config.as_deref()).map_err(McpError::app))
}

pub fn main() -> Result<()> {
    let args = Args::parse();

    if args.api {
        let mut cnpy = canopy::Canopy::new();
        todo::setup_app(&mut cnpy)?;
        print!("{}", cnpy.script_api());
        return Ok(());
    }

    match args.command {
        Some(Command::Mcp { path, config }) => {
            serve_stdio({
                let factory = make_factory(path, config);
                move || (factory.as_ref())()
            })?;
            return Ok(());
        }
        Some(Command::Smoke {
            path,
            suite,
            fail_fast,
            timeout_ms,
            config,
            scripts,
        }) => {
            let factory = make_factory(path, config);
            let result = run_suite(
                move || (factory.as_ref())(),
                &SuiteConfig {
                    suite_dir: suite,
                    scripts,
                    timeout_ms,
                    fail_fast,
                },
            )?;
            for script in &result.scripts {
                let status = match script.status {
                    canopy_mcp::ScriptStatus::Passed => "PASS",
                    canopy_mcp::ScriptStatus::Failed => "FAIL",
                };
                println!("{status} {}", script.path.display());
                if let Some(message) = &script.message {
                    println!("  {message}");
                }
            }
            if !result.success() {
                process::exit(1);
            }
            return Ok(());
        }
        None => {}
    }

    if let Some(path) = args.path {
        let cnpy = create_app_with_config(&path, args.config.as_deref())?;
        let automation = cnpy.automation_handle();
        let live_server = args
            .mcp
            .as_ref()
            .map(|socket_path| serve_uds(socket_path, automation))
            .transpose()?;

        let run_result = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump());
        if let Some(server) = live_server {
            server.stop()?;
        }
        let exit_code = run_result?;
        if exit_code != 0 {
            process::exit(exit_code);
        }
    } else {
        println!("Specify a file path");
    }

    Ok(())
}
