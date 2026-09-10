//! Consuming application setup with explicit script-root trust.

use std::path::PathBuf;

use ruau::source::{ModuleId, Source};

use super::{Canopy, ScriptOrigin};
use crate::{
    error::{Error, Result},
    script::ScriptModuleRoots,
};

/// Authority granted to scripts loaded from an application-declared root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScriptTrust {
    /// Do not mount, inspect, require, or execute this root.
    #[default]
    Disabled,
    /// Execute local scripts with the application's full native authority.
    TrustedLocal,
}

/// An owned setup callback consumed exactly once by its phase.
type SetupCallback = Box<dyn FnOnce(&mut Canopy) -> Result<()>>;

/// Script work retained in insertion order until after API finalization.
enum SetupSource {
    /// Inline bindings with a stable diagnostic name.
    Bindings {
        /// Diagnostic and journal identity supplied by the application.
        name: String,
        /// Owned Luau source evaluated before assembly.
        source: String,
    },
    /// Explicitly requested local configuration file.
    Config(PathBuf),
}

/// Assemble one application through registration, scripts, and widget creation.
///
/// Configuration callbacks run first, then the API is finalized. Binding and
/// config sources run next in their shared insertion order, followed by
/// assembly callbacks. Callbacks are owned and need not be `Send`. The first
/// runtime preparation performs startup and publishes geometry; `build` does
/// neither.
///
/// A failed build returns no application. Native or database effects performed
/// by callbacks are not rolled back. Retrying requires a fresh builder and
/// application resources suitable for retry.
#[derive(Default)]
pub struct CanopyBuilder {
    /// Registration callbacks in configuration-phase order.
    configure: Vec<SetupCallback>,
    /// Explicit binding and config sources in evaluation order.
    sources: Vec<SetupSource>,
    /// Widget assembly callbacks in assembly-phase order.
    assemble: Vec<SetupCallback>,
    /// Last declared user-root path and trust decision.
    user_root: Option<(PathBuf, ScriptTrust)>,
    /// Last declared project-root path and trust decision.
    project_root: Option<(PathBuf, ScriptTrust)>,
}

impl CanopyBuilder {
    /// Start a builder with no mounted script roots or setup callbacks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register commands, fixtures, defaults, and other pre-finalization state.
    #[must_use]
    pub fn configure(
        mut self,
        configure: impl FnOnce(&mut Canopy) -> Result<()> + 'static,
    ) -> Self {
        self.configure.push(Box::new(configure));
        self
    }

    /// Evaluate named binding source after finalization and before assembly.
    #[must_use]
    pub fn bindings(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.sources.push(SetupSource::Bindings {
            name: name.into(),
            source: source.into(),
        });
        self
    }

    /// Evaluate an explicitly trusted local config file before assembly.
    #[must_use]
    pub fn config(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(SetupSource::Config(path.into()));
        self
    }

    /// Construct widgets after the finalized setup sources have run.
    #[must_use]
    pub fn assemble(mut self, assemble: impl FnOnce(&mut Canopy) -> Result<()> + 'static) -> Self {
        self.assemble.push(Box::new(assemble));
        self
    }

    /// Declare the user script root; the last declaration for this root wins.
    ///
    /// Disabled paths are not accessed. Trusted roots use the existing `@user`
    /// module namespace and execute startup during the first preparation.
    #[must_use]
    pub fn user_script_root(mut self, path: PathBuf, trust: ScriptTrust) -> Self {
        self.user_root = Some((path, trust));
        self
    }

    /// Declare the project script root; the last declaration for this root
    /// wins.
    ///
    /// Disabled paths are not accessed. Trusted roots use the existing
    /// `@project` module namespace and retain project startup ordering.
    #[must_use]
    pub fn project_script_root(mut self, path: PathBuf, trust: ScriptTrust) -> Self {
        self.project_root = Some((path, trust));
        self
    }

