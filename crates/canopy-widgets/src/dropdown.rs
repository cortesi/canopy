//! Dropdown widget for single-value selection with expand/collapse behavior.

use canopy::{
    Context, EventOutcome, NodeName, ViewContext, Widget, derive_commands,
    error::{Error, Result},
    geom::Size,
    input::{Event, mouse},
    layout::{MeasureConstraints, Measurement},
    render::Render,
    text,
};

use crate::{
    label::Label,
    row_cursor::{RowCursor, is_primary_click, label_rows, widest_label},
};

/// A dropdown widget for single-value selection.
///
/// When collapsed, displays the currently selected item with a dropdown
/// indicator. When expanded, displays all options for selection.
/// Opening and navigation scroll the highlighted row into view after layout.
pub struct Dropdown<T>
where
    T: Label,
{
    /// Available items.
    items: Vec<T>,
    /// Currently selected index.
    selected: usize,
    /// Whether the dropdown is expanded.
    expanded: bool,
    /// Cursor over the highlighted row when expanded (for navigation before
    /// confirming).
    cursor: RowCursor,
}

#[derive_commands]
impl<T> Dropdown<T>
where
    T: Label + 'static,
{
    /// Create a new dropdown with the given items.
    ///
    /// Returns an error if `items` is empty.
    pub fn new(items: Vec<T>) -> Result<Self> {
        if items.is_empty() {
            return Err(Error::Invalid(
                "Dropdown must have at least one item".into(),
            ));
        }
        let cursor = RowCursor::new(items.len());
        Ok(Self {
            items,
            selected: 0,
            expanded: false,
            cursor,
        })
    }

    /// Get the currently selected item.
    pub fn selected(&self) -> &T {
        &self.items[self.selected]
    }

    /// Get the currently selected index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Toggle the dropdown expanded state.
    #[command]
    pub fn toggle(&mut self, c: &mut dyn Context) -> Result<()> {
        self.expanded = !self.expanded;
        self.cursor.set_index(self.selected);

        if self.expanded {
            self.cursor.reveal(c);
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Move highlight by a signed offset (when expanded).
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if !self.expanded {
            return Ok(());
        }

        self.cursor.select_by(delta);
        self.cursor.reveal(c);
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Confirm the highlighted selection and collapse.
    #[command]
    pub fn confirm(&mut self) -> Result<()> {
        if self.expanded {
            self.selected = self.cursor.index().unwrap_or(self.selected);
            self.expanded = false;
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Confirm the clicked row when expanded, or expand when collapsed.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<()> {
        if !is_primary_click(event) {
            return Ok(());
        }
        if !self.expanded {
            return self.toggle(c);
        }
        if let Some(row) = self.cursor.row_at(&c.view(), event.location) {
            self.cursor.set_index(row);
            self.confirm()?;
        }
        Ok(())
    }

    /// Collapse without changing selection.
    #[command]
    pub fn cancel(&mut self) -> Result<()> {
        if self.expanded {
            self.expanded = false;
            self.cursor.set_index(self.selected);
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Return the unclamped size required to render the current dropdown state.
    fn content_size(&self) -> Size {
        let max_label_width = widest_label(self.items.iter().map(Label::label));
        let width = u32::try_from(max_label_width)
            .unwrap_or(u32::MAX)
            .saturating_add(2);
        let height = if self.expanded {
            u32::try_from(self.items.len()).unwrap_or(u32::MAX)
        } else {
            1
        };

        Size::new(width, height)
    }

    /// Return whether selection and highlight indices point at current items.
    fn selection_invariant_holds(&self) -> bool {
        !self.items.is_empty()
            && self.selected < self.items.len()
            && self
                .cursor
                .index()
                .is_some_and(|index| index < self.items.len())
    }
}

impl<T> Widget for Dropdown<T>
where
    T: Label + 'static,
{
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
            self.handle_click(ctx, *mouse_event)?;
        }
        // Ignore so mouse bindings can also fire.
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        if self.expanded {
            // Render visible items
            for (row, idx) in label_rows(self.items.len(), view.scroll.y, rect.h) {
                let item = &self.items[idx];
                let line_rect = rect.line(row)?;
                let (label, _) =
                    text::slice_by_columns(item.label(), view.scroll.x as usize, rect.w as usize);

                if Some(idx) == self.cursor.index() {
                    // Highlighted item - inverse colors
                    rndr.fill("dropdown/highlight", line_rect.rect(), ' ')?;
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
            let (display, _) =
                text::slice_by_columns(&display, view.scroll.x as usize, rect.w as usize);
            rndr.text("dropdown", rect.line(0)?, display)?;
        }

        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(self.content_size())
    }

    fn canvas(&self, _view: Size, _ctx: &canopy::layout::CanvasContext) -> Size {
        self.content_size()
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
    use canopy::{Register, Setup, testing::harness::Harness};

    use super::*;

    impl Register for Dropdown<String> {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()?;
            Ok(())
        }
    }

    #[test]
    fn test_dropdown_creation() {
        let items = vec!["Option 1".to_string(), "Option 2".to_string()];
        let dropdown = Dropdown::new(items).expect("nonempty dropdown");
        assert_eq!(dropdown.selected_index(), 0);
        assert!(!dropdown.expanded);
    }

    #[test]
    fn test_dropdown_selection() {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let mut dropdown = Dropdown::new(items).expect("nonempty dropdown");
        dropdown.selected = 1;
        dropdown.cursor.set_index(1);
        assert_eq!(dropdown.selected_index(), 1);
        assert_eq!(dropdown.selected().label(), "Option 2");
    }

    #[test]
    fn test_dropdown_rejects_empty_items() {
        let items: Vec<String> = vec![];
        assert!(Dropdown::new(items).is_err());
    }

    #[test]
    fn test_dropdown_luau_selection() -> Result<()> {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let root = Dropdown::new(items)?;
        let mut harness = Harness::builder(root)
            .register::<Dropdown<String>>()
            .size(20, 6)
            .build()?;
        harness.render()?;
        harness.script(include_str!("../tests/luau/dropdown_select_second.luau"))?;
        harness.with_root_widget::<Dropdown<String>, _>(|dropdown| {
            assert_eq!(dropdown.selected_index(), 1);
            assert!(!dropdown.expanded);
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
        let mut harness = Harness::builder(Dropdown::new(items)?)
            .register::<Dropdown<String>>()
            .size(20, 6)
            .build()?;
        harness.with_root_widget_context(|dropdown: &mut Dropdown<String>, ctx| {
            assert!(dropdown.selection_invariant_holds());
            dropdown.selected = 1;
            dropdown.cursor.set_index(1);
            dropdown.toggle(ctx)?;
            dropdown.select_by(ctx, 99)?;
            dropdown.confirm()?;
            assert_eq!(dropdown.selected_index(), 2);
            assert!(!dropdown.expanded);
            assert!(dropdown.selection_invariant_holds());

            dropdown.toggle(ctx)?;
            dropdown.select_by(ctx, -99)?;
            dropdown.cancel()?;
            assert_eq!(dropdown.selected_index(), 2);
            assert!(!dropdown.expanded);
            assert!(dropdown.selection_invariant_holds());
            Ok(())
        })
    }
}
