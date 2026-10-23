//! The layered store, its handles, and change notification.

use std::{
    any::Any,
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt, mem,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
    result::Result as StdResult,
    sync::Arc,
};

use canopy::{
    Context,
    error::Result as CanopyResult,
    runtime::{NodeWakeHandle, NoticePost, WakeOutcome},
};

use crate::{
    file::{self, FileLayer, Scope},
    layout::Layout,
    model::{OptionGroup, Schema, SetError, Value, join_path},
};

/// The root value as the handles see it.
pub type AnyRoot = dyn Any + Send + Sync;

/// A projection from the root to a group.
type View<G> = Arc<dyn for<'a> Fn(&'a AnyRoot) -> &'a G + Send + Sync>;

/// A mutable projection from the root to a group.
type ViewMut<G> = Rc<dyn for<'a> Fn(&'a mut dyn Any) -> &'a mut G>;

/// A callback that a subscriber registers, which gets the root.
type RootCallback = Box<dyn FnMut(&mut dyn Context, &AnyRoot, &Changes) -> CanopyResult<()>>;

/// A write to the root.
type RootWrite = Box<dyn FnOnce(&mut dyn Any)>;

/// The paths of the leaves that one change touched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The changed leaf paths.
    paths: Vec<String>,
}

impl Changes {
    /// Return whether no leaf changed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Return whether the leaf at `path` changed.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.paths.iter().any(|changed| changed == path)
    }

    /// Return whether a leaf at or below `prefix` changed. An empty prefix
    /// matches every change.
    #[must_use]
    pub fn touches(&self, prefix: &str) -> bool {
        self.paths.iter().any(|path| below(path, prefix))
    }

    /// Return the changed leaf paths.
    #[must_use]
    pub fn paths(&self) -> &[String] {
        &self.paths
    }
}

/// Return whether `path` is `prefix` or a path below it. An empty prefix
/// holds every path.
fn below(path: &str, prefix: &str) -> bool {
    prefix.is_empty()
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Work that waits for a notification round to end.
enum Deferred {
    /// A write to the root.
    Write(RootWrite),
    /// A change that a layer operation made, such as a revert or a save.
    Notify(Changes),
}

/// The layer that sets the effective value of a leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The declaration, or a default that a script set.
    Default,
    /// An option file.
    File {
        /// The file.
        path: PathBuf,
        /// The scope of the file.
        scope: Scope,
    },
    /// A live value of this run.
    Live,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Live => f.write_str("live"),
            Self::File { path, .. } => write!(f, "{}", path.display()),
        }
    }
}

/// A store error that the options panel and scripts report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// A value cannot go into a path.
    Set {
        /// The path.
        path: String,
        /// The reason.
        error: SetError,
    },
    /// A save or a clear has no file to write.
    NoFile(Scope),
    /// A file cannot be read or written.
    File(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set { path, error } => write!(f, "{path}: {error}"),
            Self::NoFile(Scope::Global) => f.write_str("no global options file"),
            Self::NoFile(Scope::Local) => f.write_str("no local directory"),
            Self::File(message) => f.write_str(message),
        }
    }
}

impl Error for StoreError {}

/// A subscriber: a path prefix, and its callback.
#[derive(Clone)]
struct Subscriber {
    /// The prefix of the paths the subscriber hears about.
    prefix: String,
    /// The callback.
    callback: Rc<RefCell<RootCallback>>,
}

/// The state of a store.
struct Inner<T> {
    /// The schema of the root.
    schema: Rc<Schema>,
    /// The defaults that scripts set.
    script_defaults: BTreeMap<String, Value>,
    /// The global file, which may not exist yet.
    global: Option<FileLayer>,
    /// The local files, from the outermost to the nearest.
    locals: Vec<FileLayer>,
    /// The file that a local save writes.
    local_target: Option<PathBuf>,
    /// The live values of this run.
    live: BTreeMap<String, Value>,
    /// The merged defaults and files.
    saved: Arc<T>,
    /// The saved value with the live values.
    current: Arc<T>,
    /// The subscribers.
    subscribers: Vec<Subscriber>,
    /// The widgets to poll as the options panel closes.
    close_wakes: Vec<NodeWakeHandle>,
    /// Whether subscribers are being notified.
    notifying: bool,
    /// Work that arrived during a notification.
    queue: VecDeque<Deferred>,
    /// Warnings that wait for a context to show them.
    warnings: Vec<String>,
}

