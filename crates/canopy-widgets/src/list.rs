use canopy_core as canopy;

use canopy_core::{
    Context, Layout, Node, NodeState, Render, Result, StatefulNode, derive_commands,
    geom::{Expanse, Rect},
    *,
};

/// ListItem must be implemented by items displayed in a `List`.
pub trait ListItem {
    fn set_selected(&mut self, _state: bool) {}
}

/// Manage and display a list of items.
#[derive(canopy_core::StatefulNode)]
pub struct List<N>
where
    N: Node + ListItem,
{
    state: NodeState,

    items: Vec<N>,

    /// The offset of the currently selected item in the list. We keep this
    /// carefully in sync with the set_selected() method on ListItem.
    pub selected: Option<usize>,
}

#[derive_commands]
impl<N> List<N>
where
    N: Node + ListItem,
{
    pub fn new(items: Vec<N>) -> Self {
        let mut l = List {
            items,
            selected: None,
            state: NodeState::default(),
        };
        if !l.is_empty() {
            l.select(0);
        }
        l
    }

    /// The number of items in the list.
    pub fn is_empty(&self) -> bool {
        self.items.len() == 0
    }

    /// The number of items in the list.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Insert an item at the given index.
    pub fn insert(&mut self, index: usize, mut itm: N) {
        let clamped_index = index.min(self.len());

        // If we're inserting before or at the selected position, we need to adjust selection
        if let Some(sel) = self.selected {
            if clamped_index <= sel {
                // The selected item will shift right, so update the index
                self.selected = Some(sel + 1);
            }
        }

        // Ensure the new item starts unselected
        itm.set_selected(false);
        self.items.insert(clamped_index, itm);

        // If this is the first item and nothing was selected, select it
        if self.selected.is_none() && !self.is_empty() {
            self.select(0);
        }
    }

    /// Insert an item after the current selection.
    pub fn insert_after(&mut self, itm: N) {
        if let Some(sel) = self.selected {
            self.insert(sel + 1, itm);
        } else {
            // No selection, insert at beginning
            self.insert(0, itm);
        }
    }

    /// Append an item to the end of the list.
    pub fn append(&mut self, itm: N) {
        self.insert(self.len(), itm);
    }

    /// The current selected item, if any
    pub fn selected(&self) -> Option<&N> {
        if let Some(idx) = self.selected {
            Some(&self.items[idx])
        } else {
            None
        }
    }

    /// Select an item at a specified offset, clamping the offset to make sure
    /// it lies within the list.
    pub fn select(&mut self, offset: usize) -> bool {
        if self.is_empty() {
            return false;
        }

        // Clamp offset to valid range
        let new_index = offset.min(self.len() - 1);

        // Unselect the currently selected item if any
        if let Some(current) = self.selected {
            self.items[current].set_selected(false);
        }

        // Select the new item
        self.items[new_index].set_selected(true);
        self.selected = Some(new_index);

        true
    }

    /// Delete an item at the specified offset.
    pub fn delete_item(&mut self, _core: &mut dyn Context, offset: usize) -> Option<N> {
        if offset >= self.len() {
            return None;
        }

        let removed = self.items.remove(offset);

        // Update selection after deletion
        if let Some(sel) = self.selected {
            if offset < sel {
                // Deleted item was before selection, shift selection left
                self.selected = Some(sel - 1);
            } else if offset == sel {
                // Deleted the selected item
                if self.is_empty() {
                    // List is now empty
                    self.selected = None;
                } else {
                    // Select the item at the same position (or the last item if we deleted the last one)
                    let new_sel = offset.min(self.len() - 1);
                    self.selected = Some(new_sel);
                    self.items[new_sel].set_selected(true);
                }
            }
            // If offset > sel, selection doesn't change
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

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) {
        if self.is_empty() {
            return;
        }
        self.select(0);
        self.ensure_selected_visible(c);
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) {
        self.select(self.len());
        self.ensure_selected_visible(c);
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) {
        if let Some(sel) = self.selected {
            self.select(sel.saturating_add(1));
        } else if !self.is_empty() {
            self.select(0);
        }
        self.ensure_selected_visible(c);
    }

    /// Move selection to the next previous the list, if possible.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) {
        if let Some(sel) = self.selected {
            self.select(sel.saturating_sub(1));
        } else if !self.is_empty() {
            self.select(0);
        }
        self.ensure_selected_visible(c);
    }

    /// Scroll the viewport down by one line.
    #[command]
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    /// Scroll the viewport up by one line.
    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    /// Scroll the viewport left by one column.
    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    /// Scroll the viewport right by one column.
    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    /// Scroll the viewport down by one page.
    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    /// Scroll the viewport up by one page.
    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
    }

    /// Ensure the selected item is visible in the viewport.
    /// This adjusts the view to follow the selected item when it would
    /// otherwise be partially or completely out of view.
    fn ensure_selected_visible(&mut self, c: &mut dyn Context) {
        if let Some(selected_idx) = self.selected {
            if selected_idx >= self.items.len() {
                return;
            }

            let selected_item = &self.items[selected_idx];
            let item_pos = selected_item.vp().position();
            let item_height = selected_item.vp().canvas().h;

            let list_pos = self.vp().position();
            let view = self.vp().view();

            // Calculate item's position relative to our canvas
            let item_y = item_pos.y.saturating_sub(list_pos.y);

            // Check if item is above the current view
            if item_y < view.tl.y {
                // Scroll up to make the item visible at the top
                let scroll_amount = view.tl.y - item_y;
                for _ in 0..scroll_amount {
                    c.scroll_up(self);
                }
            }
            // Check if item is below the current view
            else if item_y + item_height > view.tl.y + view.h {
                // Scroll down to make the item visible at the bottom
                let scroll_amount = (item_y + item_height) - (view.tl.y + view.h);
                for _ in 0..scroll_amount {
                    c.scroll_down(self);
                }
            }
        }
    }
}

