//! Selector widget for choosing one value from a visible list.

use canopy::{
    Context, EventOutcome, NodeName, ViewContext, Widget, derive_commands,
    error::Result,
    geom::Size,
    input::{Event, mouse},
    layout::{MeasureConstraints, Measurement},
    render::Render,
    runtime::WidgetSemantics,
    style::roles,
    text,
};

use crate::{
    label::ItemLabel,
    row_cursor::{CursorMove, RowCursor, is_primary_click, label_rows, widest_label},
};

/// The glyphs a selector draws before an unchosen and the chosen item.
const CHOICE_GLYPHS: (&str, &str) = ("( ) ", "(•) ");

/// A single-choice widget that shows every item.
///
/// The selection is the cursor that navigation moves; [`Selector::choose`]
/// makes the selected item the choice. Navigation scrolls the selected row
/// into view after layout. A host that changes its item set calls
/// [`Selector::show`], which reinstalls both the items and the choice.
pub struct Selector<T>
where
    T: ItemLabel,
{
    /// Available items.
    items: Vec<T>,
    /// Cursor over the selected row.
    cursor: RowCursor,
    /// The chosen index, if any.
    chosen: Option<usize>,
    /// Optional semantic label.
    label: Option<String>,
    /// Glyphs drawn before an unchosen and the chosen item.
    glyphs: (&'static str, &'static str),
}

#[derive_commands]
impl<T> Selector<T>
where
    T: ItemLabel + 'static,
{
    /// Create a new selector with the given items and no choice.
    pub fn new(items: Vec<T>) -> Self {
        let cursor = RowCursor::new(items.len());
        Self {
            items,
            cursor,
            chosen: None,
            label: None,
            glyphs: CHOICE_GLYPHS,
        }
    }

    /// Set the semantic label.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Start with `index` chosen and selected; an out-of-range index is
    /// ignored.
    #[must_use]
    pub fn with_chosen(mut self, index: usize) -> Self {
        if index < self.items.len() {
            self.chosen = Some(index);
            self.cursor.set_index(index);
        }
        self
    }

    /// Replace the choice glyphs, unchosen first.
    #[must_use]
    pub fn with_glyphs(mut self, unchosen: &'static str, chosen: &'static str) -> Self {
        self.glyphs = (unchosen, chosen);
        self
    }

    /// Replace the items and the chosen index, in place.
    ///
    /// A choice outside the new items is dropped. The selection moves to the
    /// choice, or to the first item without one.
    pub fn set_items(&mut self, items: Vec<T>, chosen: Option<usize>) {
        self.items = items;
        self.cursor = RowCursor::new(self.items.len());
        self.chosen = chosen.filter(|index| *index < self.items.len());
        if let Some(index) = self.chosen {
            self.cursor.set_index(index);
        }
        debug_assert!(self.invariant_holds());
    }

    /// Return the selected item index, or 0 for an empty selector.
    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.cursor.index().unwrap_or(0)
    }

    /// Return the chosen index.
    #[must_use]
    pub fn chosen_index(&self) -> Option<usize> {
        self.chosen
    }

    /// Return the chosen item.
    #[must_use]
    pub fn chosen(&self) -> Option<&T> {
        self.chosen.and_then(|index| self.items.get(index))
    }

    /// Make the selected item the choice.
    #[command]
    pub fn choose(&mut self, _c: &mut dyn Context) -> Result<()> {
        if let Some(selected) = self.cursor.index() {
            self.chosen = Some(selected);
        }
        debug_assert!(self.invariant_holds());
        Ok(())
    }

    /// Clear the choice.
    #[command]
    pub fn clear_choice(&mut self, _c: &mut dyn Context) -> Result<()> {
        self.chosen = None;
        Ok(())
    }

    /// Move the selection by a signed offset.
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.cursor.select_by(delta);
        self.cursor.reveal(c);
        debug_assert!(self.invariant_holds());
        Ok(())
    }

    /// Select the first item.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) -> Result<()> {
        self.cursor.select_first();
        self.cursor.reveal(c);
        debug_assert!(self.invariant_holds());
        Ok(())
    }

    /// Select the last item.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) -> Result<()> {
        self.cursor.select_last();
        self.cursor.reveal(c);
        debug_assert!(self.invariant_holds());
        Ok(())
    }

    /// Move the selection by whole pages.
    /// @param delta Negative values move up; positive values move down.
    #[command]
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let rows = c.view().view_rect().h;
        self.cursor.page(delta, rows);
        self.cursor.reveal(c);
        debug_assert!(self.invariant_holds());
        Ok(())
    }

    /// Select and choose the clicked row.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<()> {
        if !is_primary_click(event) {
            return Ok(());
        }
        if let Some(row) = self.cursor.row_at(&c.view(), event.location) {
            self.cursor.set_index(row);
            self.choose(c)?;
        }
        Ok(())
    }

    /// Return the unclamped size required to render all selector items.
    ///
    /// Every row reserves the wider choice glyph, so choosing an item never
    /// changes the width.
    fn content_size(&self) -> Size {
        let glyph_width = text::width(self.glyphs.0).max(text::width(self.glyphs.1));
        let max_label_width = widest_label(self.items.iter().map(ItemLabel::label));
        let width = glyph_width.saturating_add(max_label_width);
        let height = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        Size::new(width, height)
    }

    /// Return whether the selection and the choice point at current items.
    fn invariant_holds(&self) -> bool {
        let selection_valid = self.cursor.len() == self.items.len()
            && match self.cursor.index() {
                Some(index) => index < self.items.len(),
                None => self.items.is_empty(),
            };
        selection_valid && self.chosen.is_none_or(|index| index < self.items.len())
    }
}

