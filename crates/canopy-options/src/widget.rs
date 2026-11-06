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
    runtime::{NoticePost, PollLifetime},
    widget::Mount,
};
use canopy_widgets::menu;

use crate::{
    file::Scope,
    model::{Kind, Leaf, OptionGroup, Value},
    panel::{self, OptionsPanel},
    store::{Backend, Cleared, Saved, Source, Store, StoreError},
};

/// What `options.save` and `options.clear` return for a change that waits
/// for a notification round.
const DEFERRED: &str = "deferred";

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
    /// The application, until it mounts, and the nodes the mount built.
    mount: Mount<Option<PendingApp>, Parts>,
    /// Whether the panel is open, and its modal.
    panel: PanelState,
}

/// The nodes that the options widget builds as it mounts.
struct Parts {
    /// The application node.
    app: NodeId,
    /// The panel, in the overlay layer.
    panel: TypedId<OptionsPanel>,
}

/// Whether the options panel is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelState {
    /// The panel is closed.
    Closed,
    /// The panel opened as this modal, which may be closing.
    Open(ModalToken),
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
            mount: Mount::new(Some(Box::new(move |context, parent| {
                Ok(context.add_child(parent, app)?.into())
            }))),
            panel: PanelState::Closed,
        }
    }

    /// Note the end of the panel's modal, for any reason: a close, or
    /// retirement with another modal that it opened over. The panel then
    /// reads closed, and the store hears of it once.
    fn reconcile(&mut self, context: &dyn ViewContext) {
        if let PanelState::Open(token) = self.panel
            && context.modal_retired(token)
        {
            self.panel = PanelState::Closed;
            self.backend.panel_closed();
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

    /// Close the panel, if it is open. The panel reads closed at once, and
    /// the widget's poll notes the end of its modal once the close commits.
    pub(crate) fn close_panel(&mut self, context: &mut dyn Context) {
        self.reconcile(context);
        if let PanelState::Open(token) = self.panel
            && context.modal_is_open(token)
        {
            // A modal that closed with its owner has nothing left to close.
            context.close_modal(token).ok();
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

    /// Remove an option from the nearest option file that sets it. A clear
    /// that a subscriber of a change makes waits until the change's
    /// subscribers have run, and posts its outcome as a notice.
    /// @param path The dotted path of the option.
    /// @return The file that changed, nil if no file sets the option, or
    /// "deferred" for a clear that waits.
    #[command]
    pub fn clear(&mut self, context: &mut dyn Context, path: String) -> Result<Option<String>> {
        self.leaf(&path)?;
        Ok(
            match self.backend.clear(context, &path)?.map_err(store_error)? {
                Cleared::Done(file) => file.map(|file| file.display().to_string()),
                Cleared::Deferred => Some(DEFERRED.to_string()),
            },
        )
    }

    /// Save the live values to the global or the local option file. A save
    /// that a subscriber of a change makes waits until the change's
    /// subscribers have run, and posts its outcome as a notice.
    /// @param scope "global" or "local".
    /// @return The file that the save wrote, or "deferred" for a save that
    /// waits.
    #[command]
    pub fn save(&mut self, context: &mut dyn Context, scope: String) -> Result<String> {
        let scope = parse_scope(&scope)?;
        Ok(
            match self.backend.save(context, scope)?.map_err(store_error)? {
                Saved::Written(file) => file.display().to_string(),
                Saved::Deferred => DEFERRED.to_string(),
            },
        )
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
        self.reconcile(context);
        let &Parts { app, panel } = self.mount.mounted()?;
        let group = path.unwrap_or_default();
        if self.backend.schema().group(&group).is_none() {
            return Err(Error::Invalid(format!("unknown option group: {group}")));
        }
        let host = context.node_id();
        let body = context.with_widget_mut(panel, |panel: &mut OptionsPanel, context| {
            panel.reset(context, host, &group, quick.unwrap_or(false))
        })?;
        // An open panel shows the new group in place.
        if let PanelState::Open(token) = self.panel
            && context.modal_is_open(token)
        {
            context.set_focus(body.into())?;
            return Ok(());
        }
        let options = |owner| ModalOptions {
            owner,
            modal: panel.into(),
            initial_focus: body.into(),
            dim_target: Some(app),
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
        self.panel = PanelState::Open(token);
        // The poll notes the end of the modal, also when it ends with the
        // modal it opened over. The widgets that wait for the close poll in
        // the same turn.
        context.wake_on_modal_close(token, context.wake_handle(PollLifetime::Node)?)?;
        for wake in self.backend.close_wakes() {
            context.wake_on_modal_close(token, wake)?;
        }
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
        matches!(self.panel, PanelState::Open(token) if context.modal_is_open(token))
    }
}

impl Widget for Options {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let backend = Rc::clone(&self.backend);
        self.mount.mount(|app| {
            // The application is one value, which its first mount takes.
            if app.is_none() {
                return Err(Error::Invalid(
                    "the options host's application mounted once already".into(),
                ));
            }
            // The panel goes first, so a panel that fails leaves the
            // application to a later mount.
            let panel = context.add_modal(OptionsPanel::new(backend))?;
            let build = app.take().expect("checked above");
            let host = context.node_id();
            let app = build(context, host)?;
            Ok(Parts { app, panel })
        })?;
        context.request_poll()
    }

    fn on_unmount(&mut self, _context: &mut dyn Context) {
        self.mount.unmount();
        self.panel = PanelState::Closed;
    }

    fn poll(&mut self, context: &mut dyn Context) -> Result<Option<Duration>> {
        self.reconcile(context);
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

#[cfg(test)]
mod tests {
    use canopy::{
        derive_commands,
        input::{ModalBindings, key::KeyCode},
        runtime::{PollLifetime, TurnInput},
        script::EvalRequest,
        testing::harness::Harness,
    };

    use super::*;
    use crate::tests::Root;

    /// A modal pane that takes the focus.
    struct Pane;

    impl Widget for Pane {
        fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
            true
        }

        fn name(&self) -> NodeName {
            NodeName::convert("pane")
        }
    }

    /// An application that opens a modal of its own, as an inspector does,
    /// and counts the polls that the panel's close sends it.
    struct Opener {
        /// The store, to hear the panel close.
        store: Store<Root>,
        /// The modal pane.
        pane: Option<TypedId<Pane>>,
        /// The pane's open modal.
        other: Option<ModalToken>,
        /// How many times the opener polled.
        polls: usize,
    }

    #[derive_commands]
    impl Opener {
        /// Open the pane as a modal.
        #[command]
        pub fn open_other(&mut self, context: &mut dyn Context) -> Result<()> {
            let pane = self.pane.expect("the opener mounted");
            self.other = Some(context.open_modal(ModalOptions {
                owner: context.node_id(),
                modal: pane.into(),
                initial_focus: pane.into(),
                dim_target: None,
                bindings: ModalBindings::Application,
            })?);
            Ok(())
        }

        /// Close the pane's modal.
        #[command]
        pub fn close_other(&mut self, context: &mut dyn Context) -> Result<()> {
            context.close_modal(self.other.take().expect("the pane is open"))
        }

        /// Close the options panel, check that it reads closed, and fail, so
        /// that the dispatch discards the close.
        #[command]
        pub fn close_options_and_fail(&self, context: &mut dyn Context) -> Result<()> {
            let root = context.root_id();
            context.with_widget_mut(root, |options: &mut Options, context| {
                options.close_panel(context);
                assert!(!options.is_open(context), "a closing panel reads closed");
                Ok(())
            })?;
            Err(Error::Invalid("the dispatch fails".into()))
        }
    }

    impl Widget for Opener {
        fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
            self.pane = Some(context.add_modal(Pane)?);
            self.store
                .handle()
                .wake_on_close(context.wake_handle(PollLifetime::Node)?);
            Ok(())
        }

        fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
            true
        }

        fn poll(&mut self, _context: &mut dyn Context) -> Result<Option<Duration>> {
            self.polls += 1;
            Ok(None)
        }

        fn name(&self) -> NodeName {
            NodeName::convert("opener")
        }
    }

    impl Register for Opener {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    /// Host an opener inside the options widget of `store`.
    fn harness(store: &Store<Root>) -> Harness {
        let opener = Opener {
            store: store.clone(),
            pane: None,
            other: None,
            polls: 0,
        };
        let mut harness = Harness::builder(Options::new(store, opener))
            .register::<Options>()
            .register::<Opener>()
            .size(80, 24)
            .build()
            .expect("the harness builds");
        harness.render().expect("the harness renders");
        harness
    }

    /// Run `source` in one turn of the app.
    fn turn_eval(harness: &mut Harness, source: &str) {
        harness
            .canopy
            .turn(TurnInput::StartEval(EvalRequest::new(source)))
            .expect("the turn runs");
    }

    /// Return the opener's poll count.
    fn polls(harness: &mut Harness) -> usize {
        harness
            .with_unique(|opener: &mut Opener, _| Ok(opener.polls))
            .expect("one opener")
    }

    /// Return whether the panel is open.
    fn panel_open(harness: &mut Harness) -> bool {
        harness
            .with_root_widget_context(|options: &mut Options, context| Ok(options.is_open(context)))
            .expect("the root is the options widget")
    }

    #[test]
    fn regression_a_panel_over_another_modal_closes_with_it() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        harness.script("opener.open_other()").unwrap();
        harness.script("options.open()").unwrap();
        assert!(panel_open(&mut harness));
        let before = polls(&mut harness);
        turn_eval(&mut harness, "opener.close_other()");
        assert!(!panel_open(&mut harness));
        // The eval runs after the turn's polls, so the runtime polls the
        // widgets that its close woke in the next turn.
        harness.canopy.turn(TurnInput::Wake).unwrap();
        assert_eq!(store.backend().panel_closes(), 1);
        assert!(polls(&mut harness) > before, "the close polls the follower");
        // The panel opens again over the application.
        harness.script("options.open()").unwrap();
        assert!(panel_open(&mut harness));
        assert_eq!(store.backend().panel_closes(), 1);
    }

    #[test]
    fn an_explicit_close_notes_the_close_once() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        harness.script("options.open()").unwrap();
        let before = polls(&mut harness);
        turn_eval(
            &mut harness,
            "options.close(); canopy.assert(not options.is_open()); options.close()",
        );
        assert!(!panel_open(&mut harness));
        harness.script("options.close()").unwrap();
        assert_eq!(store.backend().panel_closes(), 1);
        assert!(polls(&mut harness) > before);
    }

    #[test]
    fn esc_closes_the_panel_and_polls_the_follower_in_that_turn() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        harness.script("options.open()").unwrap();
        let before = polls(&mut harness);
        harness.key(KeyCode::Esc).unwrap();
        assert!(!panel_open(&mut harness));
        assert_eq!(store.backend().panel_closes(), 1);
        assert!(
            polls(&mut harness) > before,
            "the follower polled in the turn"
        );
    }

    #[test]
    fn a_discarded_close_reads_closed_then_open_and_notes_nothing() {
        let store = Store::<Root>::default();
        let mut harness = harness(&store);
        harness.script("options.open()").unwrap();
        assert!(harness.script("opener.close_options_and_fail()").is_err());
        assert!(panel_open(&mut harness));
        assert_eq!(store.backend().panel_closes(), 0);
    }

    /// An application whose mount fails.
    struct Failing;

    impl Widget for Failing {
        fn on_mount(&mut self, _context: &mut dyn Context) -> Result<()> {
            Err(Error::Invalid("the application fails to mount".into()))
        }
    }

    #[test]
    fn a_failed_mount_leaves_the_widget_pending() {
        let store = Store::<Root>::default();
        let mut harness = Harness::new(Pane).expect("the harness builds");
        let result = harness.with_root_widget_context(|_: &mut Pane, context| {
            let options = context.create_detached(Options::new(&store, Failing))?;
            let root = context.root_id();
            let attached = context.attach(root, options.into());
            assert!(attached.is_err(), "the application's mount fails");
            context.with_widget_mut(options, |options: &mut Options, _| {
                assert!(!options.mount.is_mounted(), "the mount is pending");
                assert_eq!(options.panel, PanelState::Closed);
                Ok(())
            })
        });
        result.expect("the detached widget stays");
    }
}
