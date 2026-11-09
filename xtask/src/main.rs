#![deny(unsafe_code)]
//! Developer workflow tasks for the canopy workspace.

mod big_text;
mod cargo_env;
mod luau_grammar;
mod soak;
mod unicode_mosaics;
mod xmark_specs;

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
    /// Score the block mosaics on the standard suite of images and text, as
    /// the eye sees them at two viewing distances, against the baseline.
    /// Arguments pass to `mosaic-score`, for example `--check`, `--update`,
    /// `--filter NAME`, `--sheets`, `--worst N`, or `--detail`.
    Score {
        /// The arguments of `mosaic-score`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Extract the names of the mosaic characters from a copy of
    /// `UnicodeData.txt` into the fixture of canopy-mosaic.
    UnicodeMosaics {
        /// A copy of `UnicodeData.txt`, version 16.0 or later.
        data: PathBuf,
        /// Fail when the fixture differs from what the data gives, and write
        /// nothing.
        #[arg(long)]
        check: bool,
    },
    /// Run the property tests with many cases and random seeds. Each
    /// failure stays in its test's `proptest-regressions` file.
    Soak {
        /// The suites to run, `units`, `cursor`, or `edit`; every suite by
        /// default.
        suites: Vec<String>,
        /// The cases of each property test, in place of each suite's own.
        #[arg(long)]
        cases: Option<u32>,
        /// The proptest seed, to repeat a run; random by default.
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Generate canopy-xmark's spec tests in `tests/suite` from its spec
    /// files.
    XmarkSpecs {
        /// Fail when the tests differ from what the specs give, and write
        /// nothing.
        #[arg(long)]
        check: bool,
    },
    /// Compare vi visual mode of the editor with Neovim, which must be on
    /// the path: the curated cases, and generated ones.
    VimDiff {
        /// The number of generated cases.
        #[arg(long, default_value_t = 5000)]
        cases: u64,
        /// The seed of the generator.
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Run a fuzz target of `fuzz/` for a fixed time, with `cargo fuzz` on
    /// the nightly toolchain. New inputs go to `fuzz/corpus/TARGET`, and the
    /// checked-in seeds of `fuzz/seeds/TARGET` start the run.
    Fuzz {
        /// The fuzz target: `markdown_units`, `markdown_edit`, or `vi_keys`.
        target: String,
        /// How long to run, in seconds.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
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
        Task::UnicodeMosaics { data, check } => unicode_mosaics::run(&root, &data, check),
        Task::XmarkSpecs { check } => xmark_specs::run(&root, check),
        Task::Soak {
            suites,
            cases,
            seed,
        } => soak::run(&root, &suites, cases, seed),
        Task::VimDiff { cases, seed } => run_vim_diff(&root, cases, seed),
        Task::Fuzz { target, seconds } => run_fuzz(&root, &target, seconds),
        Task::Score { args } => {
            let mut command = vec!["run", "--quiet", "-p", "mosaic-score", "--"];
            command.extend(args.iter().map(String::as_str));
            run_cargo_command(&root, &command)
        }
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
const GALLERY: [(&str, &[&str]); 11] = [
    ("style.luau", &["style"]),
    ("table.luau", &["table"]),
    ("markdown.luau", &["markdown"]),
    ("blocks.luau", &["markdown"]),
    ("block-edit.luau", &["blocks"]),
    ("chart.luau", &["chart"]),
    ("big.luau", &["big"]),
    (
        "cedit.luau",
        &["cedit", "../../crates/canopy-widgets/src/spinner.rs"],
    ),
    ("editor.luau", &["editor"]),
    ("font.luau", &["font"]),
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
    for features in [vec![], vec!["--no-default-features"]] {
        let mut args = vec!["check", "-p", "canopy-markdown"];
        args.extend(features);
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

/// Run the ignored Neovim comparison of `canopy-widgets` with `cases`
/// generated cases.
fn run_vim_diff(workspace_root: &Path, cases: u64, seed: Option<u64>) -> bool {
    let mut command = cargo_env::command("cargo");
    command
        .args([
            "test",
            "-p",
            "canopy-widgets",
            "--all-features",
            "--test",
            "vim_diff",
            "--",
            "--ignored",
            "--nocapture",
        ])
        .env("VIM_DIFF_CASES", cases.to_string());
    if let Some(seed) = seed {
        command.env("VIM_DIFF_SEED", seed.to_string());
    }
    run_command(
        command.current_dir(workspace_root),
        "cargo test --test vim_diff",
    )
}

/// The targets of `fuzz/`.
const FUZZ_TARGETS: [&str; 3] = ["markdown_units", "markdown_edit", "vi_keys"];

/// Run the fuzz target `target` for `seconds` seconds.
fn run_fuzz(workspace_root: &Path, target: &str, seconds: u64) -> bool {
    if !FUZZ_TARGETS.contains(&target) {
        eprintln!("unknown fuzz target {target:?}; the targets are {FUZZ_TARGETS:?}");
        return false;
    }
    let fuzz = workspace_root.join("fuzz");
    let corpus = fuzz.join("corpus").join(target);
    if let Err(error) = fs::create_dir_all(&corpus) {
        eprintln!("Failed to create {}: {error}", corpus.display());
        return false;
    }
    // Name the target: `cargo fuzz` otherwise builds for the triple it was
    // built for, which can differ from the host.
    let Some(host) = nightly_host() else {
        eprintln!("Failed to read the host of the nightly toolchain");
        return false;
    };
    let mut command = cargo_env::command("rustup");
    command
        .args([
            "run", "nightly", "cargo", "fuzz", "run", "--target", &host, target,
        ])
        .arg(&corpus);
    let seeds = fuzz.join("seeds").join(target);
    if seeds.is_dir() {
        command.arg(&seeds);
    }
    command.arg("--").arg(format!("-max_total_time={seconds}"));
    run_command(
        command.current_dir(workspace_root),
        &format!("cargo fuzz run {target}"),
    )
}

/// Return the host triple of the nightly toolchain.
fn nightly_host() -> Option<String> {
    let output = cargo_env::command("rustup")
        .args(["run", "nightly", "rustc", "-vV"])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_string)
}

/// Run `command`, which `label` names in messages, with inherited output.
fn run_command(command: &mut Command, label: &str) -> bool {
    match command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("Command `{label}` failed with status {status}");
            false
        }
        Err(error) => {
            eprintln!("Failed to run `{label}`: {error}");
            false
        }
    }
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
    run_command(
        cargo_env::command("cargo")
            .args(args)
            .current_dir(directory),
        &format!("cargo {}", args.join(" ")),
    )
}

/// Write `generated` to `output` under the workspace root, or with `check`,
/// fail when the file on disk differs from it. `regenerate` names the command
/// that rewrites the file.
fn write_generated(
    workspace_root: &Path,
    output: &str,
    generated: Result<String, String>,
    check: bool,
    regenerate: &str,
) -> bool {
    let result = generated.and_then(|contents| {
        let path = workspace_root.join(output);
        if check {
            let current = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {output}: {error}"))?;
            if current != contents {
                return Err(format!("{output} is stale: run `{regenerate}`"));
            }
            println!("{output} is current");
        } else {
            fs::write(&path, contents)
                .map_err(|error| format!("cannot write {output}: {error}"))?;
            println!("wrote {output}");
        }
        Ok(())
    });
    match result {
        Ok(()) => true,
        Err(error) => {
            eprintln!("{error}");
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
