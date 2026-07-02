use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[cfg(test)]
use ruau::source::{ModuleSourceError, poll_ready_once};
use ruau::{
    fs::{FilesystemEpoch, FilesystemSource},
    source::{
        InstanceKey, ModuleId, ModuleSource, ModuleSourceFuture, MountedSource, ReadRequest,
        SourceMetadata,
    },
};

/// Module id prefix for the per-user script root.
const USER_PREFIX: &str = "@user/";
/// Module id prefix for the per-project script root.
const PROJECT_PREFIX: &str = "@project/";
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

    /// Return this root set with `@user` mounted at `root`.
    #[must_use]
    pub fn with_user_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.user = Some(root.into());
        self
    }

    /// Return this root set with `@project` mounted at `root`.
    #[must_use]
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.project = Some(root.into());
        self
    }

    /// Mount `@user` at `root`.
    pub fn set_user_root(&mut self, root: impl Into<PathBuf>) {
        self.user = Some(root.into());
    }

    /// Mount `@project` at `root`.
    pub fn set_project_root(&mut self, root: impl Into<PathBuf>) {
        self.project = Some(root.into());
    }

    /// Remove the `@user` mount.
    pub fn clear_user_root(&mut self) {
        self.user = None;
    }

    /// Remove the `@project` mount.
    pub fn clear_project_root(&mut self) {
        self.project = None;
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
                path.is_file().then(|| StartupModule {
                    namespace,
                    path,
                    module_id: prefixed_module_id(namespace, INIT_MODULE),
                })
            })
            .collect()
    }

    /// Resolve a filesystem path under one of the configured roots to a module id.
    pub(crate) fn module_id_for_path(&self, path: &Path) -> Option<ModuleId> {
        let user = self
            .user
            .as_deref()
            .and_then(|root| module_id_for_root(Namespace::User, root, path));
        user.or_else(|| {
            self.project
                .as_deref()
                .and_then(|root| module_id_for_root(Namespace::Project, root, path))
        })
    }

    /// Build a composite module source for the configured roots.
    pub(crate) fn module_source(&self) -> Option<Arc<ScriptModuleSource>> {
        (self.user.is_some() || self.project.is_some()).then(|| {
            let mut source = ScriptModuleSource::new();
            if let Some(root) = &self.user {
                source.mount(Namespace::User, root.clone());
            }
            if let Some(root) = &self.project {
                source.mount(Namespace::Project, root.clone());
            }
            Arc::new(source)
        })
    }

    /// Return the configured root for a namespace.
    fn root_for(&self, namespace: Namespace) -> Option<&Path> {
        match namespace {
            Namespace::User => self.user.as_deref(),
            Namespace::Project => self.project.as_deref(),
        }
    }
}

/// Canopy's persistent Luau module source.
#[derive(Debug)]
pub struct ScriptModuleSource {
    /// Prefix-dispatching source shared with Ruau embedders.
    source: MountedSource,
    /// Root backing `@user`, if configured.
    user: Option<ScriptRootMount>,
    /// Root backing `@project`, if configured.
    project: Option<ScriptRootMount>,
}

impl ScriptModuleSource {
    /// Construct an empty persistent source.
    fn new() -> Self {
        Self {
            source: MountedSource::new(),
            user: None,
            project: None,
        }
    }

    /// Invalidate all configured roots and return the composite epoch.
    pub fn invalidate(&self) -> u64 {
        if let Some(root) = &self.user {
            root.epoch.bump();
        }
        if let Some(root) = &self.project {
            root.epoch.bump();
        }
        self.source.epoch()
    }

    /// Invalidate the `@user` root and return the composite epoch.
    pub fn invalidate_user(&self) -> Option<u64> {
        self.user.as_ref().map(|root| {
            root.epoch.bump();
            self.source.epoch()
        })
    }

    /// Invalidate the `@project` root and return the composite epoch.
    pub fn invalidate_project(&self) -> Option<u64> {
        self.project.as_ref().map(|root| {
            root.epoch.bump();
            self.source.epoch()
        })
    }

