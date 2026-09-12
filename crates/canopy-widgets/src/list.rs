//! Widget-based list container.
//!
//! A typed list container where items are actual widgets in the tree.
//! Items participate in focus management and can be composed from other
//! widgets.

use std::{collections::HashSet, hash::Hash};

use canopy::{
    Context, ContextExt, EventOutcome, FocusDirection, KeyedChildren, NodeId, NodeName, Render,
    TypedId, ViewContext, Widget, WidgetSemantics,
    commands::{
        ArgValue, CommandAction, CommandArgs, CommandCall, CommandInvocation, CommandScopeFrame,
        CommandStatus, CommandTarget, ListRowContext, ToArgValue,
    },
    derive_commands,
    error::{Error, Result},
    event::{Event, mouse},
    geom::{Line, Point, PointI32, Size},
    layout::{
        CanvasContext, Constraint, Edges, Layout, MeasureConstraints, MeasureOverflow, Measurement,
    },
};
use unicode_width::UnicodeWidthStr;

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

/// Build an activation invocation that appends the row index to a stored
/// command.
fn invocation_with_index(command: &CommandInvocation, index: usize) -> CommandInvocation {
    let args = match &command.args {
        CommandArgs::Positional(values) => {
            let mut out = values.clone();
            out.push(index.to_arg_value());
            CommandArgs::Positional(out)
        }
        CommandArgs::Named(values) => {
            let mut out = values.clone();
            out.insert("index".to_string(), index.to_arg_value());
            CommandArgs::Named(out)
        }
    };
    CommandInvocation {
        id: command.id,
        args,
    }
}

/// Pending activation state for list row clicks.
#[derive(Debug, Clone, Copy)]
struct PendingActivate<K> {
    /// Stable key of the pressed row.
    key: K,
    /// Pointer origin when the press began.
    origin: Point,
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
    /// Optional list-level selection indicator.
    selection_indicator: Option<SelectionIndicator>,
    /// Optional activation command configuration.
    on_activate: Option<CommandAction>,
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
    /// Construct an empty list.
    pub fn new() -> Self {
        Self {
            items: KeyedChildren::new(),
            next_key: 0,
            selected: None,
            selection_indicator: None,
            on_activate: None,
            pending_activate: None,
            label: None,
        }
    }

    /// Set the semantic label of the collection.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Inspect the selected row's configured activation command.
    fn command_status(&self, ctx: &dyn ViewContext) -> Result<Option<CommandStatus>> {
        let Some(action) = self.on_activate.as_ref() else {
            return Ok(None);
        };
        if !ctx.is_attached_of(ctx.node_id()) {
            return Ok(Some(CommandStatus::Disabled("List is detached".into())));
        }
        let Some(index) = self.selected_index() else {
            return Ok(Some(CommandStatus::Disabled("No item selected".into())));
        };
        ctx.command_status(
            action.target.unwrap_or(CommandTarget::From(ctx.node_id())),
            &invocation_with_index(&action.invocation, index),
        )
        .map(Some)
    }

    /// Build a list with a list-level selection indicator.
    /// Repeat controls whether the indicator renders on every visible line.
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

    /// Build a list that dispatches a command when a row is activated.
    pub fn with_on_activate(mut self, command: CommandCall) -> Self {
        self.on_activate = Some(command.action());
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
        let had_focus = ctx.is_on_focus_path_of(ctx.node_id());
        let previous_focus = ctx.focused_node();
        let ordered = self.items.reconcile(ctx, desired, create, |key, id, ctx| {
            update(key, id, ctx)?;
            ctx.with_widget_mut(id, |widget: &mut W, _| {
                widget.set_selected(selected.as_ref() == Some(key));
                Ok(())
            })
        })?;
        self.selected = selected;
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
            && ctx.is_attached_of(focus)
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

    /// Select an item by index, focus it, and scroll it into view.
    ///
    /// The index is clamped to the last item. An empty list is a no-op.
    fn select_and_reveal(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }
        self.update_selection(c, Some(index.min(self.items.len() - 1)))?;
        self.focus_selected(c)?;
        self.ensure_selected_visible(c);
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
                            self.dispatch_activate(c, index)?;
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
            && c.is_attached_of(id.into())
        {
            c.set_focus(id.into())?;
        }
        Ok(())
    }

    /// Dispatch the activation command for a selected row.
    fn dispatch_activate(&self, c: &mut dyn Context, index: usize) -> Result<()> {
        let Some(config) = self.on_activate.as_ref() else {
            return Ok(());
        };
        let frame = CommandScopeFrame {
            event: c.current_event().cloned(),
            mouse: c.current_mouse_event(),
            list_row: Some(ListRowContext {
                list: c.node_id(),
                index,
                key: self.items.keys()[index].clone().to_arg_value(),
            }),
        };
        let invocation = invocation_with_index(&config.invocation, index);
        c.dispatch_scoped(
            config.target.unwrap_or(CommandTarget::From(c.node_id())),
            frame,
            &invocation,
        )?;
        Ok(())
    }

