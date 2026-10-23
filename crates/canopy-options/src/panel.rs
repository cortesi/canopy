//! The options panel: a modal dialog built from the schema, which shows each
//! value with a control for its kind.

use std::{
    env, mem,
    path::{Path, PathBuf},
    rc::Rc,
    result::Result as StdResult,
};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Setup, TypedId, ViewContext, Widget,
    error::{Error, Result},
    geom::{Line, Rect, Size},
    input::{
        BindingAction, BindingOptions, BindingTier, Event, FrameworkBindingGroup, IntentName,
        IntentSpec, NavIntent,
        key::{Key, KeyCode},
    },
    layout::{CanvasContext, Direction, Edges, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::NoticePost,
    text,
};
use canopy_widgets::{
    Dialog, Frame,
    menu::{self, MenuRow, RowLayout},
};

use crate::{
    file::Scope,
    model::{Item, Kind, Leaf, Value, assign_keys, join_path, panel_items},
    store::{Backend, Source, StoreError},
    widget::{Options, display_value, source_mark, step_value},
};

/// The narrowest the text of the panel gets, without its padding and
/// borders.
const MIN_WIDTH: u32 = 42;

/// The widest the text of the panel gets, without its padding and borders.
const MAX_WIDTH: u32 = 70;

/// The columns that the padding and the borders of the panel add to its
/// text.
const CHROME_WIDTH: u32 = 4;

/// The bindings of the panel as it moves between rows and acts on them.
const BROWSE_BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("options.panel");

/// The bindings of the panel as it edits a value in its row.
const EDIT_BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("options.edit");

/// The binding groups that the panel's modal admits, one for each mode.
///
/// Each binding runs an intent that the panel accepts only in its mode, so
/// help lists the keys of the mode that the panel is in.
pub const BINDING_GROUPS: &[FrameworkBindingGroup] = &[BROWSE_BINDINGS, EDIT_BINDINGS];

/// An action of the panel that a key runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intent {
    /// Move the selection.
    Nav(NavIntent),
    /// Act on the selected row.
    Activate,
    /// Open the level of the selected row, step a choice to its next
    /// variant, or act on any other row.
    Forward,
    /// Go back one level, but not past the top.
    Pop,
    /// Go back one level, or close the panel at the top.
    Back,
    /// Open the save menu.
    Save,
    /// Revert the selected leaf.
    Revert,
    /// Revert every leaf.
    RevertAll,
    /// Clear the selected leaf from the nearest file that sets it.
    Clear,
    /// Step the selected integer up.
    Increase,
    /// Step the selected integer down.
    Decrease,
    /// Delete the selected list item.
    Delete,
    /// Move the selected list item up.
    ShiftUp,
    /// Move the selected list item down.
    ShiftDown,
    /// Select every member of the selected set, or none if it holds all.
    ToggleAll,
    /// Commit the edit.
    Commit,
    /// Cancel the edit.
    CancelEdit,
    /// Delete the last character of the edit.
    Erase,
    /// Delete the text of the edit.
    EraseAll,
}

impl Intent {
    /// The intents of the panel's own, which it registers. The navigation
    /// intents belong to Canopy.
    const OWN: [Self; 18] = [
        Self::Activate,
        Self::Forward,
        Self::Pop,
        Self::Back,
        Self::Save,
        Self::Revert,
        Self::RevertAll,
        Self::Clear,
        Self::Increase,
        Self::Decrease,
        Self::Delete,
        Self::ShiftUp,
        Self::ShiftDown,
        Self::ToggleAll,
        Self::Commit,
        Self::CancelEdit,
        Self::Erase,
        Self::EraseAll,
    ];

    /// Return the intent name.
    const fn name(self) -> &'static str {
        match self {
            Self::Nav(nav) => nav.name(),
            Self::Activate => "options.activate",
            Self::Forward => "options.forward",
            Self::Pop => "options.pop",
            Self::Back => "options.back",
            Self::Save => "options.save",
            Self::Revert => "options.revert",
            Self::RevertAll => "options.revert_all",
            Self::Clear => "options.clear",
            Self::Increase => "options.increase",
            Self::Decrease => "options.decrease",
            Self::Delete => "options.delete",
            Self::ShiftUp => "options.shift_up",
            Self::ShiftDown => "options.shift_down",
            Self::ToggleAll => "options.toggle_all",
            Self::Commit => "options.edit.commit",
            Self::CancelEdit => "options.edit.cancel",
            Self::Erase => "options.edit.erase",
            Self::EraseAll => "options.edit.erase_all",
        }
    }

    /// Return the intent named `name`, if the panel binds it.
    fn from_name(name: &str) -> Option<Self> {
        NavIntent::from_name(name)
            .map(Self::Nav)
            .or_else(|| Self::OWN.into_iter().find(|intent| intent.name() == name))
    }
}

