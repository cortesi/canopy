#![deny(unsafe_code)]
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
    /// Build the workspace and isolated widget capability profiles.
    FeatureCheck,
    /// Check API skeletons and tracked Luau sources.
    Checks,
    /// Compile every benchmark target without running benchmarks.
    BenchCheck,
    /// Run all smoke-test integration targets.
    Smoke,
}

/// Run the `cargo xtask` entry point.
fn main() -> ExitCode {
    let root = workspace_root();
    exit_code(match Cli::parse().task {
        Task::FeatureCheck => run_default_check(&root),
        Task::Checks => run_luau_check(&root),
        Task::BenchCheck => run_bench_check(&root),
        Task::Smoke => run_smoke(&root),
    })
}

/// Run the workspace smoke-test workflow.
fn run_smoke(workspace_root: &Path) -> bool {
    let suites = match discover_smoke_suites(workspace_root) {
        Ok(suites) => suites,
        Err(error) => {
            eprintln!("{error}");
            return false;
        }
    };

    if suites.is_empty() {
        eprintln!("No smoke suites found under {}", workspace_root.display());
        return false;
    }

    for suite in suites {
        let label = suite
            .strip_prefix(workspace_root)
            .unwrap_or(&suite)
            .display()
            .to_string();
        println!("Suite {label}");
        if !run_cargo_command(
            &suite,
            &["run", "--quiet", "-p", "canopyctl", "--", "smoke"],
        ) {
            return false;
        }
    }

    true
}

/// Return the workspace root for the xtask crate.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate should live under the workspace root")
        .to_path_buf()
}

/// Build the production profile, the full workspace, and isolated minimum and
/// independent widget profiles.
fn run_default_check(workspace_root: &Path) -> bool {
    // The production profile omits `--all-targets` and `--all-features` on
    // purpose. Either flag pulls in dev-dependencies, which re-enable the
    // `testing` feature and hide the warnings this step exists to catch.
    if !run_cargo_command(workspace_root, &["clippy", "--workspace", "--", "-D", "warnings"]) {
        return false;
    }
    if !run_cargo_command(workspace_root, &["check", "--workspace", "--all-targets"]) {
        return false;
    }
    for capability in [
        None,
        Some("editor"),
        Some("terminal-widget"),
        Some("graphics"),
        Some("devtools"),
    ] {
        let mut args = vec![
            "check",
            "-p",
            "canopy-widgets",
            "--no-default-features",
            "--all-targets",
        ];
        if let Some(capability) = capability {
            args.extend(["--features", capability]);
        }
        if !run_cargo_command(workspace_root, &args) {
            return false;
        }
    }
    true
}

/// Type-check every tracked Luau source under its owning application surface.
fn run_luau_check(workspace_root: &Path) -> bool {
    if let Err(error) = validate_luau_inventory(workspace_root) {
        eprintln!("{error}");
        return false;
    }
    run_cargo_command(
        workspace_root,
        &[
            "nextest",
            "run",
            "--workspace",
            "--all-features",
            "-E",
            "test(tracked_luau)",
        ],
    )
}

/// Reject tracked Luau files outside a directory with an explicit checker
/// owner.
fn validate_luau_inventory(workspace_root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["ls-files", "--", "*.luau"])
        .current_dir(workspace_root)
        .output()
        .map_err(|error| format!("listing tracked Luau files failed: {error}"))?;
    if !output.status.success() {
        return Err("listing tracked Luau files failed".to_string());
    }
    let files = String::from_utf8(output.stdout)
        .map_err(|error| format!("tracked Luau path is not UTF-8: {error}"))?;
    for file in files.lines() {
        let owned = file == "crates/canopy/luau/preamble.d.luau"
            || file.starts_with("crates/canopy-widgets/tests/luau/")
            || file.starts_with("examples/todo/smoke/");
        if !owned {
            return Err(format!("tracked Luau file has no checker owner: {file}"));
        }
    }
    Ok(())
}

/// Compile every benchmark target without running benchmarks.
fn run_bench_check(workspace_root: &Path) -> bool {
    run_cargo_command(
        workspace_root,
        &[
            "test",
            "--workspace",
            "--benches",
            "--no-run",
            "--all-features",
        ],
    )
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
