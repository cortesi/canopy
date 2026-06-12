use std::{
    path::{Component, Path, PathBuf},
    str,
    sync::Arc,
};

use oxau::{
    fs::{FilesystemModuleSource, FilesystemSourceEpoch},
    source::{
        ModuleId, ModuleSource, ModuleSourceError, ModuleSourceFuture, ModuleSourceMetadata,
        poll_ready_once, ready,
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
    pub(crate) fn module_source(&self) -> Option<Arc<CanopyModuleSource>> {
        (self.user.is_some() || self.project.is_some()).then(|| {
            Arc::new(CanopyModuleSource {
                user: self.user.clone().map(ModuleRootSource::new),
                project: self.project.clone().map(ModuleRootSource::new),
            })
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

/// Canopy's composite Luau module source for persistent script roots.
#[derive(Debug)]
pub struct CanopyModuleSource {
    /// Root backing `@user`.
    user: Option<ModuleRootSource>,
    /// Root backing `@project`.
    project: Option<ModuleRootSource>,
}

impl CanopyModuleSource {
    /// Return the configured `@user` root.
    #[must_use]
    pub fn user_root(&self) -> Option<&Path> {
        self.user.as_ref().map(ModuleRootSource::root)
    }

    /// Return the configured `@project` root.
    #[must_use]
    pub fn project_root(&self) -> Option<&Path> {
        self.project.as_ref().map(ModuleRootSource::root)
    }

    /// Invalidate all configured roots and return the composite epoch.
    pub fn invalidate(&self) -> u64 {
        if let Some(root) = &self.user {
            root.epoch.bump();
        }
        if let Some(root) = &self.project {
            root.epoch.bump();
        }
        self.composite_epoch()
    }

    /// Invalidate the `@user` root and return the composite epoch.
    pub fn invalidate_user(&self) -> Option<u64> {
        self.user.as_ref().map(|root| {
            root.epoch.bump();
            self.composite_epoch()
        })
    }

    /// Invalidate the `@project` root and return the composite epoch.
    pub fn invalidate_project(&self) -> Option<u64> {
        self.project.as_ref().map(|root| {
            root.epoch.bump();
            self.composite_epoch()
        })
    }

    /// Return the mounted source for a namespace.
    fn root(&self, namespace: Namespace) -> Result<&ModuleRootSource, ModuleSourceError> {
        match namespace {
            Namespace::User => self.user.as_ref(),
            Namespace::Project => self.project.as_ref(),
        }
        .ok_or_else(|| ModuleSourceError::MissingModule {
            id: ModuleId::from(namespace.name()),
        })
    }

    /// Resolve a module request across the configured root namespace sources.
    fn resolve_root(
        &self,
        requester: Option<&ModuleId>,
        request: &[u8],
    ) -> Result<ModuleId, ModuleSourceError> {
        let request_text = str::from_utf8(request).map_err(|error| {
            ModuleSourceError::other(format!("module request is not UTF-8: {error}"))
        })?;
        if let Some((namespace, inner)) = split_prefixed(request_text) {
            return self.resolve_in(namespace, None, inner);
        }
        let Some((namespace, requester_inner)) = requester.and_then(prefixed_id_parts) else {
            if is_relative_request(request_text) {
                return Err(ModuleSourceError::UnresolvableRelativeRequest {
                    request: request.to_vec(),
                });
            }
            return Err(ModuleSourceError::MissingModule {
                id: ModuleId::new(request.to_vec()),
            });
        };
        let requester_id = ModuleId::canonicalized(&requester_inner);
        self.resolve_in(namespace, Some(&requester_id), request_text)
    }

    /// Resolve one request inside a namespace, optionally relative to a requester.
    fn resolve_in(
        &self,
        namespace: Namespace,
        requester: Option<&ModuleId>,
        request: &str,
    ) -> Result<ModuleId, ModuleSourceError> {
        if request.is_empty() {
            return Err(ModuleSourceError::MissingModule {
                id: prefixed_module_id(namespace, request),
            });
        }
        let root = self.root(namespace)?;
        let id = poll_ready_once(
            root.source.resolve(requester, request.as_bytes()),
            "resolving canopy module",
        )?;
        prefix_resolved_id(namespace, &id)
    }

    /// Read a prefixed module id from its backing namespace source.
    fn read_root(&self, id: &ModuleId) -> Result<Vec<u8>, ModuleSourceError> {
        let (namespace, inner) = prefixed_id_parts(id)
            .ok_or_else(|| ModuleSourceError::MissingModule { id: id.clone() })?;
        let root = self.root(namespace)?;
        let inner_id = ModuleId::canonicalized(&inner);
        poll_ready_once(root.source.read(&inner_id), "reading canopy module")
    }

    /// Return diagnostic metadata for a prefixed module id.
    fn metadata_root(&self, id: &ModuleId) -> ModuleSourceMetadata {
        let Some((namespace, inner)) = prefixed_id_parts(id) else {
            return ModuleSourceMetadata::new(id.to_diagnostic_string());
        };
        let Ok(root) = self.root(namespace) else {
            return ModuleSourceMetadata::new(id.to_diagnostic_string());
        };
        let inner_id = ModuleId::canonicalized(&inner);
        let mut metadata = root.source.metadata(&inner_id);
        metadata.display_name = format!("{}{}", namespace.prefix(), metadata.display_name);
        metadata
    }

    /// Combine configured root epochs into one stable source epoch.
    fn composite_epoch(&self) -> u64 {
        let mut epoch = 0xcbf2_9ce4_8422_2325;
        for value in [
            self.user.as_ref().map(ModuleRootSource::epoch),
            self.project.as_ref().map(ModuleRootSource::epoch),
        ]
        .into_iter()
        .flatten()
        {
            epoch ^= value;
            epoch = epoch.wrapping_mul(0x1000_0000_01b3);
        }
        epoch
    }
}

impl ModuleSource for CanopyModuleSource {
    fn resolve(
        &self,
        requester: Option<&ModuleId>,
        request: &[u8],
    ) -> ModuleSourceFuture<ModuleId> {
        ready(self.resolve_root(requester, request))
    }

    fn read(&self, id: &ModuleId) -> ModuleSourceFuture<Vec<u8>> {
        ready(self.read_root(id))
    }

    fn metadata(&self, id: &ModuleId) -> ModuleSourceMetadata {
        self.metadata_root(id)
    }

    fn epoch(&self) -> u64 {
        self.composite_epoch()
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

/// Filesystem-backed module source and epoch handle for one root.
#[derive(Debug)]
struct ModuleRootSource {
    /// Mounted root directory.
    root: PathBuf,
    /// Oxau filesystem source for this root.
    source: FilesystemModuleSource,
    /// Shared epoch handle used for explicit invalidation.
    epoch: FilesystemSourceEpoch,
}

impl ModuleRootSource {
    /// Construct a root source from a filesystem path.
    fn new(root: PathBuf) -> Self {
        let source = FilesystemModuleSource::new(root.clone());
        let epoch = source.epoch_handle();
        Self {
            root,
            source,
            epoch,
        }
    }

    /// Return the mounted filesystem root.
    fn root(&self) -> &Path {
        &self.root
    }

    /// Return the current root epoch.
    fn epoch(&self) -> u64 {
        self.epoch.get()
    }
}

/// Split a request or id into a known namespace and unprefixed module name.
fn split_prefixed(request: &str) -> Option<(Namespace, &str)> {
    request
        .strip_prefix(USER_PREFIX)
        .map(|inner| (Namespace::User, inner))
        .or_else(|| {
            request
                .strip_prefix(PROJECT_PREFIX)
                .map(|inner| (Namespace::Project, inner))
        })
}

/// Split a module id into a known namespace and unprefixed module name.
fn prefixed_id_parts(id: &ModuleId) -> Option<(Namespace, String)> {
    id.as_str()
        .and_then(split_prefixed)
        .map(|(namespace, inner)| (namespace, inner.to_owned()))
}

/// Prefix an id returned by a namespace source.
fn prefix_resolved_id(namespace: Namespace, id: &ModuleId) -> Result<ModuleId, ModuleSourceError> {
    let inner = id.as_str().ok_or_else(|| {
        ModuleSourceError::other(format!(
            "resolved module id '{}' is not UTF-8",
            id.to_diagnostic_string()
        ))
    })?;
    Ok(prefixed_module_id(namespace, inner))
}

/// Build a canonical module id in a namespace.
fn prefixed_module_id(namespace: Namespace, inner: &str) -> ModuleId {
    ModuleId::canonicalized(&format!("{}{}", namespace.prefix(), inner))
}

/// Return true when a module request is relative to the requester.
fn is_relative_request(request: &str) -> bool {
    request.starts_with("./") || request.starts_with("../")
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
            Some(ModuleId::from("@user/keymap"))
        );
        assert_eq!(
            roots.module_id_for_path(Path::new("tmp/work/.canopy/nested/init.luau")),
            Some(ModuleId::from("@project/nested/init"))
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
