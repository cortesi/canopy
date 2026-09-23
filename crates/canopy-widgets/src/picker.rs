//! A modal list of items, filtered and chosen with the keyboard.
//!
//! [`Picker`] is the modal subtree: it centres a titled frame over whatever it
//! covers, and holds a [`PickerList`] of items above an [`Input`] filter field.
//! The host opens it with Canopy's modal, which gives the list the
//! keyboard and dims the application behind it.
//!
//! The widget owns the list and the filter, and nothing else. It never reads
//! or writes what it shows, so a host can back it with bookmarks, a history, or
//! anything else that renders as one line. A host that needs more than
//! choosing, such as a question before a row is removed, opens its own modal
//! in the overlay layer (`ContextExt::add_modal`) with the picker as its
//! owner: the list keeps its filter and selection under it, and takes the
//! keyboard back when the overlay closes.

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    Widget,
    commands::CommandCall,
    derive_commands,
    error::{Error, Result},
    geom::{Line, Point, Size},
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, Event, FrameworkBindingGroup,
        IntentName, NavIntent,
        key::{self, Key, KeyCode},
    },
    layout::{
        CanvasContext, Direction, Edges, Layout, MeasureConstraints, Measurement, ScrollOp, Sizing,
    },
    render::Render,
    style::roles,
    text::Truncate,
};

use crate::{
    Dialog, ItemLabel,
    frame::Frame,
    input::{Input, register_clear_intent},
    row_cursor::{CursorMove, RowCursor, label_rows, widest_label},
};

/// Columns a row spends on its blank lead column and a trailing gutter.
const ROW_PADDING: u32 = 2;
/// Narrowest the modal frame gets, so a short list still reads as a dialog.
const MIN_FRAME_WIDTH: u32 = 32;
/// Rows and columns left around the frame, so the view shows through.
const FRAME_MARGIN: u32 = 1;
/// Rows the filter field occupies.
const FILTER_ROWS: u32 = 1;
/// The prompt before the filter, naming the key that opened it.
const FILTER_PROMPT: &str = " / ";

/// A centred modal holding a filtered list of items.
///
/// The widget draws nothing of its own. It centres its dialog and swallows
/// mouse input that lands on the margin around it. The root is a stack, so an
/// overlay a host adds with [`Picker::add_overlay`] draws over the dialog
/// within the same margin.
pub struct Picker<T>
where
    T: ItemLabel,
{
    /// The framed dialog, once mounted, which an overlay covers and dims.
    dialog: Option<NodeId>,
    /// The list of items, once mounted. The list titles the frame itself, so
    /// the frame needs no second owner.
    list: Option<TypedId<PickerList<T>>>,
    /// The filter field under the list, once mounted.
    filter: Option<NodeId>,
    /// Which end a row drops when it does not fit.
    truncate: Truncate,
    /// Call that accepting the selection runs.
    on_accept: Option<CommandCall>,
    /// Call that closing the picker runs.
    on_cancel: Option<CommandCall>,
}

impl<T> Default for Picker<T>
where
    T: ItemLabel + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl<T> Picker<T>
where
    T: ItemLabel + 'static,
{
    /// Build an empty picker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            dialog: None,
            list: None,
            filter: None,
            truncate: Truncate::default(),
            on_accept: None,
            on_cancel: None,
        }
    }

    /// Build a picker whose rows drop the given end when they do not fit.
    ///
    /// This is read when the list mounts, so set it before the picker is added
    /// to the tree.
    #[must_use]
    pub fn with_truncate(mut self, truncate: Truncate) -> Self {
        self.truncate = truncate;
        self
    }

    /// Show `items` under `title`, with `placeholder` in place of an empty
    /// list.
    ///
    /// The filter is dropped, so a picker always opens on the whole list.
    pub fn set_items(
        &mut self,
        context: &mut dyn Context,
        title: &str,
        placeholder: &'static str,
        items: Vec<T>,
    ) -> Result<()> {
        let list = self.typed_list()?;
        let title = title.to_owned();
        context.with_widget_mut(list, |list: &mut PickerList<T>, context| {
            list.set_items(context, title, placeholder, items)
        })
    }

    /// The framework group a picker's modal admits.
    ///
    /// The arrows, `j` and `k`, the page keys, and Home and End offer the
    /// navigation intents, `/` opens the filter, Enter accepts, and Esc
    /// closes. A host admits it beside its own group, which carries keys such
    /// as a delete.
    pub const BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("picker");

    /// Set what accepting the selection and closing the picker run.
    ///
    /// Both calls usually target the host, which reads the selection and
    /// closes the modal.
    pub fn set_commands(&mut self, accept: CommandCall, cancel: CommandCall) {
        self.on_accept = Some(accept);
        self.on_cancel = Some(cancel);
    }

    /// Accept the selection by running the host's accept call.
    ///
    /// The call runs from the picker rather than the list, so a host that
    /// reads the list while it accepts finds it free.
    #[command]
    pub fn accept(&mut self, context: &mut dyn Context) -> Result<()> {
        if let Some(call) = &self.on_accept {
            context.dispatch(call)?;
        }
        Ok(())
    }

    /// Close the picker by running the host's cancel call.
    #[command]
    pub fn cancel(&mut self, context: &mut dyn Context) -> Result<()> {
        if let Some(call) = &self.on_cancel {
            context.dispatch(call)?;
        }
        Ok(())
    }

    /// Return the list, which takes the keyboard while the modal is open, or
    /// an error before the picker mounts.
    pub fn list(&self) -> Result<NodeId> {
        self.typed_list().map(Into::into)
    }

    /// Return the filter field, or an error before the picker mounts.
    pub fn filter(&self) -> Result<NodeId> {
        self.filter
            .ok_or_else(|| Error::NotFound("picker filter".into()))
    }

    /// Return the typed list, or an error before the picker mounts.
    fn typed_list(&self) -> Result<TypedId<PickerList<T>>> {
        self.list
            .ok_or_else(|| Error::NotFound("picker list".into()))
    }
}