    /// Scroll the view by one line in the specified direction.
    /// @param dir The direction to scroll.
    #[command]
    pub fn scroll(&mut self, c: &mut dyn Context, dir: FocusDirection) {
        match dir {
            FocusDirection::Up | FocusDirection::Prev => {
                c.scroll_up();
            }
            FocusDirection::Down | FocusDirection::Next => {
                c.scroll_down();
            }
            FocusDirection::Left => {
                c.scroll_left();
            }
            FocusDirection::Right => {
                c.scroll_right();
            }
        }
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

    /// Ensure the selected item is visible in the view.
    fn ensure_selected_visible(&self, c: &mut dyn Context) {
        let Some(selected_idx) = self.selected_index() else {
            return;
        };

        let view = c.view();
        let view_rect = view.view_rect();

        // Compute item positions by measuring each child
        let metrics = self.item_metrics(c);
        let Some((start, height)) = metrics.get(selected_idx).copied() else {
            return;
        };

        if start < view_rect.tl.y {
            let delta = view_rect.tl.y - start;
            let _ = c.scroll_by(0, -(delta as i32));
        } else if start.saturating_add(height) > view_rect.tl.y.saturating_add(view_rect.h) {
            let delta = start.saturating_add(height) - (view_rect.tl.y + view_rect.h);
            let _ = c.scroll_by(0, delta as i32);
        }
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
    fn index_at_location(&self, c: &dyn Context, location: Point) -> Option<usize> {
        let view = c.view();
        let content = view
            .viewport_to_content(PointI32::try_from(location).ok()?)
            .ok()?;
        let content_y = u32::try_from(content.y).ok()?;
        let metrics = self.item_metrics(c);
        Self::index_at_y(&metrics, content_y)
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
            action_status: self.command_status(ctx)?,
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
    text.lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        .try_into()
        .unwrap_or(0)
}

/// Return true when drag distance exceeds the configured threshold.
fn drag_exceeded(origin: Point, current: Point, threshold: u32) -> bool {
    let dx = origin.x.abs_diff(current.x);
    let dy = origin.y.abs_diff(current.y);
    dx.max(dy) > threshold
}

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Loader, NodeId, NodeName, ViewContext, derive_commands, event::key,
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
            let list =
                ctx.add_child(List::<Text>::new().with_on_activate(Self::cmd_activate().call()))?;
            ctx.with_widget_mut::<List<Text>, _>(list, |list, ctx| {
                list.append(ctx, Text::new("First row"))?;
                Ok(())
            })
        }
    }

    impl Loader for ActivationRoot {
        fn load(canopy: &mut Canopy) -> Result<()> {
            canopy.add_commands::<Self>()
        }
    }

