use std::{
    fs,
    path::{Path, PathBuf},
};

use canopy::Canopy;
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    script::{AppEvaluator, ScriptEvalOutcome, ScriptEvalRequest, app_factory},
};

/// Configuration for a smoke-suite run.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteConfig {
    /// Root directory to scan for `.luau` scripts when no explicit script list is provided.
    pub suite_dir: PathBuf,
    /// Optional subset of scripts to run. Relative paths are resolved against `suite_dir`.
    pub scripts: Vec<PathBuf>,
    /// Optional timeout per script in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Stop after the first failing script when true.
    pub fail_fast: bool,
}

impl SuiteConfig {
    /// Construct a config using a suite directory and default options.
    pub fn new(suite_dir: impl Into<PathBuf>) -> Self {
        Self {
            suite_dir: suite_dir.into(),
            scripts: Vec::new(),
            timeout_ms: None,
            fail_fast: false,
        }
    }
}

/// Final status for a smoke script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptStatus {
    /// The script passed.
    Passed,
    /// The script failed.
    Failed,
}

/// Result of running one smoke script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptResult {
    /// Script path on disk.
    pub path: PathBuf,
    /// Pass or fail status.
    pub status: ScriptStatus,
    /// Total script duration in milliseconds.
    pub elapsed_ms: u64,
    /// Optional summary message.
    pub message: Option<String>,
    /// Structured script outcome.
    pub outcome: ScriptEvalOutcome,
}

/// Aggregated result for a smoke suite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuiteResult {
    /// Per-script results in execution order.
    pub scripts: Vec<ScriptResult>,
}

impl SuiteResult {
    /// Return true when all smoke scripts passed.
    pub fn success(&self) -> bool {
        self.scripts
            .iter()
            .all(|script| script.status == ScriptStatus::Passed)
    }
}

/// Run a smoke suite against fresh headless app instances.
pub fn run_suite(
    factory: impl Fn() -> Result<Canopy> + Send + Sync + 'static,
    config: &SuiteConfig,
) -> Result<SuiteResult> {
    let evaluator = AppEvaluator::new(app_factory(factory));
    let scripts = discover_scripts(config)?;
    let mut results = Vec::with_capacity(scripts.len());
    for path in scripts {
        let source = fs::read_to_string(&path)?;
        let outcome = evaluator.evaluate_with_timeout(ScriptEvalRequest {
            script: source,
            timeout_ms: config.timeout_ms,
        });
        let message = outcome
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .or_else(|| {
                outcome
                    .assertions
                    .iter()
                    .find(|assertion| !assertion.passed)
                    .map(|assertion| assertion.message.clone())
            });
        let status = if outcome.success {
            ScriptStatus::Passed
        } else {
            ScriptStatus::Failed
        };
        results.push(ScriptResult {
            path,
            status,
            elapsed_ms: outcome.timing.total_ms,
            message,
            outcome,
        });
        if config.fail_fast && results.last().is_some_and(|result| !result.outcome.success) {
            break;
        }
    }
    Ok(SuiteResult { scripts: results })
}

/// Resolve the ordered list of smoke scripts for a suite run.
fn discover_scripts(config: &SuiteConfig) -> Result<Vec<PathBuf>> {
    let mut scripts = if config.scripts.is_empty() {
        let mut discovered = Vec::new();
        collect_luau_scripts(&config.suite_dir, &mut discovered)?;
        discovered
    } else {
        config
            .scripts
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    config.suite_dir.join(path)
                }
            })
            .collect()
    };
    scripts.sort();
    if scripts.is_empty() {
        return Err(Error::NoScripts(config.suite_dir.clone()));
    }
    Ok(scripts)
}

/// Recursively collect `.luau` scripts under a directory.
fn collect_luau_scripts(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_luau_scripts(&path, output)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "luau")
        {
            output.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn unique_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        env::temp_dir().join(format!("canopy-smoke-{name}-{stamp}"))
    }

    #[test]
    fn discover_scripts_recurses_and_sorts() -> Result<()> {
        let root = unique_dir("discover");
        fs::create_dir_all(root.join("nested"))?;
        fs::write(root.join("b.luau"), "return true")?;
        fs::write(root.join("nested").join("a.luau"), "return true")?;
        let paths = discover_scripts(&SuiteConfig::new(&root))?;
        let names = paths
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("file name")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["b.luau".to_string(), "a.luau".to_string()]);
        Ok(())
    }
}