impl<T> Widget for Picker<T>
where
    T: ItemLabel + 'static,
{
    fn layout(&self) -> Layout {
        // A stack centres the dialog over the dimmed application, with any
        // overlay over that, and the margin keeps the view visible around
        // them.
        Layout::fill()
            .direction(Direction::Stack)
            .align_center()
            .padding(Edges::all(FRAME_MARGIN))
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let root = context.node_id();
        // The dialog fits its contents rather than filling the view, so a
        // short list is a small dialog. The picker's own margin caps a long
        // one, which then nearly fills the view. The width a narrow dialog
        // holds comes from the list's own measurement, which the view bounds,
        // rather than from a minimum here that a narrow terminal could not
        // honour. The list and the field are siblings in a column, so the
        // field sits outside whatever the list scrolls. However many items the
        // list holds, the field keeps its row at the bottom of the frame.
        let (dialog, body) = Dialog::new()
            .with_margin(Edges::all(0))
            .add(context, root, PickerBody)?;
        let dialog: NodeId = dialog.into();
        let frame: NodeId = Dialog::frame(context, dialog)?.into();
        let list = context.add_child(body, PickerList::<T>::new())?;
        // The field takes no focus and no keys of its own: the list owns the
        // keyboard while the modal is open, writes what it holds there, and
        // lights it up while it takes filter text.
        let filter = context.add_child(body, Input::new("").with_prompt(FILTER_PROMPT))?;
        // The list takes whatever height is left, and the field keeps its one
        // measured row, so a list too tall to fit scrolls instead of pushing
        // the field off the bottom.
        let rows: NodeId = list.into();
        let field: NodeId = filter.into();
        context.set_layout_override(rows, Layout::fill().into())?;
        context.set_layout_override(
            field,
            Layout::fill()
                .height(Sizing::Measure)
                .fixed_height(FILTER_ROWS)
                .into(),
        )?;

        let truncate = self.truncate;
        context.with_widget_mut(list, |list: &mut PickerList<T>, _| {
            list.frame = Some(frame);
            list.field = Some(field);
            list.truncate = truncate;
            Ok(())
        })?;
        self.dialog = Some(dialog);
        self.list = Some(list);
        self.filter = Some(field);
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        // A click on the margin belongs to the modal, not to what it covers.
        match event {
            Event::Mouse(_) => Ok(EventOutcome::Handle),
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("picker")
    }
}

/// The body of a picker's dialog, which carries the picker's style layer.
///
/// The layer is pushed here rather than at the picker's root, so an overlay a
/// host adds beside the dialog is styled as its own widget rather than as a
/// part of the picker, and the dialog's frame takes the shared dialog rules.
struct PickerBody;

impl Widget for PickerBody {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("picker");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("picker_body")
    }
}

/// A list of items, scrolled by its own view.
///
/// The list shows the items its filter passes, one per row, trimmed to the
/// width it is given. It titles the frame around it with its label and the row
/// count, and writes the filter into the field below it.
pub struct PickerList<T>
where
    T: ItemLabel,
{
    /// Every item the host supplied.
    items: Vec<T>,
    /// Positions in `items` that the filter passes, in order.
    shown: Vec<usize>,
    /// Cursor over `shown`, giving a position within it, not within `items`.
    cursor: RowCursor,
    /// Text a label must contain, ignoring ASCII case.
    filter: String,
    /// Whether the field is taking filter text.
    filtering: bool,
    /// What the frame's title calls this list.
    label: String,
    /// Text shown in place of an empty list.
    placeholder: &'static str,
    /// The frame this list titles, once mounted.
    frame: Option<NodeId>,
    /// The filter field this list writes, once mounted.
    field: Option<NodeId>,
    /// Which end a row drops when it does not fit.
    truncate: Truncate,
    /// Width that shows the widest label unclipped.
    fitted_width: u32,
}

