use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use ruau::source::fs::{DirectoryMounts, DirectoryMountsError};
#[cfg(test)]
use ruau::source::{ModuleId, ReadySourceFutureExt, SourceError, SourceProvider};

/// Module id prefix for the per-user script root.
const USER_PREFIX: &str = "@user";
/// Module id prefix for the per-project script root.
const PROJECT_PREFIX: &str = "@project";
/// Conventional startup module name under each script root.
const INIT_MODULE: &str = "init";
/// Luau source file extension used by filesystem module ids.
const LUAU_EXTENSION: &str = ".luau";

/// Filesystem roots used by Canopy's persistent Luau module source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptModuleRoots {
    /// Per-user script root mounted at `@user`.
    user: Option<PathBuf>,
    /// Per-project script root mounted at `@project`.
    project: Option<PathBuf>,
}

impl ScriptModuleRoots {
    /// Construct an empty root set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the configured `@user` root.
    #[must_use]
    pub fn user_root(&self) -> Option<&Path> {
        self.user.as_deref()
    }

    /// Return the configured `@project` root.
    #[must_use]
    pub fn project_root(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// Mount `@user` at `root`.
    pub fn set_user_root(&mut self, root: impl Into<PathBuf>) {
        self.user = Some(root.into());
    }

    /// Mount `@project` at `root`.
    pub fn set_project_root(&mut self, root: impl Into<PathBuf>) {
        self.project = Some(root.into());
    }

    /// Locate the nearest `.canopy` directory at or above `start`.
    #[must_use]
    pub fn discover_project_root(start: impl AsRef<Path>) -> Option<PathBuf> {
        let mut current = start.as_ref();
        if current.is_file() {
            current = current.parent()?;
        }
        let mut current = current.to_path_buf();
        loop {
            let candidate = current.join(".canopy");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// Return the startup modules that exist for the configured roots, in layer order.
    pub(crate) fn startup_modules(&self) -> Vec<StartupModule> {
        [Namespace::User, Namespace::Project]
            .into_iter()
            .filter_map(|namespace| {
                let root = self.root_for(namespace)?;
                let path = root.join(format!("{INIT_MODULE}{LUAU_EXTENSION}"));
                path.is_file().then_some(StartupModule { namespace, path })
            })
            .collect()
    }

    /// Build the validated filesystem source for the configured roots.
    pub(crate) fn module_source(
        &self,
    ) -> Result<Option<Arc<ScriptModuleSource>>, DirectoryMountsError> {
        if self.user.is_none() && self.project.is_none() {
            return Ok(None);
        }
        let mut builder = DirectoryMounts::builder();
        if let Some(root) = &self.user {
            builder = builder.mount(USER_PREFIX, root);
        }
        if let Some(root) = &self.project {
            builder = builder.mount(PROJECT_PREFIX, root);
        }
        builder.build().map(Arc::new).map(Some)
    }

    /// Return the configured root for a namespace.
    fn root_for(&self, namespace: Namespace) -> Option<&Path> {
        match namespace {
            Namespace::User => self.user.as_deref(),
            Namespace::Project => self.project.as_deref(),
        }
    }
}

/// Canopy's persistent source is Ruau's validated multi-root filesystem source.
pub type ScriptModuleSource = DirectoryMounts;

/// Persistent script namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// Per-user script root.
    User,
    /// Per-project script root.
    Project,
}

impl Namespace {
    /// Return the user-facing namespace name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::User => "@user",
            Self::Project => "@project",
        }
    }
}

/// Startup module discovered under a persistent script root.
#[derive(Debug)]
pub struct StartupModule {
    /// Namespace that owns the startup module.
    pub(crate) namespace: Namespace,
    /// Filesystem path to the startup script.
    pub(crate) path: PathBuf,
}

#[cfg(test)]
mod tests {
    use std::{fs, process, thread};

    use super::*;

    /// Create an isolated repository-local filesystem fixture.
    fn fixture_root(label: &str) -> PathBuf {
        let thread = thread::current();
        let name = thread.name().unwrap_or("test");
        let root = Path::new("tmp").join(format!("modules-{}-{name}-{label}", process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture removes");
        }
        fs::create_dir_all(&root).expect("fixture root creates");
        root
    }

    /// Write one fixture file, creating its parent directories.
    fn write(path: &Path, source: &str) {
        fs::create_dir_all(path.parent().expect("fixture file has parent"))
            .expect("fixture parent creates");
        fs::write(path, source).expect("fixture file writes");
    }

    #[test]
    fn module_id_for_path_maps_configured_roots() {
        let base = fixture_root("reverse");
        let user = base.join("user");
        let project = base.join("project");
        let user_file = user.join("keymap.luau");
        let project_file = project.join("nested/init.luau");
        write(&user_file, "return {}");
        write(&project_file, "return {}");
        let mut roots = ScriptModuleRoots::new();
        roots.set_user_root(&user);
        roots.set_project_root(&project);
        let source = roots
            .module_source()
            .expect("mounts build")
            .expect("source exists");

        assert_eq!(
            source.module_id_for_path(&user_file),
            Ok(ModuleId::canonicalized("@user/keymap"))
        );
        assert_eq!(
            source.module_id_for_path(&project_file),
            Ok(ModuleId::canonicalized("@project/nested"))
        );
        fs::remove_dir_all(base).expect("fixture removes");
    }

    #[test]
    fn composite_source_requires_explicit_roots_for_root_imports() {
        let user = fixture_root("explicit");
        let mut roots = ScriptModuleRoots::new();
        roots.set_user_root(&user);
        let source = roots
            .module_source()
            .expect("mounts build")
            .expect("source");

        let error = source
            .resolve(None, b"keymap")
            .ready_only("resolving")
            .expect_err("bare root imports are rejected");
        assert!(matches!(error, SourceError::MissingModule { .. }));
        fs::remove_dir_all(user).expect("fixture removes");
    }
}
