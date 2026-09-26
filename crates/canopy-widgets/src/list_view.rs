//! Widget-based list container.
//!
//! A typed list container where items are actual widgets in the tree.
//! Items participate in focus management and can be composed from other
//! widgets.

use std::{collections::HashSet, hash::Hash};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Setup, TypedId, ViewContext, Widget,
    commands::{ArgValue, CommandCall, CommandStatus, ToArgValue},
    derive_commands,
    error::{Error, Result},
    geom::{Line, Point, PointI32, Size},
    input::{
        BindingAction, BindingOptions, BindingTier, Event, FrameworkBindingGroup, IntentName,
        NavIntent, key::Key, mouse,
    },
    layout::{
        CanvasContext, Constraint, Edges, Layout, MeasureConstraints, MeasureOverflow, Measurement,
        RevealAlign,
    },
    render::Render,
    runtime::WidgetSemantics,
    text,
};

use crate::{keyed::KeyedChildren, row_cursor::CursorMove};

/// List selection indicator configuration.
struct SelectionIndicator {
    /// Style path for the indicator.
    style: String,
    /// Indicator text.
    text: String,
    /// Indicator width in cells.
    width: u32,
    /// Indicator repeat behavior.
    repeat: bool,
}

/// Default drag threshold in cells before cancelling activation.
const DEFAULT_ACTIVATE_DRAG_THRESHOLD: u32 = 4;

/// Pending activation state for list row clicks.
#[derive(Debug, Clone, Copy)]
struct PendingActivate<K> {
    /// Stable key of the pressed row.
    key: K,
    /// Pointer origin when the press began.
    origin: PointI32,
    /// Whether the drag threshold was exceeded.
    dragged: bool,
}

/// Monotonic key for list items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AutoKey(u64);

impl ToArgValue for AutoKey {
    fn to_arg_value(self) -> ArgValue {
        self.0.to_arg_value()
    }
}

/// Trait for widgets that can be selected in a list.
///
/// Items in a `List` must implement this trait so the list can manage
/// their selection state. Selection is independent of focus - an item
/// remains selected even when the list loses focus.
pub trait Selectable: Widget {
    /// Set the selection state of this item.
    fn set_selected(&mut self, selected: bool);

    /// Set the checked state of this item.
    ///
    /// Only a list with checks enabled calls this. A row that never shows a
    /// check keeps the default no-op.
    fn set_checked(&mut self, _checked: bool) {}
}

/// A typed list container for widget items.
///
/// List items are actual widgets in the tree, enabling composition and focus
/// management. The list arranges items vertically and supports scrolling.
///
/// Items must implement the [`Selectable`] trait so the list can manage their
/// selection state independently of focus.
pub struct List<W: Selectable, K: Eq + Hash + Clone + ToArgValue + 'static = AutoKey> {
    /// Keyed list items in order.
    items: KeyedChildren<K, W>,
    /// Next monotonic key to assign.
    next_key: u64,
    /// Stable key of the selected item.
    selected: Option<K>,
    /// Checked item keys, absent until checks are enabled.
    checks: Option<HashSet<K>>,
    /// Optional list-level selection indicator.
    selection_indicator: Option<SelectionIndicator>,
    /// Optional activation command call.
    on_activate: Option<CommandCall>,
    /// Pending activation state while handling clicks.
    pending_activate: Option<PendingActivate<K>>,
    /// Optional semantic label for the collection.
    label: Option<String>,
}

impl<W: Selectable, K: Eq + Hash + Clone + ToArgValue + 'static> Default for List<W, K> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl<W: Selectable, K: Eq + Hash + Clone + ToArgValue + 'static> List<W, K> {
    /// The framework group a modal that holds a list admits for it.
    ///
    /// The arrows, `j` and `k`, the page keys, and Home and End offer the
    /// navigation intents, which the list answers by moving its selection.
    /// Install it with [`List::register_bindings`].
    pub const BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("list");

