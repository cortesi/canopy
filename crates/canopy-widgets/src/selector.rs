//! Selector widget for multi-value selection with checkbox-style items.

use canopy::{
    Context, EventOutcome, NodeName, Render, RevealAlign, ViewContext, Widget, WidgetSemantics,
    derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::{Rect, Size},
    layout::{MeasureConstraints, Measurement},
    text,
};
use unicode_width::UnicodeWidthStr;

use crate::label::Label;

/// The glyphs a selector draws before an unchecked and checked item.
const CHECK_GLYPHS: (&str, &str) = ("[ ] ", "[x] ");

/// A multi-select widget with checkbox-style items.
///
/// Items can be toggled on/off independently. The selected indices are tracked
/// in the order they were selected, allowing for ordered selection if needed.
/// Navigation scrolls the focused row into view after layout. A host that
/// changes its item set calls [`Selector::show`], which reinstalls both the
/// items and the check state.
pub struct Selector<T>
where
    T: Label,
{
    /// Available items.
    items: Vec<T>,
    /// Currently focused index.
    focused: usize,
    /// Selected indices, in selection order.
    selected: Vec<usize>,
    /// Optional semantic label.
    label: Option<String>,
    /// Glyphs drawn before an unchecked and checked item.
    glyphs: (&'static str, &'static str),
}

