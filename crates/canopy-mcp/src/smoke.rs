use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    AppFactory, Error, Result,
    script::{ScriptEvalOutcome, ScriptEvalRequest},
};

/// Configuration for a smoke-suite run.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteConfig {
    /// Root directory to scan for `.luau` scripts when no explicit script list
    /// is provided.
    pub suite_dir: PathBuf,
    /// Optional subset of scripts to run. Relative paths are resolved against
    /// `suite_dir`.
    pub scripts: Vec<PathBuf>,
    /// Optional per-script timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Stop after the first failed script.
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

/// Result of running one smoke script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptOutcome {
    /// Script path on disk.
    pub path: PathBuf,
    /// Fixture derived for this script, if any.
    pub fixture: Option<String>,
    /// Structured script outcome, carrying success, timing, and error details.
    pub outcome: ScriptEvalOutcome,
}

/// Aggregated result for a smoke suite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuiteOutcome {
    /// Per-script results in execution order.
    pub scripts: Vec<ScriptOutcome>,
}

impl SuiteOutcome {
    /// Return true when all smoke scripts passed.
    pub fn success(&self) -> bool {
        self.scripts.iter().all(|script| script.outcome.success)
    }
}

/// Run a smoke suite against fresh headless app instances.
pub fn run_suite(factory: &AppFactory, config: &SuiteConfig) -> Result<SuiteOutcome> {
    let scripts = discover_scripts(config)?;
    let mut results = Vec::with_capacity(scripts.len());
    for path in scripts {
        let fixture = fixture_for_script(&config.suite_dir, &path);
        let source = fs::read_to_string(&path)?;
        let outcome = factory.evaluate(&ScriptEvalRequest {
            fixture: fixture.clone(),
            timeout_ms: config.timeout_ms,
            ..ScriptEvalRequest::new(source)
        });
        results.push(ScriptOutcome {
            path,
            fixture,
            outcome,
        });
        if config.fail_fast && !results.last().expect("just pushed").outcome.success {
            break;
        }
    }
    Ok(SuiteOutcome { scripts: results })
}

/// Derive a fixture name from the first path component under the suite root.
///
/// Only a normal component names a fixture; a root, prefix, or `..` component
/// does not.
pub fn fixture_for_script(suite_dir: &Path, script: &Path) -> Option<String> {
    let relative = script.strip_prefix(suite_dir).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    components.next()?;
    match first {
        Component::Normal(name) => Some(name.to_string_lossy().to_string()),
        _ => None,
    }
}

/// Resolve the ordered list of smoke scripts for a suite run.
///
/// An explicit script list keeps its given order, because that order decides
/// which script a fail-fast run stops on. Discovered files are sorted so a
/// directory walk is reproducible.
pub fn discover_scripts(config: &SuiteConfig) -> Result<Vec<PathBuf>> {
    let scripts = if config.scripts.is_empty() {
        let mut discovered = Vec::new();
        collect_luau_scripts(&config.suite_dir, &mut discovered)?;
        discovered.sort();
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
    use std::fs;

    use canopy::testing::contracts;
    use tempfile::TempDir;

    use super::*;

    fn unique_dir() -> TempDir {
        tempfile::tempdir().expect("create test directory")
    }

    fn file_names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("file name")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn discover_scripts_recurses_and_sorts() -> Result<()> {
        let dir = unique_dir();
        let root = dir.path();
        fs::create_dir_all(root.join("nested"))?;
        fs::write(root.join("b.luau"), "return true")?;
        fs::write(root.join("nested").join("a.luau"), "return true")?;
        let paths = discover_scripts(&SuiteConfig::new(root))?;
        assert_eq!(
            file_names(&paths),
            vec!["b.luau".to_string(), "a.luau".to_string()]
        );
        Ok(())
    }

    #[test]
    fn explicit_scripts_keep_their_given_order() -> Result<()> {
        let dir = unique_dir();
        let root = dir.path();
        let mut config = SuiteConfig::new(root);
        config.scripts = vec![PathBuf::from("z.luau"), PathBuf::from("a.luau")];
        let paths = discover_scripts(&config)?;
        assert_eq!(
            file_names(&paths),
            vec!["z.luau".to_string(), "a.luau".to_string()]
        );
        assert_eq!(paths[0], root.join("z.luau"));
        Ok(())
    }

    #[test]
    fn fixture_is_the_first_normal_component() {
        let suite = Path::new("/tmp/smoke");
        assert_eq!(
            fixture_for_script(suite, Path::new("/tmp/smoke/with_items/navigation.luau")),
            Some("with_items".to_string())
        );
        assert_eq!(
            fixture_for_script(suite, Path::new("/tmp/smoke/bootstrap.luau")),
            None
        );
    }

    #[test]
    fn a_parent_component_does_not_name_a_fixture() {
        let suite = Path::new("smoke");
        assert_eq!(
            fixture_for_script(suite, Path::new("smoke/../outside/navigation.luau")),
            None
        );
    }

    #[test]
    fn suite_timeout_and_fail_fast_share_the_request_path() -> Result<()> {
        let dir = unique_dir();
        let first = dir.path().join("first.luau");
        let second = dir.path().join("second.luau");
        fs::write(&first, "while true do end")?;
        fs::write(&second, "return true")?;
        let factory = AppFactory::new(
            crate::AppMetadata {
                app: "smoke-test".into(),
                reset: crate::ResetPolicy::Isolated,
            },
            || Ok(contracts::app()?),
        );
        let mut config = SuiteConfig::new(dir.path());
        config.scripts = vec![first, second];
        config.timeout_ms = Some(1);
        config.fail_fast = true;

        let outcome = run_suite(&factory, &config)?;

        assert_eq!(outcome.scripts.len(), 1);
        assert_eq!(
            outcome.scripts[0].outcome.state,
            crate::ScriptTaskState::TimedOut
        );
        Ok(())
    }
}
