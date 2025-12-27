use crate::{
    Context, ViewContext, command, derive_commands,
    error::Result,
    geom::{Expanse, Point, Rect},
    layout::{Constraint, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// ListItem must be implemented by items displayed in a `List`.
pub trait ListItem {
    /// Update selection state for the item.
    fn set_selected(&mut self, _state: bool) {}

    /// Measure the item given an available width.
    fn measure(&self, available_width: u32) -> Expanse;

    /// Render the item into the list's render buffer.
    fn render(
        &mut self,
        rndr: &mut Render,
        area: Rect,
        selected: bool,
        offset: Point,
        full_size: Expanse,
    ) -> Result<()>;
}

/// Manage and display a list of items.
pub struct List<N>
where
    N: ListItem,
{
    /// Stored list items.
    items: Vec<N>,

    /// The offset of the currently selected item in the list.
    selected: Option<usize>,
}

#[derive_commands]
impl<N> List<N>
where
    N: ListItem,
{
    /// Construct a list from the provided items.
    pub fn new(items: Vec<N>) -> Self {
        let mut l = Self {
            items,
            selected: None,
        };
        if !l.is_empty() {
            l.select(0);
        }
        l
    }

    /// The number of items in the list.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The number of items in the list.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Insert an item at the given index.
    pub fn insert(&mut self, index: usize, mut itm: N) {
        let clamped_index = index.min(self.len());

        if let Some(sel) = self.selected
            && clamped_index <= sel
        {
            self.selected = Some(sel + 1);
        }

        itm.set_selected(false);
        self.items.insert(clamped_index, itm);

        if self.selected.is_none() && !self.is_empty() {
            self.select(0);
        }
    }

    /// Insert an item after the current selection.
    pub fn insert_after(&mut self, itm: N) {
        if let Some(sel) = self.selected {
            self.insert(sel + 1, itm);
        } else {
            self.insert(0, itm);
        }
    }

    /// Append an item to the end of the list.
    pub fn append(&mut self, itm: N) {
        self.insert(self.len(), itm);
    }

    /// Apply a closure to every item in the list.
    pub fn for_each_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut N),
    {
        for item in &mut self.items {
            f(item);
        }
    }

    /// The current selected item, if any.
    pub fn selected(&self) -> Option<&N> {
        self.selected.and_then(|idx| self.items.get(idx))
    }

    /// The current selected index, if any.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Select an item at a specified offset.
    pub fn select(&mut self, offset: usize) -> bool {
        if self.is_empty() {
            return false;
        }

        let new_index = offset.min(self.len() - 1);

        if let Some(current) = self.selected
            && let Some(item) = self.items.get_mut(current)
        {
            item.set_selected(false);
        }

        if let Some(item) = self.items.get_mut(new_index) {
            item.set_selected(true);
        }
        self.selected = Some(new_index);

        true
    }

    /// Delete an item at the specified offset.
    pub fn delete_item(&mut self, _core: &mut dyn Context, offset: usize) -> Option<N> {
        if offset >= self.len() {
            return None;
        }

        let removed = self.items.remove(offset);

        if let Some(sel) = self.selected {
            if offset < sel {
                self.selected = Some(sel - 1);
            } else if offset == sel {
                if self.is_empty() {
                    self.selected = None;
                } else {
                    let new_sel = offset.min(self.len() - 1);
                    self.selected = Some(new_sel);
                    if let Some(item) = self.items.get_mut(new_sel) {
                        item.set_selected(true);
                    }
                }
            }
        }

        Some(removed)
    }

    /// Clear all items.
    #[command(ignore_result)]
    pub fn clear(&mut self) -> Vec<N> {
        self.selected = None;
        self.items.drain(..).collect()
    }

    /// Delete the currently selected item.
    #[command(ignore_result)]
    pub fn delete_selected(&mut self, core: &mut dyn Context) -> Option<N> {
        if let Some(sel) = self.selected {
            self.delete_item(core, sel)
        } else {
            None
        }
    }

    /// Move selection to the first item.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) {
        if self.is_empty() {
            return;
        }
        self.select(0);
        self.ensure_selected_visible(c);
    }

    /// Move selection to the last item.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) {
        if self.is_empty() {
            return;
        }
        self.select(self.len());
        self.ensure_selected_visible(c);
    }

    /// Move selection to the next item.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) {
        if let Some(sel) = self.selected
            && sel + 1 < self.len()
        {
            self.select(sel + 1);
            self.ensure_selected_visible(c);
        }
    }

    /// Move selection to the previous item.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) {
        if let Some(sel) = self.selected
            && sel > 0
        {
            self.select(sel - 1);
            self.ensure_selected_visible(c);
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

    /// Ensure the selected item is visible in the view.
    fn ensure_selected_visible(&self, c: &mut dyn Context) {
        let selected_idx = match self.selected {
            Some(idx) => idx,
            None => return,
        };

        let view_rect = c.view().view_rect();
        let metrics = self.item_metrics(view_rect.w.max(1));
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
        if self.is_empty() {
            return;
        }

        let view_rect = c.view().view_rect();
        if view_rect.h == 0 {
            return;
        }

        let metrics = self.item_metrics(view_rect.w.max(1));
        let selected_idx = self.selected.unwrap_or(0).min(self.len() - 1);
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
            self.select(target_idx);
            self.ensure_selected_visible(c);
        }
    }

    /// Build (start, height) tuples for each item.
    fn item_metrics(&self, available_width: u32) -> Vec<(u32, u32)> {
        let mut metrics = Vec::with_capacity(self.items.len());
        let mut y_offset = 0u32;
        for item in &self.items {
            let size = item.measure(available_width);
            metrics.push((y_offset, size.h));
            y_offset = y_offset.saturating_add(size.h);
        }
        metrics
    }

    /// Find the item index covering a y coordinate; fallback to last item.
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

