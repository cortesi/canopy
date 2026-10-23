//! The `options` command owner, which wraps the application and opens the
//! options panel.

use std::{rc::Rc, result::Result as StdResult, time::Duration};

use canopy::{
    Context, ContextExt, NodeId, NodeName, Register, Setup, TypedId, ViewContext, Widget,
    commands::ArgValue,
    derive_commands,
    error::{Error, Result},
    input::{ModalBindings, ModalOptions, ModalToken},
    layout::Layout,
    runtime::NoticePost,
};
use canopy_widgets::menu;

use crate::{
    file::Scope,
    model::{Kind, Leaf, OptionGroup, Value},
    panel::{self, OptionsPanel},
    store::{Backend, Source, Store, StoreError},
};

/// Builds the application child when the widget mounts.
type PendingApp = Box<dyn FnOnce(&mut dyn Context, NodeId) -> Result<NodeId>>;

/// The `options` command owner.
///
/// The widget wraps the application, so that a binding anywhere in the
/// application finds the `options` commands. It also owns the options panel,
/// which it opens as a modal over the application.
pub struct Options {
    /// The store.
    backend: Rc<dyn Backend>,
    /// The application, until it mounts.
    pending: Option<PendingApp>,
    /// The application node.
    app: Option<NodeId>,
    /// The panel, in the overlay layer.
    panel: Option<TypedId<OptionsPanel>>,
    /// The open panel's modal.
    token: Option<ModalToken>,
}

#[derive_commands]
#[allow(
    clippy::needless_pass_by_value,
    reason = "command arguments are owned values"
)]
impl Options {
    /// Wrap `app`, and own the options of `store`.
    pub fn new<T, W>(store: &Store<T>, app: W) -> Self
    where
        T: OptionGroup + Send + Sync,
        W: Widget + 'static,
    {
        Self {
            backend: store.backend(),
            pending: Some(Box::new(move |context, parent| {
                Ok(context.add_child(parent, app)?.into())
            })),
            app: None,
            panel: None,
            token: None,
        }
    }

