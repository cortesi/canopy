//! A menu of keyed rows, chosen with the keyboard.
//!
//! Each row names one action: its key, a label, and a value at the right,
//! such as a setting's current state. In a selectable menu, the arrows, `j`
//! and `k`, the page keys, and Home and End move the selection, Enter or Space
//! activates the selected row, a row's key activates that row, and Esc closes.
//! A menu without selection only lists its rows, as the menu of a transient
//! mode does while the mode waits for its key.
//!
//! [`Menu`] is the modal: it centres a titled frame over whatever it covers
//! and holds a [`MenuList`] of rows. A host opens it with Canopy's modal, as
//! it opens a [`Picker`](crate::Picker). A panel that keeps rows of its own,
//! such as an options panel with levels and inline edits, draws them with
//! [`render_row`] and the same styles, so every menu reads the same.
//!
//! The rows draw under the `menu` style layer: `row`, `row/key`,
//! `row/value`, `row/muted`, `row/mark`, `selection`, and `edit`. [`styles`]
//! derives them from the palette.

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    Widget,
    commands::CommandCall,
    derive_commands,
    error::{Error, Result},
    geom::{Line, Rect, Size},
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, Event, FrameworkBindingGroup,
        IntentName, NavIntent,
        key::{Key, KeyCode},
    },
    layout::{CanvasContext, Direction, Edges, Layout, MeasureConstraints, Measurement},
    render::Render,
    style::{PartialStyle, StyleRules, roles, themes::Palette},
    text,
};

use crate::{
    Dialog,
    frame::Frame,
    row_cursor::{CursorMove, RowCursor},
};

/// Columns of the mark gutter after a row, for a list that shows marks. `●`
/// has ambiguous width, so the gutter keeps a blank cell after the mark for a
/// font that draws it wide.
const MARK_GUTTER: u32 = 2;
/// Narrowest a selectable menu's rows get, so a short menu still reads as a
/// dialog.
const MIN_WIDTH: u32 = 24;

/// One row of a menu: what it shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuRow {
    /// The key that activates the row, such as `c`, or the keys that share
    /// it, such as `j, down`. Empty for none.
    pub key: String,
    /// The label.
    pub label: String,
    /// The value at the right, such as a setting's current state.
    pub value: String,
    /// Whether the value reads quietly, as for a default.
    pub muted: bool,
    /// A mark in the gutter after the row, such as where a value comes from.
    /// It shows only in a list that shows marks.
    pub mark: &'static str,
}

impl MenuRow {
    /// Return a row with the accelerator `key` and `label`, and no value.
    #[must_use]
    pub fn new(key: Option<char>, label: impl Into<String>) -> Self {
        Self::keyed(key.map_or_else(String::new, String::from), label)
    }

    /// Return a row that names its keys as `keys`, such as `ctrl+r` or
    /// `j, down`, with `label`, and no value.
    #[must_use]
    pub fn keyed(keys: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: keys.into(),
            label: label.into(),
            ..Self::default()
        }
    }

    /// Return the row with `value` at the right.
    #[must_use]
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Return the row with its value muted, or not.
    #[must_use]
    pub const fn with_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// Return the row with `mark` in its gutter.
    #[must_use]
    pub const fn with_mark(mut self, mark: &'static str) -> Self {
        self.mark = mark;
        self
    }

    /// Return the width that shows the whole row in `layout`.
    #[must_use]
    pub fn width(&self, layout: RowLayout) -> u32 {
        let value = text::width(&self.value);
        // A blank cell on each side of the value keeps it off the label and
        // off the end of the selection.
        let gap = if value > 0 { 2 } else { 1 };
        layout
            .key_columns
            .saturating_add(text::width(&self.label))
            .saturating_add(value)
            .saturating_add(gap)
            .saturating_add(if layout.marks { MARK_GUTTER } else { 0 })
    }
}

/// The columns that every row of one menu shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLayout {
    /// Columns before the label: the widest key and a blank.
    pub key_columns: u32,
    /// Whether the rows keep a mark gutter at the right.
    pub marks: bool,
}

impl RowLayout {
    /// Return the layout that fits `rows`: a key column as wide as the widest
    /// key, and a mark gutter if any row has a mark.
    pub fn fit<'a>(rows: impl IntoIterator<Item = &'a MenuRow>) -> Self {
        let mut layout = Self {
            key_columns: 2,
            marks: false,
        };
        for row in rows {
            layout.key_columns = layout.key_columns.max(text::width(&row.key) + 1);
            layout.marks |= !row.mark.is_empty();
        }
        layout
    }
}

