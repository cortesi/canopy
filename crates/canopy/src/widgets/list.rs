//! Widget-based list container.
//!
//! A typed list container where items are actual widgets in the tree.
//! Items participate in focus management and can be composed from other widgets.

use std::marker::PhantomData;

use crate::{
    Context, NodeId, TypedId, ViewContext, command, derive_commands,
    error::Result,
    layout::{CanvasContext, Constraint, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::Widget,
};

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
    /// Item widget node IDs in order.
    items: Vec<TypedId<W>>,
    /// Currently selected item index.
    selected: Option<usize>,
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
            items: Vec::new(),
            selected: None,
            _marker: PhantomData,
        }
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
        self.items.get(index).copied()
    }

    /// Returns the currently selected index.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Returns the typed ID of the currently selected item.
    pub fn selected_item(&self) -> Option<TypedId<W>> {
        self.selected.and_then(|idx| self.items.get(idx).copied())
    }

    /// Append an item widget to the end of the list.
    pub fn append(&mut self, ctx: &mut dyn Context, widget: W) -> Result<TypedId<W>>
    where
        W: 'static,
    {
        let id = ctx.add_orphan_typed(widget);
        ctx.mount_child(id.into())?;
        self.items.push(id);

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
        let was_empty = self.selected.is_none();
        let id = ctx.add_orphan_typed(widget);
        ctx.mount_child(id.into())?;
        self.items.insert(clamped, id);

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
        if was_empty && let Some(first_id) = self.items.first() {
            ctx.set_focus((*first_id).into());
        }

        // Sync children order with the arena
        self.sync_children(ctx)?;

        Ok(id)
    }

    /// Remove the item at the specified index.
    pub fn remove(&mut self, ctx: &mut dyn Context, index: usize) -> Result<Option<TypedId<W>>> {
        if index >= self.items.len() {
            return Ok(None);
        }

        let id = self.items.remove(index);
        ctx.detach_child(id.into())?;

        // Adjust selection
        if let Some(sel) = self.selected {
            if index < sel {
                self.selected = Some(sel - 1);
            } else if index == sel {
                // The selected item was removed, select the next valid item
                let new_sel = if self.items.is_empty() {
                    None
                } else {
                    Some(sel.min(self.items.len() - 1))
                };
                self.update_selection(ctx, new_sel);
            }
        }

        Ok(Some(id))
    }

    /// Clear all items from the list.
    #[command(ignore_result)]
    pub fn clear(&mut self, ctx: &mut dyn Context) -> Result<Vec<TypedId<W>>> {
        let ids: Vec<_> = self.items.drain(..).collect();
        for id in &ids {
            ctx.detach_child((*id).into())?;
        }
        self.selected = None;
        Ok(ids)
    }

    /// Delete the currently selected item.
    #[command(ignore_result)]
    pub fn delete_selected(&mut self, ctx: &mut dyn Context) -> Result<Option<TypedId<W>>> {
        match self.selected {
            Some(sel) => self.remove(ctx, sel),
            None => Ok(None),
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
            && let Some(old_id) = self.items.get(old_idx).copied()
        {
            ctx.with_widget(old_id.into(), |w: &mut W, _| {
                w.set_selected(false);
                Ok(())
            })
            .ok();
        }

        // Set new selection
        if let Some(new_idx) = new_selected
            && let Some(new_id) = self.items.get(new_idx).copied()
        {
            ctx.with_widget(new_id.into(), |w: &mut W, _| {
                w.set_selected(true);
                Ok(())
            })
            .ok();
        }

        self.selected = new_selected;
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

    /// Move selection to the next item.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) {
        if let Some(sel) = self.selected
            && sel + 1 < self.items.len()
        {
            self.update_selection(c, Some(sel + 1));
            self.focus_selected(c);
            self.ensure_selected_visible(c);
        }
    }

    /// Move selection to the previous item.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) {
        if let Some(sel) = self.selected
            && sel > 0
        {
            self.update_selection(c, Some(sel - 1));
            self.focus_selected(c);
            self.ensure_selected_visible(c);
        }
    }

    /// Set focus on the currently selected item.
    fn focus_selected(&self, c: &mut dyn Context) {
        if let Some(id) = self.selected_item() {
            c.set_focus(id.into());
        }
    }

    /// Scroll the view down by one line.
    #[command]
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down();
    }

    /// Scroll the view up by one line.
    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up();
    }

    /// Scroll the view left by one line.
    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left();
    }

    /// Scroll the view right by one line.
    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right();
    }

    /// Scroll the view down by one page.
    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.page_shift(c, true);
    }

    /// Scroll the view up by one page.
    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.page_shift(c, false);
    }

    /// Sync children order with the items vec.
    fn sync_children(&self, ctx: &mut dyn Context) -> Result<()> {
        let children: Vec<NodeId> = self.items.iter().map(|id| (*id).into()).collect();
        ctx.set_children(children)
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

    /// Build (start_y, height) tuples for each item.
    fn item_metrics(&self, c: &dyn ViewContext, available_width: u32) -> Vec<(u32, u32)> {
        let mut metrics = Vec::with_capacity(self.items.len());
        let mut y_offset = 0u32;

        for id in &self.items {
            // Get the child's layout and compute its height
            let height = c.node_view((*id).into()).map(|v| v.outer.h).unwrap_or(1);

            metrics.push((y_offset, height));
            y_offset = y_offset.saturating_add(height);
        }

        // If no layout data yet, estimate with 1-height items
        if metrics.is_empty() && !self.items.is_empty() {
            for (i, _) in self.items.iter().enumerate() {
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
        Layout::column()
            .flex_vertical(1)
            .flex_horizontal(1)
            .overflow_x()
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let area = view.view_rect_local();

        // Fill background using list style.
        rndr.fill("list", area, ' ')?;

        // Note: Selection indicator is rendered by item widgets themselves.
        // Items implement Selectable to update their selection state.
        // Selection state is managed by the List and persists when focus moves away.

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

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        // List itself doesn't accept focus; items do
        false
    }

    fn name(&self) -> NodeName {
        NodeName::convert("list")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canopy, Loader, testing::harness::Harness, widgets::Text};

    impl Loader for List<Text> {
        fn load(c: &mut Canopy) {
            c.add_commands::<Self>();
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
        List::<Text>::load(&mut harness.canopy);

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
        List::<Text>::load(&mut harness.canopy);

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