impl<N> Widget for List<N>
where
    N: ListItem + Send + 'static,
{
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        let scroll = view.tl;
        rndr.fill("", view.view_rect_local(), ' ')?;

        let mut y_offset = 0u32;
        for (idx, item) in self.items.iter_mut().enumerate() {
            let size = item.measure(view_rect.w.max(1));
            let item_rect = Rect::new(0, y_offset, size.w, size.h);
            let selected = self.selected == Some(idx);
            if let Some(visible) = item_rect.intersect(&view_rect) {
                let offset = Point {
                    x: visible.tl.x.saturating_sub(item_rect.tl.x),
                    y: visible.tl.y.saturating_sub(item_rect.tl.y),
                };
                let local_rect = Rect::new(
                    content_origin
                        .x
                        .saturating_add(visible.tl.x.saturating_sub(scroll.x)),
                    content_origin
                        .y
                        .saturating_add(visible.tl.y.saturating_sub(scroll.y)),
                    visible.w,
                    visible.h,
                );
                item.render(rndr, local_rect, selected, offset, size)?;
            }
            y_offset = y_offset.saturating_add(size.h);
        }
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let available_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => u32::MAX,
        };

        let mut height = 0u32;
        let mut max_width = 0u32;
        for item in &self.items {
            let size = item.measure(available_width);
            height = height.saturating_add(size.h);
            max_width = max_width.max(size.w);
        }

        c.clamp(Size::new(max_width, height))
    }

    fn canvas(&self, view: Size<u32>, _ctx: &crate::layout::CanvasContext) -> Size<u32> {
        let available_width = view.width.max(1);

        let mut height = 0u32;
        let mut max_width = 0u32;
        for item in &self.items {
            let size = item.measure(available_width);
            height = height.saturating_add(size.h);
            max_width = max_width.max(size.w);
        }

        Size::new(max_width, height)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("list")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canopy, Loader, testing::harness::Harness};

    struct TestItem {
        height: u32,
    }

    impl ListItem for TestItem {
        fn measure(&self, _available_width: u32) -> Expanse {
            Expanse::new(10, self.height)
        }

        fn render(
            &mut self,
            _rndr: &mut Render,
            _area: Rect,
            _selected: bool,
            _offset: Point,
            _full_size: Expanse,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl Loader for List<TestItem> {
        fn load(c: &mut Canopy) {
            c.add_commands::<Self>();
        }
    }

    #[test]
    fn test_page_down_moves_selection_and_allows_prev() -> Result<()> {
        let items = (0..20).map(|_| TestItem { height: 1 }).collect();
        let root = List::new(items);
        let mut harness = Harness::builder(root).size(20, 5).build()?;
        harness.render()?;

        let mut initial = None;
        harness.with_root_widget::<List<TestItem>, _>(|list| {
            initial = list.selected_index();
        });

        for _ in 0..3 {
            harness.script("list::page_down()")?;
        }

        let mut after = None;
        harness.with_root_widget::<List<TestItem>, _>(|list| {
            after = list.selected_index();
        });

        let after_idx = after.expect("selection missing");
        let initial_idx = initial.unwrap_or(0);
        assert!(after_idx > initial_idx);

        harness.script("list::select_prev()")?;

        let mut final_sel = None;
        harness.with_root_widget::<List<TestItem>, _>(|list| {
            final_sel = list.selected_index();
        });

        assert_eq!(final_sel, Some(after_idx.saturating_sub(1)));

        Ok(())
    }
}
