use crate::{
    Context, ViewContext, command, derive_commands,
    error::Result,
    geom::{Expanse, Rect},
    layout::{AvailableSpace, Size},
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
    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool) -> Result<()>;
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

    /// Scroll the viewport down by one line.
    #[command]
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down();
    }

    /// Scroll the viewport up by one line.
    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up();
    }

    /// Scroll the viewport left by one line.
    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left();
    }

    /// Scroll the viewport right by one line.
    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right();
    }

    /// Scroll the viewport down by one page.
    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down();
    }

    /// Scroll the viewport up by one page.
    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up();
    }

    /// Ensure the selected item is visible in the viewport.
    fn ensure_selected_visible(&self, c: &mut dyn Context) {
        let selected_idx = match self.selected {
            Some(idx) => idx,
            None => return,
        };

        let view = c.view();
        let mut y_offset = 0u32;

        for (idx, item) in self.items.iter().enumerate() {
            let size = item.measure(view.w);
            if idx == selected_idx {
                if y_offset < view.tl.y {
                    let delta = view.tl.y - y_offset;
                    let _ = c.scroll_by(0, -(delta as i32));
                } else if y_offset + size.h > view.tl.y + view.h {
                    let delta = (y_offset + size.h) - (view.tl.y + view.h);
                    let _ = c.scroll_by(0, delta as i32);
                }
                break;
            }
            y_offset = y_offset.saturating_add(size.h);
        }
    }
}

impl<N> Widget for List<N>
where
    N: ListItem + Send + 'static,
{
    fn render(&mut self, rndr: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        rndr.fill("", ctx.canvas().rect(), ' ')?;

        let mut y_offset = 0u32;
        for (idx, item) in self.items.iter_mut().enumerate() {
            let size = item.measure(view.w);
            let item_rect = Rect::new(0, y_offset, size.w, size.h);
            let selected = self.selected == Some(idx);
            if item_rect.intersect(&view).is_some() {
                item.render(rndr, item_rect, selected)?;
            }
            y_offset = y_offset.saturating_add(size.h);
        }
        Ok(())
    }

    fn canvas_size(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = known_dimensions
            .width
            .or_else(|| available_space.width.into_option())
            .unwrap_or(0.0);
        let available_width = width.max(1.0) as u32;

        let mut height = 0u32;
        let mut max_width = 0u32;
        for item in &self.items {
            let size = item.measure(available_width);
            height = height.saturating_add(size.h);
            max_width = max_width.max(size.w);
        }

        Size {
            width: max_width.max(available_width) as f32,
            height: height as f32,
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("list")
    }
}