impl<T: OptionGroup> Inner<T> {
    /// Return the file layers, from the lowest precedence to the highest.
    fn files(&self) -> impl Iterator<Item = &FileLayer> {
        self.global.iter().chain(self.locals.iter())
    }

    /// Merge the defaults and the files that precede the layer at `index` in
    /// [`Self::files`]. An index past the end merges every file.
    fn merged_below(&self, index: usize) -> T {
        let mut value = T::default();
        for (path, default) in &self.script_defaults {
            value.set(path, default).ok();
        }
        for layer in self.files().take(index) {
            for (path, file_value) in &layer.values {
                value.set(path, file_value).ok();
            }
        }
        value
    }

    /// Rebuild the saved and current values from the layers, and return the
    /// paths whose current value changed.
    fn rebuild(&mut self) -> Changes {
        let saved = self.merged_below(usize::MAX);
        let mut current = saved.clone();
        let live = mem::take(&mut self.live);
        for (path, value) in live {
            if current.set(&path, &value).is_ok() && saved.get(&path).as_ref() != Some(&value) {
                self.live.insert(path, value);
            }
        }
        let mut paths = Vec::new();
        self.current.changed(&current, "", &mut paths);
        self.saved = Arc::new(saved);
        self.current = Arc::new(current);
        Changes { paths }
    }

    /// Make `next` the current value, record its changed leaves as live
    /// values, and return them.
    ///
    /// Each changed leaf must pass the checks of its path, or nothing
    /// changes.
    fn commit(&mut self, next: T) -> StdResult<Changes, StoreError> {
        let mut paths = Vec::new();
        self.current.changed(&next, "", &mut paths);
        let mut probe = (*self.current).clone();
        for path in &paths {
            let value = next.get(path).unwrap_or(Value::Unset);
            probe.set(path, &value).map_err(|error| StoreError::Set {
                path: path.clone(),
                error,
            })?;
        }
        for path in &paths {
            let value = next.get(path).unwrap_or(Value::Unset);
            if self.saved.get(path).as_ref() == Some(&value) {
                self.live.remove(path);
            } else {
                self.live.insert(path.clone(), value);
            }
        }
        self.current = Arc::new(next);
        Ok(Changes { paths })
    }

    /// Return the index in [`Self::files`] of the file for `scope`, adding
    /// an empty layer when the file is not loaded yet.
    fn target_index(&mut self, scope: Scope) -> StdResult<usize, StoreError> {
        match scope {
            Scope::Global => {
                if self.global.is_none() {
                    return Err(StoreError::NoFile(Scope::Global));
                }
                Ok(0)
            }
            Scope::Local => {
                let target = self
                    .local_target
                    .clone()
                    .ok_or(StoreError::NoFile(Scope::Local))?;
                let offset = usize::from(self.global.is_some());
                if let Some(index) = self.locals.iter().position(|layer| layer.path == target) {
                    return Ok(offset + index);
                }
                self.locals.push(FileLayer::empty(target, Scope::Local));
                Ok(offset + self.locals.len() - 1)
            }
        }
    }

    /// Return the layer at `index` in [`Self::files`].
    fn layer_mut(&mut self, index: usize) -> &mut FileLayer {
        if let Some(global) = &mut self.global {
            if index == 0 {
                return global;
            }
            return &mut self.locals[index - 1];
        }
        &mut self.locals[index]
    }
}

/// The shared state of a store, which the handles and the widget reach.
pub struct Shared<T> {
    /// The state.
    inner: RefCell<Inner<T>>,
}

/// A layered store of the options of type `T`.
///
/// The store merges the defaults, the global file, the local files, and the
/// live values of the run. Clones of a store share its state.
pub struct Store<T> {
    /// The shared state.
    shared: Rc<Shared<T>>,
}

