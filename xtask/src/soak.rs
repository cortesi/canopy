//! The soak tier: property tests with many cases and random seeds.
//!
//! Routine runs use a fixed seed and few cases. A soak run sets
//! `PROPTEST_CASES` and `PROPTEST_RNG_SEED` for each suite, which the tests'
//! configuration reads. Proptest keeps each failure in the
//! `proptest-regressions` file beside the test, so later runs try it first.

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    path::Path,
    process::Stdio,
};

use crate::cargo_env;

/// A suite of property tests: its name, its package, the filter that selects
/// its tests, and its cases in a soak run.
struct Suite {
    /// The name that selects the suite on the command line.
    name: &'static str,
    /// The package that holds the tests.
    package: &'static str,
    /// The test filter that selects them.
    filter: &'static str,
    /// The cases of each property test.
    cases: u32,
}

/// The soak suites, with the case counts of the testing plan.
const SUITES: [Suite; 2] = [
    Suite {
        name: "units",
        package: "canopy-markdown",
        filter: "units::tests::",
        cases: 10_000,
    },
    Suite {
        name: "cursor",
        package: "canopy-markdown",
        filter: "widget::cursor_tests::",
        cases: 2_000,
    },
];

/// Run the soak suites that `names` select, or every suite when it is empty,
/// with `cases` in place of each suite's own count, and `seed`, or a random
/// one. Return whether every suite passed.
pub fn run(workspace_root: &Path, names: &[String], cases: Option<u32>, seed: Option<u64>) -> bool {
    let unknown: Vec<_> = names
        .iter()
        .filter(|name| !SUITES.iter().any(|suite| suite.name == name.as_str()))
        .collect();
    if !unknown.is_empty() {
        let known: Vec<_> = SUITES.iter().map(|suite| suite.name).collect();
        eprintln!("unknown soak suites {unknown:?}; the suites are {known:?}");
        return false;
    }
    let mut ok = true;
    for suite in SUITES
        .iter()
        .filter(|suite| names.is_empty() || names.iter().any(|name| name == suite.name))
    {
        let seed = seed.unwrap_or_else(random_seed);
        let cases = cases.unwrap_or(suite.cases);
        println!(
            "Soak {}: {cases} cases, PROPTEST_RNG_SEED={seed}",
            suite.name
        );
        let status = cargo_env::command("cargo")
            .args([
                "test",
                "--quiet",
                "-p",
                suite.package,
                "--all-features",
                "--lib",
                suite.filter,
            ])
            .env("PROPTEST_CASES", cases.to_string())
            .env("PROPTEST_RNG_SEED", seed.to_string())
            .current_dir(workspace_root)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!(
                    "soak {} failed with status {status}; proptest kept the failure in its proptest-regressions file",
                    suite.name
                );
                ok = false;
            }
            Err(error) => {
                eprintln!("failed to run cargo for soak {}: {error}", suite.name);
                ok = false;
            }
        }
    }
    ok
}

/// Return a random seed from the standard library's hash keys.
fn random_seed() -> u64 {
    RandomState::new().build_hasher().finish()
}