    #[test]
    fn list_activation_propagates_errors_after_releasing_capture() -> Result<()> {
        for (fail, drag) in [(false, false), (true, false), (true, true)] {
            let mut harness = Harness::builder(ActivationRoot {
                fail,
                activations: 0,
            })
            .size(20, 4)
            .build()?;
            harness.render()?;
            let mut event = mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: Point { x: 0, y: 0 },
            };
            harness.mouse(event)?;
            if drag {
                event.action = mouse::Action::Drag;
                event.location.x = 8;
                harness.mouse(event)?;
            }
            event.action = mouse::Action::Up;
            event.location.x = 0;
            let result = harness.mouse(event);
            assert_eq!(result.is_err(), fail && !drag);
            harness.with_root_context(|root: &mut ActivationRoot, ctx| {
                assert_eq!(root.activations, usize::from(!drag));
                assert_eq!(ctx.take_mouse_capture()?, None);
                Ok(())
            })?;
        }
        Ok(())
    }

    struct Row {
        selected: bool,
    }

    #[derive_commands]
    impl Row {
        fn new() -> Self {
            Self { selected: false }
        }
    }

    impl Selectable for Row {
        fn set_selected(&mut self, selected: bool) {
            self.selected = selected;
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

    impl Loader for List<Text> {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            Ok(())
        }
    }

    impl Loader for List<Row> {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            Ok(())
        }
    }

    impl Loader for List<Row, i64> {
        fn load(canopy: &mut Canopy) -> Result<()> {
            canopy.add_commands::<Self>()
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
            .size(20, 10)
            .build()?;
        let selected = harness.with_root_context(|list: &mut List<Row, i64>, ctx| {
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
    fn keyed_selection_removal_prefers_old_neighbors() -> Result<()> {
        let mut harness = Harness::builder(List::<Row, i64>::new())
            .size(20, 10)
            .build()?;
        harness.with_root_context(|list: &mut List<Row, i64>, ctx| {
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
            .size(20, 10)
            .build()?;
        harness.with_root_context(|list: &mut List<Row, i64>, ctx| {
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
            .size(20, 10)
            .build()?;
        harness.with_root_context(|list: &mut List<Row, i64>, ctx| {
            let header = ctx.add_child(Text::new("unmanaged header"))?;
            let result = list.reconcile(
                ctx,
                [1],
                |_| panic!("create must not run with unmanaged children"),
                |_, _, _| panic!("update must not run with unmanaged children"),
            );
            assert!(result.is_err());
            assert_eq!(ctx.children(), vec![header.into()]);
            assert!(list.is_empty());
            Ok(())
        })
    }

    /// Receives domain-keyed row activation together with its current index.
    #[derive(Default)]
    struct KeyedActivationRoot {
        /// Last delivered activation index and stable key.
        activation: Option<(usize, ArgValue)>,
    }

    #[derive_commands]
    impl KeyedActivationRoot {
        #[command]
        fn activate(&mut self, index: usize, row: ListRowContext) {
            assert_eq!(index, row.index);
            self.activation = Some((index, row.key));
        }
    }

    impl Widget for KeyedActivationRoot {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let list = ctx.add_child(
                List::<Text, i64>::new().with_on_activate(
                    Self::cmd_activate()
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

    impl Loader for KeyedActivationRoot {
        fn load(canopy: &mut Canopy) -> Result<()> {
            canopy.add_commands::<Self>()
        }
    }

    #[test]
    fn blank_space_below_rows_neither_selects_nor_activates() -> Result<()> {
        for press_y in [1, 2, 4] {
            let mut harness = Harness::builder(KeyedActivationRoot::default())
                .size(20, 5)
                .build()?;
            harness.render()?;
            let mut event = mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: Point { x: 0, y: press_y },
            };
            harness.mouse(event)?;
            if press_y >= 2 {
                harness.with_root_context(|_: &mut KeyedActivationRoot, ctx| {
                    assert_eq!(
                        ctx.take_mouse_capture()?,
                        None,
                        "blank space cannot capture"
                    );
                    Ok(())
                })?;
            }
            event.action = mouse::Action::Up;
            event.location.y = 4;
            harness.mouse(event)?;
            harness.with_root_context(|root: &mut KeyedActivationRoot, ctx| {
                assert_eq!(
                    root.activation, None,
                    "release outside a row cannot activate"
                );
                assert_eq!(ctx.take_mouse_capture()?, None);
                ctx.with_unique_descendant::<List<Text, i64>, _>(|list, _| {
                    let expected = if press_y == 1 { 20 } else { 10 };
                    assert_eq!(list.selected_key(), Some(&expected));
                    Ok(())
                })
            })?;
        }
        Ok(())
    }

    #[test]
    fn paging_beyond_short_list_clamps_to_first_and_last_rows() -> Result<()> {
        let mut harness = Harness::builder(KeyedActivationRoot::default())
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness.with_root_context(|_: &mut KeyedActivationRoot, ctx| {
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
            .size(20, 4)
            .build()?;
        harness.render()?;
        let mut event = mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: Point { x: 0, y: 0 },
        };
        harness.mouse(event)?;
        harness.with_root_context(|_: &mut KeyedActivationRoot, ctx| {
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
            assert_eq!(root.activation, Some((1, 10_i64.to_arg_value())));
        });
        Ok(())
    }

    #[test]
    fn removing_pressed_key_cancels_activation_and_capture() -> Result<()> {
        let mut harness = Harness::builder(KeyedActivationRoot::default())
            .size(20, 4)
            .build()?;
        harness.render()?;
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: Point { x: 0, y: 0 },
        })?;
        harness.with_root_context(|root: &mut KeyedActivationRoot, ctx| {
            ctx.with_unique_descendant::<List<Text, i64>, _>(|list, ctx| {
                list.reconcile(
                    ctx,
                    [20],
                    |_| unreachable!("row retained"),
                    |_, _, _| Ok(()),
                )?;
                assert!(list.pending_activate.is_none());
                assert_eq!(ctx.take_mouse_capture()?, None);
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
        let mut harness = Harness::builder(root).size(20, 10).build()?;

        // Add items
        harness.with_root_widget::<List<Text>, _>(|list| {
            assert!(list.is_empty());
            assert_eq!(list.len(), 0);
        });

        harness.with_root_context(|list: &mut List<Text>, ctx| {
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
        let mut harness = Harness::builder(root).size(20, 10).build()?;

        harness.with_root_context(|list: &mut List<Text>, ctx| {
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
        let mut harness = Harness::builder(root).size(20, 10).build()?;

        harness.with_root_context(|list: &mut List<Text>, ctx| {
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
        let mut harness = Harness::builder(root).size(20, 10).build()?;

        harness.with_root_context(|list: &mut List<Text>, ctx| {
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
        let mut harness = Harness::builder(root).size(20, 10).build()?;

        let first = harness.with_root_context(|list: &mut List<Row>, ctx| {
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

        let third = harness.with_root_context(|list: &mut List<Row>, ctx| {
            list.select_by(ctx, 2)?;
            Ok(list.selected_item().expect("selected row"))
        })?;

        harness.with_root_widget::<List<Row>, _>(|list| {
            assert!(list.selection_invariant_holds());
            assert_eq!(list.selected_index(), Some(2));
        });
        assert_eq!(row_selection(&mut harness), [false, false, true]);
        assert_eq!(focused_row(&harness), Some(third.into()));

        let second = harness.with_root_context(|list: &mut List<Row>, ctx| {
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