impl<T> Clone for Store<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl<T: OptionGroup + Send + Sync> Default for Store<T> {
    fn default() -> Self {
        Self::load(&Layout::default())
    }
}

impl<T: OptionGroup + Send + Sync> Store<T> {
    /// Load a store from the files of `layout`.
    ///
    /// Entries that do not load become warnings, which the options widget
    /// shows once it mounts.
    #[must_use]
    pub fn load(layout: &Layout) -> Self {
        let schema = Rc::new(Schema::of::<T>());
        let mut warnings: Vec<String> = schema
            .key_problems()
            .into_iter()
            .map(|problem| format!("options schema: {problem}"))
            .collect();
        let global = layout.global.as_deref().map(|path| {
            let (layer, mut problems) = file::load_layer::<T>(&schema, path, Scope::Global);
            warnings.append(&mut problems);
            layer
        });
        let locals = layout
            .locals
            .iter()
            .map(|path| {
                let (layer, mut problems) = file::load_layer::<T>(&schema, path, Scope::Local);
                warnings.append(&mut problems);
                layer
            })
            .collect();
        let mut inner = Inner {
            schema,
            script_defaults: BTreeMap::new(),
            global,
            locals,
            local_target: layout.local_target.clone(),
            live: BTreeMap::new(),
            saved: Arc::new(T::default()),
            current: Arc::new(T::default()),
            subscribers: Vec::new(),
            close_wakes: Vec::new(),
            notifying: false,
            queue: VecDeque::new(),
            warnings,
        };
        inner.rebuild();
        Self {
            shared: Rc::new(Shared {
                inner: RefCell::new(inner),
            }),
        }
    }

    /// Return a handle to the whole root.
    #[must_use]
    pub fn handle(&self) -> OptionsHandle<T> {
        OptionsHandle {
            backend: Rc::clone(&self.shared) as Rc<dyn Backend>,
            prefix: String::new(),
            view: Arc::new(root_view::<T>),
            view_mut: Rc::new(root_view_mut::<T>),
        }
    }

    /// Return the dynamic interface of the store, for the options widget.
    #[must_use]
    pub(crate) fn backend(&self) -> Rc<dyn Backend> {
        Rc::clone(&self.shared) as Rc<dyn Backend>
    }

    /// Return the current value.
    #[must_use]
    pub fn current(&self) -> Arc<T> {
        Arc::clone(&self.shared.inner.borrow().current)
    }
}

/// Project the root.
fn root_view<T: 'static>(root: &AnyRoot) -> &T {
    root.downcast_ref::<T>()
        .expect("an options handle projects its own root type")
}

/// Project the root for a write.
fn root_view_mut<T: 'static>(root: &mut dyn Any) -> &mut T {
    root.downcast_mut::<T>()
        .expect("an options handle projects its own root type")
}

/// The dynamic interface of a store, which hides its root type.
pub trait Backend {
    /// Return the schema of the root.
    fn schema(&self) -> Rc<Schema>;

    /// Return the current root.
    fn root(&self) -> Arc<AnyRoot>;

    /// Apply a typed write to the root, then notify the subscribers.
    fn update_root(&self, context: &mut dyn Context, write: RootWrite) -> CanopyResult<()>;

    /// Register a subscriber for the paths at or below `prefix`.
    fn subscribe_root(&self, prefix: String, callback: RootCallback);

    /// Poll the widget of `wake` as the options panel closes.
    fn wake_on_close(&self, wake: NodeWakeHandle);

    /// Poll the widgets that asked to hear the panel close.
    fn panel_closed(&self);

    /// Return the current value of the leaf at `path`.
    fn get(&self, path: &str) -> Option<Value>;

    /// Return the saved value of the leaf at `path`.
    #[cfg(test)]
    fn saved(&self, path: &str) -> Option<Value>;

    /// Write a live value to the leaf at `path`.
    fn set(
        &self,
        context: &mut dyn Context,
        path: &str,
        value: Value,
    ) -> CanopyResult<StdResult<(), StoreError>>;