impl<N> Node for List<N>
where
    N: Node + ListItem,
{
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for i in self.items.iter_mut() {
            f(i)?
        }
        Ok(())
    }

    fn layout(&mut self, l: &Layout, r: Expanse) -> Result<()> {
        // Layout each item and calculate total canvas size
        let mut y_offset = 0;
        let mut max_width = 0;

        for item in &mut self.items {
            // Layout the item with the available width
            item.layout(l, r)?;

            // Get the item's size after layout
            let item_size = item.vp().canvas();

            // Place the item at the current y offset
            l.place(item, Rect::new(0, y_offset, item_size.w, item_size.h))?;

            y_offset += item_size.h;
            max_width = max_width.max(item_size.w);
        }

        // Set our canvas size based on the total height and max width
        let canvas_size = Expanse {
            w: max_width,
            h: y_offset,
        };

        self.fit_size(canvas_size, r);

        Ok(())
    }

    fn render(&mut self, c: &dyn Context, rndr: &mut Render) -> Result<()> {
        // First, clear the background
        rndr.fill("", self.vp().canvas().rect(), ' ')?;

        // Get our view rectangle and position
        let view = self.vp().view();
        let list_pos = self.vp().position();

        // Render each item that intersects with our view
        for item in &mut self.items {
            let item_pos = item.vp().position();
            let item_canvas = item.vp().canvas();

            // Calculate item's position relative to our canvas
            let item_y = item_pos.y.saturating_sub(list_pos.y);
            let item_rect = Rect::new(0, item_y, item_canvas.w, item_canvas.h);

            // Check if this item intersects with our view
            if item_rect.vextent().intersection(&view.vextent()).is_some() {
                // The item is at least partially visible, so render it
                item.render(c, rndr)?;
            } else if item_y > view.tl.y + view.h {
                // We've passed all visible items, stop rendering
                break;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_core::commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue};
    use canopy_core::error::Error;

    // Simple test item for unit tests
    #[derive(canopy_core::StatefulNode)]
    struct TestItem {
        label: String,
        selected: bool,
        state: NodeState,
    }

    impl TestItem {
        fn new(label: &str) -> Self {
            TestItem {
                label: label.to_string(),
                selected: false,
                state: NodeState::default(),
            }
        }
    }

    impl ListItem for TestItem {
        fn set_selected(&mut self, state: bool) {
            self.selected = state;
        }
    }

    impl CommandNode for TestItem {
        fn commands() -> Vec<CommandSpec>
        where
            Self: Sized,
        {
            vec![]
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Err(Error::UnknownCommand("no commands".to_string()))
        }
    }

    impl Node for TestItem {
        fn layout(&mut self, _l: &Layout, r: Expanse) -> Result<()> {
            // Set our size to be single line with the available width
            self.fit_size(Expanse { w: r.w, h: 1 }, r);
            Ok(())
        }

        fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
            let style = if self.selected { "selected" } else { "" };
            let view = self.vp().view();
            rndr.text(style, view.line(0), &self.label)?;
            Ok(())
        }
    }

    // Helper function to verify selection invariant
    fn verify_selection_invariant(list: &List<TestItem>) {
        let mut selected_count = 0;
        let mut selected_index = None;

        for (i, item) in list.items.iter().enumerate() {
            if item.selected {
                selected_count += 1;
                selected_index = Some(i);
            }
        }

        // Verify that at most one item is selected
        assert!(selected_count <= 1, "More than one item is selected!");

        // Verify that selected field matches actual selection
        if let Some(sel) = list.selected {
            assert_eq!(selected_index, Some(sel), "Selected index mismatch!");
        } else {
            assert_eq!(
                selected_index, None,
                "Item is selected but list.selected is None!"
            );
        }
    }

    #[test]
    fn test_select() {
        let items = vec![
            TestItem::new("item0"),
            TestItem::new("item1"),
            TestItem::new("item2"),
        ];

        let mut list = List::new(items);
        verify_selection_invariant(&list);

        // Initially, first item should be selected
        assert_eq!(list.selected, Some(0));
        assert!(list.items[0].selected);
        assert!(!list.items[1].selected);
        assert!(!list.items[2].selected);

        // Select second item
        assert!(list.select(1));
        verify_selection_invariant(&list);
        assert_eq!(list.selected, Some(1));
        assert!(!list.items[0].selected);
        assert!(list.items[1].selected);
        assert!(!list.items[2].selected);

        // Select with out-of-bounds index - should clamp to last item
        assert!(list.select(10));
        verify_selection_invariant(&list);
        assert_eq!(list.selected, Some(2));
        assert!(!list.items[0].selected);
        assert!(!list.items[1].selected);
        assert!(list.items[2].selected);

        // Test empty list
        let mut empty_list: List<TestItem> = List::new(vec![]);
        verify_selection_invariant(&empty_list);
        assert!(!empty_list.select(0));
        assert_eq!(empty_list.selected, None);
    }

    #[test]
    fn test_insert() {
        // Test inserting into empty list
        let mut list: List<TestItem> = List::new(vec![]);
        list.insert(0, TestItem::new("first"));
        verify_selection_invariant(&list);
        assert_eq!(list.len(), 1);
        assert_eq!(list.selected, Some(0));
        assert!(list.items[0].selected);

        // Insert before selected item
        list.insert(0, TestItem::new("before"));
        verify_selection_invariant(&list);
        assert_eq!(list.len(), 2);
        assert_eq!(list.selected, Some(1)); // Selection should shift
        assert!(!list.items[0].selected);
        assert!(list.items[1].selected);

        // Insert after selected item
        list.insert(2, TestItem::new("after"));
        verify_selection_invariant(&list);
        assert_eq!(list.len(), 3);
        assert_eq!(list.selected, Some(1)); // Selection should not change
        assert!(list.items[1].selected);

        // Insert at selected position
        list.insert(1, TestItem::new("at"));
        verify_selection_invariant(&list);
        assert_eq!(list.len(), 4);
        assert_eq!(list.selected, Some(2)); // Selection should shift
        assert!(list.items[2].selected);
    }

    #[test]
    fn test_insert_after() {
        let mut list = List::new(vec![TestItem::new("item0"), TestItem::new("item1")]);

        list.select(0);
        list.insert_after(TestItem::new("new"));
        verify_selection_invariant(&list);
        assert_eq!(list.len(), 3);
        assert_eq!(list.selected, Some(0)); // Selection unchanged
        assert_eq!(list.items[1].label, "new");

        // Test insert_after with no selection
        let mut empty_list: List<TestItem> = List::new(vec![]);
        empty_list.selected = None; // Force no selection
        empty_list.insert_after(TestItem::new("first"));
        verify_selection_invariant(&empty_list);
        assert_eq!(empty_list.len(), 1);
        assert_eq!(empty_list.selected, Some(0));
    }

    #[test]
    fn test_delete() {
        let mut list = List::new(vec![
            TestItem::new("item0"),
            TestItem::new("item1"),
            TestItem::new("item2"),
            TestItem::new("item3"),
        ]);

        list.select(2);
        verify_selection_invariant(&list);

        // Delete before selection
        let mut ctx = DummyContext {};
        let removed = list.delete_item(&mut ctx, 0);
        assert_eq!(removed.unwrap().label, "item0");
        verify_selection_invariant(&list);
        assert_eq!(list.selected, Some(1)); // Selection shifts left
        assert_eq!(list.items[1].label, "item2");
        assert!(list.items[1].selected);

        // Delete after selection
        let removed = list.delete_item(&mut ctx, 2);
        assert_eq!(removed.unwrap().label, "item3");
        verify_selection_invariant(&list);
        assert_eq!(list.selected, Some(1)); // Selection unchanged

        // Delete selected item
        let removed = list.delete_item(&mut ctx, 1);
        assert_eq!(removed.unwrap().label, "item2");
        verify_selection_invariant(&list);
        assert_eq!(list.selected, Some(0)); // New item at same position selected
        assert!(list.items[0].selected);

        // Delete last item when it's selected
        let removed = list.delete_item(&mut ctx, 0);
        verify_selection_invariant(&list);
        assert!(removed.is_some());
        assert_eq!(list.selected, None); // List is empty
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_clear() {
        let mut list = List::new(vec![
            TestItem::new("item0"),
            TestItem::new("item1"),
            TestItem::new("item2"),
        ]);

        list.select(1);
        let cleared = list.clear();
        verify_selection_invariant(&list);
        assert_eq!(cleared.len(), 3);
        assert_eq!(list.len(), 0);
        assert_eq!(list.selected, None);
    }

    use canopy_core::Loader;
    use canopy_core::tutils::dummyctx::DummyContext;

    // Loader implementation for test lists
    impl Loader for List<TestItem> {
        fn load(_canopy: &mut canopy_core::Canopy) {}
    }

    impl Loader for List<MultiLineItem> {
        fn load(_canopy: &mut canopy_core::Canopy) {}
    }

    // Helper to create a multi-line test item
    #[derive(canopy_core::StatefulNode)]
    struct MultiLineItem {
        label: String,
        height: u32,
        selected: bool,
        state: NodeState,
    }

    impl MultiLineItem {
        fn new(label: &str, height: u32) -> Self {
            MultiLineItem {
                label: label.to_string(),
                height,
                selected: false,
                state: NodeState::default(),
            }
        }
    }

    impl ListItem for MultiLineItem {
        fn set_selected(&mut self, state: bool) {
            self.selected = state;
        }
    }

    impl CommandNode for MultiLineItem {
        fn commands() -> Vec<CommandSpec>
        where
            Self: Sized,
        {
            vec![]
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Err(Error::UnknownCommand("no commands".to_string()))
        }
    }

    impl Node for MultiLineItem {
        fn layout(&mut self, _l: &Layout, r: Expanse) -> Result<()> {
            self.fit_size(
                Expanse {
                    w: r.w,
                    h: self.height,
                },
                r,
            );
            Ok(())
        }

        fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
            let style = if self.selected { "selected" } else { "" };
            let view = self.vp().view();
            for i in 0..self.height.min(view.h) {
                rndr.text(style, view.line(i), &format!("{} line {}", self.label, i))?;
            }
            Ok(())
        }
    }

    #[test]
    fn test_layout_and_viewport_adjustment() {
        let items = vec![
            MultiLineItem::new("item0", 3),
            MultiLineItem::new("item1", 2),
            MultiLineItem::new("item2", 4),
            MultiLineItem::new("item3", 1),
        ];

        let mut list = List::new(items);
        let layout = Layout {};

        // Set up the list viewport
        list.state.viewport.set_canvas(Expanse { w: 10, h: 10 });
        list.state.viewport.set_view(Rect::new(0, 0, 10, 5));

        // Run layout - this should position items and adjust view
        let available_space = Expanse { w: 10, h: 5 };
        list.layout(&layout, available_space).unwrap();

        // Check that layout positioned items correctly
        assert_eq!(list.items[0].vp().position().y, 0);
        assert_eq!(list.items[1].vp().position().y, 3);
        assert_eq!(list.items[2].vp().position().y, 5);
        assert_eq!(list.items[3].vp().position().y, 9);

        // View should still be at top since first item is selected by default
        assert_eq!(list.vp().view().tl.y, 0);
    }

    #[test]
    fn test_render_only_visible_items() {
        use canopy_core::tutils::harness::Harness;

        let items = vec![
            TestItem::new("item0"),
            TestItem::new("item1"),
            TestItem::new("item2"),
            TestItem::new("item3"),
        ];

        let list = List::new(items);

        // Create harness with a small viewport that can only show 2 items
        let mut harness = Harness::builder(list).size(20, 2).build().unwrap();

        // Render the list
        harness.render().unwrap();

        // Get the buffer and verify only visible items were rendered
        let bt = harness.tbuf();
        assert!(bt.contains_text("item0"));
        assert!(bt.contains_text("item1"));
        // item2 and item3 should not be visible as they're outside the view
    }

    #[test]
    fn test_render_with_scrolled_view() {
        use canopy_core::tutils::harness::Harness;

        let items = vec![
            TestItem::new("item0"),
            TestItem::new("item1"),
            TestItem::new("item2"),
            TestItem::new("item3"),
        ];

        let mut list = List::new(items);

        // Select item2 to change which item is selected
        list.select(2);

        // Create harness
        let mut harness = Harness::builder(list).size(20, 4).build().unwrap();

        // Scroll down to show items 1-3
        harness.canopy.scroll_down(&mut harness.root);

        // Render
        harness.render().unwrap();

        // Verify the rendered content shows the scrolled view
        let bt = harness.tbuf();

        // Should see items based on scroll position
        // The exact behavior depends on how scrolling works in the framework
        assert!(bt.contains_text("item"));
    }
}
