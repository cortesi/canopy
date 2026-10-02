#![deny(unsafe_code)]
//! Developer workflow tasks for the canopy workspace.

mod big_text;
mod cargo_env;
mod luau_grammar;

use std::{
    fs,
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
    /// Build the production profile and isolated widget capability profiles.
    FeatureCheck,
    /// Compile every benchmark target without running benchmarks.
    BenchCheck,
    /// Run all smoke-test integration targets.
    Smoke,
    /// Capture the screenshots of the README: each gallery script of the
    /// gyms against its own gym, into `docs/gallery`.
    Gallery,
    /// Convert the upstream Luau grammar into the one canopy-widgets bundles,
    /// and check it against the upstream baselines.
    LuauGrammar {
        /// A checkout of JohnnyMorganz/Luau.tmLanguage.
        checkout: PathBuf,
    },
    /// Convert the bitmap faces of `BigText` from `assets/fonts` into the
    /// module that canopy-widgets compiles.
    BigText {
        /// Fail when the module differs from what the sources give, and
        /// write nothing.
        #[arg(long)]
        check: bool,
    },
}

/// Run the `cargo xtask` entry point.
fn main() -> ExitCode {
    let root = workspace_root();
    exit_code(match Cli::parse().task {
        Task::FeatureCheck => run_feature_check(&root),
        Task::BenchCheck => run_bench_check(&root),
        Task::Smoke => run_smoke(&root),
        Task::Gallery => run_gallery(&root),
        Task::LuauGrammar { checkout } => luau_grammar::run(&root, &checkout),
        Task::BigText { check } => big_text::run(&root, check),
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

/// The gallery of the README: each gallery script of `examples/gyms/gallery`,
/// and the arguments of the gym that it runs against. Paths are relative to
/// `examples/gyms`, where the gyms run.
const GALLERY: [(&str, &[&str]); 7] = [
    ("stylegym.luau", &["stylegym"]),
    ("chartgym.luau", &["chartgym"]),
    ("biggym.luau", &["biggym"]),
    (
        "cedit.luau",
        &["cedit", "../../crates/canopy-widgets/src/spinner.rs"],
    ),
    ("editorgym.luau", &["editorgym"]),
    ("fontgym.luau", &["fontgym"]),
    ("imgview.luau", &["imgview", "../../.assets/shyness.jpg"]),
];

/// Capture the README gallery afresh: every script of [`GALLERY`] runs
/// against its gym, and each run adds its pages to `docs/gallery`.
fn run_gallery(workspace_root: &Path) -> bool {
    let gyms = workspace_root.join("examples/gyms");
    let out = workspace_root.join("docs/gallery");
    if out.exists()
        && let Err(error) = fs::remove_dir_all(&out)
    {
        eprintln!("Failed to remove {}: {error}", out.display());
        return false;
    }
    // Canopyctl starts each gym with `cargo run`, and its first request times
    // out while Cargo compiles, so the build comes first.
    if !run_cargo_command(
        workspace_root,
        &["build", "--quiet", "-p", "gyms", "-p", "canopyctl"],
    ) {
        return false;
    }
    let mut ok = true;
    for (script, gym) in GALLERY {
        let mut args = vec![
            "run",
            "--quiet",
            "-p",
            "canopyctl",
            "--",
            "gallery",
            script,
            "--",
            "cargo",
            "run",
            "--quiet",
            "-p",
            "gyms",
            "--",
        ];
        args.extend_from_slice(gym);
        args.push("--headless");
        ok &= run_cargo_command(&gyms, &args);
    }
    ok
}

/// Return the workspace root for the xtask crate.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate should live under the workspace root")
        .to_path_buf()
}

/// Build the production profile, then isolated minimum and independent
/// widget capability profiles.
fn run_feature_check(workspace_root: &Path) -> bool {
    // The production profile omits `--all-targets` and `--all-features` on
    // purpose. Either flag pulls in dev-dependencies, which re-enable the
    // `testing` feature and hide the warnings this step exists to catch.
    // A plain `cargo check --workspace --all-targets` step is deliberately
    // absent here: ncode's own build already covers it.
    if !run_cargo_command(
        workspace_root,
        &["clippy", "--workspace", "--", "-D", "warnings"],
    ) {
        return false;
    }
    for capability in [
        None,
        Some("syntax"),
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

/// Discover smoke-suite directories from tracked `.canopyctl.toml` files.
fn discover_smoke_suites(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args(["ls-files", "--", "*.canopyctl.toml"])
        .current_dir(workspace_root)
        .output()
        .map_err(|error| format!("listing tracked smoke suites failed: {error}"))?;
    if !output.status.success() {
        return Err("listing tracked smoke suites failed".to_string());
    }
    let files = String::from_utf8(output.stdout)
        .map_err(|error| format!("tracked smoke suite path is not UTF-8: {error}"))?;
    let mut suites = files
        .lines()
        .filter_map(|file| Path::new(file).parent())
        .map(|suite| workspace_root.join(suite))
        .collect::<Vec<_>>();
    suites.sort();
    Ok(suites)
}

/// Run a cargo command from the given directory.
fn run_cargo_command(directory: &Path, args: &[&str]) -> bool {
    match cargo_env::command("cargo")
        .args(args)
        .current_dir(directory)
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