/// Draw `row` in `area`, one line in `layout`, under the `menu` style layer.
///
/// `selected` draws the row selected. `edit`, if given, replaces the value
/// with the text being typed and a cursor. The mark gutter stays outside the
/// selection, so a mark keeps its colour on a selected row.
pub fn render_row(
    render: &mut Render,
    area: Rect,
    row: &MenuRow,
    layout: RowLayout,
    selected: bool,
    edit: Option<&str>,
) -> Result<()> {
    let x = area.tl.x;
    let y = area.tl.y;
    let key_columns = layout.key_columns.min(area.w);
    render.text("row/key", Line::new(x, y, key_columns), &row.key)?;
    let body_x = x.saturating_add(key_columns);
    let gutter = if layout.marks { MARK_GUTTER } else { 0 };
    let body_w = area.w.saturating_sub(key_columns + gutter);
    let base = if selected {
        roles::selection(true)
    } else {
        "row"
    };
    if selected {
        render.fill(base, Rect::new(body_x, y, body_w, 1), ' ')?;
    }
    let value = match edit {
        Some(buffer) => format!("{buffer}█"),
        None => row.value.clone(),
    };
    // The label shows whole, and a value too long for the rest, such as a
    // path or an edit, keeps its end.
    let room = body_w.saturating_sub(text::width(&row.label) + 2);
    let value = text::truncate_start(&value, room as usize).into_owned();
    let value_w = text::width(&value).min(room);
    // A row without a value keeps one blank cell at the end of the selection,
    // as MenuRow::width counts.
    let gap = if value_w > 0 { 2 } else { 1 };
    let label_w = body_w.saturating_sub(value_w + gap);
    render.text(base, Line::new(body_x, y, label_w), &row.label)?;
    let value_x = body_x + body_w.saturating_sub(value_w + 1);
    let value_style = match (selected, edit.is_some(), row.muted) {
        (_, true, _) => "edit",
        (true, _, _) => base,
        (false, _, true) => "row/muted",
        (false, _, false) => "row/value",
    };
    render.text(value_style, Line::new(value_x, y, value_w), &value)?;
    if layout.marks {
        render.text(
            "row/mark",
            Line::new(
                body_x + body_w,
                y,
                1.min(area.w.saturating_sub(key_columns)),
            ),
            row.mark,
        )?;
    }
    Ok(())
}

/// Install the styles of the `menu` layer, from the palette.
pub fn styles(palette: &Palette, rules: StyleRules<'_>) {
    let panel = |fg| PartialStyle::new().fg(fg).bg(palette.panel_bg);
    rules
        .style("menu/background", panel(palette.fg))
        .style("menu/row", panel(palette.fg))
        .style("menu/row/key", panel(palette.accent))
        .style("menu/row/value", panel(palette.cyan))
        .style("menu/row/muted", panel(palette.muted_fg))
        .style("menu/row/mark", panel(palette.yellow))
        .style(
            "menu/selection",
            PartialStyle::new().fg(palette.bg).bg(palette.accent),
        )
        .style(
            "menu/edit",
            PartialStyle::new().fg(palette.fg).bg(palette.element_bg),
        )
        .apply();
}

/// One row of a [`MenuList`], and what activating it posts.
#[derive(Clone, Debug)]
pub struct MenuItem {
    /// What the row shows.
    pub row: MenuRow,
    /// The call that activating the row posts, if it does anything.
    pub call: Option<CommandCall>,
}

impl MenuItem {
    /// Return an item that shows `row` and posts `call` when activated.
    #[must_use]
    pub const fn new(row: MenuRow, call: CommandCall) -> Self {
        Self {
            row,
            call: Some(call),
        }
    }

    /// Return an item that shows `row` and does nothing, such as a note
    /// that a menu has nothing to offer.
    #[must_use]
    pub const fn note(row: MenuRow) -> Self {
        Self { row, call: None }
    }
}

/// A modal menu: a titled frame that holds a [`MenuList`].
///
/// The host fills the list with [`Menu::set_items`], opens the menu with
/// Canopy's modal, giving it [`Menu::list`] as the initial focus and
/// [`Menu::BINDINGS`] as its binding group, and closes it from the close call
/// that the list posts.
pub struct Menu {
    /// The title the frame shows.
    title: String,
    /// The frame, once mounted.
    frame: Option<TypedId<Frame>>,
    /// The list, once mounted.
    list: Option<TypedId<MenuList>>,
}

#[derive_commands]
impl Menu {
    /// The framework group a menu's modal admits.
    ///
    /// The arrows, `j` and `k`, the page keys, and Home and End move the
    /// selection, Enter and Space activate it, and Esc closes.
    pub const BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("menu");