#[derive_commands]
impl<T> PickerList<T>
where
    T: ItemLabel + 'static,
{
    /// Build an empty list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            shown: Vec::new(),
            cursor: RowCursor::new(0),
            filter: String::new(),
            filtering: false,
            label: String::new(),
            placeholder: "",
            frame: None,
            field: None,
            truncate: Truncate::default(),
            fitted_width: 0,
        }
    }

    /// Select the first item.
    #[command]
    pub fn select_first(&mut self, context: &mut dyn Context) -> Result<()> {
        self.cursor.select_first();
        self.refresh(context)
    }

    /// Select the last item.
    #[command]
    pub fn select_last(&mut self, context: &mut dyn Context) -> Result<()> {
        self.cursor.select_last();
        self.refresh(context)
    }

    /// Move the selection by a signed row count, saturating at both ends.
    /// @param delta Negative values move up; positive values move down.
    #[command]
    pub fn select_by(&mut self, context: &mut dyn Context, delta: i32) -> Result<()> {
        self.cursor.select_by(delta);
        self.refresh(context)
    }

    /// Move the selection by whole pages.
    /// @param delta Negative values move up; positive values move down.
    #[command]
    pub fn page(&mut self, context: &mut dyn Context, delta: i32) -> Result<()> {
        let rows = context.view().view_rect().h;
        self.cursor.page(delta, rows);
        self.refresh(context)
    }

    /// Open the filter field. Typed text narrows the list to the items that
    /// contain it.
    #[command]
    pub fn start_filter(&mut self, context: &mut dyn Context) -> Result<()> {
        self.filtering = true;
        self.republish(context)
    }

    /// Show only the items containing `text`, ignoring ASCII case.
    /// @param text Text a label must contain. Empty text shows every item.
    #[command]
    pub fn set_filter(&mut self, context: &mut dyn Context, text: String) -> Result<()> {
        self.filter = text;
        self.apply_filter();
        self.refresh(context)
    }

    /// Close the filter field and show every item again.
    #[command]
    pub fn clear_filter(&mut self, context: &mut dyn Context) -> Result<()> {
        self.filtering = false;
        if self.filter.is_empty() {
            return self.republish(context);
        }
        self.set_filter(context, String::new())
    }

    /// Return the selected item's label, for automation.
    /// @return The label, or an empty string when the list shows none.
    #[command]
    #[must_use]
    pub fn selected_name(&self) -> String {
        self.selected()
            .map(|item| item.label().to_string())
            .unwrap_or_default()
    }

    /// Return the number of items the filter passes, for automation.
    #[command]
    #[must_use]
    pub fn shown_count(&self) -> usize {
        self.shown.len()
    }

    /// Return the filter text, for automation.
    /// @return The filter, or an empty string when every item shows.
    #[command]
    #[must_use]
    pub fn filter(&self) -> String {
        self.filter.clone()
    }

    /// Show `items` under `label`, dropping any filter.
    pub fn set_items(
        &mut self,
        context: &mut dyn Context,
        label: String,
        placeholder: &'static str,
        items: Vec<T>,
    ) -> Result<()> {
        self.items = items;
        self.label = label;
        self.placeholder = placeholder;
        self.filter.clear();
        self.filtering = false;
        self.apply_filter();
        // The list is new, so it opens at its first row rather than wherever
        // the last one was left.
        self.cursor = RowCursor::new(self.shown.len());
        context.scroll(ScrollOp::To(Point { x: 0, y: 0 }));
        self.refresh(context)
    }

    /// Return the selected item.
    #[must_use]
    pub fn selected(&self) -> Option<&T> {
        self.cursor
            .index()
            .and_then(|row| self.shown.get(row))
            .and_then(|&item| self.items.get(item))
    }

    /// Drop the selected item from the list.
    ///
    /// The host removes it from its own storage, and asks first if it wants
    /// to. The selection stays on the row, which now holds the item below, so
    /// repeated removals work without moving the hand.
    pub fn remove_selected(&mut self, context: &mut dyn Context) -> Result<()> {
        let Some(&item) = self.cursor.index().and_then(|row| self.shown.get(row)) else {
            return self.republish(context);
        };
        self.items.remove(item);
        self.apply_filter();
        self.refresh(context)
    }

    /// Rebuild the shown rows from the filter.
    fn apply_filter(&mut self) {
        let needle = self.filter.to_ascii_lowercase();
        self.shown = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.label().to_ascii_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        let labels = self
            .shown
            .iter()
            .filter_map(|&item| self.items.get(item))
            .map(ItemLabel::label);
        let widest = widest_label(labels.chain(Some(self.placeholder)));
        self.fitted_width = widest.saturating_add(ROW_PADDING);
        // A filter that hides the selected row pulls the selection back into
        // range rather than leaving it past the end.
        self.cursor.set_len(self.shown.len());
    }

    /// Redraw the title and the field, re-measure, and reveal the selection.
    fn refresh(&self, context: &mut dyn Context) -> Result<()> {
        // Nearest keeps a long list still while the selection moves within the
        // rows already on screen.
        self.cursor.reveal(context);
        self.republish(context)
    }

    /// Write the current state into the frame's title and the filter field.
    fn republish(&self, context: &mut dyn Context) -> Result<()> {
        if let Some(field) = self.field {
            let text = self.filter.clone();
            let active = self.filtering;
            // The field is part of the dialog only while there is a search to
            // show: one being typed, or one left standing after it was given.
            // Hidden, it leaves the layout entirely, so the frame gives the row
            // back to the list.
            context.set_hidden(field, !active && text.is_empty())?;
            context.with_widget_mut(field, |field: &mut Input, _| {
                field.set_value(text);
                field.set_active(Some(active));
                Ok(())
            })?;
        }
        let Some(frame) = self.frame else {
            return Ok(());
        };
        let title = self.title();
        context.with_widget_mut(frame, |frame: &mut Frame, _| {
            frame.set_title(title);
            Ok(())
        })
    }

    /// Return the title the frame shows: the label and the row count.
    ///
    /// The filter is not named here, because the field below the list shows it.
    fn title(&self) -> String {
        if self.filter.is_empty() {
            return format!("{} · {}", self.label, self.items.len());
        }
        format!(
            "{} · {} of {}",
            self.label,
            self.shown.len(),
            self.items.len()
        )
    }

    /// Edit the open filter with one event.
    ///
    /// Enter keeps the filter and closes the field, and escape drops it.
    /// Other keys pass on, so the paging keys still reach their bindings.
    fn filter_event(&mut self, event: &Event, context: &mut dyn Context) -> Result<EventOutcome> {
        let mut text = self.filter.clone();
        match event {
            Event::Key(key) => match self.filter_command(*key) {
                Some(FilterCommand::Accept) => {
                    self.filtering = false;
                    self.republish(context)?;
                    return Ok(EventOutcome::Handle);
                }
                Some(FilterCommand::Clear) => {
                    self.clear_filter(context)?;
                    return Ok(EventOutcome::Handle);
                }
                Some(FilterCommand::Backspace) => {
                    let _ = text.pop();
                }
                Some(FilterCommand::Push(character)) => text.push(character),
                None => return Ok(EventOutcome::Ignore),
            },
            Event::Paste(pasted) => {
                text.extend(pasted.chars().filter(|character| !character.is_control()));
            }
            _ => return Ok(EventOutcome::Ignore),
        }
        self.set_filter(context, text)?;
        Ok(EventOutcome::Handle)
    }

    /// Classify one key while the filter is open.
    ///
    /// `None` means the key passes on to bindings, matching
    /// [`PickerList::filter_event`].
    fn filter_command(&self, key: key::Key) -> Option<FilterCommand> {
        match key.key {
            KeyCode::Enter => Some(FilterCommand::Accept),
            KeyCode::Esc => Some(FilterCommand::Clear),
            KeyCode::Backspace if self.filter.is_empty() => Some(FilterCommand::Clear),
            KeyCode::Backspace => Some(FilterCommand::Backspace),
            KeyCode::Char(_) => key.text_char().map(FilterCommand::Push),
            _ => None,
        }
    }

    /// Return the row count, which is one per item or one placeholder row.
    fn rows(&self) -> u32 {
        u32::try_from(self.shown.len().max(1)).unwrap_or(u32::MAX)
    }
}