/// The keys of the panel: the group of the mode, the key, the intent, and
/// what help says of it.
const KEYS: [(FrameworkBindingGroup, &str, Intent, &str); 29] = [
    (
        BROWSE_BINDINGS,
        "Up",
        Intent::Nav(NavIntent::Up),
        "Previous row",
    ),
    (
        BROWSE_BINDINGS,
        "k",
        Intent::Nav(NavIntent::Up),
        "Previous row",
    ),
    (
        BROWSE_BINDINGS,
        "Down",
        Intent::Nav(NavIntent::Down),
        "Next row",
    ),
    (
        BROWSE_BINDINGS,
        "j",
        Intent::Nav(NavIntent::Down),
        "Next row",
    ),
    (
        BROWSE_BINDINGS,
        "PageUp",
        Intent::Nav(NavIntent::PageUp),
        "Page up",
    ),
    (
        BROWSE_BINDINGS,
        "PageDown",
        Intent::Nav(NavIntent::PageDown),
        "Page down",
    ),
    (
        BROWSE_BINDINGS,
        "Home",
        Intent::Nav(NavIntent::First),
        "First row",
    ),
    (
        BROWSE_BINDINGS,
        "End",
        Intent::Nav(NavIntent::Last),
        "Last row",
    ),
    (
        BROWSE_BINDINGS,
        "Enter",
        Intent::Activate,
        "Change or open the selected row",
    ),
    (
        BROWSE_BINDINGS,
        "Space",
        Intent::Activate,
        "Change or open the selected row",
    ),
    (
        BROWSE_BINDINGS,
        "l",
        Intent::Forward,
        "Open the selected row, or step its option",
    ),
    (
        BROWSE_BINDINGS,
        "Right",
        Intent::Forward,
        "Open the selected row, or step its option",
    ),
    (
        BROWSE_BINDINGS,
        "Esc",
        Intent::Back,
        "Back, or close the panel",
    ),
    (BROWSE_BINDINGS, "h", Intent::Pop, "Back"),
    (BROWSE_BINDINGS, "Left", Intent::Pop, "Back"),
    (
        BROWSE_BINDINGS,
        "S",
        Intent::Save,
        "Choose the file to save to",
    ),
    (
        BROWSE_BINDINGS,
        "x",
        Intent::Revert,
        "Revert the unsaved value",
    ),
    (
        BROWSE_BINDINGS,
        "R",
        Intent::RevertAll,
        "Revert every unsaved value",
    ),
    (
        BROWSE_BINDINGS,
        "X",
        Intent::Clear,
        "Clear the option from its file",
    ),
    (
        BROWSE_BINDINGS,
        "+",
        Intent::Increase,
        "Increase the number",
    ),
    (
        BROWSE_BINDINGS,
        "-",
        Intent::Decrease,
        "Decrease the number",
    ),
    (BROWSE_BINDINGS, "d", Intent::Delete, "Delete the item"),
    (BROWSE_BINDINGS, "K", Intent::ShiftUp, "Move the item up"),
    (
        BROWSE_BINDINGS,
        "J",
        Intent::ShiftDown,
        "Move the item down",
    ),
    (
        BROWSE_BINDINGS,
        "A",
        Intent::ToggleAll,
        "Select all, or none if all are selected",
    ),
    (EDIT_BINDINGS, "Enter", Intent::Commit, "Commit the edit"),
    (EDIT_BINDINGS, "Esc", Intent::CancelEdit, "Cancel the edit"),
    (
        EDIT_BINDINGS,
        "Backspace",
        Intent::Erase,
        "Delete the last character",
    ),
    (EDIT_BINDINGS, "Ctrl+u", Intent::EraseAll, "Delete the text"),
];

/// Register the panel's intents and bind its keys.
pub fn register(setup: &mut Setup) -> Result<()> {
    for intent in Intent::OWN {
        let description = KEYS
            .iter()
            .find(|(_, _, bound, _)| *bound == intent)
            .map_or("", |(_, _, _, description)| description);
        setup.register_intent(IntentSpec::new(
            intent.name(),
            format!("{description}, in the options panel"),
        )?)?;
    }
    for (group, key, intent, description) in KEYS {
        setup.bind(
            Key::parse_spec(key)?,
            BindingOptions {
                show_in_help: true,
                path: Some("**/options_panel/**/".parse()?),
                tier: BindingTier::Framework(group),
                description: description.to_string(),
                source: None,
                phase: None,
            },
            BindingAction::Intent(IntentName::new(intent.name())?),
        )?;
    }
    Ok(())
}

/// The modal root of the options panel, in the overlay layer.
pub struct OptionsPanel {
    /// The store.
    backend: Rc<dyn Backend>,
    /// The body, once mounted.
    body: Option<TypedId<PanelBody>>,
}

impl OptionsPanel {
    /// Build a panel for the options of `backend`.
    pub(crate) fn new(backend: Rc<dyn Backend>) -> Self {
        Self {
            backend,
            body: None,
        }
    }

    /// Show the group at `group`, in quick mode if `quick` is set, and return
    /// the body, which takes the keys.
    pub(crate) fn reset(
        &self,
        context: &mut dyn Context,
        host: NodeId,
        group: &str,
        quick: bool,
    ) -> Result<TypedId<PanelBody>> {
        let body = self
            .body
            .ok_or_else(|| Error::Invalid("the options panel body is not mounted".into()))?;
        context.with_widget_mut(body, |body: &mut PanelBody, context| {
            body.reset(host, group, quick);
            body.retitle(context)
        })?;
        Ok(body)
    }
}

impl Widget for OptionsPanel {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        // The panel's rows are a menu's, so they take the menu's styles.
        render.push_layer("menu");
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let me = context.node_id();
        let (dialog, body) = Dialog::new()
            .with_title("Options")
            .with_max_width(MAX_WIDTH + CHROME_WIDTH)
            .add(context, me, PanelBody::new(Rc::clone(&self.backend)))?;
        let frame = Dialog::frame(context, dialog.into())?;
        context.with_widget_mut(body, |body: &mut PanelBody, _| {
            body.frame = Some(frame);
            Ok(())
        })?;
        self.body = Some(body);
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("options_panel")
    }
}

