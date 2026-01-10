//! Widget-based list container.
//!
//! A typed list container where items are actual widgets in the tree.
//! Items participate in focus management and can be composed from other widgets.

use std::marker::PhantomData;

use canopy::{
    Context, EventOutcome, KeyedChildren, ReadContext, RemovePolicy, TypedId, Widget, command,
    commands::{
        CommandArgs, CommandCall, CommandInvocation, CommandScopeFrame, ListRowContext,
        ScrollDirection, ToArgValue, VerticalDirection,
    },
    derive_commands,
    error::{Error, Result},
    event::{Event, mouse},
    geom::{Line, Point},
    layout::{CanvasContext, Constraint, Edges, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
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

/// Activation configuration for list row clicks.
#[derive(Debug, Clone)]
pub struct ListActivateConfig {
    /// Command invocation to dispatch on activation.
    command: CommandInvocation,
    /// Drag threshold in cells before cancelling activation.
    drag_threshold: u32,
}

impl ListActivateConfig {
    /// Build a new activation config using the default drag threshold.
    pub fn new(command: CommandCall) -> Self {
        Self {
            command: command.invocation(),
            drag_threshold: DEFAULT_ACTIVATE_DRAG_THRESHOLD,
        }
    }

    /// Set the drag threshold in cells.
    pub fn with_drag_threshold(mut self, drag_threshold: u32) -> Self {
        self.drag_threshold = drag_threshold;
        self
    }

    /// Build an activation invocation that includes the row index.
    fn invocation_with_index(&self, index: usize) -> CommandInvocation {
        let args = match &self.command.args {
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
            id: self.command.id,
            args,
        }
    }
}

/// Pending activation state for list row clicks.
#[derive(Debug, Clone, Copy)]
struct PendingActivate {
    /// Selected row index.
    index: usize,
    /// Pointer origin when the press began.
    origin: Point,
    /// Whether the drag threshold was exceeded.
    dragged: bool,
}

/// Monotonic key for list items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ListKey(u64);

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
/// List items are actual widgets in the tree, enabling composition and focus management.
/// The list arranges items vertically and supports scrolling.
///
/// Items must implement the [`Selectable`] trait so the list can manage their
/// selection state independently of focus.
pub struct List<W: Selectable> {
    /// Keyed list items in order.
    items: KeyedChildren<ListKey>,
    /// Next monotonic key to assign.
    next_key: u64,
    /// Currently selected item index.
    selected: Option<usize>,
    /// Optional list-level selection indicator.
    selection_indicator: Option<SelectionIndicator>,
    /// Optional activation command configuration.
    on_activate: Option<ListActivateConfig>,
    /// Pending activation state while handling clicks.
    pending_activate: Option<PendingActivate>,
    /// Marker for the widget type.
    _marker: PhantomData<W>,
}

impl<W: Selectable> Default for List<W> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl<W: Selectable> List<W> {
    /// Construct an empty list.
    pub fn new() -> Self {
        Self {
            items: KeyedChildren::new(),
            next_key: 0,
            selected: None,
            selection_indicator: None,
            on_activate: None,
            pending_activate: None,
            _marker: PhantomData,
        }
    }

    /// Build a list with a list-level selection indicator.
    /// Repeat controls whether the indicator renders on every visible line.
    pub fn with_selection_indicator(
        mut self,
        style: impl Into<String>,
        text: impl Into<String>,
        repeat: bool,
    ) -> Self {
        self.set_selection_indicator(style, text, repeat);
        self
    }

    /// Set a list-level selection indicator.
    /// Repeat controls whether the indicator renders on every visible line.
    pub fn set_selection_indicator(
        &mut self,
        style: impl Into<String>,
        text: impl Into<String>,
        repeat: bool,
    ) {
        let text = text.into();
        let width = indicator_width(&text);
        self.selection_indicator = Some(SelectionIndicator {
            style: style.into(),
            text,
            width,
            repeat,
        });
    }

    /// Clear the list-level selection indicator.
    pub fn clear_selection_indicator(&mut self) {
        self.selection_indicator = None;
    }

    /// Build a list that dispatches a command when a row is activated.
    pub fn with_on_activate(mut self, command: CommandCall) -> Self {
        self.set_on_activate(Some(ListActivateConfig::new(command)));
        self
    }

    /// Configure an activation command for row clicks.
    pub fn set_on_activate(&mut self, config: Option<ListActivateConfig>) {
        self.on_activate = config;
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
        self.items.id_at(index).map(TypedId::new)
    }

    /// Returns the currently selected index.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Returns the typed ID of the currently selected item.
    pub fn selected_item(&self) -> Option<TypedId<W>> {
        self.selected.and_then(|idx| self.item(idx))
    }

    /// Append an item widget to the end of the list.
    pub fn append(&mut self, ctx: &mut dyn Context, widget: W) -> Result<TypedId<W>>
    where
        W: 'static,
    {
        let key = self.next_key();
        let mut desired = self.items.keys().to_vec();
        desired.push(key);

        let ordered =
            self.reconcile_with_widget(ctx, desired, key, widget, RemovePolicy::RemoveSubtree)?;
        let id = ordered
            .last()
            .copied()
            .ok_or_else(|| Error::Internal("list append did not return the new item".into()))?;

        // Auto-select and focus if this is the first item
        if self.selected.is_none() {
            self.update_selection(ctx, Some(self.items.len() - 1));
            ctx.set_focus(id.into());
        }

        Ok(id)
    }

    /// Insert an item widget at the specified index.
    pub fn insert(&mut self, ctx: &mut dyn Context, index: usize, widget: W) -> Result<TypedId<W>>
    where
        W: 'static,
    {
        let clamped = index.min(self.items.len());
        let key = self.next_key();
        let was_empty = self.selected.is_none();
        let mut desired = self.items.keys().to_vec();
        desired.insert(clamped, key);
        let ordered =
            self.reconcile_with_widget(ctx, desired, key, widget, RemovePolicy::RemoveSubtree)?;
        let id = ordered
            .get(clamped)
            .copied()
            .ok_or_else(|| Error::Internal("list insert did not return the new item".into()))?;

        // Adjust selection if inserting before current selection
        if let Some(sel) = self.selected {
            if clamped <= sel {
                // Just update index, don't change which item is selected
                self.selected = Some(sel + 1);
            }
        } else if !self.items.is_empty() {
            self.update_selection(ctx, Some(0));
        }

        // Focus first item if this was an empty list
        if was_empty && let Some(first_id) = self.item(0) {
            ctx.set_focus(first_id.into());
        }

        Ok(id)
    }

    /// Remove the item at the specified index.
    pub fn remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<bool> {
        let mut desired = self.items.keys().to_vec();
        if index >= desired.len() {
            return Ok(false);
        }
        desired.remove(index);
        self.reconcile_order(ctx, desired, RemovePolicy::RemoveSubtree)?;
        self.repair_selection_after_remove(ctx, index);
        Ok(true)
    }

    /// Detach the item at the specified index.
    pub fn take(&mut self, ctx: &mut dyn Context, index: usize) -> Result<Option<TypedId<W>>> {
        let mut desired = self.items.keys().to_vec();
        if index >= desired.len() {
            return Ok(None);
        }
        let removed = self
            .items
            .id_at(index)
            .map(TypedId::new)
            .ok_or_else(|| Error::Internal("list take missing node id".into()))?;
        desired.remove(index);
        self.reconcile_order(ctx, desired, RemovePolicy::Detach)?;
        self.repair_selection_after_remove(ctx, index);
        Ok(Some(removed))
    }

    /// Clear all items from the list.
    #[command(ignore_result)]
    pub fn clear(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.reconcile_order(ctx, Vec::new(), RemovePolicy::RemoveSubtree)?;
        self.selected = None;
        Ok(())
    }

    /// Delete the currently selected item.
    #[command(ignore_result)]
    pub fn delete_selected(&mut self, ctx: &mut dyn Context) -> Result<bool> {
        match self.selected {
            Some(sel) => self.remove(ctx, sel),
            None => Ok(false),
        }
    }

    /// Select an item at the given index.
    pub fn select(&mut self, ctx: &mut dyn Context, index: usize) {
        if self.items.is_empty() {
            return;
        }
        self.update_selection(ctx, Some(index.min(self.items.len() - 1)));
    }

    /// Update selection to a new index, managing item selection states.
    fn update_selection(&mut self, ctx: &mut dyn Context, new_selected: Option<usize>) {
        // Clear old selection
        if let Some(old_idx) = self.selected
            && let Some(old_id) = self.item(old_idx)
        {
            ctx.with_widget(old_id, |w: &mut W, _| {
                w.set_selected(false);
                Ok(())
            })
            .ok();
        }

        // Set new selection
        if let Some(new_idx) = new_selected
            && let Some(new_id) = self.item(new_idx)
        {
            ctx.with_widget(new_id, |w: &mut W, _| {
                w.set_selected(true);
                Ok(())
            })
            .ok();
        }

        self.selected = new_selected;
    }

    /// Repair selection and focus after removing an item.
    fn repair_selection_after_remove(&mut self, ctx: &mut dyn Context, index: usize) {
        if let Some(sel) = self.selected {
            if index < sel {
                self.selected = Some(sel - 1);
                return;
            }
            if index == sel {
                let new_sel = if self.items.is_empty() {
                    None
                } else {
                    Some(sel.min(self.items.len() - 1))
                };
                self.update_selection(ctx, new_sel);
                if new_sel.is_some() {
                    self.focus_selected(ctx);
                }
            }
        }
    }

    /// Move selection to the first item.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) {
        if self.items.is_empty() {
            return;
        }
        self.update_selection(c, Some(0));
        self.focus_selected(c);
        self.ensure_selected_visible(c);
    }

    /// Move selection to the last item.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) {
        if self.items.is_empty() {
            return;
        }
        self.update_selection(c, Some(self.items.len() - 1));
        self.focus_selected(c);
        self.ensure_selected_visible(c);
    }

    /// Move selection by a signed offset.
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let current = self.selected.unwrap_or(0);
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current.saturating_add(delta as usize)
        };
        self.update_selection(c, Some(next.min(self.items.len() - 1)));
        self.focus_selected(c);
        self.ensure_selected_visible(c);
    }

    /// Move selection to the next item.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) {
        self.select_by(c, 1);
    }

    /// Move selection to the previous item.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) {
        self.select_by(c, -1);
    }

    /// Handle a mouse click within the list.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> bool {
        match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                let Some(index) = self.index_at_location(c, event.location) else {
                    return false;
                };
                self.select(c, index);
                self.focus_selected(c);
                self.ensure_selected_visible(c);
                if self.on_activate.is_some() {
                    self.pending_activate = Some(PendingActivate {
                        index,
                        origin: event.location,
                        dragged: false,
                    });
                    c.capture_mouse();
                }
                true
            }
            mouse::Action::Drag if event.button == mouse::Button::Left => {
                if let Some(pending) = self.pending_activate.as_mut()
                    && let Some(config) = self.on_activate.as_ref()
                {
                    if drag_exceeded(pending.origin, event.location, config.drag_threshold) {
                        pending.dragged = true;
                    }
                    return true;
                }
                false
            }
            mouse::Action::Up if event.button == mouse::Button::Left => {
                let pending = self.pending_activate.take();
                if let Some(pending) = pending {
                    c.release_mouse();
                    if !pending.dragged {
                        let index = self.index_at_location(c, event.location);
                        if index == Some(pending.index) {
                            self.dispatch_activate(c, pending.index);
                        }
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Set focus on the currently selected item.
    fn focus_selected(&self, c: &mut dyn Context) {
        if let Some(id) = self.selected_item() {
            c.set_focus(id.into());
        }
    }

    /// Dispatch the activation command for a selected row.
    fn dispatch_activate(&self, c: &mut dyn Context, index: usize) -> bool {
        let Some(config) = self.on_activate.as_ref() else {
            return false;
        };
        let frame = CommandScopeFrame {
            event: c.current_event().cloned(),
            mouse: c.current_mouse_event(),
            list_row: Some(ListRowContext {
                list: c.node_id(),
                index,
            }),
        };
        let invocation = config.invocation_with_index(index);
        c.dispatch_command_scoped(frame, &invocation).is_ok()
    }

    /// Scroll the view by one line in the specified direction.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: ScrollDirection) {
        match dir {
            ScrollDirection::Up => {
                c.scroll_up();
            }
            ScrollDirection::Down => {
                c.scroll_down();
            }
            ScrollDirection::Left => {
                c.scroll_left();
            }
            ScrollDirection::Right => {
                c.scroll_right();
            }
        }
    }

    /// Move selection by one page in the specified direction.
    pub fn page(&mut self, c: &mut dyn Context, dir: VerticalDirection) {
        self.page_shift(c, matches!(dir, VerticalDirection::Down));
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Up);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Down);
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Left);
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Right);
    }

    #[command]
    /// Page up by one screen.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Up);
    }

    #[command]
    /// Page down by one screen.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Down);
    }

    /// Ensure the selected item is visible in the view.
    fn ensure_selected_visible(&self, c: &mut dyn Context) {
        let Some(selected_idx) = self.selected else {
            return;
        };

        let view = c.view();
        let view_rect = view.view_rect();

        // Compute item positions by measuring each child
        let metrics = self.item_metrics(c, view_rect.w.max(1));
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
    fn page_shift(&mut self, c: &mut dyn Context, forward: bool) {
        if self.items.is_empty() {
            return;
        }

        let view = c.view();
        let view_rect = view.view_rect();
        if view_rect.h == 0 {
            return;
        }

        let metrics = self.item_metrics(c, view_rect.w.max(1));
        let selected_idx = self.selected.unwrap_or(0).min(self.items.len() - 1);
        let Some((start, _height)) = metrics.get(selected_idx).copied() else {
            return;
        };

        let page = view_rect.h.max(1);
        let target_y = if forward {
            start.saturating_add(page)
        } else {
            start.saturating_sub(page)
        };

        if let Some(target_idx) = Self::index_at_y(&metrics, target_y) {
            self.select(c, target_idx);
            self.focus_selected(c);
            self.ensure_selected_visible(c);
        }
    }

    /// Find the item index at a local content-space location.
    fn index_at_location(&self, c: &dyn Context, location: Point) -> Option<usize> {
        let view = c.view();
        let view_rect = view.view_rect();
        let content_y = view_rect.tl.y.saturating_add(location.y);
        let metrics = self.item_metrics(c, view_rect.w.max(1));
        Self::index_at_y(&metrics, content_y)
    }

    /// Build (start_y, height) tuples for each item.
    fn item_metrics(&self, c: &dyn ReadContext, available_width: u32) -> Vec<(u32, u32)> {
        let mut metrics = Vec::with_capacity(self.items.len());
        let mut y_offset = 0u32;

        for id in self.items.iter_ids() {
            // Get the child's layout and compute its height
            let height = c.node_view(id).map(|v| v.outer.h).unwrap_or(1);

            metrics.push((y_offset, height));
            y_offset = y_offset.saturating_add(height);
        }

        // If no layout data yet, estimate with 1-height items
        if metrics.is_empty() && !self.items.is_empty() {
            for i in 0..self.items.len() {
                metrics.push((i as u32, 1));
            }
        }

        let _ = available_width; // Future: could use for responsive layouts
        metrics
    }

    /// Find the item index covering a y coordinate.
    fn index_at_y(metrics: &[(u32, u32)], y: u32) -> Option<usize> {
        for (idx, (start, height)) in metrics.iter().enumerate() {
            if y < start.saturating_add(*height) {
                return Some(idx);
            }
        }
        if metrics.is_empty() {
            None
        } else {
            Some(metrics.len() - 1)
        }
    }
}

