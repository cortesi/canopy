//! User configuration homes.
//!
//! A configuration home is a directory that may hold `init.luau`. Canopy mounts
//! the home only when that file exists; otherwise the application runs its
//! built-in defaults under the same `setup()` contract, so one file serves as
//! both. Nothing here writes unless the application opts in.

use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use canopy::{CanopyBuilder, script::ScriptTrust};

use crate::{Error, LaunchMode, Result};

/// Startup file a configuration home must hold to be mounted.
pub const INIT_SCRIPT: &str = "init.luau";

/// A resolved directory for a user's Luau configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigHome {
    /// Directory that may hold [`INIT_SCRIPT`].
    path: PathBuf,
}

impl ConfigHome {
    /// Use `path` as the configuration home.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Resolve the configuration home of `app` for a launch in `mode`.
    ///
    /// Headless MCP is hermetic: it resolves to no home, and an explicit
    /// `flag` there is an error. `no_config` also resolves to no home, and
    /// conflicts with an explicit `flag`. Otherwise the home is `flag`, then
    /// the `{APP}_CONFIG_HOME` environment variable, then `$HOME/.{app}`.
    pub fn resolve(
        app: &str,
        flag: Option<PathBuf>,
        no_config: bool,
        mode: &LaunchMode,
    ) -> Result<Option<Self>> {
        let conflict = |message: &str| Err(Error::Config(message.to_owned()));
        if matches!(mode, LaunchMode::HeadlessMcp) {
            return if flag.is_some() {
                conflict("a configuration home conflicts with headless MCP, which reads none")
            } else {
                Ok(None)
            };
        }
        if no_config {
            return if flag.is_some() {
                conflict("--no-config conflicts with an explicit configuration home")
            } else {
                Ok(None)
            };
        }
        if let Some(path) = flag {
            return Ok(Some(Self::new(path)));
        }
        let variable = format!("{}_CONFIG_HOME", app.to_ascii_uppercase());
        if let Some(path) = env::var_os(&variable) {
            return Ok(Some(Self::new(path)));
        }
        let home = env::var_os("HOME").ok_or_else(|| {
            Error::Config(format!(
                "HOME is not set; set {variable} or pass a configuration home"
            ))
        })?;
        Ok(Some(Self::new(PathBuf::from(home).join(format!(".{app}")))))
    }

    /// Return the home directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return whether the home holds [`INIT_SCRIPT`], and so is mounted.
    pub fn has_init(&self) -> bool {
        self.path.join(INIT_SCRIPT).is_file()
    }

    /// Write `defaults` as the home's [`INIT_SCRIPT`] unless one exists,
    /// creating the directory. Return whether it wrote.
    ///
    /// This is opt-in. An existing file, including one a concurrent first run
    /// just created, is never replaced.
    pub fn write_defaults(&self, defaults: &str) -> io::Result<bool> {
        fs::create_dir_all(&self.path)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join(INIT_SCRIPT))
        {
            Ok(mut file) => file.write_all(defaults.as_bytes()).map(|()| true),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(error),
        }
    }
}

/// Mount a user configuration on a [`CanopyBuilder`].
pub trait UserConfig {
    /// Mount `home` as the trusted `@user` root when it holds
    /// [`INIT_SCRIPT`], and otherwise run `defaults` as the startup script.
    ///
    /// `defaults` follows the startup-script contract: it defines `setup()`.
    #[must_use]
    fn user_config(self, home: Option<&ConfigHome>, defaults: &'static str) -> Self;
}

impl UserConfig for CanopyBuilder {
    fn user_config(self, home: Option<&ConfigHome>, defaults: &'static str) -> Self {
        match home.filter(|home| home.has_init()) {
            Some(home) => self.user_script_root(home.path().to_owned(), ScriptTrust::TrustedLocal),
            None => {
                self.configure(move |setup| setup.register_startup_script("defaults", defaults))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use canopy::terminal::RunOptions;
    use tempfile::tempdir;

    use super::*;

    /// An interactive launch.
    fn run() -> LaunchMode {
        LaunchMode::Run {
            mcp_socket: None,
            options: RunOptions::default(),
        }
    }

    #[test]
    fn headless_mcp_is_hermetic() -> Result<()> {
        assert_eq!(
            ConfigHome::resolve("app", None, false, &LaunchMode::HeadlessMcp)?,
            None
        );
        assert!(
            ConfigHome::resolve("app", Some("home".into()), false, &LaunchMode::HeadlessMcp)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn an_explicit_home_wins_and_conflicts_with_no_config() -> Result<()> {
        let home = ConfigHome::resolve("app", Some("home".into()), false, &run())?;
        assert_eq!(
            home.map(|home| home.path().to_owned()),
            Some(PathBuf::from("home"))
        );
        assert_eq!(ConfigHome::resolve("app", None, true, &run())?, None);
        assert!(ConfigHome::resolve("app", Some("home".into()), true, &run()).is_err());
        Ok(())
    }

    #[test]
    fn defaults_are_written_once_and_mount_the_home() -> Result<()> {
        let directory = tempdir()?;
        let home = ConfigHome::new(directory.path().join("nested"));
        assert!(!home.has_init());
        assert!(home.write_defaults("function setup() end\n")?);
        assert!(home.has_init());
        fs::write(home.path().join(INIT_SCRIPT), "-- edited\n")?;
        assert!(!home.write_defaults("function setup() end\n")?);
        assert_eq!(
            fs::read_to_string(home.path().join(INIT_SCRIPT))?,
            "-- edited\n"
        );
        Ok(())
    }
}
