//! Consuming application setup with explicit script-root trust.

use std::path::PathBuf;

use ruau::source::{ModuleId, Source};

use super::{Canopy, ScriptOrigin, Setup};
use crate::error::{Error, Result};

/// Authority granted to scripts loaded from an application-declared root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScriptTrust {
    /// Do not mount, inspect, require, or execute this root.
    #[default]
    Disabled,
    /// Execute local scripts with the application's full native authority.
    TrustedLocal,
}

/// An owned registration callback consumed exactly once.
type ConfigureCallback = Box<dyn FnOnce(&mut Setup) -> Result<()>>;

/// An owned assembly callback consumed exactly once.
type AssembleCallback = Box<dyn FnOnce(&mut Canopy) -> Result<()>>;

/// Script work retained in insertion order until after API finalization.
enum SetupSource {
    /// A named Luau script, such as a keymap.
    Script {
        /// Diagnostic and journal identity supplied by the application.
        name: String,
        /// Owned Luau source evaluated before assembly.
        source: String,
    },
    /// An explicitly requested local script file.
    ScriptFile(PathBuf),
}

/// Assemble one application through registration, scripts, and widget creation.
///
/// Configuration callbacks run first against a [`Setup`] handle, then the API
/// is finalized. Binding and config sources run next in their shared insertion
/// order, followed by assembly callbacks against the finalized [`Canopy`].
/// Callbacks are owned and need not be `Send`. The first runtime preparation
/// performs startup and publishes geometry; `build` does neither.
///
/// A failed build returns no application. Native or database effects performed
/// by callbacks are not rolled back. Retrying requires a fresh builder and
/// application resources suitable for retry.
///
/// Script setup is synchronous. From a current-thread Tokio task or `LocalSet`,
/// construct and run the entire application on a blocking worker. Synchronous
/// script execution rejects those task contexts instead of blocking their
/// scheduler.
#[derive(Default)]
pub struct CanopyBuilder {
    /// Registration callbacks in configuration-phase order.
    configure: Vec<ConfigureCallback>,
    /// Explicit binding and config sources in evaluation order.
    sources: Vec<SetupSource>,
    /// Widget assembly callbacks in assembly-phase order.
    assemble: Vec<AssembleCallback>,
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

    /// Register commands, bindings, fixtures, styles, and other state that the
    /// API fixes when it finalizes.
    #[must_use]
    pub fn configure(mut self, configure: impl FnOnce(&mut Setup) -> Result<()> + 'static) -> Self {
        self.configure.push(Box::new(configure));
        self
    }

    /// Evaluate a named Luau script after finalization and before assembly.
    /// A script can run any Luau; keymaps are the common case.
    #[must_use]
    pub fn script(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.sources.push(SetupSource::Script {
            name: name.into(),
            source: source.into(),
        });
        self
    }

    /// Evaluate an explicitly trusted local script file before assembly.
    #[must_use]
    pub fn script_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(SetupSource::ScriptFile(path.into()));
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
        let mut setup = Setup::new();
        for configure in self.configure {
            configure(&mut setup)?;
        }
        let trusted = |root: Option<(PathBuf, ScriptTrust)>| match root {
            Some((path, ScriptTrust::TrustedLocal)) => Some(path),
            _ => None,
        };
        let mut canopy = setup.finalize(trusted(self.user_root), trusted(self.project_root))?;
        for source in self.sources {
            match source {
                SetupSource::Script { name, source } => canopy.evaluate_bindings(&name, &source)?,
                SetupSource::ScriptFile(path) => canopy.run_config(&path)?,
            }
        }
        for assemble in self.assemble {
            assemble(&mut canopy)?;
        }
        Ok(canopy)
    }
}