impl<W: Selectable + Send + 'static> Widget for List<W> {
    fn layout(&self) -> Layout {
        let mut layout = Layout::fill().overflow_x();
        if let Some(indicator) = &self.selection_indicator
            && indicator.width > 0
        {
            layout = layout.padding(Edges::new(0, 0, 0, indicator.width));
        }
        layout
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event
            && self.handle_click(ctx, *mouse_event)
        {
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let area = view.outer_rect_local();

        // Fill background using list style.
        rndr.fill("list", area, ' ')?;

        if let Some(indicator) = &self.selection_indicator
            && let Some(selected_idx) = self.selected
            && indicator.width > 0
        {
            let metrics = self.item_metrics(ctx, view.view_rect().w.max(1));
            if let Some((start, height)) = metrics.get(selected_idx).copied() {
                let view_rect = view.view_rect();
                let visible_start = start.max(view_rect.tl.y);
                let visible_end = start
                    .saturating_add(height)
                    .min(view_rect.tl.y.saturating_add(view_rect.h));

                if visible_start < visible_end {
                    let content_origin = view.content_origin();
                    let local_y = content_origin
                        .y
                        .saturating_add(visible_start - view_rect.tl.y);
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

    fn canvas(&self, view: Size<u32>, ctx: &CanvasContext<'_>) -> Size<u32> {
        // Sum child canvas heights and find max canvas width for scrolling
        let mut total_height = 0u32;
        let mut max_width = view.width;

        for child in ctx.children() {
            // Use canvas dimensions for proper scroll support
            total_height = total_height.saturating_add(child.canvas.height);
            max_width = max_width.max(child.canvas.width);
        }

        Size::new(max_width, total_height.max(1))
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        // List itself doesn't accept focus; items do
        false
    }

    fn name(&self) -> NodeName {
        NodeName::convert("list")
    }
}

impl<W: Selectable> List<W> {
    /// Allocate the next list key.
    fn next_key(&mut self) -> ListKey {
        let key = ListKey(self.next_key);
        self.next_key = self.next_key.saturating_add(1);
        key
    }

    /// Reconcile the list order while creating a single new widget.
    fn reconcile_with_widget(
        &mut self,
        ctx: &mut dyn Context,
        desired: Vec<ListKey>,
        key: ListKey,
        widget: W,
        remove: RemovePolicy,
    ) -> Result<Vec<TypedId<W>>>
    where
        W: 'static,
    {
        if self.items.id_for(&key).is_some() {
            return Err(Error::Internal("list key collision".into()));
        }
        let mut widget = Some(widget);
        self.items.reconcile(
            ctx,
            desired,
            |requested| {
                if *requested != key {
                    panic!("list reconcile requested an unexpected key");
                }
                widget.take().expect("list widget already consumed")
            },
            |_, _, _| Ok(()),
            remove,
        )
    }

    /// Reconcile the list order without creating new widgets.
    fn reconcile_order(
        &mut self,
        ctx: &mut dyn Context,
        desired: Vec<ListKey>,
        remove: RemovePolicy,
    ) -> Result<Vec<TypedId<W>>> {
        self.items.reconcile(
            ctx,
            desired,
            |_| {
                panic!("list reconcile requested a missing widget");
            },
            |_, _, _| Ok(()),
            remove,
        )
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
    use canopy::{Canopy, Loader, testing::harness::Harness};

    use super::*;
    use crate::Text;

    impl Loader for List<Text> {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            Ok(())
        }
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

        // Navigate down
        harness.script("list::select_next()")?;
        harness.with_root_widget::<List<Text>, _>(|list| {
            assert_eq!(list.selected_index(), Some(1));
        });

        // Navigate to last
        harness.script("list::select_last()")?;
        harness.with_root_widget::<List<Text>, _>(|list| {
            assert_eq!(list.selected_index(), Some(2));
        });

        // Navigate up
        harness.script("list::select_prev()")?;
        harness.with_root_widget::<List<Text>, _>(|list| {
            assert_eq!(list.selected_index(), Some(1));
        });

        // Navigate to first
        harness.script("list::select_first()")?;
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
            list.select(ctx, 1); // Select middle item
            Ok(())
        })?;

        // Remove selected
        harness.with_root_context(|list: &mut List<Text>, ctx| {
            list.remove(ctx, 1)?;
            Ok(())
        })?;

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
        harness.script("list::clear()")?;

        harness.with_root_widget::<List<Text>, _>(|list| {
            assert!(list.is_empty());
            assert_eq!(list.selected_index(), None);
        });

        Ok(())
    }
}
