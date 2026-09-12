//! Dropdown widget for single-value selection with expand/collapse behavior.

use canopy::{
    Context, EventOutcome, NodeName, Render, ViewContext, Widget, derive_commands,
    error::{Error, Result},
    event::{Event, mouse},
    geom::{Rect, Size},
    layout::{MeasureConstraints, Measurement},
    text,
};
use unicode_width::UnicodeWidthStr;

use crate::label::Label;

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
    /// Highlighted index when expanded (for navigation before confirming).
    highlighted: usize,
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
        Ok(Self {
            items,
            selected: 0,
            expanded: false,
            highlighted: 0,
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
        self.highlighted = self.selected;
        // Mark layout dirty so parent can resize
        c.invalidate_layout();
        if self.expanded {
            self.reveal_highlighted(c);
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

        self.highlighted = self
            .highlighted
            .saturating_add_signed(delta as isize)
            .min(self.items.len() - 1);
        self.reveal_highlighted(c);
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

    /// Reveal the highlighted row after expansion or navigation is laid out.
    fn reveal_highlighted(&self, c: &mut dyn Context) {
        c.scroll_into_view(Rect::new(c.view().scroll.x, self.highlighted as u32, 1, 1));
    }

    /// Confirm the clicked row when expanded, or expand when collapsed.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<()> {
        if event.action != mouse::Action::Down || event.button != mouse::Button::Left {
            return Ok(());
        }
        let clicked_row = event.location.y.saturating_add(c.view().scroll.y) as usize;
        if !self.expanded {
            return self.toggle(c);
        }
        if clicked_row < self.items.len() {
            self.highlighted = clicked_row;
            self.confirm(c)?;
        }
        Ok(())
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

    /// Return the unclamped size required to render the current dropdown state.
    fn content_size(&self) -> Size {
        let max_label_width = self
            .items
            .iter()
            .map(|item| UnicodeWidthStr::width(item.label()))
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
            for (idx, item) in self.items.iter().enumerate().skip(view.scroll.y as usize) {
                let Ok(row) = u32::try_from(idx - view.scroll.y as usize) else {
                    break;
                };
                if row >= rect.h {
                    break;
                }
                let line_rect = rect.line(row)?;
                let (label, _) =
                    text::slice_by_columns(item.label(), view.scroll.x as usize, rect.w as usize);

                if idx == self.highlighted {
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
        dropdown.highlighted = 1;
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
        let mut harness = Harness::builder(root).size(20, 6).build()?;
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
        let mut dropdown = Dropdown::new(items)?;
        let mut ctx = DummyContext::default();

        assert!(dropdown.selection_invariant_holds());
        dropdown.selected = 1;
        dropdown.highlighted = 1;
        dropdown.toggle(&mut ctx)?;
        dropdown.select_by(&mut ctx, 99)?;
        dropdown.confirm(&mut ctx)?;
        assert_eq!(dropdown.selected_index(), 2);
        assert!(!dropdown.expanded);
        assert!(dropdown.selection_invariant_holds());

        dropdown.toggle(&mut ctx)?;
        dropdown.select_by(&mut ctx, -99)?;
        dropdown.cancel(&mut ctx)?;
        assert_eq!(dropdown.selected_index(), 2);
        assert!(!dropdown.expanded);
        assert!(dropdown.selection_invariant_holds());
        Ok(())
    }
}