    /// Write a default to the leaf at `path`.
    fn set_default(
        &self,
        context: &mut dyn Context,
        path: &str,
        value: Value,
    ) -> CanopyResult<StdResult<(), StoreError>>;

    /// Remove the live value of the leaf at `path`, or every live value.
    fn revert(&self, context: &mut dyn Context, path: Option<&str>) -> CanopyResult<()>;

    /// Remove the leaf at `path` from the nearest file that sets it, and
    /// return that file.
    fn clear(
        &self,
        context: &mut dyn Context,
        path: &str,
    ) -> CanopyResult<StdResult<Option<PathBuf>, StoreError>>;

    /// Save the live values to the file for `scope`, and return the file.
    fn save(
        &self,
        context: &mut dyn Context,
        scope: Scope,
    ) -> CanopyResult<StdResult<PathBuf, StoreError>>;

    /// Return the file that a save for `scope` writes.
    fn save_target(&self, scope: Scope) -> Option<PathBuf>;

    /// Return the layer that sets the effective value of the leaf at `path`.
    fn source(&self, path: &str) -> Source;

    /// Return whether a leaf at or below `prefix` has a live value.
    fn has_live_at(&self, prefix: &str) -> bool;

    /// Return the warnings that wait to show, and clear them.
    fn take_warnings(&self) -> Vec<String>;
}

impl<T: OptionGroup + Send + Sync> Shared<T> {
    /// Notify the subscribers of `changes`, then run the writes that arrived
    /// during the notification.
    ///
    /// Every subscriber runs, also after a failure. The failures become one
    /// error notice.
    fn notify(&self, context: &mut dyn Context, changes: &Changes) -> CanopyResult<()> {
        // A notification that runs inside another waits for it, so that each
        // callback sees one change at a time.
        if self.inner.borrow().notifying {
            self.inner
                .borrow_mut()
                .queue
                .push_back(Deferred::Notify(changes.clone()));
            return Ok(());
        }
        let mut failures = Vec::new();
        if !changes.is_empty() {
            let (subscribers, current) = {
                let mut inner = self.inner.borrow_mut();
                inner.notifying = true;
                (inner.subscribers.clone(), Arc::clone(&inner.current))
            };
            for subscriber in subscribers {
                if !changes.touches(&subscriber.prefix) {
                    continue;
                }
                let Ok(mut callback) = subscriber.callback.try_borrow_mut() else {
                    continue;
                };
                let root: &AnyRoot = &*current;
                if let Err(error) = callback(context, root, changes) {
                    failures.push(error.to_string());
                }
            }
            self.inner.borrow_mut().notifying = false;
        }
        loop {
            let deferred = self.inner.borrow_mut().queue.pop_front();
            match deferred {
                None => break,
                Some(Deferred::Notify(queued)) => self.notify(context, &queued)?,
                Some(Deferred::Write(write)) => match self.apply(write) {
                    Ok(queued) => self.notify(context, &queued)?,
                    Err(error) => failures.push(error.to_string()),
                },
            }
        }
        if !failures.is_empty() {
            context.notify(NoticePost::error(format!(
                "options: {}",
                failures.join("; ")
            )))?;
        }
        Ok(())
    }

    /// Run `write` on a copy of the current value, then commit it.
    ///
    /// The store is not borrowed while `write` runs, so the write can read
    /// the store.
    fn apply(&self, write: RootWrite) -> StdResult<Changes, StoreError> {
        let mut next = (*self.inner.borrow().current).clone();
        write(&mut next);
        self.inner.borrow_mut().commit(next)
    }

    /// Rebuild the values from the layers and notify the changes.
    fn rebuild_and_notify(&self, context: &mut dyn Context) -> CanopyResult<()> {
        let changes = self.inner.borrow_mut().rebuild();
        self.notify(context, &changes)
    }

    /// Return `path` if it names a leaf, or an error.
    fn leaf_path(&self, path: &str) -> StdResult<String, StoreError> {
        if self.inner.borrow().schema.leaf(path).is_some() {
            Ok(path.to_string())
        } else {
            Err(StoreError::Set {
                path: path.to_string(),
                error: SetError::UnknownPath,
            })
        }
    }