    /// Return the leaf at `path`, or an error that names the path.
    fn leaf(&self, path: &str) -> Result<Leaf> {
        self.backend
            .schema()
            .leaf(path)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("unknown option: {path}")))
    }

    /// Write a live value to the leaf `leaf` at `path`, and show a toast that
    /// names it.
    fn write(
        &self,
        context: &mut dyn Context,
        path: &str,
        leaf: &Leaf,
        value: Value,
    ) -> Result<()> {
        self.backend
            .set(context, path, value)?
            .map_err(store_error)?;
        let shown = self.backend.get(path).unwrap_or(Value::Unset);
        context.notify(NoticePost::info(format!(
            "{}: {}",
            leaf.label,
            display_value(leaf, &shown)
        )))
    }

    /// Close the panel, if it is open.
    pub(crate) fn close_panel(&mut self, context: &mut dyn Context) {
        // A modal that closed with its owner has nothing left to close.
        if let Some(token) = self.token.take() {
            context.close_modal(token).ok();
            self.backend.panel_closed();
        }
    }

    /// Return the current value of an option.
    /// @param path The dotted path of the option, such as `listing.hidden`.
    /// @return The value: a bool, an integer, a string, a list of strings, or
    /// nil for an unset option.
    #[command]
    pub fn get(&self, path: String) -> Result<ArgValue> {
        self.leaf(&path)?;
        Ok(to_arg(&self.backend.get(&path).unwrap_or(Value::Unset)))
    }

    /// Set the live value of an option for this run.
    /// @param path The dotted path of the option.
    /// @param value The new value.
    #[command]
    pub fn set(&mut self, context: &mut dyn Context, path: String, value: ArgValue) -> Result<()> {
        let leaf = self.leaf(&path)?;
        let value = from_arg(&leaf, &value).map_err(Error::Invalid)?;
        self.write(context, &path, &leaf, value)
    }

    /// Set the default of an option, below the option files. A
    /// configuration script calls this in `setup()`.
    /// @param path The dotted path of the option.
    /// @param value The new default.
    #[command]
    pub fn set_default(
        &mut self,
        context: &mut dyn Context,
        path: String,
        value: ArgValue,
    ) -> Result<()> {
        let leaf = self.leaf(&path)?;
        let value = from_arg(&leaf, &value).map_err(Error::Invalid)?;
        self.backend
            .set_default(context, &path, value)?
            .map_err(store_error)
    }

    /// Toggle a bool option.
    /// @param path The dotted path of the option.
    #[command]
    pub fn toggle(&mut self, context: &mut dyn Context, path: String) -> Result<()> {
        let leaf = self.leaf(&path)?;
        if leaf.kind != Kind::Bool {
            return Err(Error::Invalid(format!(
                "{path} is a {}, not a bool",
                leaf.kind.name()
            )));
        }
        // An unset optional bool reads as off.
        let current = self.backend.get(&path) == Some(Value::Bool(true));
        self.write(context, &path, &leaf, Value::Bool(!current))
    }

    /// Move a choice to a later or earlier variant, wrapping at the ends, or
    /// step an integer.
    /// @param path The dotted path of the option.
    /// @param delta The number of variants or steps, 1 if not given.
    #[command]
    pub fn cycle(
        &mut self,
        context: &mut dyn Context,
        path: String,
        delta: Option<i64>,
    ) -> Result<()> {
        let leaf = self.leaf(&path)?;
        let current = self.backend.get(&path).unwrap_or(Value::Unset);
        let next = step_value(&leaf, &current, delta.unwrap_or(1)).ok_or_else(|| {
            Error::Invalid(format!(
                "{path} is a {}, which does not cycle",
                leaf.kind.name()
            ))
        })?;
        self.write(context, &path, &leaf, next)
    }

    /// Remove the live value of an option, or of every option, so it has its
    /// saved value again.
    /// @param path The dotted path of an option or a group. Every option
    /// reverts if not given.
    #[command]
    pub fn revert(&mut self, context: &mut dyn Context, path: Option<String>) -> Result<()> {
        self.backend.revert(context, path.as_deref())
    }

    /// Remove an option from the nearest option file that sets it.
    /// @param path The dotted path of the option.
    /// @return The file that changed, or nil if no file sets the option.
    #[command]
    pub fn clear(&mut self, context: &mut dyn Context, path: String) -> Result<Option<String>> {
        self.leaf(&path)?;
        let file = self.backend.clear(context, &path)?.map_err(store_error)?;
        Ok(file.map(|file| file.display().to_string()))
    }

    /// Save the live values to the global or the local option file.
    /// @param scope "global" or "local".
    /// @return The file that the save wrote.
    #[command]
    pub fn save(&mut self, context: &mut dyn Context, scope: String) -> Result<String> {
        let scope = parse_scope(&scope)?;
        let file = self.backend.save(context, scope)?.map_err(store_error)?;
        Ok(file.display().to_string())
    }

    /// Return the layer that sets the value of an option.
    /// @param path The dotted path of the option.
    /// @return "default", "live", or the path of an option file.
    #[command]
    pub fn source(&self, path: String) -> Result<String> {
        self.leaf(&path)?;
        Ok(self.backend.source(&path).to_string())
    }

    /// Open the options panel.
    /// @param path A group to open at, the root if not given.
    /// @param quick Whether a change closes the panel. A set, an order, or a
    /// list closes it as the user leaves its level.
    #[command]
    pub fn open(
        &mut self,
        context: &mut dyn Context,
        path: Option<String>,
        quick: Option<bool>,
    ) -> Result<()> {
        let panel = self
            .panel
            .ok_or_else(|| Error::Invalid("the options panel is not mounted".into()))?;
        let app = self.app;
        let group = path.unwrap_or_default();
        if self.backend.schema().group(&group).is_none() {
            return Err(Error::Invalid(format!("unknown option group: {group}")));
        }
        let host = context.node_id();
        let body = context.with_widget_mut(panel, |panel: &mut OptionsPanel, context| {
            panel.reset(context, host, &group, quick.unwrap_or(false))
        })?;
        // An open panel shows the new group in place. A modal that closed
        // with another one that owned it is gone already.
        if let Some(token) = self.token {
            if context.modal_is_open(token) {
                context.set_focus(body.into())?;
                return Ok(());
            }
            self.token = None;
        }
        let options = |owner| ModalOptions {
            owner,
            modal: panel.into(),
            initial_focus: body.into(),
            dim_target: app,
            bindings: ModalBindings::Framework {
                groups: panel::BINDING_GROUPS,
                intents: &[],
            },
        };
        // Another modal, such as an inspector, admits only owners inside
        // it. The panel then opens over the focused node, and closes with
        // that modal if it closes first.
        let token = match context.open_modal(options(host)) {
            Ok(token) => token,
            Err(error) => match context.focused_node().filter(|focus| *focus != host) {
                Some(focus) => context.open_modal(options(focus))?,
                None => return Err(error),
            },
        };
        self.token = Some(token);
        Ok(())
    }

    /// Close the options panel.
    #[command]
    pub fn close(&mut self, context: &mut dyn Context) {
        self.close_panel(context);
    }

    /// Return whether the options panel is open.
    #[command]
    #[must_use]
    pub fn is_open(&self, context: &dyn Context) -> bool {
        self.token.is_some_and(|token| context.modal_is_open(token))
    }
}

