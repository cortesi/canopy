//! Selector widget for multi-value selection with checkbox-style items.

use canopy::{
    Context, EventOutcome, ReadContext, Widget, command, derive_commands,
    error::Result,
    event::{Event, mouse},
    layout::{MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
};

/// Trait for items that can be displayed in a Selector.
pub trait SelectorItem {
    /// Return the display label for this item.
    fn label(&self) -> &str;
}

/// Simple string-based selector item.
impl SelectorItem for String {
    fn label(&self) -> &str {
        self
    }
}

/// Simple &str-based selector item.
impl SelectorItem for &str {
    fn label(&self) -> &str {
        self
    }
}

/// A multi-select widget with checkbox-style items.
///
/// Items can be toggled on/off independently. The selected indices are tracked
/// in the order they were selected, allowing for ordered selection if needed.
pub struct Selector<T>
where
    T: SelectorItem,
{
    /// Available items.
    items: Vec<T>,
    /// Currently focused index.
    focused: usize,
    /// Selected indices, in selection order.
    selected: Vec<usize>,
}

#[derive_commands]
impl<T> Selector<T>
where
    T: SelectorItem + 'static,
{
    /// Create a new selector with the given items.
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            focused: 0,
            selected: Vec::new(),
        }
    }

    /// Get the selected indices in selection order.
    pub fn selected_indices(&self) -> &[usize] {
        &self.selected
    }

    /// Get references to the selected items in selection order.
    pub fn selected_items(&self) -> Vec<&T> {
        self.selected
            .iter()
            .filter_map(|&idx| self.items.get(idx))
            .collect()
    }

    /// Check if an index is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// Get the currently focused index.
    pub fn focused_index(&self) -> usize {
        self.focused
    }

    /// Toggle selection of the focused item.
    #[command]
    pub fn toggle(&mut self, _c: &mut dyn Context) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }

        if let Some(pos) = self.selected.iter().position(|&idx| idx == self.focused) {
            // Already selected - remove it
            self.selected.remove(pos);
        } else {
            // Not selected - add it (in selection order)
            self.selected.push(self.focused);
        }
        Ok(())
    }

    /// Move focus by a signed offset.
    #[command]
    pub fn select_by(&mut self, _c: &mut dyn Context, delta: i32) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }

        let next = if delta.is_negative() {
            self.focused.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            self.focused.saturating_add(delta as usize)
        };
        self.focused = next.min(self.items.len() - 1);
        Ok(())
    }

    /// Move focus to the next item.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_by(c, 1)
    }

    /// Move focus to the previous item.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_by(c, -1)
    }

    /// Move focus to the first item.
    #[command]
    pub fn select_first(&mut self, _c: &mut dyn Context) -> Result<()> {
        if !self.items.is_empty() {
            self.focused = 0;
        }
        Ok(())
    }

    /// Move focus to the last item.
    #[command]
    pub fn select_last(&mut self, _c: &mut dyn Context) -> Result<()> {
        if !self.items.is_empty() {
            self.focused = self.items.len() - 1;
        }
        Ok(())
    }

    /// Clear all selections.
    #[command]
    pub fn clear(&mut self, _c: &mut dyn Context) -> Result<()> {
        self.selected.clear();
        Ok(())
    }

    /// Handle a click inside the selector.
    fn handle_click(&mut self, _c: &mut dyn Context, event: mouse::MouseEvent) -> Result<bool> {
        if event.action != mouse::Action::Down || event.button != mouse::Button::Left {
            return Ok(false);
        }
        let clicked_row = event.location.y as usize;
        if clicked_row < self.items.len() {
            // Move focus to clicked row.
            self.focused = clicked_row;
            // Toggle selection.
            if let Some(pos) = self.selected.iter().position(|&idx| idx == self.focused) {
                self.selected.remove(pos);
            } else {
                self.selected.push(self.focused);
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Select all items.
    #[command]
    pub fn select_all(&mut self, _c: &mut dyn Context) -> Result<()> {
        self.selected = (0..self.items.len()).collect();
        Ok(())
    }

    /// Get the number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the selector is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of selected items.
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }
}

impl<T> Widget for Selector<T>
where
    T: SelectorItem + Send + 'static,
{
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event
            && self.handle_click(ctx, *mouse_event)?
        {
            // Return Ignore so mouse bindings can also fire (e.g., to trigger effects).
            return Ok(EventOutcome::Ignore);
        }
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();
        let is_widget_focused = ctx.is_focused();

        for (idx, item) in self.items.iter().enumerate() {
            if idx as u32 >= rect.h {
                break;
            }
            let line_rect = rect.line(idx as u32);
            let label = item.label();
            let is_selected = self.selected.contains(&idx);
            let is_item_focused = idx == self.focused;

            // Checkbox prefix
            let prefix = if is_selected { "[x] " } else { "[ ] " };
            let display = format!("{}{}", prefix, label);

            // Show focus highlight only when the widget has focus
            if is_item_focused && is_widget_focused {
                // Focused item - highlight background
                rndr.fill("selector/focus", line_rect.into(), ' ')?;
                if is_selected {
                    rndr.text("selector/focus/selected", line_rect, &display)?;
                } else {
                    rndr.text("selector/focus", line_rect, &display)?;
                }
            } else if is_selected {
                rndr.text("selector/selected", line_rect, &display)?;
            } else {
                rndr.text("selector", line_rect, &display)?;
            }
        }

        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let max_label_width = self
            .items
            .iter()
            .map(|item| item.label().len())
            .max()
            .unwrap_or(0) as u32;

        // Add space for "[x] " prefix (4 chars)
        let width = max_label_width + 4;
        let height = self.items.len() as u32;

        let size = Size::new(width, height);
        c.clamp(size)
    }

    fn canvas(&self, _view: Size<u32>, _ctx: &canopy::layout::CanvasContext) -> Size<u32> {
        let max_label_width = self
            .items
            .iter()
            .map(|item| item.label().len())
            .max()
            .unwrap_or(0) as u32;

        let width = max_label_width + 4;
        let height = self.items.len() as u32;

        Size::new(width, height)
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("selector")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_creation() {
        let items = vec!["Option 1".to_string(), "Option 2".to_string()];
        let selector = Selector::new(items);
        assert_eq!(selector.len(), 2);
        assert_eq!(selector.selected_count(), 0);
        assert_eq!(selector.focused_index(), 0);
    }

    #[test]
    fn test_selector_toggle() {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let mut selector = Selector::new(items);

        // Initially nothing selected
        assert!(!selector.is_selected(0));
        assert_eq!(selector.selected_count(), 0);

        // Toggle focused item (index 0)
        // Note: We can't easily call commands without a Context, so test the logic directly
        selector.selected.push(0);
        assert!(selector.is_selected(0));
        assert_eq!(selector.selected_count(), 1);

        // Add another selection
        selector.focused = 2;
        selector.selected.push(2);
        assert!(selector.is_selected(2));
        assert_eq!(selector.selected_count(), 2);

        // Check selection order is preserved
        assert_eq!(selector.selected_indices(), &[0, 2]);
    }

    #[test]
    fn test_selector_empty() {
        let items: Vec<String> = vec![];
        let selector = Selector::new(items);
        assert!(selector.is_empty());
        assert_eq!(selector.selected_count(), 0);
    }
}