/// What one level of the panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LevelKind {
    /// The items of a group.
    Group(String),
    /// The variants of a choice leaf.
    Variants(String),
    /// The members of a set leaf.
    Set(String),
    /// The variants of an order leaf, in its order.
    Order(String),
    /// The items of a list leaf.
    List(String),
    /// The files that a save can write.
    Save,
}

/// One level of the panel, with its selection.
#[derive(Clone, Debug)]
struct Level {
    /// What the level shows.
    kind: LevelKind,
    /// The selected row.
    selected: usize,
}

/// What an inline edit changes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EditTarget {
    /// The value of the leaf.
    Value,
    /// One item of a list, or a new item at the end.
    ListItem(Option<usize>),
}

/// An inline edit of a text, an integer, or a list item.
#[derive(Clone, Debug)]
struct Edit {
    /// The leaf path.
    path: String,
    /// The text so far.
    buffer: String,
    /// What the edit changes.
    target: EditTarget,
}

/// What the panel does with keys.
#[derive(Clone, Debug)]
enum Mode {
    /// Move between rows and act on them.
    Browse,
    /// Edit a value in its row.
    Edit(Edit),
}

/// What a row does when it is activated.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    /// Act on a leaf.
    Leaf(String),
    /// Open a group.
    Group(String),
    /// Choose a variant of a choice.
    Variant(String),
    /// Toggle a member of a set.
    Member(String),
    /// A variant of an order, at its place.
    OrderItem(usize),
    /// Edit an item of a list.
    ListItem(usize),
    /// Add an item to a list.
    ListAdd,
    /// Save the unsaved values to the file of a scope.
    Save(Scope),
}

/// One row of the panel.
#[derive(Clone, Debug)]
struct Row {
    /// The accelerator.
    key: Option<char>,
    /// The label.
    label: String,
    /// The value, drawn as its control.
    value: String,
    /// Whether the value reads quietly, as for a default of an optional
    /// leaf.
    muted: bool,
    /// The source mark.
    mark: &'static str,
    /// What the row does.
    action: Action,
}

/// Return the columns of the panel's `rows`. Every row keeps the mark
/// gutter, so a mark that comes or goes never moves the values.
fn row_layout(rows: &[MenuRow]) -> RowLayout {
    RowLayout {
        marks: true,
        ..RowLayout::fit(rows)
    }
}

impl Row {
    /// Return what the row shows, as a menu row.
    fn menu_row(&self) -> MenuRow {
        MenuRow::new(self.key, self.label.clone())
            .with_value(self.value.clone())
            .with_muted(self.muted)
            .with_mark(self.mark)
    }
}

/// The body of the options panel, which draws the rows and reads the keys.
pub struct PanelBody {
    /// The store.
    backend: Rc<dyn Backend>,
    /// The `options` widget that opened the panel.
    host: Option<NodeId>,
    /// The frame, to retitle.
    frame: Option<TypedId<Frame>>,
    /// Whether a change closes the panel. A set, an order, or a list closes
    /// it as the user leaves its level.
    quick: bool,
    /// Whether a level that takes more than one step changed in quick mode,
    /// so that leaving it closes the panel.
    level_changed: bool,
    /// The levels, from the opened group to the current one.
    levels: Vec<Level>,
    /// What the panel does with keys.
    mode: Mode,
}

impl PanelBody {
    /// Build a body for the options of `backend`.
    fn new(backend: Rc<dyn Backend>) -> Self {
        Self {
            backend,
            host: None,
            frame: None,
            quick: false,
            level_changed: false,
            levels: vec![Level {
                kind: LevelKind::Group(String::new()),
                selected: 0,
            }],
            mode: Mode::Browse,
        }
    }

    /// Show the group at `group`.
    fn reset(&mut self, host: NodeId, group: &str, quick: bool) {
        self.host = Some(host);
        self.quick = quick;
        self.level_changed = false;
        self.levels = vec![Level {
            kind: LevelKind::Group(group.to_string()),
            selected: 0,
        }];
        self.mode = Mode::Browse;
    }

    /// Return the current level.
    fn level(&self) -> &Level {
        self.levels.last().expect("the panel always has a level")
    }

    /// Return the current level, for a change.
    fn level_mut(&mut self) -> &mut Level {
        self.levels
            .last_mut()
            .expect("the panel always has a level")
    }

    /// Return the title of the current level.
    fn title(&self) -> String {
        let schema = self.backend.schema();
        let mut parts = vec!["Options".to_string()];
        for level in &self.levels {
            let path = match &level.kind {
                LevelKind::Group(path)
                | LevelKind::Variants(path)
                | LevelKind::Set(path)
                | LevelKind::Order(path)
                | LevelKind::List(path) => path,
                LevelKind::Save => {
                    parts.push("save".to_string());
                    continue;
                }
            };
            if path.is_empty() {
                continue;
            }
            let label = schema
                .leaf(path)
                .map(|leaf| leaf.label)
                .or_else(|| schema.group(path).map(|group| group.label))
                .unwrap_or_default();
            if parts.last().map(String::as_str) != Some(label) {
                parts.push(label.to_string());
            }
        }
        let mut title = parts.join(" › ");
        if self.quick {
            title.push_str(" (quick)");
        }
        title
    }