    /// Construct an empty menu titled `title`.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            frame: None,
            list: None,
        }
    }

    /// Return the list, which takes the keyboard while the menu is open, or
    /// an error before the menu mounts.
    pub fn list(&self) -> Result<NodeId> {
        self.list
            .map(Into::into)
            .ok_or_else(|| Error::Invalid("the menu is not mounted".into()))
    }

    /// Show `items` under `title`, with the row at `selected` selected.
    ///
    /// `close` is the call that closes the menu: the list posts it after an
    /// item's own call, and for Esc.
    pub fn set_items(
        &mut self,
        context: &mut dyn Context,
        title: impl Into<String>,
        items: Vec<MenuItem>,
        selected: usize,
        close: CommandCall,
    ) -> Result<()> {
        self.title = title.into();
        if let Some(frame) = self.frame {
            let title = self.title.clone();
            context.with_widget_mut(frame, |frame: &mut Frame, _| {
                frame.set_title(title);
                Ok(())
            })?;
        }
        let list = self
            .list
            .ok_or_else(|| Error::Invalid("the menu is not mounted".into()))?;
        context.with_widget_mut(list, |list: &mut MenuList, _| {
            list.set_items(items, Some(selected));
            list.set_close(Some(close));
            Ok(())
        })
    }
}

impl Widget for Menu {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("menu");
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let me = context.node_id();
        let (dialog, list) =
            Dialog::new()
                .with_title(self.title.clone())
                .add(context, me, MenuList::new())?;
        self.frame = Some(Dialog::frame(context, dialog.into())?);
        self.list = Some(list);
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("menu")
    }
}

/// The rows of a menu.
///
/// A selectable list takes the keyboard: it moves its selection, and
/// activates rows. A list without selection takes no focus and no keys, and
/// only shows its rows, in the same columns and styles.
pub struct MenuList {
    /// The rows.
    items: Vec<MenuItem>,
    /// The selected row.
    cursor: RowCursor,
    /// The call that closes the menu.
    close: Option<CommandCall>,
    /// The columns the rows share.
    layout: RowLayout,
    /// Whether the list has a selection and takes keys.
    selectable: bool,
}

#[derive_commands]
impl MenuList {
    /// Construct an empty, selectable list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            cursor: RowCursor::new(0),
            close: None,
            layout: RowLayout::fit([]),
            selectable: true,
        }
    }

    /// Return the list with a selection, or without one.
    #[must_use]
    pub const fn with_selection(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Show `items`, with the row at `selected` selected, or the first row.
    pub fn set_items(&mut self, items: Vec<MenuItem>, selected: Option<usize>) {
        self.layout = RowLayout::fit(items.iter().map(|item| &item.row));
        self.cursor = RowCursor::new(items.len());
        self.cursor.set_index(selected.unwrap_or(0));
        self.items = items;
    }

    /// Show `rows`, which do nothing, as a list without selection does.
    pub fn set_rows(&mut self, rows: Vec<MenuRow>) {
        self.set_items(rows.into_iter().map(MenuItem::note).collect(), None);
    }

    /// Set the call that closes the menu: the list posts it after a row's
    /// own call, and for Esc.
    pub fn set_close(&mut self, close: Option<CommandCall>) {
        self.close = close;
    }

    /// Return the index of the selected row, if the list has a selection.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.cursor.index().filter(|_| self.selectable)
    }

    /// Activate the selected row: post its call, then the close call.
    #[command]
    pub fn activate(&mut self, context: &mut dyn Context) -> Result<()> {
        match self.selected() {
            Some(index) => self.activate_row(context, index),
            None => Ok(()),
        }
    }

    /// Close the menu by posting the close call.
    #[command]
    pub fn cancel(&mut self, context: &mut dyn Context) -> Result<()> {
        if let Some(call) = &self.close {
            context.post(call)?;
        }
        Ok(())
    }

    /// Post the call of the row at `index`, then the close call. A row with
    /// no call does nothing.
    fn activate_row(&mut self, context: &mut dyn Context, index: usize) -> Result<()> {
        let Some(call) = self.items.get(index).and_then(|item| item.call.clone()) else {
            return Ok(());
        };
        self.cursor.set_index(index);
        context.post(&call)?;
        self.cancel(context)
    }

    /// Return the row whose accelerator `key` is, for a plain character, in
    /// a selectable list.
    fn accelerated(&self, key: Key) -> Option<usize> {
        let key = key.normalize();
        if !self.selectable || key.mods.ctrl || key.mods.alt {
            return None;
        }
        let KeyCode::Char(c) = key.key else {
            return None;
        };
        let mut buffer = [0; 4];
        let c: &str = c.encode_utf8(&mut buffer);
        self.items.iter().position(|item| item.row.key == c)
    }

    /// Return the width that shows every row.
    fn natural_width(&self) -> u32 {
        self.items
            .iter()
            .map(|item| item.row.width(self.layout))
            .max()
            .unwrap_or(0)
            .max(if self.selectable { MIN_WIDTH } else { 0 })
    }
}