    /// Install [`List::BINDINGS`]. Installing it again is harmless.
    pub fn register_bindings(setup: &mut Setup) -> Result<()> {
        let keys: [(&str, &str, NavIntent); 8] = [
            ("Up", "Previous row", NavIntent::Up),
            ("k", "Previous row", NavIntent::Up),
            ("Down", "Next row", NavIntent::Down),
            ("j", "Next row", NavIntent::Down),
            ("PageUp", "Page up", NavIntent::PageUp),
            ("PageDown", "Page down", NavIntent::PageDown),
            ("Home", "First row", NavIntent::First),
            ("End", "Last row", NavIntent::Last),
        ];
        for (key, description, intent) in keys {
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    path: Some("**/list/**/".parse()?),
                    tier: BindingTier::Framework(Self::BINDINGS),
                    description: description.to_string(),
                    source: None,
                    phase: None,
                },
                BindingAction::Intent(IntentName::new(intent.name())?),
            )?;
        }
        Ok(())
    }

    /// Construct an empty list.
    pub fn new() -> Self {
        Self {
            items: KeyedChildren::new(),
            next_key: 0,
            selected: None,
            checks: None,
            selection_indicator: None,
            on_activate: None,
            pending_activate: None,
            label: None,
        }
    }

    /// Set the semantic label of the collection.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Enable multi-select checks.
    ///
    /// Checked keys are independent of the selected row. The row widget
    /// learns its state through [`Selectable::set_checked`] when the list
    /// reconciles it or its check changes.
    #[must_use]
    pub fn with_checks(mut self) -> Self {
        self.checks = Some(HashSet::new());
        self
    }

    /// Return whether this list tracks checks.
    #[must_use]
    pub fn checks_enabled(&self) -> bool {
        self.checks.is_some()
    }

    /// Return whether `key` is checked.
    #[must_use]
    pub fn is_checked(&self, key: &K) -> bool {
        self.checks
            .as_ref()
            .is_some_and(|checks| checks.contains(key))
    }

    /// Return the checked keys in display order.
    #[must_use]
    pub fn checked_keys(&self) -> Vec<&K> {
        let Some(checks) = self.checks.as_ref() else {
            return Vec::new();
        };
        self.items
            .keys()
            .iter()
            .filter(|key| checks.contains(*key))
            .collect()
    }

    /// Return the number of checked keys.
    #[must_use]
    pub fn checked_len(&self) -> usize {
        self.checks.as_ref().map_or(0, HashSet::len)
    }

    /// Check or uncheck `key`, updating its row widget.
    ///
    /// A list without checks ignores the call.
    pub fn set_checked(&mut self, ctx: &mut dyn Context, key: &K, checked: bool) -> Result<()> {
        let Some(id) = self.items.id_for(key) else {
            return Err(Error::Invalid("list check key is absent".into()));
        };
        let Some(checks) = self.checks.as_mut() else {
            return Ok(());
        };
        let changed = if checked {
            checks.insert(key.clone())
        } else {
            checks.remove(key)
        };
        if !changed {
            return Ok(());
        }
        ctx.with_widget_mut(id, |widget: &mut W, _| {
            widget.set_checked(checked);
            Ok(())
        })?;
        debug_assert!(self.checks_invariant_holds());
        Ok(())
    }

    /// Inspect the selected row's configured activation command.
    fn command_status(&self, ctx: &dyn ViewContext) -> Result<Option<CommandStatus>> {
        let Some(call) = self.on_activate.as_ref() else {
            return Ok(None);
        };
        if !ctx.is_attached(ctx.node_id()) {
            return Ok(Some(CommandStatus::Disabled("List is detached".into())));
        }
        let Some(index) = self.selected_index() else {
            return Ok(Some(CommandStatus::Disabled("No item selected".into())));
        };
        ctx.command_status(&call.with_arg("index", index)).map(Some)
    }

    /// Build a list with a list-level selection indicator.
    /// Repeat controls whether the indicator renders on every visible line.
    #[must_use]
    pub fn with_selection_indicator(
        mut self,
        style: impl Into<String>,
        text: impl Into<String>,
        repeat: bool,
    ) -> Self {
        let text = text.into();
        let width = indicator_width(&text);
        self.selection_indicator = Some(SelectionIndicator {
            style: style.into(),
            text,
            width,
            repeat,
        });
        self
    }

    /// Build a list that posts a command when a row is activated.
    ///
    /// The list posts the command with [`Context::post`] and the row's index
    /// appended, so it runs once the list's handler has returned. The index
    /// names a position, not a row: a list that changes its rows before the
    /// command runs can put another row at that index.
    #[must_use]
    pub fn with_command(mut self, command: CommandCall) -> Self {
        self.on_activate = Some(command);
        self
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the number of items in the list.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Borrow the stable keys in display order.
    pub fn keys(&self) -> &[K] {
        self.items.keys()
    }

    /// Returns the typed ID of the item at the given index.
    pub fn item(&self, index: usize) -> Option<TypedId<W>> {
        self.items.id_at(index)
    }

    /// Returns the currently selected index.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
            .as_ref()
            .and_then(|key| self.items.keys().iter().position(|item| item == key))
    }

    /// Returns the typed ID of the currently selected item.
    pub fn selected_item(&self) -> Option<TypedId<W>> {
        self.selected
            .as_ref()
            .and_then(|key| self.items.id_for(key))
    }

    /// Return the stable key of the selected row.
    pub fn selected_key(&self) -> Option<&K> {
        self.selected.as_ref()
    }

    /// Return the widget for a domain key.
    #[cfg(test)]
    pub(crate) fn item_for_key(&self, key: &K) -> Option<TypedId<W>> {
        self.items.id_for(key)
    }

    /// Select a domain key, returning an error when it is absent.
    pub fn select_key(&mut self, ctx: &mut dyn Context, key: &K) -> Result<()> {
        let index = self
            .items
            .keys()
            .iter()
            .position(|item| item == key)
            .ok_or_else(|| Error::Invalid("list selection key is absent".into()))?;
        self.select(ctx, index)
    }

    /// Reconcile domain keys while retaining widgets and selection for
    /// surviving keys.
    ///
    /// Duplicate keys fail before callbacks run. Updates have the same
    /// structural rollback boundary as [`KeyedChildren::reconcile`]. If
    /// selection is removed, the next surviving old neighbor wins, then the
    /// previous neighbor.
    pub fn reconcile<I, C, U>(
        &mut self,
        ctx: &mut dyn Context,
        desired: I,
        create: C,
        mut update: U,
    ) -> Result<Vec<TypedId<W>>>
    where
        I: IntoIterator<Item = K>,
        C: FnMut(&K) -> Result<W>,
        U: FnMut(&K, TypedId<W>, &mut dyn Context) -> Result<()>,
    {
        let desired: Vec<K> = desired.into_iter().collect();
        let keys: HashSet<&K> = desired.iter().collect();
        if keys.len() != desired.len() {
            return Err(Error::Invalid("duplicate list key".into()));
        }
        let selected = self.selection_after_reconcile(&desired);
        let selection_changed = selected != self.selected;
        let had_focus = ctx.is_on_focus_path(ctx.node_id());
        let previous_focus = ctx.focused_node();
        let checks = self.checks.as_ref();
        let desired_checks = checks.map(|_| desired.iter().cloned().collect::<HashSet<K>>());
        let ordered = self.items.reconcile(ctx, desired, create, |key, id, ctx| {
            update(key, id, ctx)?;
            ctx.with_widget_mut(id, |widget: &mut W, _| {
                widget.set_selected(selected.as_ref() == Some(key));
                if let Some(checks) = checks {
                    widget.set_checked(checks.contains(key));
                }
                Ok(())
            })
        })?;
        self.selected = selected;
        if let Some(checks) = self.checks.as_mut()
            && let Some(desired) = &desired_checks
        {
            checks.retain(|key| desired.contains(key));
            debug_assert!(self.checks_invariant_holds());
        }
        if self
            .pending_activate
            .as_ref()
            .is_some_and(|pending| self.items.id_for(&pending.key).is_none())
        {
            self.pending_activate = None;
            ctx.release_mouse()?;
        }
        if selection_changed && had_focus {
            self.focus_selected(ctx)?;
        } else if let Some(focus) = previous_focus
            && ctx.is_attached(focus)
        {
            ctx.set_focus(focus)?;
        }
        Ok(ordered)
    }

    /// Choose a surviving selection using the previous row order.
    fn selection_after_reconcile(&self, desired: &[K]) -> Option<K> {
        let Some(selected) = self.selected.as_ref() else {
            return desired.first().cloned();
        };
        if desired.contains(selected) {
            return Some(selected.clone());
        }
        let old_index = self.selected_index()?;
        self.items.keys()[old_index + 1..]
            .iter()
            .chain(self.items.keys()[..old_index].iter().rev())
            .find(|key| desired.contains(key))
            .cloned()
    }

    /// Remove the item at the specified index.
    pub(crate) fn remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<bool> {
        let mut desired = self.items.keys().to_vec();
        if index >= desired.len() {
            return Ok(false);
        }
        desired.remove(index);
        self.reconcile_order(ctx, desired)?;
        Ok(true)
    }

    /// Clear all items from the list.
    #[command(ignore_result)]
    pub fn clear(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.reconcile_order(ctx, Vec::new())?;
        Ok(())
    }

    /// Delete the currently selected item.
    #[command(ignore_result)]
    pub fn delete_selected(&mut self, ctx: &mut dyn Context) -> Result<bool> {
        match self.selected_index() {
            Some(sel) => self.remove(ctx, sel),
            None => Ok(false),
        }
    }

    /// Toggle the checked state of the selected row.
    ///
    /// Returns the new state, or false when the list has no selection.
    #[command(ignore_result)]
    pub fn toggle(&mut self, ctx: &mut dyn Context) -> Result<bool> {
        if self.checks.is_none() {
            return Ok(false);
        }
        let Some(key) = self.selected.clone() else {
            return Ok(false);
        };
        let checked = !self.is_checked(&key);
        self.set_checked(ctx, &key, checked)?;
        Ok(checked)
    }

    /// Check every row.
    #[command]
    pub fn check_all(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.checks.is_none() {
            return Ok(());
        }
        self.checks = Some(self.items.keys().iter().cloned().collect());
        let ids = self.items.iter_ids().collect::<Vec<_>>();
        for id in ids {
            ctx.with_widget_mut(id, |widget: &mut W, _| {
                widget.set_checked(true);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Uncheck every row.
    #[command]
    pub fn clear_checks(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.checks.is_none() {
            return Ok(());
        }
        self.checks = Some(HashSet::new());
        let ids = self.items.iter_ids().collect::<Vec<_>>();
        for id in ids {
            ctx.with_widget_mut(id, |widget: &mut W, _| {
                widget.set_checked(false);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Select an item at the given index.
    pub fn select(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }
        self.update_selection(ctx, Some(index.min(self.items.len() - 1)))
    }

    /// Update selection to a new index, managing item selection states.
    fn update_selection(
        &mut self,
        ctx: &mut dyn Context,
        new_selected: Option<usize>,
    ) -> Result<()> {
        if let Some(index) = new_selected
            && index >= self.items.len()
        {
            return Err(Error::Invalid(
                "list selection index is out of bounds".into(),
            ));
        }

        let old_id = self.selection_id(
            self.selected_index(),
            "list selection points at a missing item",
        )?;
        let new_id =
            self.selection_id(new_selected, "new list selection points at a missing item")?;
        let same_selection = old_id.as_ref().map(|id| NodeId::from(*id))
            == new_id.as_ref().map(|id| NodeId::from(*id));
        if same_selection {
            self.selected = new_selected.map(|index| self.items.keys()[index].clone());
            return Ok(());
        }

        if let Some(old_id) = old_id {
            ctx.with_widget_mut(old_id, |w: &mut W, _| {
                w.set_selected(false);
                Ok(())
            })?;
        }

        if let Some(new_id) = new_id {
            ctx.with_widget_mut(new_id, |w: &mut W, _| {
                w.set_selected(true);
                Ok(())
            })?;
        }

        self.selected = new_selected.map(|index| self.items.keys()[index].clone());
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Return a selected item ID or an error when the list invariants are
    /// broken.
    fn selection_id(&self, index: Option<usize>, message: &str) -> Result<Option<TypedId<W>>> {
        let Some(index) = index else {
            return Ok(None);
        };
        self.item(index)
            .map(Some)
            .ok_or_else(|| Error::Internal(message.into()))
    }

    /// Return whether the stored selection points at a live list item.
    fn selection_invariant_holds(&self) -> bool {
        self.selected
            .as_ref()
            .is_none_or(|key| self.items.id_for(key).is_some())
    }

    /// Return whether every checked key points at a live list item.
    fn checks_invariant_holds(&self) -> bool {
        self.checks
            .as_ref()
            .is_none_or(|checks| checks.iter().all(|key| self.items.id_for(key).is_some()))
    }

    /// Select an item by index, focus it, and scroll it into view.
    ///
    /// The index is clamped to the last item. An empty list is a no-op.
    fn select_and_reveal(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }
        self.update_selection(c, Some(index.min(self.items.len() - 1)))?;
        self.focus_selected(c)?;
        if let Some(item) = self.selected_item() {
            c.reveal_node(item.into(), RevealAlign::Nearest)?;
        }
        Ok(())
    }

    /// Move selection to the first item.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_and_reveal(c, 0)
    }

    /// Move selection to the last item.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_and_reveal(c, self.items.len().saturating_sub(1))
    }

    /// Move selection by a signed offset.
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let next = self
            .selected_index()
            .unwrap_or(0)
            .saturating_add_signed(delta as isize);
        self.select_and_reveal(c, next)
    }

    /// Handle a mouse click within the list.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<bool> {
        match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                let Some(index) = self.index_at_location(c, event.location) else {
                    return Ok(false);
                };
                self.select_and_reveal(c, index)?;
                if self.on_activate.is_some() {
                    self.pending_activate = Some(PendingActivate {
                        key: self.items.keys()[index].clone(),
                        origin: event.location,
                        dragged: false,
                    });
                    c.capture_mouse()?;
                }
                Ok(true)
            }
            mouse::Action::Drag if event.button == mouse::Button::Left => {
                if let Some(pending) = self.pending_activate.as_mut()
                    && self.on_activate.is_some()
                {
                    if drag_exceeded(
                        pending.origin,
                        event.location,
                        DEFAULT_ACTIVATE_DRAG_THRESHOLD,
                    ) {
                        pending.dragged = true;
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            mouse::Action::Up if event.button == mouse::Button::Left => {
                let pending = self.pending_activate.take();
                if let Some(pending) = pending {
                    c.release_mouse()?;
                    if !pending.dragged {
                        let index = self.index_at_location(c, event.location);
                        if let Some(index) = index
                            && self.items.keys().get(index) == Some(&pending.key)
                        {
                            self.post_activate(c, index)?;
                        }
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Set focus on the currently selected item.
    fn focus_selected(&self, c: &mut dyn Context) -> Result<()> {
        if let Some(id) = self.selected_item()
            && c.is_attached(id.into())
        {
            c.set_focus(id.into())?;
        }
        Ok(())
    }

    /// Post the activation command for a selected row.
    fn post_activate(&self, c: &mut dyn Context, index: usize) -> Result<()> {
        let Some(call) = self.on_activate.as_ref() else {
            return Ok(());
        };
        c.post(&call.with_arg("index", index))
    }

    /// Move selection by one page.
    /// Positive values move down; negative values move up. Zero is a no-op.
    /// @param delta Signed page delta. Positive moves down and negative moves
    /// up.
    #[command]
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }
        self.page_shift(c, delta > 0)
    }

    /// Move selection by one page and keep it visible.
    fn page_shift(&mut self, c: &mut dyn Context, forward: bool) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }

        let view = c.view();
        let view_rect = view.view_rect();
        if view_rect.h == 0 {
            return Ok(());
        }

        let metrics = self.item_metrics(c);
        let selected_idx = self.selected_index().unwrap_or(0).min(self.items.len() - 1);
        let Some((start, _height)) = metrics.get(selected_idx).copied() else {
            return Ok(());
        };

        let page = view_rect.h.max(1);
        let target_y = if forward {
            start.saturating_add(page)
        } else {
            start.saturating_sub(page)
        };

        // Keyboard paging clamps at the last row; mouse hit testing does not.
        let target_idx = Self::index_at_y(&metrics, target_y).unwrap_or(self.items.len() - 1);
        self.select_and_reveal(c, target_idx)
    }

    /// Find the item index at a viewport-local location.
    fn index_at_location(&self, c: &dyn Context, location: PointI32) -> Option<usize> {
        let content = c.view().content_point(location)?;
        let metrics = self.item_metrics(c);
        Self::index_at_y(&metrics, content.y)
    }

    /// Build (start_y, height) tuples for each item, one per keyed child.
    ///
    /// An item that has not been laid out yet counts as one row high.
    fn item_metrics(&self, c: &dyn ViewContext) -> Vec<(u32, u32)> {
        let mut metrics = Vec::with_capacity(self.items.len());
        let mut y_offset = 0u32;

        for id in self.items.iter_ids() {
            let height = c.view_of(id.into()).map(|v| v.outer.h).unwrap_or(1);
            metrics.push((y_offset, height));
            y_offset = y_offset.saturating_add(height);
        }

        metrics
    }

    /// Find the item index covering a y coordinate.
    fn index_at_y(metrics: &[(u32, u32)], y: u32) -> Option<usize> {
        metrics
            .iter()
            .position(|(start, height)| *start <= y && y < start.saturating_add(*height))
    }
    /// Reconcile the list order without creating new widgets.
    fn reconcile_order(
        &mut self,
        ctx: &mut dyn Context,
        desired: Vec<K>,
    ) -> Result<Vec<TypedId<W>>> {
        self.reconcile(
            ctx,
            desired,
            |_| {
                Err(Error::Internal(
                    "list reconcile requested a missing widget".into(),
                ))
            },
            |_, _, _| Ok(()),
        )
    }
}

impl<W: Selectable> List<W, AutoKey> {
    /// Append a widget with a fresh automatic key.
    pub fn append(&mut self, ctx: &mut dyn Context, widget: W) -> Result<TypedId<W>> {
        self.insert(ctx, self.len(), widget)
    }

    /// Insert a widget with a fresh automatic key at the clamped index.
    pub fn insert(&mut self, ctx: &mut dyn Context, index: usize, widget: W) -> Result<TypedId<W>> {
        let index = index.min(self.len());
        let key = AutoKey(self.next_key);
        self.next_key = self
            .next_key
            .checked_add(1)
            .ok_or_else(|| Error::Internal("list automatic keys exhausted".into()))?;
        let previous_focus = ctx.focused_node();
        let was_empty = self.is_empty();
        let mut desired = self.items.keys().to_vec();
        desired.insert(index, key);
        let mut widget = Some(widget);
        let ordered = self.reconcile(
            ctx,
            desired,
            |_| {
                widget
                    .take()
                    .ok_or_else(|| Error::Internal("list widget already consumed".into()))
            },
            |_, _, _| Ok(()),
        )?;
        if was_empty {
            self.focus_selected(ctx)?;
        } else if let Some(focus) = previous_focus {
            ctx.set_focus(focus)?;
        }
        Ok(ordered[index])
    }
}

impl<W: Selectable + 'static, K: Eq + Hash + Clone + ToArgValue + 'static> Widget for List<W, K> {
    fn accepts_intent(&self, intent: &str, _ctx: &dyn ViewContext) -> bool {
        !self.items.is_empty() && CursorMove::of(intent).is_some()
    }

    fn on_intent(&mut self, intent: &str, ctx: &mut dyn Context) -> Result<EventOutcome> {
        match CursorMove::of(intent) {
            Some(CursorMove::By(delta)) => self.select_by(ctx, delta)?,
            Some(CursorMove::Page(delta)) => self.page(ctx, delta)?,
            Some(CursorMove::First) => self.select_first(ctx)?,
            Some(CursorMove::Last) => self.select_last(ctx)?,
            None => return Ok(EventOutcome::Ignore),
        }
        Ok(EventOutcome::Handle)
    }

    fn semantics(&self, ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("list".into()),
            label: self.label.clone(),
            selected_keys: self
                .selected
                .iter()
                .cloned()
                .map(ToArgValue::to_arg_value)
                .collect(),
            checked_keys: self
                .checked_keys()
                .into_iter()
                .cloned()
                .map(ToArgValue::to_arg_value)
                .collect(),
            activation_status: self.command_status(ctx)?,
            ..WidgetSemantics::default()
        })
    }

    fn layout(&self) -> Layout {
        let mut layout = Layout::fill().overflow_x(MeasureOverflow::Unbounded);
        if let Some(indicator) = &self.selection_indicator
            && indicator.width > 0
        {
            layout = layout.padding(Edges::new(0, 0, 0, indicator.width));
        }
        layout
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event
            && self.handle_click(ctx, *mouse_event)?
        {
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let area = view.outer_rect_local();

        // Fill background using list style.
        rndr.fill("list", area, ' ')?;

        if let Some(indicator) = &self.selection_indicator
            && let Some(selected_idx) = self.selected_index()
            && indicator.width > 0
        {
            let metrics = self.item_metrics(ctx);
            if let Some((start, height)) = metrics.get(selected_idx).copied() {
                let view_rect = view.view_rect();
                let visible_start = start.max(view_rect.tl.y);
                let visible_end = start
                    .saturating_add(height)
                    .min(view_rect.tl.y.saturating_add(view_rect.h));

                if visible_start < visible_end {
                    let outer =
                        Point::try_from(view.viewport_to_outer(PointI32::try_from(Point {
                            x: 0,
                            y: visible_start - view_rect.tl.y,
                        })?)?)?;
                    let local_y = outer.y;
                    let width = indicator.width.min(area.w);

                    if width > 0 {
                        if indicator.repeat {
                            for offset in 0..(visible_end - visible_start) {
                                let line = Line::new(0, local_y.saturating_add(offset), width);
                                rndr.text(&indicator.style, line, &indicator.text)?;
                            }
                        } else {
                            let line = Line::new(0, local_y, width);
                            rndr.text(&indicator.style, line, &indicator.text)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        // For now, defer to intrinsic content sizing
        // The actual layout will be handled by the column layout
        let available_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => 100,
        };

        // Estimate based on item count (items will self-measure)
        let height = self.items.len() as u32;
        c.clamp(Size::new(available_width, height.max(1)))
    }

    fn canvas(&self, view: Size, ctx: &CanvasContext<'_>) -> Size {
        // Sum child canvas heights and find max canvas width for scrolling
        let mut total_height = 0u32;
        let mut max_width = view.w;

        for child in ctx.children() {
            // Use canvas dimensions for proper scroll support
            total_height = total_height.saturating_add(child.canvas.h);
            max_width = max_width.max(child.canvas.w);
        }

        Size::new(max_width, total_height.max(1))
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        // List itself doesn't accept focus; items do
        false
    }

    fn name(&self) -> NodeName {
        NodeName::convert("list")
    }
}

/// Compute the indicator width in cells from a multi-line string.
fn indicator_width(text: &str) -> u32 {
    text.lines().map(text::width).max().unwrap_or(0)
}

/// Return true when drag distance exceeds the configured threshold.
fn drag_exceeded(origin: PointI32, current: PointI32, threshold: u32) -> bool {
    let dx = origin.x.abs_diff(current.x);
    let dy = origin.y.abs_diff(current.y);
    dx.max(dy) > threshold
}

#[cfg(test)]
mod tests {
    use canopy::{
        NodeId, NodeName, Register, Setup, ViewContext,
        commands::CommandTarget,
        derive_commands,
        input::key,
        layout::{Edges, Layout},
        testing::harness::Harness,
    };

    use super::*;
    use crate::Text;

    struct ActivationRoot {
        fail: bool,
        activations: usize,
    }

    #[derive_commands]
    impl ActivationRoot {
        #[command]
        fn activate(&mut self, index: usize) -> Result<()> {
            assert_eq!(index, 0);
            self.activations += 1;
            if self.fail {
                Err(Error::Invalid("activation failed".into()))
            } else {
                Ok(())
            }
        }
    }

    impl Widget for ActivationRoot {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let list = ctx.add_child(
                ctx.node_id(),
                List::<Text>::new().with_command(Self::spec_activate().call()),
            )?;
            ctx.with_widget_mut::<List<Text>, _>(list, |list, ctx| {
                list.append(ctx, Text::new("First row"))?;
                Ok(())
            })
        }
    }

    impl Register for ActivationRoot {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    #[test]
    fn list_activation_reports_errors_as_notices_after_releasing_capture() -> Result<()> {
        for (fail, drag) in [(false, false), (true, false), (true, true)] {
            let mut harness = Harness::builder(ActivationRoot {
                fail,
                activations: 0,
            })
            .register::<ActivationRoot>()
            .size(20, 4)
            .build()?;
            harness.render()?;
            let mut event = mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: PointI32 { x: 0, y: 0 },
            };
            harness.mouse(event)?;
            if drag {
                event.action = mouse::Action::Drag;
                event.location.x = 8;
                harness.mouse(event)?;
            }
            event.action = mouse::Action::Up;
            event.location.x = 0;
            harness.mouse(event)?;
            assert_eq!(
                harness.canopy.notices().len(),
                usize::from(fail && !drag),
                "a failed activation is a notice"
            );
            harness.with_root_widget_context(|root: &mut ActivationRoot, ctx| {
                assert_eq!(root.activations, usize::from(!drag));
                assert!(!ctx.with_unique_descendant::<List<Text>, _>(|_, ctx| {
                    Ok(ctx.has_mouse_capture())
                })?);
                Ok(())
            })?;
        }
        Ok(())
    }

    struct Row {
        selected: bool,
        checked: bool,
    }

    #[derive_commands]
    impl Row {
        fn new() -> Self {
            Self {
                selected: false,
                checked: false,
            }
        }
    }

    impl Selectable for Row {
        fn set_selected(&mut self, selected: bool) {
            self.selected = selected;
        }

        fn set_checked(&mut self, checked: bool) {
            self.checked = checked;
        }
    }

    impl Widget for Row {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn name(&self) -> NodeName {
            NodeName::convert("row")
        }
    }

    impl Register for List<Text> {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()?;
            Ok(())
        }
    }

    impl Register for List<Row> {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()?;
            Ok(())
        }
    }

    impl Register for List<Row, i64> {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    /// Reconcile a domain-keyed fixture with selectable rows.
    fn reconcile_rows(
        list: &mut List<Row, i64>,
        ctx: &mut dyn Context,
        keys: &[i64],
    ) -> Result<()> {
        list.reconcile(
            ctx,
            keys.iter().copied(),
            |_| Ok(Row::new()),
            |_, _, _| Ok(()),
        )?;
        Ok(())
    }

    #[test]
    fn keyed_reorder_preserves_selection_widget_and_focus() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        let selected = harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10, 20, 30])?;
            list.select_by(ctx, 1)?;
            let selected = list.item_for_key(&20).expect("selected row");
            reconcile_rows(list, ctx, &[30, 10, 20])?;
            assert_eq!(list.selected_key(), Some(&20));
            assert_eq!(list.selected_index(), Some(2));
            assert_eq!(list.item_for_key(&20), Some(selected));
            let semantics = list.semantics(ctx)?;
            assert_eq!(semantics.role.as_deref(), Some("list"));
            assert_eq!(semantics.selected_keys, vec![20_i64.to_arg_value()]);
            assert_eq!(semantics.selected, None);
            Ok(selected)
        })?;
        assert_eq!(focused_row(&harness), Some(selected.into()));
        assert!(harness.with_widget(selected, |row: &mut Row| row.selected));
        Ok(())
    }

    #[test]
    fn navigation_intents_move_the_selection() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .register::<List<Row, i64>>()
            .script(
                "nav",
                r#"canopy.keymap({
                    { key = "j", description = "Down", action = "canopy.nav.down" },
                    { key = "G", description = "Last", action = "canopy.nav.last" },
                    { key = "g", description = "First", action = "canopy.nav.first" },
                })"#,
            )
            .size(20, 10)
            .build()?;
        harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10, 20, 30])
        })?;
        harness.render()?;
        let selected = |harness: &mut Harness| {
            harness.with_root_widget(|list: &mut List<Row, i64>| list.selected_index())
        };
        harness.key('j')?;
        assert_eq!(selected(&mut harness), Some(1));
        harness.key('G')?;
        assert_eq!(selected(&mut harness), Some(2));
        harness.key('g')?;
        assert_eq!(selected(&mut harness), Some(0));
        Ok(())
    }

    #[test]
    fn checks_toggle_the_selected_row_and_update_widgets() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new().with_checks())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        let (toggled, other) =
            harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
                reconcile_rows(list, ctx, &[10, 20, 30])?;
                let toggled = list.item_for_key(&20).expect("toggled row");
                let other = list.item_for_key(&10).expect("other row");
                list.select_key(ctx, &20)?;
                assert!(list.checks_enabled());
                assert!(list.checked_keys().is_empty());
                assert!(list.toggle(ctx)?, "toggle returns the new state");
                assert!(list.is_checked(&20));
                assert_eq!(list.checked_len(), 1);
                assert_eq!(list.checked_keys(), vec![&20]);
                list.select_key(ctx, &10)?;
                let semantics = list.semantics(ctx)?;
                assert_eq!(
                    semantics.checked_keys,
                    vec![20_i64.to_arg_value()],
                    "semantics report the checks"
                );
                assert_eq!(
                    semantics.selected_keys,
                    vec![10_i64.to_arg_value()],
                    "semantics report the cursor apart from the checks"
                );
                list.select_key(ctx, &20)?;
                assert!(!list.toggle(ctx)?, "a second toggle clears it");
                assert!(!list.is_checked(&20));
                assert!(list.toggle(ctx)?);
                Ok((toggled, other))
            })?;
        assert!(
            harness.with_widget(toggled, |row: &mut Row| row.checked),
            "the checked row learns its state"
        );
        assert!(
            !harness.with_widget(other, |row: &mut Row| row.checked),
            "other rows stay unchecked"
        );
        Ok(())
    }

    #[test]
    fn check_all_and_clear_update_every_row() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new().with_checks())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        let ids = harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10, 20])?;
            list.check_all(ctx)?;
            assert_eq!(list.checked_keys(), vec![&10, &20]);
            Ok((0..list.len())
                .map(|index| list.item(index).expect("row"))
                .collect::<Vec<_>>())
        })?;
        for id in &ids {
            assert!(harness.with_widget(*id, |row: &mut Row| row.checked));
        }
        harness
            .with_root_widget_context(|list: &mut List<Row, i64>, ctx| list.clear_checks(ctx))?;
        for id in &ids {
            assert!(!harness.with_widget(*id, |row: &mut Row| row.checked));
        }
        assert!(harness.with_root_widget(|list: &mut List<Row, i64>| list.checked_len()) == 0);
        Ok(())
    }

    #[test]
    fn checks_follow_reconcile_and_validate_keys() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new().with_checks())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10, 20, 30])?;
            assert!(list.set_checked(ctx, &99, true).is_err());
            list.set_checked(ctx, &20, true)?;
            list.set_checked(ctx, &20, true)?;
            assert_eq!(list.checked_len(), 1, "setting twice is idempotent");
            reconcile_rows(list, ctx, &[30, 10])?;
            assert!(
                list.checked_keys().is_empty(),
                "a removed row drops its check"
            );
            list.select_key(ctx, &30)?;
            list.toggle(ctx)?;
            reconcile_rows(list, ctx, &[10, 30])?;
            assert_eq!(
                list.checked_keys(),
                vec![&30],
                "a surviving row keeps its check"
            );
            assert!(list.checks_invariant_holds());
            Ok(())
        })
    }

    #[test]
    fn a_plain_list_has_no_checks() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10])?;
            assert!(!list.checks_enabled());
            assert!(!list.toggle(ctx)?, "toggle is inert without checks");
            assert!(list.checked_keys().is_empty());
            list.set_checked(ctx, &10, true)?;
            assert!(!list.is_checked(&10));
            Ok(())
        })
    }

    #[test]
    fn keyed_selection_removal_prefers_old_neighbors() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10, 20, 30, 40])?;
            list.select_key(ctx, &20)?;
            reconcile_rows(list, ctx, &[40, 10, 30])?;
            assert_eq!(list.selected_key(), Some(&30));
            reconcile_rows(list, ctx, &[40, 10])?;
            assert_eq!(list.selected_key(), Some(&10));
            reconcile_rows(list, ctx, &[99])?;
            assert_eq!(list.selected_key(), None);
            assert!(list.select_key(ctx, &100).is_err());
            list.select_key(ctx, &99)?;
            assert_eq!(list.selected_index(), Some(0));
            Ok(())
        })
    }

    #[test]
    fn keyed_duplicate_and_failed_updates_preserve_mapping() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            reconcile_rows(list, ctx, &[10, 20])?;
            let first = list.item_for_key(&10);
            let mut creates = 0;
            let mut updates = 0;
            assert!(
                list.reconcile(
                    ctx,
                    [30, 30],
                    |_| {
                        creates += 1;
                        Ok(Row::new())
                    },
                    |_, _, _| {
                        updates += 1;
                        Ok(())
                    }
                )
                .is_err()
            );
            assert_eq!((creates, updates), (0, 0));
            assert!(
                list.reconcile(
                    ctx,
                    [20, 30],
                    |_| Ok(Row::new()),
                    |_, _, _| { Err(Error::Invalid("update failed".into())) }
                )
                .is_err()
            );
            assert_eq!(list.item(0), first);
            assert_eq!(list.selected_key(), Some(&10));
            assert_eq!(list.item_for_key(&30), None);
            Ok(())
        })
    }

    #[test]
    fn keyed_list_rejects_unmanaged_children_before_callbacks() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .register::<List<Row, i64>>()
            .size(20, 10)
            .build()?;
        harness.with_root_widget_context(|list: &mut List<Row, i64>, ctx| {
            let header = ctx.add_child(ctx.node_id(), Text::new("unmanaged header"))?;
            let result = list.reconcile(
                ctx,
                [1],
                |_| panic!("create must not run with unmanaged children"),
                |_, _, _| panic!("update must not run with unmanaged children"),
            );
            assert!(result.is_err());
            assert_eq!(ctx.children_of(ctx.node_id()), vec![header.into()]);
            assert!(list.is_empty());
            Ok(())
        })
    }

    /// Receives domain-keyed row activation by its current index.
    #[derive(Default)]
    struct KeyedActivationRoot {
        /// Last delivered activation index.
        activation: Option<usize>,
    }

    #[derive_commands]
    impl KeyedActivationRoot {
        #[command]
        fn activate(&mut self, index: usize) {
            self.activation = Some(index);
        }
    }

    impl Widget for KeyedActivationRoot {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let list = ctx.add_child(
                ctx.node_id(),
                List::<Text, i64>::new().with_command(
                    Self::spec_activate()
                        .call()
                        .with_target(CommandTarget::Exact(ctx.node_id())),
                ),
            )?;
            ctx.with_widget_mut(list, |list: &mut List<Text, i64>, ctx| {
                list.reconcile(
                    ctx,
                    [10, 20],
                    |key| Ok(Text::new(key.to_string())),
                    |_, _, _| Ok(()),
                )?;
                Ok(())
            })
        }
    }

    impl Register for KeyedActivationRoot {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    /// Holds a keyed list, and records each activation with the key at its
    /// index when the activation runs.
    #[derive(Default)]
    struct KeyedHost {
        /// Index and key of each activation.
        activated: Vec<(usize, Option<i64>)>,
        /// Whether an activation removes the list.
        remove_on_activate: bool,
    }

    #[derive_commands]
    impl KeyedHost {
        #[command]
        fn activate(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {
            let (list, key) = ctx.with_unique_descendant::<List<Text, i64>, _>(|list, ctx| {
                Ok((ctx.node_id(), list.keys().get(index).copied()))
            })?;
            self.activated.push((index, key));
            if self.remove_on_activate {
                ctx.remove_subtree(list)?;
            }
            Ok(())
        }
    }

    impl Widget for KeyedHost {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let list = ctx.add_child(
                ctx.node_id(),
                List::<Text, i64>::new().with_command(
                    Self::spec_activate()
                        .call()
                        .with_target(CommandTarget::Exact(ctx.node_id())),
                ),
            )?;
            ctx.with_widget_mut(list, |list: &mut List<Text, i64>, ctx| {
                list.reconcile(
                    ctx,
                    [10, 20],
                    |key| Ok(Text::new(key.to_string())),
                    |_, _, _| Ok(()),
                )?;
                Ok(())
            })
        }
    }

    impl Register for KeyedHost {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    /// Build a keyed host, rendered once, and return its list.
    fn keyed_host(remove_on_activate: bool) -> Result<(Harness, NodeId)> {
        let mut harness = Harness::builder(KeyedHost {
            remove_on_activate,
            ..KeyedHost::default()
        })
        .register::<KeyedHost>()
        .size(20, 4)
        .build()?;
        harness.render()?;
        let list = harness.with_root_widget_context(|_: &mut KeyedHost, ctx| {
            ctx.with_unique_descendant::<List<Text, i64>, _>(|_, ctx| Ok(ctx.node_id()))
        })?;
        Ok((harness, list))
    }

    #[test]
    fn an_activation_index_names_a_position_not_a_row() -> Result<()> {
        let (mut harness, list) = keyed_host(false)?;
        let press = |action| {
            Event::Mouse(mouse::MouseEvent {
                action,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: PointI32 { x: 0, y: 0 },
            })
        };
        harness.canopy.with_context(list, |ctx| {
            ctx.with_widget_mut(list, |list: &mut List<Text, i64>, ctx| {
                list.on_event(&press(mouse::Action::Down), ctx)?;
                list.on_event(&press(mouse::Action::Up), ctx)?;
                // The rows change before the posted activation runs.
                list.reconcile(
                    ctx,
                    [20, 10],
                    |key| Ok(Text::new(key.to_string())),
                    |_, _, _| Ok(()),
                )?;
                Ok(())
            })
        })?;
        let activated = harness.with_root_widget(|host: &mut KeyedHost| host.activated.clone());
        assert_eq!(
            activated,
            [(0, Some(20))],
            "index 0 now holds the other row"
        );
        Ok(())
    }

    #[test]
    fn a_host_can_remove_the_list_from_its_activation() -> Result<()> {
        let (mut harness, list) = keyed_host(true)?;
        let mut event = mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 0, y: 0 },
        };
        harness.mouse(event)?;
        event.action = mouse::Action::Up;
        harness.mouse(event)?;
        let activated = harness.with_root_widget(|host: &mut KeyedHost| host.activated.clone());
        assert_eq!(activated, [(0, Some(10))]);
        let gone = harness
            .canopy
            .with_root_view(|ctx| ctx.type_id_of(list).is_none());
        assert!(gone, "the host removed the list that activated");
        assert!(harness.canopy.notices().is_empty());
        Ok(())
    }

    #[test]
    fn blank_space_below_rows_neither_selects_nor_activates() -> Result<()> {
        for press_y in [1, 2, 4] {
            let mut harness = Harness::builder(KeyedActivationRoot::default())
                .register::<KeyedActivationRoot>()
                .size(20, 5)
                .build()?;
            harness.render()?;
            let mut event = mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: PointI32 { x: 0, y: press_y },
            };
            harness.mouse(event)?;
            if press_y >= 2 {
                harness.with_root_widget_context(|_: &mut KeyedActivationRoot, ctx| {
                    ctx.with_unique_descendant::<List<Text, i64>, _>(|_, ctx| {
                        assert!(!ctx.has_mouse_capture(), "blank space cannot capture");
                        Ok(())
                    })
                })?;
            }
            event.action = mouse::Action::Up;
            event.location.y = 4;
            harness.mouse(event)?;
            harness.with_root_widget_context(|root: &mut KeyedActivationRoot, ctx| {
                assert_eq!(
                    root.activation, None,
                    "release outside a row cannot activate"
                );
                ctx.with_unique_descendant::<List<Text, i64>, _>(|list, ctx| {
                    assert!(!ctx.has_mouse_capture());
                    let expected = if press_y == 1 { 20 } else { 10 };
                    assert_eq!(list.selected_key(), Some(&expected));
                    Ok(())
                })
            })?;
        }
        Ok(())
    }

    #[test]
    fn release_in_padding_above_rows_does_not_activate() -> Result<()> {
        let mut harness = Harness::builder(KeyedActivationRoot::default())
            .register::<KeyedActivationRoot>()
            .size(20, 5)
            .build()?;
        harness.with_root_widget_context(|_: &mut KeyedActivationRoot, ctx| {
            ctx.with_unique_descendant::<List<Text, i64>, _>(|_, ctx| {
                ctx.set_layout_override(
                    ctx.node_id(),
                    Layout::fill().padding(Edges::new(1, 0, 0, 0)).into(),
                )
            })
        })?;
        harness.render()?;
        let mut event = mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 0, y: 1 },
        };
        harness.mouse(event)?;
        event.action = mouse::Action::Up;
        event.location.y = 0;
        harness.mouse(event)?;
        harness.with_root_widget::<KeyedActivationRoot, _>(|root| {
            assert_eq!(
                root.activation, None,
                "a release in the padding above the rows is outside every row"
            );
        });
        Ok(())
    }

    #[test]
    fn paging_beyond_short_list_clamps_to_first_and_last_rows() -> Result<()> {
        let mut harness = Harness::builder(KeyedActivationRoot::default())
            .register::<KeyedActivationRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness.with_root_widget_context(|_: &mut KeyedActivationRoot, ctx| {
            ctx.with_unique_descendant::<List<Text, i64>, _>(|list, ctx| {
                list.page(ctx, 0)?;
                assert_eq!(list.selected_key(), Some(&10));
                list.page(ctx, 1)?;
                assert_eq!(list.selected_key(), Some(&20));
                list.page(ctx, 1)?;
                assert_eq!(list.selected_key(), Some(&20));
                list.page(ctx, -1)?;
                assert_eq!(list.selected_key(), Some(&10));
                list.page(ctx, -1)?;
                assert_eq!(list.selected_key(), Some(&10));
                Ok(())
            })
        })
    }

    #[test]
    fn pending_activation_follows_key_through_reorder() -> Result<()> {
        let mut harness = Harness::builder(KeyedActivationRoot::default())
            .register::<KeyedActivationRoot>()
            .size(20, 4)
            .build()?;
        harness.render()?;
        let mut event = mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 0, y: 0 },
        };
        harness.mouse(event)?;
        harness.with_root_widget_context(|_: &mut KeyedActivationRoot, ctx| {
            ctx.with_unique_descendant::<List<Text, i64>, _>(|list, ctx| {
                list.reconcile(
                    ctx,
                    [20, 10],
                    |_| unreachable!("rows retained"),
                    |_, _, _| Ok(()),
                )?;
                Ok(())
            })
        })?;
        harness.render()?;
        event.action = mouse::Action::Up;
        event.location.y = 1;
        harness.mouse(event)?;
        harness.with_root_widget::<KeyedActivationRoot, _>(|root| {
            assert_eq!(
                root.activation,
                Some(1),
                "the row keyed 10 moved to index 1 while pressed"
            );
        });
        Ok(())
    }

    #[test]
    fn removing_pressed_key_cancels_activation_and_capture() -> Result<()> {
        let mut harness = Harness::builder(KeyedActivationRoot::default())
            .register::<KeyedActivationRoot>()
            .size(20, 4)
            .build()?;
        harness.render()?;
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 0, y: 0 },
        })?;
        harness.with_root_widget_context(|root: &mut KeyedActivationRoot, ctx| {
            ctx.with_unique_descendant::<List<Text, i64>, _>(|list, ctx| {
                list.reconcile(
                    ctx,
                    [20],
                    |_| unreachable!("row retained"),
                    |_, _, _| Ok(()),
                )?;
                assert!(list.pending_activate.is_none());
                assert!(!ctx.has_mouse_capture());
                Ok(())
            })?;
            assert_eq!(root.activation, None);
            Ok(())
        })
    }

    fn row_selection(harness: &mut Harness) -> Vec<bool> {
        let ids = harness.with_root_widget::<List<Row>, _>(|list| {
            (0..list.len())
                .map(|index| list.item(index).expect("row id"))
                .collect::<Vec<_>>()
        });
        ids.into_iter()
            .map(|id| harness.with_widget::<Row, _>(id, |row| row.selected))
            .collect()
    }

    fn focused_row(harness: &Harness) -> Option<NodeId> {
        harness
            .canopy
            .with_root_view(|context| context.focused_node())
    }

    #[test]
    fn test_list_append_and_select() -> Result<()> {
        let root = List::<Text>::new();
        let mut harness = Harness::builder(root)
            .register::<List<Text>>()
            .size(20, 10)
            .build()?;

        // Add items
        harness.with_root_widget::<List<Text>, _>(|list| {
            assert!(list.is_empty());
            assert_eq!(list.len(), 0);
        });

        harness.with_root_widget_context(|list: &mut List<Text>, ctx| {
            list.append(ctx, Text::new("Item 1"))?;
            list.append(ctx, Text::new("Item 2"))?;
            list.append(ctx, Text::new("Item 3"))?;
            Ok(())
        })?;

        harness.with_root_widget::<List<Text>, _>(|list| {
            assert_eq!(list.len(), 3);
            assert_eq!(list.selected_index(), Some(0)); // First item auto-selected
        });

        Ok(())
    }

    #[test]
    fn test_list_navigation() -> Result<()> {
        let root = List::<Text>::new();
        let mut harness = Harness::builder(root)
            .register::<List<Text>>()
            .size(20, 10)
            .build()?;

        harness.with_root_widget_context(|list: &mut List<Text>, ctx| {
            list.append(ctx, Text::new("Item 1"))?;
            list.append(ctx, Text::new("Item 2"))?;
            list.append(ctx, Text::new("Item 3"))?;
            Ok(())
        })?;

        harness.render()?;

        harness.script(include_str!("../tests/luau/list_navigation.luau"))?;
        harness.with_root_widget::<List<Text>, _>(|list| {
            assert_eq!(list.selected_index(), Some(0));
        });

        Ok(())
    }

    #[test]
    fn test_list_remove() -> Result<()> {
        let root = List::<Text>::new();
        let mut harness = Harness::builder(root)
            .register::<List<Text>>()
            .size(20, 10)
            .build()?;

        harness.with_root_widget_context(|list: &mut List<Text>, ctx| {
            list.append(ctx, Text::new("Item 1"))?;
            list.append(ctx, Text::new("Item 2"))?;
            list.append(ctx, Text::new("Item 3"))?;
            Ok(())
        })?;

        harness.render()?;
        harness.script(include_str!("../tests/luau/list_remove.luau"))?;

        harness.with_root_widget::<List<Text>, _>(|list| {
            assert_eq!(list.len(), 2);
            assert_eq!(list.selected_index(), Some(1)); // Selection stays at index 1 (now last item)
        });

        Ok(())
    }

    #[test]
    fn test_list_clear() -> Result<()> {
        let root = List::<Text>::new();
        let mut harness = Harness::builder(root)
            .register::<List<Text>>()
            .size(20, 10)
            .build()?;

        harness.with_root_widget_context(|list: &mut List<Text>, ctx| {
            list.append(ctx, Text::new("Item 1"))?;
            list.append(ctx, Text::new("Item 2"))?;
            Ok(())
        })?;

        harness.render()?;
        harness.script(include_str!("../tests/luau/list_clear.luau"))?;

        harness.with_root_widget::<List<Text>, _>(|list| {
            assert!(list.is_empty());
            assert_eq!(list.selected_index(), None);
        });

        Ok(())
    }

    #[test]
    fn selection_and_focus_follow_selected_row() -> Result<()> {
        let root = List::<Row>::new();
        let mut harness = Harness::builder(root)
            .register::<List<Row>>()
            .size(20, 10)
            .build()?;

        let first = harness.with_root_widget_context(|list: &mut List<Row>, ctx| {
            let first = list.append(ctx, Row::new())?;
            list.append(ctx, Row::new())?;
            list.append(ctx, Row::new())?;
            Ok(first)
        })?;

        harness.with_root_widget::<List<Row>, _>(|list| {
            assert!(list.selection_invariant_holds());
            assert_eq!(list.selected_index(), Some(0));
        });
        assert_eq!(row_selection(&mut harness), [true, false, false]);
        assert_eq!(focused_row(&harness), Some(first.into()));

        let third = harness.with_root_widget_context(|list: &mut List<Row>, ctx| {
            list.select_by(ctx, 2)?;
            Ok(list.selected_item().expect("selected row"))
        })?;

        harness.with_root_widget::<List<Row>, _>(|list| {
            assert!(list.selection_invariant_holds());
            assert_eq!(list.selected_index(), Some(2));
        });
        assert_eq!(row_selection(&mut harness), [false, false, true]);
        assert_eq!(focused_row(&harness), Some(third.into()));

        let second = harness.with_root_widget_context(|list: &mut List<Row>, ctx| {
            assert!(list.remove(ctx, 2)?);
            Ok(list.selected_item().expect("selected row"))
        })?;

        harness.with_root_widget::<List<Row>, _>(|list| {
            assert!(list.selection_invariant_holds());
            assert_eq!(list.selected_index(), Some(1));
        });
        assert_eq!(row_selection(&mut harness), [false, true]);
        assert_eq!(focused_row(&harness), Some(second.into()));

        Ok(())
    }
}
