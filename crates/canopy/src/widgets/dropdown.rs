//! Dropdown widget for single-value selection with expand/collapse behavior.

use crate::{
    Context, ViewContext, command, derive_commands,
    error::Result,
    event::{Event, mouse},
    layout::{MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
};

/// Trait for items that can be displayed in a Dropdown.
pub trait DropdownItem {
    /// Return the display label for this item.
    fn label(&self) -> &str;
}

/// Simple string-based dropdown item.
impl DropdownItem for String {
    fn label(&self) -> &str {
        self
    }
}

/// Simple &str-based dropdown item.
impl DropdownItem for &str {
    fn label(&self) -> &str {
        self
    }
}

/// A dropdown widget for single-value selection.
///
/// When collapsed, displays the currently selected item with a dropdown indicator.
/// When expanded, displays all options for selection.
pub struct Dropdown<T>
where
    T: DropdownItem,
{
    /// Available items.
    items: Vec<T>,
    /// Currently selected index.
    selected: usize,
    /// Whether the dropdown is expanded.
    expanded: bool,
    /// Highlighted index when expanded (for navigation before confirming).
    highlighted: usize,
}

#[derive_commands]
impl<T> Dropdown<T>
where
    T: DropdownItem + 'static,
{
    /// Create a new dropdown with the given items.
    ///
    /// Panics if items is empty.
    pub fn new(items: Vec<T>) -> Self {
        assert!(!items.is_empty(), "Dropdown must have at least one item");
        Self {
            items,
            selected: 0,
            expanded: false,
            highlighted: 0,
        }
    }

    /// Get the currently selected item.
    pub fn selected(&self) -> &T {
        &self.items[self.selected]
    }

    /// Get the currently selected index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Set the selected index.
    pub fn set_selected(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
            self.highlighted = index;
        }
    }

    /// Check if the dropdown is expanded.
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Toggle the dropdown expanded state.
    #[command]
    pub fn toggle(&mut self, c: &mut dyn Context) -> Result<()> {
        self.expanded = !self.expanded;
        self.highlighted = self.selected;
        // Mark layout dirty so parent can resize
        c.invalidate_layout();
        Ok(())
    }

    /// Move highlight by a signed offset (when expanded).
    #[command]
    pub fn select_by(&mut self, _c: &mut dyn Context, delta: i32) -> Result<()> {
        if !self.expanded || self.items.is_empty() {
            return Ok(());
        }

        let next = if delta.is_negative() {
            self.highlighted
                .saturating_sub(delta.unsigned_abs() as usize)
        } else {
            self.highlighted.saturating_add(delta as usize)
        };
        self.highlighted = next.min(self.items.len() - 1);
        Ok(())
    }

    /// Move highlight to the next item.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_by(c, 1)
    }

    /// Move highlight to the previous item.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_by(c, -1)
    }

    /// Confirm the highlighted selection and collapse.
    #[command]
    pub fn confirm(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.expanded {
            self.selected = self.highlighted;
            self.expanded = false;
            c.invalidate_layout();
        }
        Ok(())
    }

    /// Handle a click inside the dropdown.
    fn handle_click(&mut self, c: &dyn Context, event: mouse::MouseEvent) -> Result<bool> {
        if event.action != mouse::Action::Down || event.button != mouse::Button::Left {
            return Ok(false);
        }
        let clicked_row = event.location.y as usize;
        if self.expanded {
            // When expanded, click selects and confirms.
            if clicked_row < self.items.len() {
                self.highlighted = clicked_row;
                self.selected = self.highlighted;
                self.expanded = false;
                c.invalidate_layout();
                return Ok(true);
            }
            return Ok(false);
        }
        // When collapsed, click toggles expansion.
        self.expanded = true;
        self.highlighted = self.selected;
        c.invalidate_layout();
        Ok(true)
    }

    /// Collapse without changing selection.
    #[command]
    pub fn cancel(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.expanded {
            self.expanded = false;
            self.highlighted = self.selected;
            c.invalidate_layout();
        }
        Ok(())
    }

    /// Get the number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the dropdown is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T> Widget for Dropdown<T>
where
    T: DropdownItem + Send + 'static,
{
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if let Event::Mouse(mouse_event) = event
            && matches!(self.handle_click(ctx, *mouse_event), Ok(true))
        {
            // Return Ignore so mouse bindings can also fire.
            return EventOutcome::Ignore;
        }
        EventOutcome::Ignore
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        if self.expanded {
            // Render all items
            for (idx, item) in self.items.iter().enumerate() {
                if idx as u32 >= rect.h {
                    break;
                }
                let line_rect = rect.line(idx as u32);
                let label = item.label();

                if idx == self.highlighted {
                    // Highlighted item - inverse colors
                    rndr.fill("dropdown/highlight", line_rect.into(), ' ')?;
                    rndr.text("dropdown/highlight", line_rect, label)?;
                } else if idx == self.selected {
                    // Selected but not highlighted
                    rndr.text("dropdown/selected", line_rect, label)?;
                } else {
                    rndr.text("dropdown", line_rect, label)?;
                }
            }
        } else {
            // Render collapsed state: selected item with indicator
            let label = self.items[self.selected].label();
            let indicator = " ▼";
            let display = format!("{}{}", label, indicator);
            rndr.text("dropdown", rect.line(0), &display)?;
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

        // Add space for the ▼ indicator (2 chars)
        let width = max_label_width + 2;

        let height = if self.expanded {
            self.items.len() as u32
        } else {
            1
        };

        let size = Size::new(width, height);
        c.clamp(size)
    }

    fn canvas(&self, _view: Size<u32>, _ctx: &crate::layout::CanvasContext) -> Size<u32> {
        let max_label_width = self
            .items
            .iter()
            .map(|item| item.label().len())
            .max()
            .unwrap_or(0) as u32;

        let width = max_label_width + 2;
        let height = if self.expanded {
            self.items.len() as u32
        } else {
            1
        };

        Size::new(width, height)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("dropdown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dropdown_creation() {
        let items = vec!["Option 1".to_string(), "Option 2".to_string()];
        let dropdown = Dropdown::new(items);
        assert_eq!(dropdown.selected_index(), 0);
        assert!(!dropdown.is_expanded());
    }

    #[test]
    fn test_dropdown_selection() {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let mut dropdown = Dropdown::new(items);
        dropdown.set_selected(1);
        assert_eq!(dropdown.selected_index(), 1);
        assert_eq!(dropdown.selected().label(), "Option 2");
    }

    #[test]
    #[should_panic(expected = "Dropdown must have at least one item")]
    fn test_dropdown_empty_panics() {
        let items: Vec<String> = vec![];
        let _ = Dropdown::new(items);
    }
}