    /// Consume all setup phases, returning the application only on success.
    pub fn build(self) -> Result<Canopy> {
        let mut canopy = Canopy::empty();
        for configure in self.configure {
            configure(&mut canopy)?;
        }
        canopy.ensure_api_unfinalized("builder registration phase")?;
        // Builder root declarations are authoritative, including disabled
        // defaults.
        canopy.script_module_roots = ScriptModuleRoots::default();
        if let Some((path, ScriptTrust::TrustedLocal)) = self.user_root {
            canopy.set_user_script_root_inner(path)?;
        }
        if let Some((path, ScriptTrust::TrustedLocal)) = self.project_root {
            canopy.set_project_script_root_inner(path)?;
        }
        canopy.finalize_api_inner()?;
        for source in self.sources {
            match source {
                SetupSource::Bindings { name, source } => {
                    evaluate_bindings(&mut canopy, &name, &source)?
                }
                SetupSource::Config(path) => canopy.run_config_inner(&path)?,
            }
        }
        for assemble in self.assemble {
            assemble(&mut canopy)?;
        }
        Ok(canopy)
    }
}

/// Evaluate named setup source without entering a frame-preparation turn.
fn evaluate_bindings(canopy: &mut Canopy, name: &str, text: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Invalid(
            "builder binding name cannot be empty".into(),
        ));
    }
    let baseline = canopy.begin_script_journal();
    let source = Source::text(ModuleId::new(name.as_bytes().to_vec()), text);
    let result = (|| {
        let host = canopy.script_host.clone();
        let script = host.compile_source(&source)?;
        host.execute(canopy, canopy.root_id(), script, None)
            .map(|_| ())
    })();
    canopy.record_script_journal(
        ScriptOrigin::Bindings(name.to_owned()),
        text,
        baseline,
        &result,
    );
    result
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, fs, rc::Rc};

    use tempfile::tempdir;

    use super::*;
    use crate::{Work, geom::Size};

    #[test]
    fn build_preserves_phase_order_without_preparing_or_running_startup() -> Result<()> {
        let directory = tempdir().expect("create test directory");
        let config = directory.path().join("config.luau");
        fs::write(&config, "canopy.set_mode(\"config\")").expect("write test script");
        let phases = Rc::new(RefCell::new(Vec::new()));
        let configure_first = Rc::clone(&phases);
        let configure_second = Rc::clone(&phases);
        let assemble_first = Rc::clone(&phases);
        let assemble_second = Rc::clone(&phases);
        let mut canopy = CanopyBuilder::new()
            .assemble(move |canopy| {
                assert!(canopy.script_host.is_finalized());
                assert_eq!(canopy.input_mode(), "last");
                assert!(canopy.snapshot().is_none());
                assemble_first.borrow_mut().push("assemble first");
                Ok(())
            })
            .configure(move |canopy| {
                assert!(!canopy.script_host.is_finalized());
                configure_first.borrow_mut().push("configure first");
                canopy.register_startup_script(
                    "deferred",
                    "function setup() canopy.set_mode(\"startup\") end",
                )
            })
            .bindings("first", "canopy.set_mode(\"first\")")
            .config(config)
            .configure(move |canopy| {
                assert!(!canopy.script_host.is_finalized());
                configure_second.borrow_mut().push("configure second");
                Ok(())
            })
            .bindings(
                "last",
                "canopy.assert(canopy.input_mode() == \"config\"); canopy.set_mode(\"last\")",
            )
            .assemble(move |_| {
                assemble_second.borrow_mut().push("assemble second");
                Ok(())
            })
            .build()?;
        assert_eq!(
            *phases.borrow(),
            [
                "configure first",
                "configure second",
                "assemble first",
                "assemble second"
            ]
        );
        assert!(canopy.snapshot().is_none());
        let journal = canopy.script_journal();
        assert_eq!(journal.len(), 3);
        assert_eq!(journal[0].origin.to_string(), "bindings:first");
        assert!(journal[1].origin.to_string().starts_with("config:"));
        assert_eq!(journal[2].origin.to_string(), "bindings:last");
        canopy.set_root_size(Size::new(10, 3))?;
        canopy.turn(Work::Prepare)?;
        assert_eq!(canopy.input_mode(), "startup");
        Ok(())
    }

    #[test]
    fn setup_failures_skip_every_later_phase() -> Result<()> {
        for phase in ["configure", "bindings", "config", "assemble"] {
            let reached = Rc::new(RefCell::new(Vec::new()));
            let first = Rc::clone(&reached);
            let last = Rc::clone(&reached);
            let mut builder = CanopyBuilder::new().configure(move |_| {
                first.borrow_mut().push("configure");
                if phase == "configure" {
                    return Err(Error::Invalid("configure failed".into()));
                }
                Ok(())
            });
            if phase == "bindings" {
                builder = builder.bindings("bad", "error(\"bindings failed\")");
            }
            if phase == "config" {
                let directory = tempdir().expect("create test directory");
                builder = builder.config(directory.path().join("missing.luau"));
            }
            builder = builder.assemble(move |_| {
                last.borrow_mut().push("assemble");
                Err(Error::Invalid("assemble failed".into()))
            });
            assert!(builder.build().is_err());
            assert_eq!(
                reached.borrow().as_slice(),
                if phase == "assemble" {
                    &["configure", "assemble"][..]
                } else {
                    &["configure"][..]
                }
            );
        }
        Ok(())
    }

    #[test]
    fn finalization_failure_never_reaches_sources_or_assembly() -> Result<()> {
        let assembled = Rc::new(RefCell::new(false));
        let observed = Rc::clone(&assembled);
        let result = CanopyBuilder::new()
            .configure(|canopy| canopy.register_startup_script("invalid", "function setup("))
            .bindings("unreachable", "error(\"binding should not run\")")
            .assemble(move |_| {
                *observed.borrow_mut() = true;
                Ok(())
            })
            .build();
        assert!(result.is_err());
        assert!(!*assembled.borrow());
        Ok(())
    }

    #[test]
    fn disabled_roots_are_not_mounted_required_or_started() -> Result<()> {
        let directory = tempdir().expect("create test directory");
        fs::write(
            directory.path().join("init.luau"),
            "function setup() error(\"disabled startup executed\") end",
        )
        .expect("write test script");
        fs::write(directory.path().join("payload.luau"), "return 9").expect("write test script");
        for namespace in ["user", "project"] {
            let configured_path = directory.path().to_owned();
            let mut builder = CanopyBuilder::new().configure(move |canopy| {
                canopy.set_user_script_root(&configured_path)?;
                canopy.set_project_script_root(&configured_path)
            });
            if namespace == "user" {
                builder = builder
                    .user_script_root(directory.path().to_owned(), ScriptTrust::TrustedLocal)
                    .user_script_root(directory.path().to_owned(), ScriptTrust::Disabled);
            } else {
                builder =
                    builder.project_script_root(directory.path().to_owned(), ScriptTrust::Disabled);
            }
            let mut canopy = builder.build()?;
            assert!(canopy.script_module_source.is_none());
            canopy.set_root_size(Size::new(10, 3))?;
            canopy.turn(Work::Prepare)?;
            assert!(
                canopy
                    .eval_script(&format!("return require(\"@{namespace}/payload\")"))
                    .is_err()
            );
        }
        let missing = directory.path().join("does-not-exist");
        assert!(
            CanopyBuilder::new()
                .user_script_root(missing.clone(), ScriptTrust::Disabled)
                .project_script_root(missing, ScriptTrust::Disabled)
                .build()
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn trusted_root_startup_remains_deferred_until_preparation() -> Result<()> {
        let directory = tempdir().expect("create test directory");
        fs::write(
            directory.path().join("init.luau"),
            "function setup() canopy.set_mode(\"trusted\") end",
        )
        .expect("write test script");
        let mut canopy = CanopyBuilder::new()
            .project_script_root(directory.path().to_owned(), ScriptTrust::TrustedLocal)
            .build()?;
        assert_eq!(canopy.input_mode(), "");
        assert!(canopy.snapshot().is_none());
        canopy.set_root_size(Size::new(10, 3))?;
        canopy.turn(Work::Prepare)?;
        assert_eq!(canopy.input_mode(), "trusted");
        Ok(())
    }
}