impl Canopy {
    /// Evaluate named setup source without entering a frame-preparation turn.
    pub(crate) fn evaluate_bindings(&mut self, name: &str, text: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(Error::Invalid(
                "builder binding name cannot be empty".into(),
            ));
        }
        let baseline = self.begin_script_journal();
        let source = Source::text(ModuleId::new(name.as_bytes().to_vec()), text);
        let result = (|| {
            let host = self.script.host.clone();
            let script = host.compile_source(&source)?;
            host.execute(self, self.root_id(), script).map(|_| ())
        })();
        self.record_script_journal(
            ScriptOrigin::Build(name.to_owned()),
            text,
            baseline,
            &result,
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, fs, rc::Rc};

    use tempfile::tempdir;

    use super::*;
    use crate::{geom::Size, runtime::TurnInput};

    #[test]
    fn build_preserves_phase_order_without_preparing_or_running_startup() -> Result<()> {
        let directory = tempdir().expect("create test directory");
        let config = directory.path().join("config.luau");
        fs::write(&config, "canopy.enter_mode(\"config\")").expect("write test script");
        let phases = Rc::new(RefCell::new(Vec::new()));
        let configure_first = Rc::clone(&phases);
        let configure_second = Rc::clone(&phases);
        let assemble_first = Rc::clone(&phases);
        let assemble_second = Rc::clone(&phases);
        let mut canopy = CanopyBuilder::new()
            .assemble(move |canopy| {
                assert!(canopy.script.host.is_finalized());
                assert_eq!(canopy.mode(), "last");
                assert!(canopy.snapshot().is_none());
                assemble_first.borrow_mut().push("assemble first");
                Ok(())
            })
            .configure(move |setup| {
                configure_first.borrow_mut().push("configure first");
                setup.register_startup_script(
                    "deferred",
                    "function setup() canopy.enter_mode(\"startup\") end",
                )
            })
            .script("first", "canopy.enter_mode(\"first\")")
            .script_file(config)
            .configure(move |_| {
                configure_second.borrow_mut().push("configure second");
                Ok(())
            })
            .script(
                "last",
                "canopy.assert(canopy.mode() == \"config\"); canopy.enter_mode(\"last\")",
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
        assert_eq!(journal[0].origin.to_string(), "build:first");
        assert!(journal[1].origin.to_string().starts_with("script-file:"));
        assert_eq!(journal[2].origin.to_string(), "build:last");
        canopy.set_screen_size(Size::new(10, 3))?;
        canopy.turn(TurnInput::Prepare)?;
        assert_eq!(canopy.mode(), "startup");
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
                builder = builder.script("bad", "error(\"bindings failed\")");
            }
            if phase == "config" {
                let directory = tempdir().expect("create test directory");
                builder = builder.script_file(directory.path().join("missing.luau"));
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
            .configure(|setup| setup.register_startup_script("invalid", "function setup("))
            .script("unreachable", "error(\"binding should not run\")")
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
            let mut builder = CanopyBuilder::new();
            if namespace == "user" {
                builder = builder
                    .user_script_root(directory.path().to_owned(), ScriptTrust::TrustedLocal)
                    .user_script_root(directory.path().to_owned(), ScriptTrust::Disabled);
            } else {
                builder =
                    builder.project_script_root(directory.path().to_owned(), ScriptTrust::Disabled);
            }
            let mut canopy = builder.build()?;
            assert!(canopy.script.module_source.is_none());
            canopy.set_screen_size(Size::new(10, 3))?;
            canopy.turn(TurnInput::Prepare)?;
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
            "function setup() canopy.enter_mode(\"trusted\") end",
        )
        .expect("write test script");
        let mut canopy = CanopyBuilder::new()
            .project_script_root(directory.path().to_owned(), ScriptTrust::TrustedLocal)
            .build()?;
        assert_eq!(canopy.mode(), "");
        assert!(canopy.snapshot().is_none());
        canopy.set_screen_size(Size::new(10, 3))?;
        canopy.turn(TurnInput::Prepare)?;
        assert_eq!(canopy.mode(), "trusted");
        Ok(())
    }
}