impl Widget for Options {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| Error::Invalid("the options host mounts once".into()))?;
        let host = context.node_id();
        self.app = Some(pending(context, host)?);
        self.panel = Some(context.add_modal(OptionsPanel::new(Rc::clone(&self.backend)))?);
        context.request_poll()
    }

    fn poll(&mut self, context: &mut dyn Context) -> Result<Option<Duration>> {
        for warning in self.backend.take_warnings() {
            context.notify(NoticePost::warning(warning))?;
        }
        Ok(None)
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        false
    }

    fn name(&self) -> NodeName {
        NodeName::convert("options")
    }
}

impl Register for Options {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        setup.widget_styles(menu::styles);
        panel::register(setup)
    }
}

/// Convert a store error into a command error.
#[allow(clippy::needless_pass_by_value, reason = "it maps `Result` errors")]
fn store_error(error: StoreError) -> Error {
    Error::Invalid(error.to_string())
}

/// Parse the name of a save scope.
fn parse_scope(scope: &str) -> Result<Scope> {
    match scope.to_ascii_lowercase().as_str() {
        "global" => Ok(Scope::Global),
        "local" => Ok(Scope::Local),
        other => Err(Error::Invalid(format!(
            "unknown scope {other:?}, expected \"global\" or \"local\""
        ))),
    }
}

/// Convert a value into a script value.
#[must_use]
pub fn to_arg(value: &Value) -> ArgValue {
    match value {
        Value::Bool(value) => ArgValue::Bool(*value),
        Value::Int(value) => ArgValue::Int(*value),
        Value::Text(text) | Value::Choice(text) => ArgValue::String(text.clone()),
        Value::Set(items) | Value::List(items) => ArgValue::Array(
            items
                .iter()
                .map(|item| ArgValue::String(item.clone()))
                .collect(),
        ),
        Value::Unset => ArgValue::Null,
    }
}