    /// Write `edits` to the file at `path`, reading it first so that the
    /// rest of the file stays.
    fn edit_file(path: &Path, edits: &[(String, Option<Value>)]) -> StdResult<(), StoreError> {
        let mut document = file::read_document(path)
            .map_err(StoreError::File)?
            .unwrap_or_default();
        file::apply_edits(&mut document, edits);
        file::write_atomic(path, &document.to_string())
            .map_err(|error| StoreError::File(format!("{}: {error}", path.display())))
    }
}

impl<T: OptionGroup + Send + Sync> Backend for Shared<T> {
    fn schema(&self) -> Rc<Schema> {
        Rc::clone(&self.inner.borrow().schema)
    }

    fn root(&self) -> Arc<AnyRoot> {
        Arc::clone(&self.inner.borrow().current) as Arc<AnyRoot>
    }

    fn update_root(&self, context: &mut dyn Context, write: RootWrite) -> CanopyResult<()> {
        if self.inner.borrow().notifying {
            self.inner
                .borrow_mut()
                .queue
                .push_back(Deferred::Write(write));
            return Ok(());
        }
        match self.apply(write) {
            Ok(changes) => self.notify(context, &changes),
            Err(error) => context.notify(NoticePost::error(format!("options: {error}"))),
        }
    }

    fn subscribe_root(&self, prefix: String, callback: RootCallback) {
        self.inner.borrow_mut().subscribers.push(Subscriber {
            prefix,
            callback: Rc::new(RefCell::new(callback)),
        });
    }

    fn wake_on_close(&self, wake: NodeWakeHandle) {
        self.inner.borrow_mut().close_wakes.push(wake);
    }

    fn panel_closed(&self) {
        // A widget that is gone cannot be woken, so it is forgotten.
        self.inner
            .borrow_mut()
            .close_wakes
            .retain(|wake| !matches!(wake.wake(), Ok(WakeOutcome::Expired) | Err(_)));
    }

    fn get(&self, path: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        inner.schema.leaf(path)?;
        inner.current.get(path)
    }

    #[cfg(test)]
    fn saved(&self, path: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        inner.schema.leaf(path)?;
        inner.saved.get(path)
    }

    fn set(
        &self,
        context: &mut dyn Context,
        path: &str,
        value: Value,
    ) -> CanopyResult<StdResult<(), StoreError>> {
        let path = match self.leaf_path(path) {
            Ok(path) => path,
            Err(error) => return Ok(Err(error)),
        };
        let mut probe = (*self.inner.borrow().current).clone();
        if let Err(error) = probe.set(&path, &value) {
            return Ok(Err(StoreError::Set { path, error }));
        }
        let write: RootWrite = Box::new(move |root| {
            root_view_mut::<T>(root).set(&path, &value).ok();
        });
        self.update_root(context, write).map(Ok)
    }

    fn set_default(
        &self,
        context: &mut dyn Context,
        path: &str,
        value: Value,
    ) -> CanopyResult<StdResult<(), StoreError>> {
        let path = match self.leaf_path(path) {
            Ok(path) => path,
            Err(error) => return Ok(Err(error)),
        };
        let mut probe = T::default();
        if let Err(error) = probe.set(&path, &value) {
            return Ok(Err(StoreError::Set { path, error }));
        }
        self.inner.borrow_mut().script_defaults.insert(path, value);
        self.rebuild_and_notify(context).map(Ok)
    }

    fn revert(&self, context: &mut dyn Context, path: Option<&str>) -> CanopyResult<()> {
        {
            let mut inner = self.inner.borrow_mut();
            match path {
                Some(path) => {
                    if inner.schema.leaf(path).is_some() {
                        inner.live.remove(path);
                    } else {
                        inner.live.retain(|live, _| !below(live, path));
                    }
                }
                None => inner.live.clear(),
            }
        }
        self.rebuild_and_notify(context)
    }