impl Default for MenuList {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for MenuList {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        setup.add_commands::<Menu>()?;
        setup.widget_styles(styles);
        let nav = |intent: NavIntent| -> Result<BindingAction> {
            Ok(BindingAction::Intent(IntentName::new(intent.name())?))
        };
        let bindings: [(&str, &str, BindingAction); 11] = [
            ("Up", "Previous row", nav(NavIntent::Up)?),
            ("k", "Previous row", nav(NavIntent::Up)?),
            ("Down", "Next row", nav(NavIntent::Down)?),
            ("j", "Next row", nav(NavIntent::Down)?),
            ("PageUp", "Page up", nav(NavIntent::PageUp)?),
            ("PageDown", "Page down", nav(NavIntent::PageDown)?),
            ("Home", "First row", nav(NavIntent::First)?),
            ("End", "Last row", nav(NavIntent::Last)?),
            (
                "Enter",
                "Activate the selected row",
                BindingAction::Command(Self::call_activate()),
            ),
            (
                "Space",
                "Activate the selected row",
                BindingAction::Command(Self::call_activate()),
            ),
            (
                "Esc",
                "Close the menu",
                BindingAction::Command(Self::call_cancel()),
            ),
        ];
        for (key, description, action) in bindings {
            // Intents always run before the widget; commands run after it, so
            // a row's accelerator reaches the list first.
            let phase = match action {
                BindingAction::Intent(_) => None,
                _ => Some(BindingPhase::AfterWidget),
            };
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    show_in_help: true,
                    path: Some("**/menu/**/".parse()?),
                    tier: BindingTier::Framework(Menu::BINDINGS),
                    description: description.to_string(),
                    source: None,
                    phase,
                },
                action,
            )?;
        }
        Ok(())
    }
}