/// Convert a script value into a value for `leaf`.
pub fn from_arg(leaf: &Leaf, value: &ArgValue) -> StdResult<Value, String> {
    let mismatch = || format!("expected a {}, got {value:?}", leaf.kind.name());
    if matches!(value, ArgValue::Null) {
        return if leaf.optional {
            Ok(Value::Unset)
        } else {
            Err(mismatch())
        };
    }
    match &leaf.kind {
        Kind::Bool => match value {
            ArgValue::Bool(value) => Ok(Value::Bool(*value)),
            _ => Err(mismatch()),
        },
        Kind::Int { .. } => match value {
            ArgValue::Int(value) => Ok(Value::Int(*value)),
            ArgValue::UInt(value) => i64::try_from(*value)
                .map(Value::Int)
                .map_err(|_| mismatch()),
            ArgValue::Float(value) if value.fract() == 0.0 && value.abs() < 9.0e15 =>
            {
                #[allow(clippy::cast_possible_truncation)]
                Ok(Value::Int(*value as i64))
            }
            _ => Err(mismatch()),
        },
        Kind::Text => match value {
            ArgValue::String(text) => Ok(Value::Text(text.clone())),
            _ => Err(mismatch()),
        },
        Kind::Choice(_) => match value {
            ArgValue::String(text) => Ok(Value::Choice(text.clone())),
            _ => Err(mismatch()),
        },
        Kind::Set(_) | Kind::Order(_) | Kind::List => {
            // An empty Luau table carries no shape, so it arrives as a map.
            let items = match value {
                ArgValue::Array(items) => items.as_slice(),
                ArgValue::Map(map) if map.is_empty() => &[],
                _ => return Err(mismatch()),
            };
            let items = items
                .iter()
                .map(|item| match item {
                    ArgValue::String(text) => Ok(text.clone()),
                    _ => Err(mismatch()),
                })
                .collect::<StdResult<Vec<_>, _>>()?;
            Ok(if matches!(leaf.kind, Kind::Set(_)) {
                Value::Set(items)
            } else {
                Value::List(items)
            })
        }
    }
}

/// Return the value of a choice `delta` variants on, or of an integer
/// `delta` steps on.
#[must_use]
pub fn step_value(leaf: &Leaf, current: &Value, delta: i64) -> Option<Value> {
    match (&leaf.kind, current) {
        // An unset optional starts at its first variant or its lowest value.
        (Kind::Choice(variants), Value::Unset) => variants
            .first()
            .map(|variant| Value::Choice(variant.name.to_string())),
        (Kind::Int { min, .. }, Value::Unset) => Some(Value::Int(*min)),
        (Kind::Choice(variants), Value::Choice(name)) => {
            let count = i64::try_from(variants.len()).ok()?;
            let index = variants.iter().position(|variant| variant.name == name)?;
            let next = (i64::try_from(index).ok()? + delta).rem_euclid(count);
            Some(Value::Choice(
                variants[usize::try_from(next).ok()?].name.to_string(),
            ))
        }
        (Kind::Int { min, max, step }, Value::Int(value)) => Some(Value::Int(
            value
                .saturating_add(step.saturating_mul(delta))
                .max(*min)
                .min(*max),
        )),
        _ => None,
    }
}

/// Return the text that shows the value of `leaf`.
#[must_use]
pub fn display_value(leaf: &Leaf, value: &Value) -> String {
    let label_of = |name: &str| {
        leaf.kind
            .variants()
            .iter()
            .find(|variant| variant.name == name)
            .map_or_else(|| name.to_string(), |variant| variant.label.to_lowercase())
    };
    match value {
        Value::Choice(name) => label_of(name),
        Value::Set(names) if names.is_empty() => "none".into(),
        Value::Set(names) => names
            .iter()
            .map(|name| label_of(name))
            .collect::<Vec<_>>()
            .join(", "),
        Value::List(names) if matches!(leaf.kind, Kind::Order(_)) => names
            .iter()
            .map(|name| label_of(name))
            .collect::<Vec<_>>()
            .join(" › "),
        other => other.to_string(),
    }
}

/// Return the mark that shows the source of a value.
#[must_use]
pub const fn source_mark(source: &Source) -> &'static str {
    match source {
        Source::Default => " ",
        Source::File {
            scope: Scope::Global,
            ..
        } => "G",
        Source::File {
            scope: Scope::Local,
            ..
        } => "L",
        Source::Live => "●",
    }
}
