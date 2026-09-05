#![deny(unsafe_code)]
//! Developer workflow tasks for the canopy workspace.

use std::{
    collections::BTreeSet,
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
    /// Build all workspace targets with default features.
    FeatureCheck,
    /// Check API skeletons and tracked Luau sources.
    Checks,
    /// Compile every benchmark target without running benchmarks.
    BenchCheck,
    /// Run targeted Miri checks for unsafe code.
    Dynamic,
    /// Run all smoke-test integration targets.
    Smoke,
    /// Regenerate the public API skeletons and report the tracked surface
    /// sizes.
    Api {
        /// Verify the checked-in skeletons instead of rewriting them.
        #[arg(long)]
        check: bool,
    },
}

/// Run the `cargo xtask` entry point.
fn main() -> ExitCode {
    let root = workspace_root();
    exit_code(match Cli::parse().task {
        Task::FeatureCheck => run_default_check(&root),
        Task::Checks => run_api_check(&root) && run_luau_check(&root),
        Task::BenchCheck => run_bench_check(&root),
        Task::Dynamic => run_dynamic(&root),
        Task::Smoke => run_smoke(&root),
        Task::Api { check: true } => run_api_check(&root),
        Task::Api { check: false } => run_api(&root),
    })
}

/// Rust nightly used for the repository's Miri checks.
const MIRI_TOOLCHAIN: &str = "+nightly";

/// Ruskel version required locally and in CI.
const RUSKEL_VERSION: &str = "0.0.11";

/// Package directories and the API skeleton each one generates.
const API_SURFACES: &[(&str, &str)] = &[
    ("crates/canopy", "api-surface/canopy.rs"),
    ("crates/canopy-derive", "api-surface/canopy-derive.rs"),
    ("crates/canopy-geom", "api-surface/canopy-geom.rs"),
    ("crates/canopy-mcp", "api-surface/canopy-mcp.rs"),
    ("crates/canopy-widgets", "api-surface/canopy-widgets.rs"),
    ("crates/examples", "api-surface/canopy-examples.rs"),
    ("examples/todo", "api-surface/todo.rs"),
];

/// Intent-level surfaces whose method counts the API budget tracks.
const INTENT_SURFACES: &[(&str, &str)] = &[
    ("Canopy", "api-surface/canopy.rs"),
    ("ViewContext", "api-surface/canopy.rs"),
    ("Context", "api-surface/canopy.rs"),
    ("Editor", "api-surface/canopy-widgets.rs"),
];