    /// Set the frame title to the title of the current level.
    fn retitle(&self, context: &mut dyn Context) -> Result<()> {
        let title = self.title();
        if let Some(frame) = self.frame {
            context.with_widget_mut(frame, |frame: &mut Frame, _| {
                frame.set_title(title);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Return the rows of the current level.
    fn rows(&self) -> Vec<Row> {
        let schema = self.backend.schema();
        match &self.level().kind {
            LevelKind::Group(path) => {
                let Some(group) = schema.group(path) else {
                    return Vec::new();
                };
                panel_items(group, path)
                    .into_iter()
                    .map(|(holder, item)| self.item_row(&holder, item))
                    .collect()
            }
            LevelKind::Variants(path) => {
                let Some(leaf) = schema.leaf(path) else {
                    return Vec::new();
                };
                let current = self.backend.get(path);
                variant_rows(leaf, |name| {
                    let chosen = current.as_ref() == Some(&Value::Choice(name.to_string()));
                    (
                        if chosen { "(•)" } else { "( )" },
                        Action::Variant(name.to_string()),
                    )
                })
            }
            LevelKind::Set(path) => {
                let Some(leaf) = schema.leaf(path) else {
                    return Vec::new();
                };
                let members = self.set_members(path);
                variant_rows(leaf, |name| {
                    let checked = members.iter().any(|member| member == name);
                    (
                        if checked { "[x]" } else { "[ ]" },
                        Action::Member(name.to_string()),
                    )
                })
            }
            LevelKind::Order(path) => self.order_rows(path),
            LevelKind::List(path) => {
                let mut rows: Vec<Row> = self
                    .list_items(path)
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| Row {
                        key: None,
                        label: item,
                        value: String::new(),
                        muted: false,
                        mark: " ",
                        action: Action::ListItem(index),
                    })
                    .collect();
                rows.push(Row {
                    key: Some('a'),
                    label: "add an item".into(),
                    value: String::new(),
                    muted: true,
                    mark: " ",
                    action: Action::ListAdd,
                });
                rows
            }
            LevelKind::Save => {
                // The rows take keys as options do, so none is a key that
                // the panel reserves.
                let mut keys = [None, None];
                let [global, local] = &mut keys;
                assign_keys(&mut [("global", global), ("local", local)]);
                [Scope::Global, Scope::Local]
                    .into_iter()
                    .zip(keys)
                    .map(|(scope, key)| {
                        let target = self.backend.save_target(scope);
                        Row {
                            key,
                            label: scope.name().to_string(),
                            muted: target.is_none(),
                            value: target.map_or_else(
                                || format!("no {} file", scope.name()),
                                |file| display_path(&file),
                            ),
                            mark: " ",
                            action: Action::Save(scope),
                        }
                    })
                    .collect()
            }
        }
    }

    /// Return one row for each variant of the order leaf at `path`, in its
    /// order, numbered by place.
    fn order_rows(&self, path: &str) -> Vec<Row> {
        let Some(leaf) = self.leaf(path) else {
            return Vec::new();
        };
        self.list_items(path)
            .into_iter()
            .enumerate()
            .map(|(index, name)| Row {
                key: None,
                label: display_value(&leaf, &Value::Choice(name)),
                value: format!("{}", index + 1),
                muted: true,
                mark: " ",
                action: Action::OrderItem(index),
            })
            .collect()
    }

    /// Return the row for one item of the group at `group`.
    fn item_row(&self, group: &str, item: &Item) -> Row {
        let path = join_path(group, item.name());
        match item {
            Item::Group(child) => Row {
                key: child.key,
                label: child.label.to_string(),
                value: "...".into(),
                muted: true,
                // A group with a live value below it shows the live mark, so
                // a change shows at every level above it.
                mark: if self.backend.has_live_at(&path) {
                    source_mark(&Source::Live)
                } else {
                    " "
                },
                action: Action::Group(path),
            },
            Item::Leaf(leaf) => {
                let value = self.backend.get(&path).unwrap_or(Value::Unset);
                let (shown, muted) = control(leaf, &value);
                let source = self.backend.source(&path);
                Row {
                    key: leaf.key,
                    label: leaf.label.to_string(),
                    value: shown,
                    muted,
                    mark: source_mark(&source),
                    action: Action::Leaf(path),
                }
            }
        }
    }

    /// Return the members of the set leaf at `path`.
    fn set_members(&self, path: &str) -> Vec<String> {
        match self.backend.get(path) {
            Some(Value::Set(members)) => members,
            _ => Vec::new(),
        }
    }

    /// Return the items of the list leaf at `path`.
    fn list_items(&self, path: &str) -> Vec<String> {
        match self.backend.get(path) {
            Some(Value::List(items)) => items,
            _ => Vec::new(),
        }
    }

    /// Return the leaf at `path`.
    fn leaf(&self, path: &str) -> Option<Leaf> {
        self.backend.schema().leaf(path).cloned()
    }

    /// Return the path of the leaf that the current level edits, if it is a
    /// submenu of one leaf.
    fn level_leaf(&self) -> Option<String> {
        match &self.level().kind {
            LevelKind::Variants(path)
            | LevelKind::Set(path)
            | LevelKind::Order(path)
            | LevelKind::List(path) => Some(path.clone()),
            LevelKind::Group(_) | LevelKind::Save => None,
        }
    }

    /// Return the path of the leaf of the selected row, or of the current
    /// submenu.
    fn selected_leaf(&self) -> Option<String> {
        if let Some(path) = self.level_leaf() {
            return Some(path);
        }
        match self.selected_action() {
            Some(Action::Leaf(path)) => Some(path),
            _ => None,
        }
    }

    /// Return whether the panel takes `key` itself: an accelerator as it
    /// browses, or a character of an edit. Its bindings take every other key.
    fn handles(&self, key: Key) -> bool {
        let key = key.normalize();
        let KeyCode::Char(c) = key.key else {
            return false;
        };
        if key.mods.ctrl || key.mods.alt {
            return false;
        }
        match &self.mode {
            Mode::Browse => self.accelerator(c).is_some(),
            Mode::Edit(_) => true,
        }
    }

    /// Return the index of the row whose accelerator is `c`.
    fn accelerator(&self, c: char) -> Option<usize> {
        self.rows().iter().position(|row| row.key == Some(c))
    }

    /// Return the action of the selected row.
    fn selected_action(&self) -> Option<Action> {
        self.rows()
            .get(self.level().selected)
            .map(|row| row.action.clone())
    }

    /// Return the path of the set leaf of the selected row, or of the
    /// current checklist.
    fn selected_set(&self) -> Option<String> {
        self.selected_leaf().filter(|path| {
            self.leaf(path)
                .is_some_and(|leaf| matches!(leaf.kind, Kind::Set(_)))
        })
    }

    /// Select every member of the selected set, or none if it holds every
    /// member already.
    fn toggle_all(&mut self, context: &mut dyn Context) -> Result<()> {
        let Some(path) = self.selected_set() else {
            return Ok(());
        };
        let Some(leaf) = self.leaf(&path) else {
            return Ok(());
        };
        let all: Vec<String> = leaf
            .kind
            .variants()
            .iter()
            .map(|variant| variant.name.to_string())
            .collect();
        let members = self.set_members(&path);
        let next = if all.iter().all(|name| members.contains(name)) {
            Vec::new()
        } else {
            all
        };
        if self.write(context, &path, Value::Set(next))? {
            self.changed(context)?;
        }
        Ok(())
    }

    /// Return whether the selected row is an integer leaf.
    fn selected_int(&self) -> bool {
        match self.selected_action() {
            Some(Action::Leaf(path)) => self
                .leaf(&path)
                .is_some_and(|leaf| matches!(leaf.kind, Kind::Int { .. })),
            _ => false,
        }
    }

    /// Return whether an accelerator of the current level takes a key that
    /// binds `intent`. The panel then leaves the key to the accelerator.
    fn yields(&self, intent: Intent) -> bool {
        KEYS.iter()
            .filter(|(group, _, bound, _)| *group == BROWSE_BINDINGS && *bound == intent)
            .filter_map(|(_, key, _, _)| {
                let mut chars = key.chars();
                chars.next().filter(|_| chars.next().is_none())
            })
            .any(|key| self.accelerator(key).is_some())
    }

    /// Return whether `intent` acts in the panel's current state.
    fn accepts(&self, intent: Intent) -> bool {
        match &self.mode {
            Mode::Edit(_) => matches!(
                intent,
                Intent::Commit | Intent::CancelEdit | Intent::Erase | Intent::EraseAll
            ),
            Mode::Browse if self.yields(intent) => false,
            Mode::Browse => match intent {
                Intent::Back => true,
                Intent::Nav(_) | Intent::Activate => !self.rows().is_empty(),
                Intent::Save => self.level().kind != LevelKind::Save,
                Intent::Forward => self.selected_action().is_some(),
                Intent::Pop => self.levels.len() > 1,
                Intent::Revert => self
                    .selected_leaf()
                    .is_some_and(|path| self.backend.has_live_at(&path)),
                Intent::RevertAll => self.backend.has_live_at(""),
                Intent::Clear => self
                    .selected_leaf()
                    .is_some_and(|path| matches!(self.backend.source(&path), Source::File { .. })),
                Intent::Increase | Intent::Decrease => self.selected_int(),
                Intent::Delete => matches!(self.selected_action(), Some(Action::ListItem(_))),
                Intent::ShiftUp | Intent::ShiftDown => matches!(
                    self.selected_action(),
                    Some(Action::ListItem(_) | Action::OrderItem(_))
                ),
                Intent::ToggleAll => self.selected_set().is_some(),
                _ => false,
            },
        }
    }

    /// Move the selection by `delta` rows, within the rows.
    fn move_by(&mut self, delta: isize) {
        let count = self.rows().len();
        if count == 0 {
            return;
        }
        let level = self.level_mut();
        level.selected = level
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
    }

    /// Close the panel through its host.
    fn close(&mut self, context: &mut dyn Context) -> Result<()> {
        self.mode = Mode::Browse;
        let Some(host) = self.host else {
            return Ok(());
        };
        context.with_widget_mut(host, |options: &mut Options, context| {
            options.close_panel(context);
            Ok(())
        })
    }

    /// Go back one level, or close the panel at the top.
    fn back(&mut self, context: &mut dyn Context) -> Result<()> {
        if self.levels.len() > 1 {
            self.levels.pop();
            // Quick mode closes once a level of many steps is done.
            if self.quick && mem::take(&mut self.level_changed) {
                return self.close(context);
            }
            self.retitle(context)
        } else {
            self.close(context)
        }
    }

    /// Open a level, selecting the row at `selected`.
    fn push(&mut self, context: &mut dyn Context, kind: LevelKind, selected: usize) -> Result<()> {
        self.levels.push(Level { kind, selected });
        self.retitle(context)
    }

    /// Note a completed change: in quick mode, close the panel. A set, an
    /// order, or a list takes more than one step, so its level stays open
    /// until the user leaves it.
    fn changed(&mut self, context: &mut dyn Context) -> Result<()> {
        if !self.quick {
            return Ok(());
        }
        if matches!(
            self.level().kind,
            LevelKind::Set(_) | LevelKind::Order(_) | LevelKind::List(_)
        ) {
            self.level_changed = true;
            return Ok(());
        }
        self.close(context)
    }

    /// Write a live value, and report its error as a toast.
    fn write(&self, context: &mut dyn Context, path: &str, value: Value) -> Result<bool> {
        match self.backend.set(context, path, value)? {
            Ok(()) => Ok(true),
            Err(error) => {
                context.notify(NoticePost::error(error.to_string()))?;
                Ok(false)
            }
        }
    }

    /// Act on the selected row.
    fn activate(&mut self, context: &mut dyn Context) -> Result<()> {
        let Some(action) = self.selected_action() else {
            return Ok(());
        };
        match action {
            Action::Group(path) => self.push(context, LevelKind::Group(path), 0),
            Action::Leaf(path) => self.activate_leaf(context, &path),
            Action::Variant(name) => {
                let Some(path) = self.level_leaf() else {
                    return Ok(());
                };
                if self.write(context, &path, Value::Choice(name))? {
                    self.levels.pop();
                    self.retitle(context)?;
                    self.changed(context)?;
                }
                Ok(())
            }
            Action::Member(name) => {
                let Some(path) = self.level_leaf() else {
                    return Ok(());
                };
                let mut members = self.set_members(&path);
                if let Some(index) = members.iter().position(|member| *member == name) {
                    members.remove(index);
                } else {
                    members.push(name);
                }
                let order: Vec<&str> = self.leaf(&path).map_or_else(Vec::new, |leaf| {
                    leaf.kind
                        .variants()
                        .iter()
                        .map(|variant| variant.name)
                        .collect()
                });
                members.sort_by_key(|member| order.iter().position(|name| name == member));
                if self.write(context, &path, Value::Set(members))? {
                    self.changed(context)?;
                }
                Ok(())
            }
            // An order's variants move with the shift keys, and do nothing
            // on their own.
            Action::OrderItem(_) => Ok(()),
            Action::ListItem(index) => {
                let Some(path) = self.level_leaf() else {
                    return Ok(());
                };
                let buffer = self
                    .list_items(&path)
                    .get(index)
                    .cloned()
                    .unwrap_or_default();
                self.mode = Mode::Edit(Edit {
                    path,
                    buffer,
                    target: EditTarget::ListItem(Some(index)),
                });
                Ok(())
            }
            Action::ListAdd => {
                let Some(path) = self.level_leaf() else {
                    return Ok(());
                };
                self.mode = Mode::Edit(Edit {
                    path,
                    buffer: String::new(),
                    target: EditTarget::ListItem(None),
                });
                Ok(())
            }
            Action::Save(scope) => self.save(context, scope),
        }
    }

    /// Act on the leaf at `path`.
    fn activate_leaf(&mut self, context: &mut dyn Context, path: &str) -> Result<()> {
        let Some(leaf) = self.leaf(path) else {
            return Ok(());
        };
        let value = self.backend.get(path).unwrap_or(Value::Unset);
        match &leaf.kind {
            Kind::Bool => {
                // An unset optional bool reads as off.
                let current = value == Value::Bool(true);
                if self.write(context, path, Value::Bool(!current))? {
                    self.changed(context)?;
                }
                Ok(())
            }
            Kind::Choice(variants) => {
                let selected = match &value {
                    Value::Choice(name) => variants
                        .iter()
                        .position(|variant| variant.name == name)
                        .unwrap_or_default(),
                    _ => 0,
                };
                self.push(context, LevelKind::Variants(path.to_string()), selected)
            }
            Kind::Set(_) => self.push(context, LevelKind::Set(path.to_string()), 0),
            Kind::Order(_) => self.push(context, LevelKind::Order(path.to_string()), 0),
            Kind::List => self.push(context, LevelKind::List(path.to_string()), 0),
            Kind::Int { .. } | Kind::Text => {
                let buffer = match value {
                    Value::Unset => String::new(),
                    other => other.to_string(),
                };
                self.mode = Mode::Edit(Edit {
                    path: path.to_string(),
                    buffer,
                    target: EditTarget::Value,
                });
                Ok(())
            }
        }
    }

    /// Commit the inline edit.
    fn commit(&mut self, context: &mut dyn Context, edit: Edit) -> Result<()> {
        let Some(leaf) = self.leaf(&edit.path) else {
            return Ok(());
        };
        let text = edit.buffer.trim().to_string();
        let value = match edit.target {
            EditTarget::Value => match &leaf.kind {
                _ if text.is_empty() && leaf.optional => Value::Unset,
                Kind::Int { .. } => {
                    let Ok(number) = text.parse::<i64>() else {
                        context.notify(NoticePost::error(format!("{text:?} is not an integer")))?;
                        self.mode = Mode::Edit(edit);
                        return Ok(());
                    };
                    Value::Int(number)
                }
                _ => Value::Text(edit.buffer.clone()),
            },
            EditTarget::ListItem(index) => {
                let mut items = self.list_items(&edit.path);
                match index {
                    Some(index) if text.is_empty() => {
                        if index < items.len() {
                            items.remove(index);
                        }
                    }
                    Some(index) => {
                        if let Some(item) = items.get_mut(index) {
                            item.clone_from(&text);
                        }
                    }
                    None if text.is_empty() => {
                        self.mode = Mode::Browse;
                        return Ok(());
                    }
                    None => items.push(text),
                }
                list_value(&leaf, items)
            }
        };
        self.mode = Mode::Browse;
        if self.write(context, &edit.path, value)? {
            self.changed(context)?;
        } else {
            self.mode = Mode::Edit(edit);
        }
        Ok(())
    }

    /// Delete the selected list item.
    fn delete_item(&mut self, context: &mut dyn Context) -> Result<()> {
        let Some(path) = self.level_leaf() else {
            return Ok(());
        };
        let Some(leaf) = self.leaf(&path) else {
            return Ok(());
        };
        let index = self.level().selected;
        let mut items = self.list_items(&path);
        if index >= items.len() {
            return Ok(());
        }
        items.remove(index);
        if self.write(context, &path, list_value(&leaf, items))? {
            self.move_by(0);
            self.changed(context)?;
        }
        Ok(())
    }

    /// Move the selected list item by `delta` places.
    fn shift_item(&mut self, context: &mut dyn Context, delta: isize) -> Result<()> {
        let Some(path) = self.level_leaf() else {
            return Ok(());
        };
        let Some(leaf) = self.leaf(&path) else {
            return Ok(());
        };
        let index = self.level().selected;
        let mut items = self.list_items(&path);
        let Some(target) = index.checked_add_signed(delta) else {
            return Ok(());
        };
        if index >= items.len() || target >= items.len() {
            return Ok(());
        }
        items.swap(index, target);
        if self.write(context, &path, list_value(&leaf, items))? {
            self.level_mut().selected = target;
            self.changed(context)?;
        }
        Ok(())
    }

    /// Run `intent`.
    fn run(&mut self, context: &mut dyn Context, intent: Intent) -> Result<()> {
        match intent {
            Intent::Nav(nav) => self.navigate(nav),
            Intent::Activate => self.activate(context)?,
            Intent::Forward => self.forward(context)?,
            Intent::Pop | Intent::Back => self.back(context)?,
            Intent::Save => self.push(context, LevelKind::Save, 0)?,
            Intent::Revert => {
                if let Some(path) = self.selected_leaf() {
                    self.backend.revert(context, Some(&path))?;
                }
            }
            Intent::RevertAll => {
                self.backend.revert(context, None)?;
                context.notify(NoticePost::info("Reverted unsaved options"))?;
            }
            Intent::Clear => {
                if let Some(path) = self.selected_leaf() {
                    let result = self.backend.clear(context, &path)?;
                    report(context, result, |file| match file {
                        Some(file) => format!("Cleared from {}", display_path(&file)),
                        None => "No option file sets this option".into(),
                    })?;
                }
            }
            Intent::Increase => self.step(context, 1)?,
            Intent::Decrease => self.step(context, -1)?,
            Intent::Delete => self.delete_item(context)?,
            Intent::ShiftUp => self.shift_item(context, -1)?,
            Intent::ShiftDown => self.shift_item(context, 1)?,
            Intent::ToggleAll => self.toggle_all(context)?,
            Intent::Commit => {
                if let Mode::Edit(edit) = self.mode.clone() {
                    self.commit(context, edit)?;
                }
            }
            Intent::CancelEdit => {
                self.mode = Mode::Browse;
            }
            Intent::Erase => {
                if let Mode::Edit(edit) = &mut self.mode {
                    edit.buffer.pop();
                }
            }
            Intent::EraseAll => {
                if let Mode::Edit(edit) = &mut self.mode {
                    edit.buffer.clear();
                }
            }
        }
        Ok(())
    }

    /// Move the selection as `nav` says.
    fn navigate(&mut self, nav: NavIntent) {
        let page = 10;
        match nav {
            NavIntent::Up => self.move_by(-1),
            NavIntent::Down => self.move_by(1),
            NavIntent::PageUp => self.move_by(-page),
            NavIntent::PageDown => self.move_by(page),
            NavIntent::First => self.level_mut().selected = 0,
            NavIntent::Last => self.move_by(isize::MAX),
            NavIntent::Left | NavIntent::Right => {}
        }
    }

    /// Take a character that [`Self::handles`] accepts: add it to the edit,
    /// or activate the row of its accelerator.
    fn type_char(&mut self, context: &mut dyn Context, c: char) -> Result<()> {
        if let Mode::Edit(edit) = &mut self.mode {
            edit.buffer.push(c);
            return Ok(());
        }
        if let Some(index) = self.accelerator(c) {
            self.level_mut().selected = index;
            self.activate(context)?;
        }
        Ok(())
    }

    /// Step the selected integer by `delta`.
    fn step(&mut self, context: &mut dyn Context, delta: i64) -> Result<()> {
        if let Some(path) = self.selected_leaf()
            && let Some(leaf) = self.leaf(&path)
            && let Some(value) = self.backend.get(&path)
            && let Some(next) = step_value(&leaf, &value, delta)
            && self.write(context, &path, next)?
        {
            self.changed(context)?;
        }
        Ok(())
    }

    /// Step the choice of the selected row to its next variant, or act on
    /// any other row.
    fn forward(&mut self, context: &mut dyn Context) -> Result<()> {
        if let Some(Action::Leaf(path)) = self.selected_action()
            && let Some(leaf) = self.leaf(&path)
            && matches!(leaf.kind, Kind::Choice(_))
        {
            let current = self.backend.get(&path).unwrap_or(Value::Unset);
            if let Some(next) = step_value(&leaf, &current, 1)
                && self.write(context, &path, next)?
            {
                self.changed(context)?;
            }
            return Ok(());
        }
        self.activate(context)
    }

    /// Save the unsaved values to the file of `scope`, and leave the save
    /// level.
    fn save(&mut self, context: &mut dyn Context, scope: Scope) -> Result<()> {
        self.levels.pop();
        self.retitle(context)?;
        let result = self.backend.save(context, scope)?;
        let saved = result.is_ok();
        report(context, result, |file| {
            format!("Saved to {}", display_path(&file))
        })?;
        if saved {
            self.changed(context)?;
        }
        Ok(())
    }

    /// Return the width the text of the panel wants.
    fn natural_width(&self) -> u32 {
        let rows: Vec<MenuRow> = self.rows().iter().map(Row::menu_row).collect();
        let layout = row_layout(&rows);
        let row_width = rows
            .iter()
            .map(|row| row.width(layout))
            .max()
            .unwrap_or_default();
        let title = text::width(&self.title()).saturating_add(4);
        row_width.max(title).clamp(MIN_WIDTH, MAX_WIDTH)
    }

    /// Return the height the panel wants: a line for each row.
    fn natural_height(&self) -> u32 {
        u32::try_from(self.rows().len().max(1)).unwrap_or(u32::MAX)
    }
}

/// Report the result of a store action as a toast.
fn report<T>(
    context: &mut dyn Context,
    result: StdResult<T, StoreError>,
    done: impl FnOnce(T) -> String,
) -> Result<()> {
    context.notify(match result {
        Ok(value) => NoticePost::info(done(value)),
        Err(error) => NoticePost::error(error.to_string()),
    })
}

/// Return a list value for `leaf`, which is unset when an optional list has
/// no items.
fn list_value(leaf: &Leaf, items: Vec<String>) -> Value {
    if items.is_empty() && leaf.optional {
        Value::Unset
    } else {
        Value::List(items)
    }
}

/// Return one row for each variant of `leaf`, with the mark and the action
/// that `row` gives for the variant's name.
fn variant_rows(leaf: &Leaf, row: impl Fn(&str) -> (&'static str, Action)) -> Vec<Row> {
    leaf.kind
        .variants()
        .iter()
        .map(|variant| {
            let (value, action) = row(variant.name);
            Row {
                key: variant.key,
                label: variant.label.to_string(),
                value: value.into(),
                muted: false,
                mark: " ",
                action,
            }
        })
        .collect()
}

/// Return the control text of a leaf value, and whether it reads quietly.
fn control(leaf: &Leaf, value: &Value) -> (String, bool) {
    match (&leaf.kind, value) {
        // A set shows its members only in its checklist, so the row stays
        // short whatever the set holds.
        (Kind::Set(_), _) => ("...".into(), true),
        (Kind::Order(_), _) => (display_value(leaf, value), false),
        (_, Value::Unset) => ("default".into(), true),
        (Kind::Bool, Value::Bool(true)) => ("[x]".into(), false),
        (Kind::Bool, _) => ("[ ]".into(), false),
        (Kind::Int { .. }, Value::Int(number)) => (format!("‹ {number} ›"), false),
        (Kind::Choice(_), _) => (format!("{} ▾", display_value(leaf, value)), false),
        (Kind::Text, Value::Text(text)) if text.is_empty() => ("(empty)".into(), true),
        (Kind::List, Value::List(items)) => {
            let count = items.len();
            let noun = if count == 1 { "item" } else { "items" };
            (format!("{count} {noun} ..."), false)
        }
        _ => (value.to_string(), false),
    }
}

/// Return `path` for display, with the home directory as `~`.
fn display_path(path: &Path) -> String {
    let home = env::var_os("HOME").map(PathBuf::from);
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

impl Widget for PanelBody {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Column)
            .padding(Edges::new(0, 1, 0, 1))
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        constraints.clamp(Size::new(self.natural_width(), self.natural_height()))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        view
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn key_outcome(&self, key: Key, _context: &dyn ViewContext) -> EventOutcome {
        if self.handles(key) {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn on_event(&mut self, event: &Event, context: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Key(key) = event else {
            return Ok(EventOutcome::Ignore);
        };
        if !self.handles(*key) {
            return Ok(EventOutcome::Ignore);
        }
        if let KeyCode::Char(c) = key.normalize().key {
            self.type_char(context, c)?;
        }
        self.move_by(0);
        Ok(EventOutcome::Handle)
    }

    fn accepts_intent(&self, intent: &str, _context: &dyn ViewContext) -> bool {
        Intent::from_name(intent).is_some_and(|intent| self.accepts(intent))
    }

    fn on_intent(&mut self, intent: &str, context: &mut dyn Context) -> Result<EventOutcome> {
        let Some(intent) = Intent::from_name(intent).filter(|intent| self.accepts(*intent)) else {
            return Ok(EventOutcome::Ignore);
        };
        self.run(context, intent)?;
        // A revert or a clear can shorten the rows, such as a list's items.
        self.move_by(0);
        Ok(EventOutcome::Handle)
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.fill("background", context.view().outer_rect_local(), ' ')?;
        let rect = context.view().view_rect_local();
        if rect.w == 0 || rect.h == 0 {
            return Ok(());
        }
        let rows: Vec<MenuRow> = self.rows().iter().map(Row::menu_row).collect();
        let layout = row_layout(&rows);
        let visible = rect.h.max(1) as usize;
        let selected = self.level().selected.min(rows.len().saturating_sub(1));
        let offset = selected.saturating_sub(visible.saturating_sub(1));
        let mut y = rect.tl.y;
        for (index, row) in rows.iter().enumerate().skip(offset).take(visible) {
            let edit = match &self.mode {
                Mode::Edit(edit) if index == selected => Some(edit),
                _ => None,
            };
            menu::render_row(
                render,
                Rect::new(rect.tl.x, y, rect.w, 1),
                row,
                layout,
                index == selected,
                edit.map(|edit| edit.buffer.as_str()),
            )?;
            y += 1;
        }
        if rows.is_empty() {
            render.text("row/muted", Line::new(rect.tl.x, y, rect.w), "(no options)")?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("options_body")
    }
}