impl<T> Widget for Selector<T>
where
    T: ItemLabel + 'static,
{
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
            self.handle_click(ctx, *mouse_event)?;
        }
        // Ignore so mouse bindings can also fire, for example to trigger
        // effects.
        Ok(EventOutcome::Ignore)
    }

    fn accepts_intent(&self, intent: &str, _ctx: &dyn ViewContext) -> bool {
        !self.items.is_empty() && CursorMove::of(intent).is_some()
    }

    fn on_intent(&mut self, intent: &str, ctx: &mut dyn Context) -> Result<EventOutcome> {
        match CursorMove::of(intent) {
            Some(CursorMove::By(delta)) => self.select_by(ctx, delta)?,
            Some(CursorMove::Page(delta)) => self.page(ctx, delta)?,
            Some(CursorMove::First) => self.select_first(ctx)?,
            Some(CursorMove::Last) => self.select_last(ctx)?,
            None => return Ok(EventOutcome::Ignore),
        }
        Ok(EventOutcome::Handle)
    }

    fn semantics(&self, _ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("selector".into()),
            label: self.label.clone(),
            value: self.chosen().map(|item| item.label().to_owned()),
            ..WidgetSemantics::default()
        })
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();
        let selection = roles::selection(ctx.is_focused());
        rndr.push_layer("selector");

        for (row, idx) in label_rows(self.items.len(), view.scroll.y, rect.h) {
            let item = &self.items[idx];
            let line_rect = rect.line(row)?;
            let label = item.label();
            let is_chosen = self.chosen == Some(idx);
            let is_selected = Some(idx) == self.cursor.index();

            let prefix = if is_chosen {
                self.glyphs.1
            } else {
                self.glyphs.0
            };
            let display = format!("{}{}", prefix, label);
            let (display, _) =
                text::slice_by_columns(&display, view.scroll.x as usize, rect.w as usize);

            if is_selected {
                rndr.fill(selection, line_rect.rect(), ' ')?;
                rndr.text(selection, line_rect, display)?;
            } else if is_chosen {
                rndr.text("chosen", line_rect, display)?;
            } else {
                rndr.text(roles::TEXT, line_rect, display)?;
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
    use canopy::CanopyBuilder;

    use super::*;

    /// Three string items.
    fn items() -> Vec<String> {
        ["Option 1", "Option 2", "Option 3"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn a_new_selector_selects_the_first_item_and_chooses_nothing() {
        let selector = Selector::new(items());
        assert_eq!(selector.selected_index(), 0);
        assert_eq!(selector.chosen_index(), None);
        assert!(selector.invariant_holds());
    }

    #[test]
    fn choose_takes_the_selection_and_replaces_the_choice() -> Result<()> {
        let mut selector = Selector::new(items());
        CanopyBuilder::new().build()?.with_root_context(|ctx| {
            selector.choose(ctx)?;
            assert_eq!(selector.chosen_index(), Some(0));
            selector.select_by(ctx, 2)?;
            assert_eq!(selector.chosen_index(), Some(0), "moving never chooses");
            selector.choose(ctx)?;
            assert_eq!(selector.chosen(), Some(&"Option 3".to_string()));
            selector.select_by(ctx, 99)?;
            assert_eq!(selector.selected_index(), 2);
            selector.clear_choice(ctx)?;
            assert_eq!(selector.chosen_index(), None);
            selector.select_by(ctx, -99)?;
            assert_eq!(selector.selected_index(), 0);
            assert!(selector.invariant_holds());
            Ok(())
        })
    }

    #[test]
    fn content_width_measures_the_choice_glyphs() {
        let items = || vec!["ab".to_string(), "abcd".to_string()];
        assert_eq!(Selector::new(items()).content_size(), Size::new(8, 2));
        assert_eq!(
            Selector::new(items()).with_glyphs("", "✓ ").content_size(),
            Size::new(6, 2),
            "the wider glyph sets the width"
        );
        assert_eq!(
            Selector::new(items())
                .with_glyphs("【 】", "【x】")
                .content_size(),
            Size::new(9, 2),
            "wide glyphs count their display columns"
        );
        assert_eq!(
            Selector::new(vec!["日本".to_string()]).content_size(),
            Size::new(8, 1),
            "labels count display columns"
        );
    }

    #[test]
    fn show_replaces_items_and_the_choice() {
        let mut selector = Selector::new(vec!["a".to_string(), "b".to_string()]);
        selector.set_items(
            vec!["x".to_string(), "y".to_string(), "z".to_string()],
            Some(2),
        );
        assert_eq!(selector.chosen_index(), Some(2));
        assert_eq!(
            selector.selected_index(),
            2,
            "the selection starts on the choice"
        );
        selector.set_items(vec!["x".to_string()], Some(9));
        assert_eq!(
            selector.chosen_index(),
            None,
            "an out-of-range choice drops"
        );
        assert_eq!(selector.selected_index(), 0);
        assert!(selector.invariant_holds());
    }

    #[test]
    fn glyphs_and_semantics_report_the_choice() -> Result<()> {
        let mut selector = Selector::new(vec!["Size".to_string(), "Modified".to_string()])
            .with_label("Sort")
            .with_glyphs("· ", "✓ ");
        assert_eq!(selector.glyphs, ("· ", "✓ "));
        selector.set_items(vec!["Size".to_string(), "Modified".to_string()], Some(1));
        let semantics = CanopyBuilder::new()
            .build()?
            .with_root_view(|ctx| selector.semantics(ctx))?;
        assert_eq!(semantics.role.as_deref(), Some("selector"));
        assert_eq!(semantics.label.as_deref(), Some("Sort"));
        assert_eq!(semantics.value.as_deref(), Some("Modified"));
        Ok(())
    }

    #[test]
    fn empty_selector_invariants_hold_after_commands() -> Result<()> {
        let mut selector = Selector::<String>::new(Vec::new());
        CanopyBuilder::new().build()?.with_root_context(|ctx| {
            selector.choose(ctx)?;
            selector.select_by(ctx, 1)?;
            selector.select_first(ctx)?;
            selector.select_last(ctx)?;
            selector.clear_choice(ctx)
        })?;

        assert!(selector.invariant_holds());
        assert_eq!(selector.cursor.index(), None);
        assert_eq!(selector.selected_index(), 0, "an empty selector reports 0");
        assert_eq!(selector.chosen_index(), None);
        Ok(())
    }
}
