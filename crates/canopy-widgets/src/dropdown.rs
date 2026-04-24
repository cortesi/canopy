//! Dropdown widget for single-value selection with expand/collapse behavior.

use canopy::{
    Context, EventOutcome, ReadContext, Widget, command, derive_commands,
    error::Result,
    event::{Event, mouse},
    layout::{MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
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
        debug_assert!(self.selection_invariant_holds());
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
        debug_assert!(self.selection_invariant_holds());
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
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Confirm the highlighted selection and collapse.
    #[command]
    pub fn confirm(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.expanded {
            self.selected = self.highlighted;
            self.expanded = false;
            c.invalidate_layout();
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Handle a click inside the dropdown.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<bool> {
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
                debug_assert!(self.selection_invariant_holds());
                return Ok(true);
            }
            return Ok(false);
        }
        // When collapsed, click toggles expansion.
        self.expanded = true;
        self.highlighted = self.selected;
        c.invalidate_layout();
        debug_assert!(self.selection_invariant_holds());
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
        debug_assert!(self.selection_invariant_holds());
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

    /// Return the unclamped size required to render the current dropdown state.
    fn content_size(&self) -> Size<u32> {
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

    /// Return whether selection and highlight indices point at current items.
    fn selection_invariant_holds(&self) -> bool {
        !self.items.is_empty()
            && self.selected < self.items.len()
            && self.highlighted < self.items.len()
    }
}

impl<T> Widget for Dropdown<T>
where
    T: DropdownItem + Send + 'static,
{
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event
            && self.handle_click(ctx, *mouse_event)?
        {
            // Return Ignore so mouse bindings can also fire.
            return Ok(EventOutcome::Ignore);
        }
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
            if rect.h == 0 {
                return Ok(());
            }
            let label = self.items[self.selected].label();
            let indicator = " ▼";
            let display = format!("{}{}", label, indicator);
            rndr.text("dropdown", rect.line(0), &display)?;
        }

        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(self.content_size())
    }

    fn canvas(&self, _view: Size<u32>, _ctx: &canopy::layout::CanvasContext) -> Size<u32> {
        self.content_size()
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("dropdown")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Loader,
        testing::{dummyctx::DummyContext, harness::Harness},
    };

    use super::*;

    impl Loader for Dropdown<String> {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            Ok(())
        }
    }

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

    #[test]
    fn test_dropdown_luau_selection() -> Result<()> {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let root = Dropdown::new(items);
        let mut harness = Harness::builder(root).size(20, 6).build()?;
        harness.render()?;
        harness.script(include_str!("../tests/luau/dropdown_select_second.luau"))?;
        harness.with_root_widget::<Dropdown<String>, _>(|dropdown| {
            assert_eq!(dropdown.selected_index(), 1);
            assert!(!dropdown.is_expanded());
        });
        Ok(())
    }

    #[test]
    fn focus_and_selection_invariants_hold_after_commands() -> Result<()> {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let mut dropdown = Dropdown::new(items);
        let mut ctx = DummyContext::default();

        assert!(dropdown.selection_invariant_holds());
        dropdown.set_selected(1);
        dropdown.set_selected(99);
        dropdown.toggle(&mut ctx)?;
        dropdown.select_by(&mut ctx, 99)?;
        dropdown.confirm(&mut ctx)?;
        assert_eq!(dropdown.selected_index(), 2);
        assert!(!dropdown.is_expanded());
        assert!(dropdown.selection_invariant_holds());

        dropdown.toggle(&mut ctx)?;
        dropdown.select_by(&mut ctx, -99)?;
        dropdown.cancel(&mut ctx)?;
        assert_eq!(dropdown.selected_index(), 2);
        assert!(!dropdown.is_expanded());
        assert!(dropdown.selection_invariant_holds());
        Ok(())
    }
}