    fn clear(
        &self,
        context: &mut dyn Context,
        path: &str,
    ) -> CanopyResult<StdResult<Option<PathBuf>, StoreError>> {
        let path = match self.leaf_path(path) {
            Ok(path) => path,
            Err(error) => return Ok(Err(error)),
        };
        let found = {
            let inner = self.inner.borrow();
            inner
                .files()
                .enumerate()
                .filter(|(_, layer)| layer.values.contains_key(&path))
                .map(|(index, layer)| (index, layer.path.clone()))
                .last()
        };
        let Some((index, file)) = found else {
            return Ok(Ok(None));
        };
        if let Err(error) = Self::edit_file(&file, &[(path.clone(), None)]) {
            return Ok(Err(error));
        }
        self.inner
            .borrow_mut()
            .layer_mut(index)
            .values
            .remove(&path);
        self.rebuild_and_notify(context)?;
        Ok(Ok(Some(file)))
    }

    fn save(
        &self,
        context: &mut dyn Context,
        scope: Scope,
    ) -> CanopyResult<StdResult<PathBuf, StoreError>> {
        let (file, edits, index) = {
            let mut inner = self.inner.borrow_mut();
            let index = match inner.target_index(scope) {
                Ok(index) => index,
                Err(error) => return Ok(Err(error)),
            };
            let below = inner.merged_below(index);
            let file = inner.layer_mut(index).path.clone();
            let mut edits = Vec::new();
            for (path, value) in &inner.live {
                let global_only = inner.schema.leaf(path).is_some_and(|leaf| leaf.global);
                if scope == Scope::Local && global_only {
                    continue;
                }
                let lower = below.get(path);
                // A file cannot unset a value that a layer below sets, so
                // such a value stays live.
                if *value == Value::Unset && lower.as_ref() != Some(&Value::Unset) {
                    continue;
                }
                let entry = (lower.as_ref() != Some(value)).then(|| value.clone());
                edits.push((path.clone(), entry));
            }
            (file, edits, index)
        };
        if edits.is_empty() {
            return Ok(Ok(file));
        }
        if let Err(error) = Self::edit_file(&file, &edits) {
            return Ok(Err(error));
        }
        {
            let mut inner = self.inner.borrow_mut();
            let layer = inner.layer_mut(index);
            for (path, value) in &edits {
                match value {
                    Some(value) => {
                        layer.values.insert(path.clone(), value.clone());
                    }
                    None => {
                        layer.values.remove(path);
                    }
                }
            }
        }
        self.rebuild_and_notify(context)?;
        Ok(Ok(file))
    }

    fn save_target(&self, scope: Scope) -> Option<PathBuf> {
        let inner = self.inner.borrow();
        match scope {
            Scope::Global => inner.global.as_ref().map(|layer| layer.path.clone()),
            Scope::Local => inner.local_target.clone(),
        }
    }

    fn source(&self, path: &str) -> Source {
        let inner = self.inner.borrow();
        if inner.schema.leaf(path).is_none() {
            return Source::Default;
        }
        if inner.live.contains_key(path) {
            return Source::Live;
        }
        inner
            .files()
            .filter(|layer| layer.values.contains_key(path))
            .last()
            .map_or(Source::Default, |layer| Source::File {
                path: layer.path.clone(),
                scope: layer.scope,
            })
    }

    fn has_live_at(&self, prefix: &str) -> bool {
        self.inner
            .borrow()
            .live
            .keys()
            .any(|path| below(path, prefix))
    }

    fn take_warnings(&self) -> Vec<String> {
        mem::take(&mut self.inner.borrow_mut().warnings)
    }
}

/// A snapshot of one group of the options, which keeps the root alive.
///
/// A snapshot is `Send`, so a worker thread can keep it.
pub struct Snapshot<G> {
    /// The root.
    root: Arc<AnyRoot>,
    /// The projection to the group.
    view: View<G>,
}

impl<G> Clone for Snapshot<G> {
    fn clone(&self) -> Self {
        Self {
            root: Arc::clone(&self.root),
            view: Arc::clone(&self.view),
        }
    }
}

impl<G> Deref for Snapshot<G> {
    type Target = G;