impl Widget for MenuList {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Column)
            .padding(Edges::new(0, 1, 0, 1))
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let rows = u32::try_from(self.items.len().max(1)).unwrap_or(u32::MAX);
        constraints.clamp(Size::new(self.natural_width(), rows))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        view
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        self.selectable
    }

    fn key_outcome(&self, key: Key, _context: &dyn ViewContext) -> EventOutcome {
        if self.accelerated(key).is_some() {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn on_event(&mut self, event: &Event, context: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Key(key) = event else {
            return Ok(EventOutcome::Ignore);
        };
        let Some(index) = self.accelerated(*key) else {
            return Ok(EventOutcome::Ignore);
        };
        self.activate_row(context, index)?;
        Ok(EventOutcome::Handle)
    }

    fn accepts_intent(&self, intent: &str, _context: &dyn ViewContext) -> bool {
        self.selectable && !self.items.is_empty() && CursorMove::of(intent).is_some()
    }

    fn on_intent(&mut self, intent: &str, context: &mut dyn Context) -> Result<EventOutcome> {
        let rows = context.view().view_rect().h;
        match CursorMove::of(intent) {
            Some(CursorMove::By(delta)) => self.cursor.select_by(delta),
            Some(CursorMove::Page(delta)) => self.cursor.page(delta, rows),
            Some(CursorMove::First) => self.cursor.select_first(),
            Some(CursorMove::Last) => self.cursor.select_last(),
            None => return Ok(EventOutcome::Ignore),
        }
        self.cursor.reveal(context);
        Ok(EventOutcome::Handle)
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        // The list paints the menu's parts wherever it is mounted.
        render.push_layer("menu");
        let view = context.view();
        render.fill("background", view.outer_rect_local(), ' ')?;
        let rect = view.view_rect_local();
        if rect.w == 0 || rect.h == 0 {
            return Ok(());
        }
        let selected = self.selected();
        let end = rect.tl.y.saturating_add(rect.h);
        for ((y, index), item) in (rect.tl.y..end).zip(0..).zip(&self.items) {
            render_row(
                render,
                Rect::new(rect.tl.x, y, rect.w, 1),
                &item.row,
                self.layout,
                selected == Some(index),
                None,
            )?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("menu_list")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        Context, ContextExt, NodeId, Widget, commands::CommandTarget, error::Result,
        input::key::Key, testing::harness::Harness,
    };

    use super::{Menu, MenuItem, MenuList, MenuRow, RowLayout};

    /// Holds a menu, and records what its rows post.
    #[derive(Default)]
    struct MenuHost {
        /// The menu, once mounted.
        menu: Option<NodeId>,
        /// What each call posted, in order.
        posted: Vec<String>,
    }

    #[canopy::derive_commands]
    impl MenuHost {
        /// Record a chosen row.
        #[command]
        fn chose(&mut self, name: String) {
            self.posted.push(name);
        }

        /// Record a close.
        #[command]
        fn closed(&mut self) {
            self.posted.push("closed".into());
        }
    }

    impl Widget for MenuHost {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            self.menu = Some(ctx.add_child(ctx.node_id(), Menu::new("menu"))?.into());
            Ok(())
        }
    }

    /// Build a host whose menu shows rows `a` and `b`, and a note, with `b`
    /// selected. Return the harness and the list.
    fn hosting() -> Result<(Harness, NodeId)> {
        let mut harness = Harness::builder(MenuHost::default())
            .configure(|setup| setup.add_commands::<MenuHost>())
            .register::<MenuList>()
            .size(40, 12)
            .build()?;
        harness.render()?;
        let root = harness.root;
        let menu = harness
            .with_root_widget(|host: &mut MenuHost| host.menu)
            .expect("the menu mounted");
        let owner = CommandTarget::Exact(root);
        let item = |key: char, name: &str| {
            MenuItem::new(
                MenuRow::new(Some(key), name),
                MenuHost::call_chose(name.to_owned()).with_target(owner),
            )
        };
        let items = vec![
            item('a', "a"),
            item('b', "b"),
            MenuItem::note(MenuRow::new(None, "a note")),
        ];
        let list = harness.canopy.with_context(menu, |ctx| {
            ctx.with_widget_mut(menu, |menu: &mut Menu, ctx| {
                menu.set_items(
                    ctx,
                    "Pick",
                    items,
                    1,
                    MenuHost::call_closed().with_target(owner),
                )?;
                menu.list()
            })
        })?;
        harness
            .canopy
            .with_context(list, |ctx| ctx.set_focus(list).map(|_| ()))?;
        harness.render()?;
        Ok((harness, list))
    }

    /// Return what the host recorded.
    fn posted(harness: &mut Harness) -> Vec<String> {
        harness.with_root_widget(|host: &mut MenuHost| host.posted.clone())
    }

    #[test]
    fn activating_a_row_posts_its_call_then_the_close() -> Result<()> {
        let (mut harness, list) = hosting()?;
        harness.canopy.with_context(list, |ctx| {
            ctx.with_widget_mut(list, |list: &mut MenuList, ctx| list.activate(ctx))
        })?;
        assert_eq!(posted(&mut harness), ["b", "closed"]);
        Ok(())
    }

    #[test]
    fn a_rows_key_activates_it_and_a_note_does_nothing() -> Result<()> {
        let (mut harness, list) = hosting()?;
        harness.key(Key::parse_spec("a")?)?;
        assert_eq!(posted(&mut harness), ["a", "closed"]);
        harness.canopy.with_context(list, |ctx| {
            ctx.with_widget_mut(list, |list: &mut MenuList, ctx| {
                list.cursor.set_index(2);
                list.activate(ctx)?;
                list.cancel(ctx)
            })
        })?;
        assert_eq!(
            posted(&mut harness),
            ["a", "closed", "closed"],
            "a note posts nothing, and a cancel posts the close"
        );
        Ok(())
    }

    #[test]
    fn a_row_measures_its_parts() {
        let row = MenuRow::new(Some('c'), "columns").with_value("...");
        let plain = RowLayout::fit([&row]);
        assert_eq!(row.width(plain), 2 + 7 + 3 + 2);
        let marked = RowLayout {
            marks: true,
            ..plain
        };
        assert_eq!(row.width(marked), 2 + 7 + 3 + 2 + 2);
        assert_eq!(MenuRow::new(None, "note").width(plain), 2 + 4 + 1);
        let next = MenuRow::keyed("j, down", "next");
        let wide = RowLayout::fit([&row, &next]);
        assert_eq!(wide.key_columns, 8, "the widest key sets the column");
    }

    #[test]
    fn a_list_without_selection_takes_no_keys() -> Result<()> {
        let mut list = MenuList::new().with_selection(false);
        list.set_rows(vec![MenuRow::keyed("f", "files")]);
        assert_eq!(list.selected(), None);
        assert_eq!(list.accelerated(Key::parse_spec("f")?), None);
        Ok(())
    }
}