#[derive_commands]
impl<T> Selector<T>
where
    T: Label + 'static,
{
    /// Create a new selector with the given items.
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            focused: 0,
            selected: Vec::new(),
            label: None,
            glyphs: CHECK_GLYPHS,
        }
    }

    /// Set the semantic label.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Replace the checkbox glyphs, unchecked first.
    #[must_use]
    pub fn with_glyphs(mut self, unchecked: &'static str, checked: &'static str) -> Self {
        self.glyphs = (unchecked, checked);
        self
    }

    /// Replace the items and the checked indices, in place.
    ///
    /// Indices outside the new items are dropped. Focus returns to the first
    /// item, matching a fresh list.
    pub fn show(&mut self, items: Vec<T>, checked: &[usize]) {
        self.items = items;
        self.focused = 0;
        self.selected.clear();
        for index in checked {
            if *index < self.items.len() && !self.selected.contains(index) {
                self.selected.push(*index);
            }
        }
        debug_assert!(self.selection_invariant_holds());
    }

    /// Return the focused item index.
    #[must_use]
    pub fn focused_index(&self) -> usize {
        self.focused
    }

    /// Return the checked indices, in selection order.
    #[must_use]
    pub fn checked_indices(&self) -> &[usize] {
        &self.selected
    }

    /// Get references to the selected items in selection order.
    pub fn selected_items(&self) -> Vec<&T> {
        self.selected
            .iter()
            .filter_map(|&idx| self.items.get(idx))
            .collect()
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
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Move focus by a signed offset.
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if self.items.is_empty() {
            return Ok(());
        }

        self.focused = self
            .focused
            .saturating_add_signed(delta as isize)
            .min(self.items.len() - 1);
        self.reveal_focused(c);
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Move focus to the first item.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.items.is_empty() {
            self.focused = 0;
            self.reveal_focused(c);
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Move focus to the last item.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.items.is_empty() {
            self.focused = self.items.len() - 1;
            self.reveal_focused(c);
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Clear all selections.
    #[command]
    pub fn clear(&mut self, _c: &mut dyn Context) -> Result<()> {
        self.selected.clear();
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Reveal the focused row without changing horizontal scroll.
    fn reveal_focused(&self, c: &mut dyn Context) {
        c.reveal_area(
            Rect::new(c.view().scroll.x, self.focused as u32, 1, 1),
            RevealAlign::Nearest,
        );
    }

    /// Focus and toggle the clicked row.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<()> {
        if event.action != mouse::Action::Down || event.button != mouse::Button::Left {
            return Ok(());
        }
        if let Some(point) = c.view().content_point(event.location)
            && (point.y as usize) < self.items.len()
        {
            self.focused = point.y as usize;
            self.toggle(c)?;
        }
        Ok(())
    }

    /// Select all items.
    #[command]
    pub fn select_all(&mut self, _c: &mut dyn Context) -> Result<()> {
        self.selected = (0..self.items.len()).collect();
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Return the unclamped size required to render all selector items.
    fn content_size(&self) -> Size {
        let max_label_width = self
            .items
            .iter()
            .map(|item| UnicodeWidthStr::width(item.label()))
            .max()
            .unwrap_or(0) as u32;

        Size::new(max_label_width + 4, self.items.len() as u32)
    }

    /// Return whether focus and selection indices point at current items.
    fn selection_invariant_holds(&self) -> bool {
        let focus_valid = if self.items.is_empty() {
            self.focused == 0
        } else {
            self.focused < self.items.len()
        };
        let selections_valid = self.selected.iter().enumerate().all(|(position, index)| {
            *index < self.items.len() && !self.selected[..position].contains(index)
        });
        focus_valid && selections_valid
    }
}

impl<T> Widget for Selector<T>
where
    T: Label + 'static,
{
    fn key_outcome(&self, _key: key::Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
        Some(EventOutcome::Ignore)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
            self.handle_click(ctx, *mouse_event)?;
        }
        // Ignore so mouse bindings can also fire, for example to trigger
        // effects.
        Ok(EventOutcome::Ignore)
    }

    fn semantics(&self, _ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        let checked = self
            .selected
            .iter()
            .filter_map(|index| self.items.get(*index))
            .map(Label::label)
            .collect::<Vec<_>>()
            .join(", ");
        Ok(WidgetSemantics {
            role: Some("selector".into()),
            label: self.label.clone(),
            value: (!checked.is_empty()).then_some(checked),
            ..WidgetSemantics::default()
        })
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();
        let is_widget_focused = ctx.is_focused();

        for (idx, item) in self.items.iter().enumerate().skip(view.scroll.y as usize) {
            let Ok(row) = u32::try_from(idx - view.scroll.y as usize) else {
                break;
            };
            if row >= rect.h {
                break;
            }
            let line_rect = rect.line(row)?;
            let label = item.label();
            let is_selected = self.selected.contains(&idx);
            let is_item_focused = idx == self.focused;

            // Checkbox prefix
            let prefix = if is_selected {
                self.glyphs.1
            } else {
                self.glyphs.0
            };
            let display = format!("{}{}", prefix, label);
            let (display, _) =
                text::slice_by_columns(&display, view.scroll.x as usize, rect.w as usize);

            // Show focus highlight only when the widget has focus
            if is_item_focused && is_widget_focused {
                // Focused item - highlight background
                rndr.fill("selector/focus", line_rect.rect(), ' ')?;
                if is_selected {
                    rndr.text("selector/focus/selected", line_rect, display)?;
                } else {
                    rndr.text("selector/focus", line_rect, display)?;
                }
            } else if is_selected {
                rndr.text("selector/selected", line_rect, display)?;
            } else {
                rndr.text("selector", line_rect, display)?;
            }
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
        NodeName::convert("selector")
    }
}

#[cfg(test)]
mod tests {
    use canopy::testing::dummyctx::DummyContext;

    use super::*;

    #[test]
    fn test_selector_creation() {
        let items = vec!["Option 1".to_string(), "Option 2".to_string()];
        let selector = Selector::new(items);
        assert_eq!(selector.items.len(), 2);
        assert!(selector.selected.is_empty());
        assert_eq!(selector.focused, 0);
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
        assert!(!selector.selected.contains(&0));
        assert!(selector.selected.is_empty());

        // Toggle focused item (index 0)
        // Note: We can't easily call commands without a Context, so test the
        // logic directly
        selector.selected.push(0);
        assert!(selector.selected.contains(&0));
        assert_eq!(selector.selected.len(), 1);

        // Add another selection
        selector.focused = 2;
        selector.selected.push(2);
        assert!(selector.selected.contains(&2));
        assert_eq!(selector.selected.len(), 2);

        // Check selection order is preserved
        assert_eq!(selector.selected.as_slice(), &[0, 2]);
    }

    #[test]
    fn test_selector_empty() {
        let items: Vec<String> = vec![];
        let selector = Selector::new(items);
        assert!(selector.items.is_empty());
        assert!(selector.selected.is_empty());
    }

    #[test]
    fn focus_and_selection_invariants_hold_after_commands() -> Result<()> {
        let items = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
        ];
        let mut selector = Selector::new(items);
        let mut ctx = DummyContext::default();

        assert!(selector.selection_invariant_holds());
        selector.toggle(&mut ctx)?;
        selector.select_by(&mut ctx, 2)?;
        selector.toggle(&mut ctx)?;
        selector.select_by(&mut ctx, 99)?;
        selector.toggle(&mut ctx)?;
        selector.select_all(&mut ctx)?;
        assert_eq!(selector.focused, 2);
        assert_eq!(selector.selected.as_slice(), &[0, 1, 2]);
        assert!(selector.selection_invariant_holds());

        selector.clear(&mut ctx)?;
        selector.select_by(&mut ctx, -99)?;
        assert_eq!(selector.focused, 0);
        assert!(selector.selected.is_empty());
        assert!(selector.selection_invariant_holds());
        Ok(())
    }

    #[test]
    fn show_replaces_items_and_checks_in_order() {
        let mut selector = Selector::new(vec!["a".to_string(), "b".to_string()]);
        selector.show(
            vec!["x".to_string(), "y".to_string(), "z".to_string()],
            &[2, 0, 9, 2],
        );
        assert_eq!(selector.focused_index(), 0);
        assert_eq!(
            selector.checked_indices(),
            &[2, 0],
            "out-of-range and repeated indices drop"
        );
        assert!(selector.selection_invariant_holds());
    }

    #[test]
    fn glyphs_and_semantics_report_the_checks() -> Result<()> {
        let mut selector = Selector::new(vec!["Size".to_string(), "Modified".to_string()])
            .with_label("Columns")
            .with_glyphs("· ", "✓ ");
        assert_eq!(selector.glyphs, ("· ", "✓ "));
        selector.selected.push(1);
        let semantics = selector.semantics(&DummyContext::default())?;
        assert_eq!(semantics.role.as_deref(), Some("selector"));
        assert_eq!(semantics.label.as_deref(), Some("Columns"));
        assert_eq!(semantics.value.as_deref(), Some("Modified"));
        Ok(())
    }

    #[test]
    fn empty_selector_invariants_hold_after_commands() -> Result<()> {
        let mut selector = Selector::<String>::new(Vec::new());
        let mut ctx = DummyContext::default();

        selector.toggle(&mut ctx)?;
        selector.select_by(&mut ctx, 1)?;
        selector.select_first(&mut ctx)?;
        selector.select_last(&mut ctx)?;
        selector.select_all(&mut ctx)?;
        selector.clear(&mut ctx)?;

        assert!(selector.selection_invariant_holds());
        assert_eq!(selector.focused, 0);
        assert!(selector.selected.is_empty());
        Ok(())
    }
}
