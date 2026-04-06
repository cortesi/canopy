//! Developer workflow tasks for the canopy workspace.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

use clap::{Parser, Subcommand};

/// Command line interface for `cargo xtask`.
#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    /// The task to run.
    #[command(subcommand)]
    task: Task,
}

/// Supported xtask commands.
#[derive(Subcommand)]
enum Task {
    /// Run formatting and clippy fixes.
    Tidy,
    /// Run the workspace test suite.
    Test,
    /// Run all smoke-test integration targets.
    Smoke,
}

/// Run the `cargo xtask` entry point.
fn main() -> ExitCode {
    match Cli::parse().task {
        Task::Tidy => run_tidy(),
        Task::Test => run_test(),
        Task::Smoke => run_smoke(),
    }
}

/// Run the workspace tidy workflow.
fn run_tidy() -> ExitCode {
    let workspace_root = workspace_root();

    if !run_fmt(&workspace_root) {
        return ExitCode::FAILURE;
    }

    if !run_clippy(&workspace_root) {
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Run the workspace test workflow.
fn run_test() -> ExitCode {
    run_test_command(&["--all", "--all-features"])
}

/// Run the workspace smoke-test workflow.
fn run_smoke() -> ExitCode {
    let workspace_root = workspace_root();
    let suites = match discover_smoke_suites(&workspace_root) {
        Ok(suites) => suites,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    if suites.is_empty() {
        eprintln!("No smoke suites found under {}", workspace_root.display());
        return ExitCode::FAILURE;
    }

    for suite in suites {
        let label = suite
            .strip_prefix(&workspace_root)
            .unwrap_or(&suite)
            .display()
            .to_string();
        println!("Suite {label}");
        if !run_cargo_command(
            &suite,
            &["run", "--quiet", "-p", "canopyctl", "--", "smoke"],
        ) {
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

/// Return the workspace root for the xtask crate.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate should live under the workspace root")
        .to_path_buf()
}

/// Run cargo fmt for the workspace.
fn run_fmt(workspace_root: &Path) -> bool {
    if workspace_root.join("rustfmt-nightly.toml").exists() {
        run_cargo_command(
            workspace_root,
            &[
                "+nightly",
                "fmt",
                "--all",
                "--",
                "--config-path",
                "./rustfmt-nightly.toml",
            ],
        )
    } else {
        run_cargo_command(workspace_root, &["+nightly", "fmt", "--all"])
    }
}

/// Run clippy with workspace fixes enabled.
fn run_clippy(workspace_root: &Path) -> bool {
    run_cargo_command(
        workspace_root,
        &[
            "clippy",
            "-q",
            "--fix",
            "--all",
            "--all-targets",
            "--all-features",
            "--allow-dirty",
            "--tests",
            "--examples",
        ],
    )
}

/// Return true when cargo-nextest is installed.
fn nextest_available(workspace_root: &Path) -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .current_dir(workspace_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Discover directories that define smoke suites via `.canopyctl.toml`.
fn discover_smoke_suites(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut suites = Vec::new();
    collect_smoke_suites(workspace_root, &mut suites).map_err(|error| error.to_string())?;
    suites.sort();
    Ok(suites)
}

/// Recursively collect smoke-suite directories under the workspace root.
fn collect_smoke_suites(dir: &Path, suites: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if matches!(
                entry.file_name().to_str(),
                Some(".git" | ".cargo" | "target" | "tmp")
            ) {
                continue;
            }
            collect_smoke_suites(&path, suites)?;
            continue;
        }

        if file_type.is_file()
            && entry.file_name() == ".canopyctl.toml"
            && let Some(parent) = path.parent()
        {
            suites.push(parent.to_path_buf());
        }
    }
    Ok(())
}

/// Run a test command, preferring nextest when available.
fn run_test_command(test_args: &[&str]) -> ExitCode {
    let workspace_root = workspace_root();

    if nextest_available(&workspace_root) {
        let mut args = vec!["nextest", "run"];
        args.extend_from_slice(test_args);
        return exit_code(run_cargo_command(&workspace_root, &args));
    }

    let mut args = vec!["test"];
    args.extend_from_slice(test_args);
    exit_code(run_cargo_command(&workspace_root, &args))
}

/// Run a cargo command from the workspace root.
fn run_cargo_command(workspace_root: &Path, args: &[&str]) -> bool {
    match Command::new("cargo")
        .args(args)
        .current_dir(workspace_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!(
                "Command `cargo {}` failed with status {status}",
                args.join(" ")
            );
            false
        }
        Err(error) => {
            eprintln!("Failed to run `cargo {}`: {error}", args.join(" "));
            false
        }
    }
}

/// Convert a command result into an exit code.
fn exit_code(success: bool) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
