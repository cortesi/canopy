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
    /// Run the complete non-mutating repository gate.
    Ci,
    /// Run the workspace test suite.
    Test,
    /// Type-check every tracked Luau source against its owning app surface.
    Luau,
    /// Run all smoke-test integration targets.
    Smoke,
}

/// Run the `cargo xtask` entry point.
fn main() -> ExitCode {
    match Cli::parse().task {
        Task::Tidy => run_tidy(),
        Task::Ci => run_ci(),
        Task::Test => run_test(),
        Task::Luau => exit_code(run_luau_check(&workspace_root())),
        Task::Smoke => run_smoke(),
    }
}

/// Rust nightly used only for deterministic formatting.
const FORMAT_TOOLCHAIN: &str = "+nightly-2026-07-01";

/// Cargo-nextest version required locally and in CI.
const NEXTEST_VERSION: &str = "0.9.99";

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

/// Run every required repository check without modifying source files.
fn run_ci() -> ExitCode {
    let workspace_root = workspace_root();
    let checks: &[fn(&Path) -> bool] = &[
        run_fmt_check,
        run_clippy_check,
        run_default_check,
        run_all_features_check,
        run_doctests,
        run_luau_check,
        run_nextest,
        run_bench_check,
    ];

    for check in checks {
        if !check(&workspace_root) {
            return ExitCode::FAILURE;
        }
    }
    if run_smoke() == ExitCode::FAILURE {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Run the workspace test workflow.
fn run_test() -> ExitCode {
    let workspace_root = workspace_root();
    if !run_nextest(&workspace_root) || !run_doctests(&workspace_root) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
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
                FORMAT_TOOLCHAIN,
                "fmt",
                "--all",
                "--",
                "--config-path",
                "./rustfmt-nightly.toml",
            ],
        )
    } else {
        run_cargo_command(workspace_root, &[FORMAT_TOOLCHAIN, "fmt", "--all"])
    }
}

/// Verify workspace formatting without modifying files.
fn run_fmt_check(workspace_root: &Path) -> bool {
    if workspace_root.join("rustfmt-nightly.toml").exists() {
        run_cargo_command(
            workspace_root,
            &[
                FORMAT_TOOLCHAIN,
                "fmt",
                "--all",
                "--",
                "--config-path",
                "./rustfmt-nightly.toml",
                "--check",
            ],
        )
    } else {
        run_cargo_command(
            workspace_root,
            &[FORMAT_TOOLCHAIN, "fmt", "--all", "--", "--check"],
        )
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

/// Run Clippy without edits and deny every warning.
fn run_clippy_check(workspace_root: &Path) -> bool {
    run_cargo_command(
        workspace_root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )
}

/// Build all workspace targets with default features.
fn run_default_check(workspace_root: &Path) -> bool {
    run_cargo_command(workspace_root, &["check", "--workspace", "--all-targets"])
}

/// Build all workspace targets with every feature.
fn run_all_features_check(workspace_root: &Path) -> bool {
    run_cargo_command(
        workspace_root,
        &["check", "--workspace", "--all-targets", "--all-features"],
    )
}

/// Compile and run documentation examples separately from nextest.
fn run_doctests(workspace_root: &Path) -> bool {
    run_cargo_command(
        workspace_root,
        &["test", "--doc", "--workspace", "--all-features"],
    )
}

/// Type-check every tracked Luau source under its owning application surface.
fn run_luau_check(workspace_root: &Path) -> bool {
    if let Err(error) = validate_luau_inventory(workspace_root) {
        eprintln!("{error}");
        return false;
    }
    if installed_nextest_version(workspace_root).as_deref() != Some(NEXTEST_VERSION) {
        eprintln!("cargo-nextest {NEXTEST_VERSION} is required for the Luau gate");
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

/// Reject tracked Luau files outside a directory with an explicit checker owner.
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

/// Return the installed cargo-nextest version.
fn installed_nextest_version(workspace_root: &Path) -> Option<String> {
    let output = Command::new("cargo")
        .args(["nextest", "--version"])
        .current_dir(workspace_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.split_whitespace().nth(1).map(str::to_string)
}

/// Run the pinned nextest suite or report the exact installation requirement.
fn run_nextest(workspace_root: &Path) -> bool {
    match installed_nextest_version(workspace_root) {
        Some(version) if version == NEXTEST_VERSION => run_cargo_command(
            workspace_root,
            &["nextest", "run", "--workspace", "--all-features"],
        ),
        Some(version) => {
            eprintln!("cargo-nextest {NEXTEST_VERSION} is required, but {version} is installed");
            false
        }
        None => {
            eprintln!(
                "cargo-nextest {NEXTEST_VERSION} is required; install it before running this gate"
            );
            false
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_nextest_matches_repository_pin() {
        assert_eq!(
            installed_nextest_version(&workspace_root()).as_deref(),
            Some(NEXTEST_VERSION)
        );
    }
}