/// One open-filter action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterCommand {
    /// Keep the filter and close the field.
    Accept,
    /// Drop the filter and close the field.
    Clear,
    /// Delete one character from the filter.
    Backspace,
    /// Append one character to the filter.
    Push(char),
}

impl<T> Register for PickerList<T>
where
    T: ItemLabel + 'static,
{
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        setup.add_commands::<Picker<T>>()?;
        register_clear_intent(setup)?;
        let nav = |intent: NavIntent| -> Result<BindingAction> {
            Ok(BindingAction::Intent(IntentName::new(intent.name())?))
        };
        let bindings: [(&str, &str, BindingAction); 11] = [
            ("Up", "Previous entry", nav(NavIntent::Up)?),
            ("k", "Previous entry", nav(NavIntent::Up)?),
            ("Down", "Next entry", nav(NavIntent::Down)?),
            ("j", "Next entry", nav(NavIntent::Down)?),
            ("PageUp", "Page up", nav(NavIntent::PageUp)?),
            ("PageDown", "Page down", nav(NavIntent::PageDown)?),
            ("Home", "First entry", nav(NavIntent::First)?),
            ("End", "Last entry", nav(NavIntent::Last)?),
            (
                "/",
                "Filter the list",
                BindingAction::Command(Self::call_start_filter()),
            ),
            (
                "Enter",
                "Accept the selected entry",
                BindingAction::Command(Picker::<T>::call_accept()),
            ),
            (
                "Esc",
                "Close the picker",
                BindingAction::Command(Picker::<T>::call_cancel()),
            ),
        ];
        for (key, description, action) in bindings {
            // Intents always run before the widget; commands run after it, so
            // typed filter text reaches the list first.
            let phase = match action {
                BindingAction::Intent(_) => None,
                _ => Some(BindingPhase::AfterWidget),
            };
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    path: Some("**/picker/**/".parse()?),
                    tier: BindingTier::Framework(Picker::<T>::BINDINGS),
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

impl<T> Default for PickerList<T>
where
    T: ItemLabel + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Widget for PickerList<T>
where
    T: ItemLabel + 'static,
{
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(
            self.fitted_width.max(MIN_FRAME_WIDTH),
            self.rows(),
        ))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        // Rows are trimmed to the view rather than scrolled sideways, so only
        // the row count can exceed it.
        Size::new(view.w, self.rows().max(view.h))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let view = context.view();
        let area = view.view_rect_local();
        render.fill("background", area, ' ')?;
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        if self.shown.is_empty() {
            return render.text(
                "placeholder",
                area.line(0)?,
                &format!(" {}", self.placeholder),
            );
        }

        let budget = (area.w as usize).saturating_sub(ROW_PADDING as usize);
        for (offset, row) in label_rows(self.shown.len(), view.scroll.y, area.h) {
            let item = &self.items[self.shown[row]];
            // A filter taking keys leaves the list holding its place rather
            // than driving it, so the selection dims until the keys come back.
            let style = if self.cursor.index() == Some(row) {
                roles::selection(context.is_focused() && !self.filtering)
            } else {
                "text"
            };
            let shown = self.truncate.apply(item.label(), budget);
            let line = Line::new(area.tl.x, area.tl.y.saturating_add(offset), area.w);
            render.text(style, line, &format!(" {shown}"))?;
        }
        Ok(())
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, context: &mut dyn Context) -> Result<EventOutcome> {
        if self.filtering {
            return self.filter_event(event, context);
        }
        Ok(EventOutcome::Ignore)
    }

    fn key_outcome(&self, key: key::Key, _context: &dyn ViewContext) -> EventOutcome {
        if self.filtering && self.filter_command(key).is_some() {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn accepts_intent(&self, intent: &str, _context: &dyn ViewContext) -> bool {
        (self.filtering && intent == crate::CLEAR_INTENT)
            || (!self.shown.is_empty() && CursorMove::of(intent).is_some())
    }

    fn on_intent(&mut self, intent: &str, context: &mut dyn Context) -> Result<EventOutcome> {
        if let Some(movement) = CursorMove::of(intent) {
            match movement {
                CursorMove::By(delta) => self.select_by(context, delta)?,
                CursorMove::Page(delta) => self.page(context, delta)?,
                CursorMove::First => self.select_first(context)?,
                CursorMove::Last => self.select_last(context)?,
            }
            return Ok(EventOutcome::Handle);
        }
        if !self.filtering || intent != crate::CLEAR_INTENT {
            return Ok(EventOutcome::Ignore);
        }
        // The filter stays open, so the next key narrows the list again.
        self.set_filter(context, String::new())?;
        Ok(EventOutcome::Handle)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("picker_list")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        geom::Size,
        input::{ModalBindings, ModalOptions},
        testing::harness::Harness,
    };

    use super::*;
    use crate::{Confirm, input::CARET};

    /// Build a picker showing `items`, rendered once.
    fn picker(items: &[&str], width: u32, height: u32) -> Result<Harness> {
        build(items, width, height, Truncate::default())
    }

    /// Build a picker with an explicit truncation end, rendered once.
    fn build(items: &[&str], width: u32, height: u32, truncate: Truncate) -> Result<Harness> {
        let mut harness = Harness::builder(Picker::<String>::new().with_truncate(truncate))
            .size(width, height)
            .build()?;
        harness.render()?;
        let items: Vec<String> = items.iter().map(|item| (*item).to_string()).collect();
        harness.with_root_widget_context(|picker: &mut Picker<String>, context| {
            picker.set_items(context, "Bookmarks", "<no bookmarks>", items)
        })?;
        harness.render()?;
        Ok(harness)
    }

    /// Run `command` against the list inside the picker.
    fn on_list(
        harness: &mut Harness,
        command: impl FnOnce(&mut PickerList<String>, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        harness.with_root_widget_context(|picker: &mut Picker<String>, context| {
            let list = picker.list()?;
            context.with_widget_mut(list, command)
        })?;
        harness.render()?;
        Ok(())
    }

    /// Return a value read from the list inside the picker.
    fn from_list<R>(harness: &mut Harness, read: impl FnOnce(&PickerList<String>) -> R) -> R {
        harness
            .with_root_widget_context(|picker: &mut Picker<String>, context| {
                let list = picker.list()?;
                context.with_widget_mut(list, |list: &mut PickerList<String>, _| Ok(read(list)))
            })
            .expect("the picker is mounted")
    }

    /// Where one part of the picker sits: its top row, and the rows it spans.
    type Span = (i32, u32);

    /// Where the picker's frame, list, and filter field sit on screen.
    struct Geometry {
        /// The frame around the whole dialog.
        frame: Span,
        /// The scrolling list of items.
        list: Span,
        /// The filter field under the list.
        field: Span,
    }

    /// Return where the picker's frame, list, and filter field sit.
    fn parts(harness: &mut Harness) -> Result<Geometry> {
        let (list, filter) =
            harness.with_root_widget_context(|picker: &mut Picker<String>, _| {
                Ok((picker.list()?, picker.filter()?))
            })?;
        let frame = harness
            .find_nodes("**/picker/**/frame")?
            .first()
            .copied()
            .expect("the picker is framed");
        let span = |node| {
            harness.canopy.with_root_view(|context| {
                let outer = context.view_of(node).expect("live node").outer;
                (outer.tl.y, outer.h)
            })
        };
        Ok(Geometry {
            frame: span(frame),
            list: span(list),
            field: span(filter),
        })
    }

    #[test]
    fn the_modal_centres_a_frame_over_the_view() -> Result<()> {
        let harness = picker(&["/tmp/alpha", "/tmp/beta"], 40, 12)?;
        let lines = harness.tbuf().lines();
        // The margin leaves the first and last rows to whatever shows behind.
        assert!(
            lines[0].trim().is_empty(),
            "the modal leaves a margin, got {:?}",
            lines[0]
        );
        let screen = lines.join("\n");
        assert!(screen.contains("Bookmarks"), "the frame carries the title");
        assert!(screen.contains("/tmp/alpha"), "the items render");
        Ok(())
    }

    #[test]
    fn selection_moves_and_stops_at_both_ends() -> Result<()> {
        let mut harness = picker(&["/a", "/b", "/c"], 40, 12)?;
        assert_eq!(from_list(&mut harness, PickerList::selected_name), "/a");

        on_list(&mut harness, |list, context| list.select_by(context, 1))?;
        assert_eq!(from_list(&mut harness, PickerList::selected_name), "/b");
        on_list(&mut harness, |list, context| list.select_by(context, -5))?;
        assert_eq!(
            from_list(&mut harness, PickerList::selected_name),
            "/a",
            "moving up stops at the first row"
        );
        on_list(&mut harness, |list, context| {
            list.select_by(context, i32::MAX)
        })?;
        assert_eq!(from_list(&mut harness, PickerList::selected_name), "/c");
        on_list(&mut harness, |list, context| list.select_by(context, 9))?;
        assert_eq!(
            from_list(&mut harness, PickerList::selected_name),
            "/c",
            "moving down stops at the last row"
        );
        Ok(())
    }

    #[test]
    fn the_filter_narrows_the_list_and_escape_restores_it() -> Result<()> {
        let mut harness = picker(&["/tmp/alpha", "/tmp/beta", "/var/gamma"], 40, 12)?;
        on_list(&mut harness, |list, context| {
            list.set_filter(context, "TMP".into())
        })?;
        assert_eq!(
            from_list(&mut harness, PickerList::shown_count),
            2,
            "the filter ignores case"
        );
        assert!(!harness.tbuf().contains_text("/var/gamma"));

        on_list(&mut harness, |list, context| {
            list.set_filter(context, "zzz".into())
        })?;
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 0);
        assert_eq!(
            from_list(&mut harness, PickerList::selected_name),
            "",
            "an empty list selects nothing"
        );
        assert!(harness.tbuf().contains_text("<no bookmarks>"));

        on_list(&mut harness, PickerList::clear_filter)?;
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 3);
        Ok(())
    }

    #[test]
    fn the_clear_action_empties_the_filter_and_keeps_its_field() -> Result<()> {
        let mut harness = picker(&["alpha", "beta"], 40, 12)?;
        on_list(&mut harness, PickerList::start_filter)?;
        on_list(&mut harness, |list, context| {
            list.set_filter(context, "al".into())
        })?;
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 1);

        harness.with_root_widget_context(|picker: &mut Picker<String>, context| {
            let list = picker.list()?;
            context.with_widget_mut(list, |list: &mut PickerList<String>, context| {
                assert!(
                    list.accepts_intent(crate::CLEAR_INTENT, context),
                    "the open filter consumes the clear action"
                );
                assert_eq!(
                    list.on_intent(crate::CLEAR_INTENT, context)?,
                    EventOutcome::Handle
                );
                Ok(())
            })
        })?;
        harness.render()?;
        assert_eq!(from_list(&mut harness, PickerList::filter), "");
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 2);

        // The field stays open, so the next key filters again.
        harness.key('b')?;
        harness.render()?;
        assert_eq!(from_list(&mut harness, PickerList::filter), "b");
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 1);
        Ok(())
    }

    #[test]
    fn filter_keys_predict_their_handling() -> Result<()> {
        let keys = [
            "a",
            "enter",
            "esc",
            "backspace",
            "up",
            "f1",
            "ctrl-a",
            "ctrl-x",
            "alt-x",
        ];
        for filtering in [false, true] {
            for spec in keys {
                let mut harness = picker(&["alpha", "beta"], 40, 12)?;
                if filtering {
                    on_list(&mut harness, PickerList::start_filter)?;
                    on_list(&mut harness, |list, context| {
                        list.set_filter(context, "al".into())
                    })?;
                }
                harness.with_root_widget_context(|picker: &mut Picker<String>, context| {
                    let list = picker.list()?;
                    context.with_widget_mut(list, |list: &mut PickerList<String>, context| {
                        let key = key::Key::parse_spec(spec).expect("valid key spec");
                        let predicted = list.key_outcome(key, context);
                        let actual = list.on_event(&Event::Key(key), context)?;
                        assert_eq!(predicted, actual, "filtering {filtering}: {spec}");
                        Ok(())
                    })
                })?;
            }
        }
        Ok(())
    }

    #[test]
    fn the_filter_shows_in_the_field_rather_than_the_title() -> Result<()> {
        let mut harness = picker(&["/tmp/alpha", "/tmp/beta"], 40, 12)?;
        on_list(&mut harness, PickerList::start_filter)?;
        on_list(&mut harness, |list, context| {
            list.set_filter(context, "alp".into())
        })?;

        let geometry = parts(&mut harness)?;
        let row = harness
            .tbuf()
            .lines()
            .get(usize::try_from(geometry.field.0).unwrap_or(0))
            .cloned()
            .unwrap_or_default();
        assert!(
            row.contains("alp"),
            "the field carries the filter, got {row:?}"
        );
        assert!(row.contains('/'), "the field shows its prompt, got {row:?}");

        // The title keeps the label and the counts, and names no filter.
        let title = harness
            .tbuf()
            .lines()
            .get(
                usize::try_from(geometry.field.0)
                    .unwrap_or(0)
                    .saturating_sub(3),
            )
            .cloned()
            .unwrap_or_default();
        let screen = harness.tbuf().lines().join("\n");
        assert!(
            screen.contains("1 of 2"),
            "the title counts what the filter passes, got {title:?}"
        );
        assert!(
            !screen.contains("/alp ") && !screen.contains("Bookmarks /alp"),
            "the title does not repeat the filter"
        );
        Ok(())
    }

    /// Give the open filter, the way Enter does.
    ///
    /// The modal gives the list the keyboard, so the key reaches the filter
    /// here the way it does in an application.
    fn commit(harness: &mut Harness) -> Result<()> {
        let list =
            harness.with_root_widget_context(|picker: &mut Picker<String>, _| picker.list())?;
        harness.canopy.with_root_context(|context| {
            context.set_focus(list)?;
            Ok(())
        })?;
        harness.key(KeyCode::Enter)?;
        harness.render()
    }

    /// Return the rows the dialog's frame covers.
    fn framed(harness: &Harness) -> usize {
        harness
            .tbuf()
            .lines()
            .iter()
            .filter(|line| line.contains('│') || line.contains('╭') || line.contains('╰'))
            .count()
    }

    #[test]
    fn the_field_appears_only_while_there_is_a_search() -> Result<()> {
        let mut harness = picker(&["/tmp/alpha", "/tmp/beta"], 40, 12)?;
        let closed = framed(&harness);

        on_list(&mut harness, PickerList::start_filter)?;
        let open = framed(&harness);
        assert_eq!(open, closed + 1, "opening a search gives the field a row");

        // The filter passes both items, so any row the dialog gains or loses
        // from here is the field's rather than the list's.
        on_list(&mut harness, |list, context| {
            list.set_filter(context, "tmp".into())
        })?;
        commit(&mut harness)?;
        assert_eq!(
            framed(&harness),
            open,
            "a search that has been given keeps its row, so the list stays explained"
        );

        on_list(&mut harness, PickerList::clear_filter)?;
        assert_eq!(
            framed(&harness),
            closed,
            "dropping the search gives the row back to the list"
        );
        Ok(())
    }

    #[test]
    fn the_search_and_the_list_trade_emphasis() -> Result<()> {
        let mut harness = picker(&["/tmp/alpha", "/tmp/beta"], 40, 12)?;
        on_list(&mut harness, PickerList::start_filter)?;
        // `h` belongs to "alpha" alone, which is the selected row, and `b` to
        // "beta" alone, which is not.
        let ground = |harness: &Harness, needle: char| {
            harness
                .canopy
                .snapshot()
                .expect("published picker")
                .buffer
                .cells()
                .iter()
                .find(|cell| cell.ch == needle)
                .map(|cell| cell.style.bg)
                .unwrap_or_else(|| panic!("{needle:?} renders"))
        };

        assert_ne!(
            ground(&harness, CARET),
            ground(&harness, 'b'),
            "the field does not share the ground the rows sit on"
        );
        let searching = ground(&harness, 'h');

        commit(&mut harness)?;
        assert!(
            !harness.tbuf().contains_text(&CARET.to_string()),
            "a search that has been given drops its caret"
        );
        assert_ne!(
            ground(&harness, 'h'),
            searching,
            "the selection takes the emphasis back when the keys return to it"
        );
        Ok(())
    }

    #[test]
    fn the_filter_field_stays_on_screen_under_a_very_large_list() -> Result<()> {
        // Far more items than rows, so the list scrolls on every axis it can.
        let items: Vec<String> = (0..5000)
            .map(|row| format!("/tmp/entry-{row:04}"))
            .collect();
        let borrowed: Vec<&str> = items.iter().map(String::as_str).collect();

        for (width, height) in [(40, 12), (30, 6), (60, 24)] {
            let mut harness = picker(&borrowed, width, height)?;
            let screen_rows = i32::try_from(height).unwrap_or(i32::MAX);
            // The field joins the dialog only once a search is open, and this
            // is about where it sits when it does.
            on_list(&mut harness, PickerList::start_filter)?;

            for step in ["top", "bottom", "middle"] {
                match step {
                    "bottom" => on_list(&mut harness, |list, context| {
                        list.select_by(context, i32::MAX)
                    })?,
                    "middle" => on_list(&mut harness, |list, context| {
                        list.select_by(context, i32::MIN)?;
                        list.select_by(context, 2500)
                    })?,
                    _ => {}
                }

                let geometry = parts(&mut harness)?;
                let rows = |count: u32| i32::try_from(count).unwrap_or(i32::MAX);
                let (frame_top, frame_rows) = geometry.frame;
                let (list_top, list_rows) = geometry.list;
                let (field_top, field_rows) = geometry.field;
                let field_bottom = field_top.saturating_add(rows(field_rows));
                let frame_bottom = frame_top.saturating_add(rows(frame_rows));

                assert_eq!(
                    field_rows, FILTER_ROWS,
                    "the field keeps its single row at {width}x{height}, {step}"
                );
                assert_eq!(
                    field_bottom,
                    frame_bottom.saturating_sub(1),
                    "the field sits on the frame's bottom border at {width}x{height}, {step}"
                );
                assert_eq!(
                    list_top.saturating_add(rows(list_rows)),
                    field_top,
                    "the list ends where the field begins at {width}x{height}, {step}"
                );
                assert!(
                    field_top >= 0 && field_bottom <= screen_rows,
                    "the field is on screen at {width}x{height}, {step}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn removing_the_selected_row_keeps_the_selection_in_place() -> Result<()> {
        let mut harness = picker(&["/a", "/b", "/c"], 40, 12)?;
        on_list(&mut harness, |list, context| list.select_by(context, 1))?;
        on_list(&mut harness, PickerList::remove_selected)?;
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 2);
        assert_eq!(
            from_list(&mut harness, PickerList::selected_name),
            "/c",
            "the row below moves up under the selection"
        );

        on_list(&mut harness, PickerList::remove_selected)?;
        on_list(&mut harness, PickerList::remove_selected)?;
        assert_eq!(from_list(&mut harness, PickerList::shown_count), 0);
        on_list(&mut harness, PickerList::remove_selected)?;
        assert_eq!(
            from_list(&mut harness, PickerList::shown_count),
            0,
            "removing from an empty list does nothing"
        );
        Ok(())
    }

    /// Return the node holding the keyboard.
    fn focused(harness: &Harness) -> Option<NodeId> {
        harness
            .canopy
            .with_root_view(|context| context.focused_node())
    }

    #[test]
    fn an_overlay_opens_over_the_list_and_gives_it_back_on_close() -> Result<()> {
        // Enough rows that the dialog stands taller than the overlay, so its
        // title shows above the question.
        let items: Vec<String> = (0..8).map(|row| format!("/tmp/entry-{row}")).collect();
        let borrowed: Vec<&str> = items.iter().map(String::as_str).collect();
        let mut harness = picker(&borrowed, 40, 12)?;
        let (overlay, focus) =
            harness.with_root_widget_context(|picker: &mut Picker<String>, context| {
                let _ = picker;
                let overlay = context.add_modal(Confirm::new())?;
                let focus =
                    context.with_widget_mut(overlay, |confirm: &mut Confirm, context| {
                        confirm.ask(context, "Delete", "/tmp/entry-1")?;
                        // An answer takes focus only once it has something to
                        // run, as it would in a host.
                        confirm.set_commands(
                            context,
                            PickerList::<String>::call_clear_filter(),
                            PickerList::<String>::call_clear_filter(),
                        )?;
                        confirm.initial_focus()
                    })?;
                Ok((overlay, focus))
            })?;
        harness.render()?;
        assert!(
            !harness.tbuf().contains_text("Delete"),
            "an overlay waits hidden"
        );

        on_list(&mut harness, |list, context| {
            list.set_filter(context, "entry".into())?;
            list.select_by(context, 1)
        })?;
        let list =
            harness.with_root_widget_context(|picker: &mut Picker<String>, _| picker.list())?;
        harness.canopy.with_root_context(|context| {
            context.set_focus(list)?;
            Ok(())
        })?;

        let token = harness.with_root_widget_context(|_: &mut Picker<String>, context| {
            context.open_modal(ModalOptions {
                owner: context.node_id(),
                modal: overlay.into(),
                initial_focus: focus,
                dim_target: None,
                bindings: ModalBindings::Application,
            })
        })?;
        harness.render()?;
        assert!(
            harness.tbuf().contains_text("Delete"),
            "the overlay shows over the list"
        );
        assert!(
            harness.tbuf().contains_text("Bookmarks"),
            "the dialog stays under the overlay"
        );
        assert_eq!(
            focused(&harness),
            Some(focus),
            "the overlay takes the keyboard"
        );

        harness
            .canopy
            .with_root_context(|context| context.close_modal(token))?;
        harness.render()?;
        assert!(
            !harness.tbuf().contains_text("Delete"),
            "closing hides the overlay"
        );
        assert_eq!(
            focused(&harness),
            Some(list),
            "the list takes the keyboard back"
        );
        assert_eq!(
            from_list(&mut harness, PickerList::filter),
            "entry",
            "the filter survives the overlay"
        );
        assert_eq!(
            from_list(&mut harness, PickerList::selected_name),
            "/tmp/entry-1",
            "the selection survives the overlay"
        );
        Ok(())
    }

    #[test]
    fn a_long_list_scrolls_and_each_end_trims_its_own_way() -> Result<()> {
        let items: Vec<String> = (0..40).map(|row| format!("/tmp/entry-{row:02}")).collect();
        let borrowed: Vec<&str> = items.iter().map(String::as_str).collect();
        let mut harness = picker(&borrowed, 40, 12)?;
        assert!(harness.tbuf().contains_text("/tmp/entry-00"));

        on_list(&mut harness, |list, context| {
            list.select_by(context, i32::MAX)
        })?;
        assert!(
            harness.tbuf().contains_text("/tmp/entry-39"),
            "the last row scrolls into view"
        );
        assert!(
            !harness.tbuf().contains_text("/tmp/entry-00"),
            "the first row scrolls away"
        );

        on_list(&mut harness, |list, context| {
            list.select_by(context, i32::MIN)
        })?;
        assert!(
            harness.tbuf().contains_text("/tmp/entry-00"),
            "the list scrolls back"
        );

        // Trimming the head keeps a path's last components, which is what a
        // path is read by.
        let deep = format!("/tmp/{}/notes.txt", "segment/".repeat(12));
        let head = build(&[&deep], 30, 8, Truncate::Start)?;
        assert!(
            head.tbuf().contains_text("notes.txt"),
            "a trimmed head keeps the tail"
        );
        assert!(head.tbuf().contains_text("…"), "a trimmed row is marked");

        // Trimming the tail is the default, which is how ordinary text reads.
        let tail = build(&[&deep], 30, 8, Truncate::End)?;
        assert!(
            !tail.tbuf().contains_text("notes.txt"),
            "a trimmed tail drops the end"
        );
        assert!(tail.tbuf().contains_text("…"), "a trimmed row is marked");
        Ok(())
    }

    #[test]
    fn a_short_list_makes_a_small_dialog_and_a_long_one_fills_the_view() -> Result<()> {
        let height = |harness: &Harness| {
            harness
                .tbuf()
                .lines()
                .iter()
                .filter(|line| line.contains('│') || line.contains('╭') || line.contains('╰'))
                .count()
        };
        let short = picker(&["/a", "/b"], 60, 20)?;
        let long: Vec<String> = (0..40).map(|row| format!("/entry-{row:02}")).collect();
        let borrowed: Vec<&str> = long.iter().map(String::as_str).collect();
        let tall = picker(&borrowed, 60, 20)?;
        assert!(
            height(&short) < height(&tall),
            "the frame fits its list, got {} and {}",
            height(&short),
            height(&tall)
        );
        assert!(
            height(&tall) >= 18,
            "a long list nearly fills the view, got {}",
            height(&tall)
        );
        Ok(())
    }

    #[test]
    fn the_picker_renders_in_a_tiny_view() -> Result<()> {
        for size in [Size::new(4, 3), Size::new(1, 1), Size::new(12, 4)] {
            let mut harness = Harness::builder(Picker::<String>::new())
                .size(size.w, size.h)
                .build()?;
            harness.render()?;
            harness.with_root_widget_context(|picker: &mut Picker<String>, context| {
                picker.set_items(
                    context,
                    "Bookmarks",
                    "<no bookmarks>",
                    vec!["/tmp/a-rather-long-path/notes.txt".to_string()],
                )
            })?;
            harness.render()?;
        }
        Ok(())
    }
}
