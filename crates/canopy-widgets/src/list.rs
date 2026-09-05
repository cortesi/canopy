//! Widget-based list container.
//!
//! A typed list container where items are actual widgets in the tree.
//! Items participate in focus management and can be composed from other
//! widgets.

use canopy::{
    Context, EventOutcome, KeyedChildren, NodeId, TypedId, ViewContext, Widget, command,
    commands::{
        CommandAction, CommandArgs, CommandCall, CommandInvocation, CommandScopeFrame,
        CommandTarget, ListRowContext, ToArgValue,
    },
    derive_commands,
    error::{Error, Result},
    event::{Event, mouse},
    geom::{Direction, Line, Point, PointI32},
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
/// List items are actual widgets in the tree, enabling composition and focus
/// management. The list arranges items vertically and supports scrolling.
///
/// Items must implement the [`Selectable`] trait so the list can manage their
/// selection state independently of focus.
pub struct List<W: Selectable> {
    /// Keyed list items in order.
    items: KeyedChildren<ListKey, W>,
    /// Next monotonic key to assign.
    next_key: u64,
    /// Currently selected item index.
    selected: Option<usize>,
    /// Optional list-level selection indicator.
    selection_indicator: Option<SelectionIndicator>,
    /// Optional activation command configuration.
    on_activate: Option<CommandAction>,
    /// Pending activation state while handling clicks.
    pending_activate: Option<PendingActivate>,
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
        self.insert(ctx, self.items.len(), widget)
    }

    /// Insert an item widget at the specified index.
    pub fn insert(&mut self, ctx: &mut dyn Context, index: usize, widget: W) -> Result<TypedId<W>>
    where
        W: 'static,
    {
        let clamped = index.min(self.items.len());
        let key = self.next_key();
        let was_empty = self.selected.is_none();
        let previous_focus = ctx.focused_leaf(ctx.root_id());
        let mut desired = self.items.keys().to_vec();
        desired.insert(clamped, key);
        let ordered = self.reconcile_with_widget(ctx, desired, key, widget)?;
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
            self.update_selection(ctx, Some(0))?;
        }

        // Focus first item if this was an empty list
        if was_empty
            && let Some(first_id) = self.item(0)
            && ctx.node_is_attached(first_id.into())
        {
            ctx.set_focus(first_id.into())?;
        } else if let Some(previous_focus) = previous_focus {
            ctx.set_focus(previous_focus)?;
        } else {
            self.focus_selected(ctx)?;
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
        self.reconcile_order(ctx, desired)?;
        self.repair_selection_after_remove(ctx, index)?;
        Ok(true)
    }

    /// Clear all items from the list.
    #[command(ignore_result)]
    pub fn clear(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.reconcile_order(ctx, Vec::new())?;
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

        let old_id = self.selection_id(self.selected, "list selection points at a missing item")?;
        let new_id =
            self.selection_id(new_selected, "new list selection points at a missing item")?;
        let same_selection = old_id.as_ref().map(|id| NodeId::from(*id))
            == new_id.as_ref().map(|id| NodeId::from(*id));
        if same_selection {
            self.selected = new_selected;
            return Ok(());
        }

        if let Some(old_id) = old_id {
            ctx.with_widget(old_id, |w: &mut W, _| {
                w.set_selected(false);
                Ok(())
            })?;
        }

        if let Some(new_id) = new_id {
            ctx.with_widget(new_id, |w: &mut W, _| {
                w.set_selected(true);
                Ok(())
            })?;
        }

        self.selected = new_selected;
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
        self.selected.is_none_or(|index| index < self.items.len())
    }

    /// Repair selection and focus after removing an item.
    fn repair_selection_after_remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {
        if let Some(sel) = self.selected {
            if index < sel {
                self.selected = Some(sel - 1);
                debug_assert!(self.selection_invariant_holds());
                return Ok(());
            }
            if index == sel {
                let new_sel = if self.items.is_empty() {
                    None
                } else {
                    Some(sel.min(self.items.len() - 1))
                };
                self.selected = None;
                self.update_selection(ctx, new_sel)?;
                if new_sel.is_some() {
                    self.focus_selected(ctx)?;
                }
            }
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
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
            .selected
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
                        index,
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
                        if index == Some(pending.index) {
                            self.dispatch_activate(c, pending.index)?;
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
            && c.node_is_attached(id.into())
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
            }),
        };
        let invocation = invocation_with_index(&config.invocation, index);
        c.dispatch_target_scoped(
            config.target.unwrap_or(CommandTarget::From(c.node_id())),
            frame,
            &invocation,
        )?;
        Ok(())
    }

    /// Scroll the view by one line in the specified direction.
    /// @param dir The direction to scroll.
    #[command]
    pub fn scroll(&mut self, c: &mut dyn Context, dir: Direction) {
        match dir {
            Direction::Up => {
                c.scroll_up();
            }
            Direction::Down => {
                c.scroll_down();
            }
            Direction::Left => {
                c.scroll_left();
            }
            Direction::Right => {
                c.scroll_right();
            }
        }
    }

    /// Move selection by pages.
    /// Positive values move down; negative values move up.
    /// @param delta Signed page delta. Positive moves down and negative moves
    /// up.
    #[command]
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.page_shift(c, delta >= 0)
    }

    /// Ensure the selected item is visible in the view.
    fn ensure_selected_visible(&self, c: &mut dyn Context) {
        let Some(selected_idx) = self.selected else {
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
        let selected_idx = self.selected.unwrap_or(0).min(self.items.len() - 1);
        let Some((start, _height)) = metrics.get(selected_idx).copied() else {
            return Ok(());
        };

        let page = view_rect.h.max(1);
        let target_y = if forward {
            start.saturating_add(page)
        } else {
            start.saturating_sub(page)
        };

        if let Some(target_idx) = Self::index_at_y(&metrics, target_y) {
            self.select_and_reveal(c, target_idx)?;
        }
        Ok(())
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
            let height = c.node_view(id.into()).map(|v| v.outer.h).unwrap_or(1);
            metrics.push((y_offset, height));
            y_offset = y_offset.saturating_add(height);
        }

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
                    return Err(Error::Internal(
                        "list reconcile requested an unexpected key".into(),
                    ));
                }
                widget
                    .take()
                    .ok_or_else(|| Error::Internal("list widget already consumed".into()))
            },
            |_, _, _| Ok(()),
        )
    }

    /// Reconcile the list order without creating new widgets.
    fn reconcile_order(
        &mut self,
        ctx: &mut dyn Context,
        desired: Vec<ListKey>,
    ) -> Result<Vec<TypedId<W>>> {
        self.items.reconcile(
            ctx,
            desired,
            |requested| {
                Err(Error::Internal(format!(
                    "list reconcile requested missing widget for key {requested:?}",
                )))
            },
            |_, _, _| Ok(()),
        )
    }
}

impl<W: Selectable + 'static> Widget for List<W> {
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
            && let Some(selected_idx) = self.selected
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
        Canopy, Loader, NodeId, ViewContext, derive_commands, event::key, state::NodeName,
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
            ctx.with_widget::<List<Text>, _>(list, |list, ctx| {
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