    /// Add one namespace root to the mounted source.
    fn mount(&mut self, namespace: Namespace, root: PathBuf) {
        let source = FilesystemSource::new(root);
        let epoch = source.epoch_handle();
        let source: Arc<dyn ModuleSource> = Arc::new(source);
        self.source.mount(namespace.name(), source);
        let mount = ScriptRootMount { epoch };
        match namespace {
            Namespace::User => self.user = Some(mount),
            Namespace::Project => self.project = Some(mount),
        }
    }
}

impl ModuleSource for ScriptModuleSource {
    fn resolve(
        &self,
        requester: Option<&ModuleId>,
        request: &[u8],
    ) -> ModuleSourceFuture<ModuleId> {
        self.source.resolve(requester, request)
    }

    fn read(&self, id: &ModuleId) -> ModuleSourceFuture<Vec<u8>> {
        self.source.read(id)
    }

    fn read_request(&self, request: ReadRequest<'_>) -> ModuleSourceFuture<Vec<u8>> {
        self.source.read_request(request)
    }

    fn instance_key(&self, request: ReadRequest<'_>) -> InstanceKey {
        self.source.instance_key(request)
    }

    fn metadata(&self, id: &ModuleId) -> SourceMetadata {
        self.source.metadata(id)
    }

    fn epoch(&self) -> u64 {
        self.source.epoch()
    }
}

/// Persistent script namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// Per-user script root.
    User,
    /// Per-project script root.
    Project,
}

impl Namespace {
    /// Return the module id prefix for this namespace.
    fn prefix(self) -> &'static str {
        match self {
            Self::User => USER_PREFIX,
            Self::Project => PROJECT_PREFIX,
        }
    }

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
    /// Prefixed module id used when loading the script.
    pub(crate) module_id: ModuleId,
}

/// Filesystem-backed epoch handle for one mounted root.
#[derive(Debug)]
struct ScriptRootMount {
    /// Shared epoch handle used for explicit invalidation.
    epoch: FilesystemEpoch,
}

/// Build a canonical module id in a namespace.
fn prefixed_module_id(namespace: Namespace, inner: &str) -> ModuleId {
    ModuleId::canonicalized(&format!("{}{}", namespace.prefix(), inner))
}

/// Convert a filesystem path under a root to a prefixed module id.
fn module_id_for_root(namespace: Namespace, root: &Path, path: &Path) -> Option<ModuleId> {
    let relative = path.strip_prefix(root).ok()?;
    let module_name = module_name_from_relative(relative)?;
    Some(prefixed_module_id(namespace, &module_name))
}

/// Convert a relative Luau path to a slash-separated module name.
fn module_name_from_relative(relative: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        parts.push(name.to_str()?.to_owned());
    }
    let last = parts.last_mut()?;
    if let Some(stripped) = last.strip_suffix(LUAU_EXTENSION) {
        *last = stripped.to_owned();
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_id_for_path_maps_configured_roots() {
        let roots = ScriptModuleRoots::new()
            .with_user_root("tmp/canopy-user")
            .with_project_root("tmp/work/.canopy");

        assert_eq!(
            roots.module_id_for_path(Path::new("tmp/canopy-user/keymap.luau")),
            Some(ModuleId::canonicalized("@user/keymap"))
        );
        assert_eq!(
            roots.module_id_for_path(Path::new("tmp/work/.canopy/nested/init.luau")),
            Some(ModuleId::canonicalized("@project/nested/init"))
        );
        assert_eq!(
            roots.module_id_for_path(Path::new("tmp/elsewhere/init.luau")),
            None
        );
    }

    #[test]
    fn composite_source_requires_explicit_roots_for_root_imports() {
        let source = ScriptModuleRoots::new()
            .with_user_root("tmp/canopy-user")
            .module_source()
            .expect("source");

        let error = poll_ready_once(source.resolve(None, b"keymap"), "resolving")
            .expect_err("bare root imports are rejected");
        assert!(matches!(error, ModuleSourceError::MissingModule { .. }));
    }
}