    fn deref(&self) -> &G {
        (self.view)(&*self.root)
    }
}

impl<G: fmt::Debug> fmt::Debug for Snapshot<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

/// A handle to one group of a store.
///
/// The handle hides the root type of the store, so a reusable widget can
/// take a handle to its own group. Clones share the store.
pub struct OptionsHandle<G> {
    /// The store.
    backend: Rc<dyn Backend>,
    /// The path of the group.
    prefix: String,
    /// The projection to the group.
    view: View<G>,
    /// The projection to the group, for writes.
    view_mut: ViewMut<G>,
}

impl<G> Clone for OptionsHandle<G> {
    fn clone(&self) -> Self {
        Self {
            backend: Rc::clone(&self.backend),
            prefix: self.prefix.clone(),
            view: Arc::clone(&self.view),
            view_mut: Rc::clone(&self.view_mut),
        }
    }
}

impl<G: 'static> OptionsHandle<G> {
    /// Return a snapshot of the current value of the group.
    #[must_use]
    pub fn get(&self) -> Snapshot<G> {
        Snapshot {
            root: self.backend.root(),
            view: Arc::clone(&self.view),
        }
    }

    /// Return the path of the group, which is empty for the root.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Change the group with `write`, then notify the subscribers.
    ///
    /// A write during a notification waits until the notification ends.
    pub fn update(
        &self,
        context: &mut dyn Context,
        write: impl FnOnce(&mut G) + 'static,
    ) -> CanopyResult<()> {
        let view_mut = Rc::clone(&self.view_mut);
        self.backend
            .update_root(context, Box::new(move |root| write(view_mut(root))))
    }

    /// Call `callback` after each change at or below `path`, relative to the
    /// group. An empty path hears every change of the group.
    pub fn subscribe(
        &self,
        path: &str,
        mut callback: impl FnMut(&mut dyn Context, &G, &Changes) -> CanopyResult<()> + 'static,
    ) {
        let view = Arc::clone(&self.view);
        self.backend.subscribe_root(
            join_path(&self.prefix, path),
            Box::new(move |context, root, changes| callback(context, view(root), changes)),
        );
    }

    /// Poll the widget of `wake` as the options panel closes.
    ///
    /// The panel holds the focus while it is open and gives it back as it
    /// closes, so a widget that moves the focus after a change does so at
    /// this poll.
    pub fn wake_on_close(&self, wake: NodeWakeHandle) {
        self.backend.wake_on_close(wake);
    }

    /// Return the full path of a leaf below the group.
    #[must_use]
    pub fn path(&self, path: &str) -> String {
        join_path(&self.prefix, path)
    }

    /// Return a handle to a group below this one.
    ///
    /// `name` is the field name of the group, or an empty name for a
    /// flattened group. `view` and `view_mut` project this group to it.
    #[must_use]
    pub fn narrow<H: 'static>(
        &self,
        name: &str,
        view: fn(&G) -> &H,
        view_mut: fn(&mut G) -> &mut H,
    ) -> OptionsHandle<H> {
        let outer = Arc::clone(&self.view);
        let outer_mut = Rc::clone(&self.view_mut);
        OptionsHandle {
            backend: Rc::clone(&self.backend),
            prefix: join_path(&self.prefix, name),
            view: compose_view(outer, view),
            view_mut: compose_view_mut(outer_mut, view_mut),
        }
    }

    /// Return the dynamic interface of the store.
    #[must_use]
    pub(crate) fn backend(&self) -> Rc<dyn Backend> {
        Rc::clone(&self.backend)
    }
}

/// Compose a projection with a field projection.
fn compose_view<G: 'static, H: 'static>(outer: View<G>, view: fn(&G) -> &H) -> View<H> {
    Arc::new(move |root: &AnyRoot| view(outer(root)))
}

/// Compose a write projection with a field projection.
fn compose_view_mut<G: 'static, H: 'static>(
    outer: ViewMut<G>,
    view_mut: fn(&mut G) -> &mut H,
) -> ViewMut<H> {
    Rc::new(move |root: &mut dyn Any| view_mut(outer(root)))
}