/// Run the targeted unsafe-code suites under Miri.
fn run_dynamic(workspace_root: &Path) -> bool {
    for filter in [
        "widget_slot_restores",
        "core::backend::tests",
        "reentrant_canopy_guard_restores_nested_stack",
    ] {
        if !run_cargo_command(
            workspace_root,
            &[
                MIRI_TOOLCHAIN,
                "miri",
                "test",
                "-p",
                "canopy",
                "--all-features",
                "--lib",
                filter,
            ],
        ) {
            return false;
        }
    }
    true
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

/// Build all workspace targets with default features.
fn run_default_check(workspace_root: &Path) -> bool {
    run_cargo_command(workspace_root, &["check", "--workspace", "--all-targets"])
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

/// Regenerate every API skeleton and report the tracked surface sizes.
fn run_api(workspace_root: &Path) -> bool {
    let skeletons = match render_api_surfaces(workspace_root) {
        Ok(skeletons) => skeletons,
        Err(error) => {
            eprintln!("{error}");
            return false;
        }
    };

    for (artifact, skeleton) in &skeletons {
        if let Err(error) = fs::write(workspace_root.join(artifact), skeleton) {
            eprintln!("writing {artifact} failed: {error}");
            return false;
        }
    }

    print_api_report(&skeletons);
    true
}

/// Fail when a checked-in API skeleton differs from the generated one.
fn run_api_check(workspace_root: &Path) -> bool {
    let skeletons = match render_api_surfaces(workspace_root) {
        Ok(skeletons) => skeletons,
        Err(error) => {
            eprintln!("{error}");
            return false;
        }
    };

    let mut stale = Vec::new();
    for (artifact, skeleton) in &skeletons {
        match fs::read_to_string(workspace_root.join(artifact)) {
            Ok(checked_in) if &checked_in == skeleton => {}
            Ok(_) => stale.push(*artifact),
            Err(error) => {
                eprintln!("reading {artifact} failed: {error}");
                return false;
            }
        }
    }

    if stale.is_empty() {
        return true;
    }
    eprintln!("Stale API skeletons: {}", stale.join(", "));
    eprintln!("Run `cargo xtask api` and review the surface change.");
    false
}

/// Render every API skeleton with the pinned ruskel.
fn render_api_surfaces(workspace_root: &Path) -> Result<Vec<(&'static str, String)>, String> {
    if !require_tool(
        "ruskel",
        installed_ruskel_version(),
        RUSKEL_VERSION,
        &format!("run `cargo install ruskel --version {RUSKEL_VERSION}`"),
    ) {
        return Err("the pinned ruskel is not installed".to_string());
    }

    API_SURFACES
        .iter()
        .map(|(package, artifact)| {
            let output = Command::new("ruskel")
                .arg(package)
                .current_dir(workspace_root)
                .stderr(Stdio::inherit())
                .output()
                .map_err(|error| format!("running ruskel on {package} failed: {error}"))?;
            if !output.status.success() {
                return Err(format!("ruskel on {package} failed with {}", output.status));
            }
            let skeleton = String::from_utf8(output.stdout)
                .map_err(|error| format!("ruskel output for {package} is not UTF-8: {error}"))?;
            Ok((*artifact, skeleton))
        })
        .collect()
}

/// Print the tracked method counts and skeleton sizes.
fn print_api_report(skeletons: &[(&str, String)]) {
    let skeleton = |artifact: &str| {
        skeletons
            .iter()
            .find(|(name, _)| *name == artifact)
            .map(|(_, text)| text.as_str())
            .unwrap_or_default()
    };

    println!("Intent-level surfaces");
    for (surface, artifact) in INTENT_SURFACES {
        println!(
            "  {surface}: {} methods",
            surface_method_count(skeleton(artifact), surface)
        );
    }

    println!("Skeleton sizes");
    for (artifact, text) in skeletons {
        println!("  {artifact}: {} lines", text.lines().count());
    }
}

/// Return the installed ruskel version.
fn installed_ruskel_version() -> Option<String> {
    let output = Command::new("ruskel").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.split_whitespace().nth(1).map(str::to_string)
}

/// Count the distinct methods a skeleton declares for one intent-level surface.
///
/// A surface is the inherent `impl` blocks of a type or the trait definition
/// itself. Ruskel renders a re-exported type once per path, so names are
/// deduplicated; `impl dyn Trait` helpers and trait implementations for the
/// type are not part of the surface.
fn surface_method_count(skeleton: &str, surface: &str) -> usize {
    let mut names = BTreeSet::new();
    let mut lines = skeleton.lines().peekable();
    while let Some(line) = lines.next() {
        if !is_surface_header(line.trim(), surface) {
            continue;
        }
        let body_indent = indent_of(line) + 4;
        while let Some(body) = lines.peek() {
            if !body.trim().is_empty() && indent_of(body) < body_indent {
                break;
            }
            if indent_of(body) == body_indent
                && let Some(name) = method_name(body.trim())
            {
                names.insert(name);
            }
            lines.next();
        }
    }
    names.len()
}

/// Return true when the line opens an inherent impl block or trait definition
/// for the surface.
fn is_surface_header(line: &str, surface: &str) -> bool {
    let Some(head) = line.strip_suffix('{') else {
        return false;
    };
    let head = head.trim_end();
    if let Some(ty) = head.strip_prefix("impl ") {
        // Ruskel renders an inherent impl under the path the type is defined at, which
        // is longer than the re-export the budget names.
        return ty.rsplit("::").next().unwrap_or(ty) == surface;
    }
    if let Some(ty) = head.strip_prefix("pub trait ") {
        return ty.split(':').next().unwrap_or(ty).trim() == surface;
    }
    false
}

/// Return the method name a skeleton line declares.
fn method_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("pub fn ")
        .or_else(|| line.strip_prefix("fn "))?;
    let end = rest
        .find(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Return the leading space count of a line.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Report whether the installed tool matches its repository pin.
///
/// A mismatch names the installed version so the reader can see what to change.
fn require_tool(tool: &str, installed: Option<String>, required: &str, hint: &str) -> bool {
    match installed {
        Some(version) if version == required => true,
        Some(version) => {
            eprintln!("{tool} {required} is required, but {version} is installed; {hint}");
            false
        }
        None => {
            eprintln!("{tool} {required} is required; {hint}");
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

    const SKELETON: &str = "\
pub mod canopy {
    pub mod prelude {
        pub struct Canopy {}

        impl super::Canopy {
            pub fn render(&mut self) -> Result<()> {}
        }
    }

    pub struct Canopy {}

    impl super::Canopy {
        pub fn render(&mut self) -> Result<()> {}

        pub fn quit(&mut self) {}
    }

    pub trait Context: ViewContext {
        fn focus(&mut self) -> Result<()>;

        fn hide(&mut self) {}
    }

    impl dyn Context {
        pub fn add_children(&mut self) {}
    }

    impl Widget for Canopy {
        fn name(&self) -> NodeName {}
    }

    pub mod editor {
        pub struct Editor {}

        impl super::editor::widget::Editor {
            pub fn insert(&mut self) {}
        }
    }
}
";

    #[test]
    fn surface_count_deduplicates_re_exported_paths() {
        assert_eq!(surface_method_count(SKELETON, "Canopy"), 2);
    }

    #[test]
    fn surface_count_covers_a_trait_definition() {
        assert_eq!(surface_method_count(SKELETON, "Context"), 2);
    }

    #[test]
    fn surface_count_follows_a_nested_definition_path() {
        assert_eq!(surface_method_count(SKELETON, "Editor"), 1);
    }

    #[test]
    fn surface_count_ignores_an_unknown_surface() {
        assert_eq!(surface_method_count(SKELETON, "Missing"), 0);
    }
}
