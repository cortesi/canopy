//! Dropdown widget for single-value selection with expand/collapse behavior.

use canopy::{
    Context, EventOutcome, NodeName, Register, Setup, ViewContext, Widget, derive_commands,
    error::{Error, Result},
    geom::Size,
    input::{Event, mouse},
    layout::{MeasureConstraints, Measurement},
    render::Render,
    style::roles,
    text,
};

use crate::{
    label::ItemLabel,
    row_cursor::{CursorMove, RowCursor, is_primary_click, label_rows, widest_label},
};

/// Whether a dropdown shows its options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    /// The dropdown shows the selected item.
    Collapsed,
    /// The dropdown shows every option, with a highlight to navigate before
    /// a confirm.
    Expanded(RowCursor),
}

/// A dropdown widget for single-value selection.
///
/// When collapsed, displays the currently selected item with a dropdown
/// indicator. When expanded, displays all options for selection.
/// Opening and navigation scroll the highlighted row into view after layout.
pub struct Dropdown<T>
where
    T: ItemLabel,
{
    /// Available items.
    items: Vec<T>,
    /// Currently selected index.
    selected: usize,
    /// Whether the dropdown is expanded, with its highlight.
    open: Open,
}

#[derive_commands]
impl<T> Dropdown<T>
where
    T: ItemLabel + 'static,
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
            open: Open::Collapsed,
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

    /// Return whether the dropdown shows its options.
    fn expanded(&self) -> bool {
        matches!(self.open, Open::Expanded(_))
    }

    /// Toggle the dropdown expanded state. Collapsing keeps the selection,
    /// as [`Self::cancel`] does.
    #[command]
    pub fn toggle(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.expanded() {
            return self.cancel();
        }
        let mut cursor = RowCursor::new(self.items.len());
        cursor.set_index(self.selected);
        cursor.reveal(c);
        self.open = Open::Expanded(cursor);
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Move highlight by a signed offset (when expanded).
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let Open::Expanded(cursor) = &mut self.open else {
            return Ok(());
        };
        cursor.select_by(delta);
        cursor.reveal(c);
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Move the highlight to the first item (when expanded).
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_by(c, i32::MIN)
    }

    /// Move the highlight to the last item (when expanded).
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) -> Result<()> {
        self.select_by(c, i32::MAX)
    }

    /// Move the highlight by whole pages (when expanded).
    /// @param delta Negative values move up; positive values move down.
    #[command]
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let Open::Expanded(cursor) = &mut self.open else {
            return Ok(());
        };
        let rows = c.view().view_rect().h;
        cursor.page(delta, rows);
        cursor.reveal(c);
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Confirm the highlighted selection and collapse.
    #[command]
    pub fn confirm(&mut self) -> Result<()> {
        if let Open::Expanded(cursor) = self.open {
            self.selected = cursor.index().unwrap_or(self.selected);
            self.open = Open::Collapsed;
        }
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Confirm the clicked row when expanded, or expand when collapsed.
    fn handle_click(&mut self, c: &mut dyn Context, event: mouse::MouseEvent) -> Result<()> {
        if !is_primary_click(event) {
            return Ok(());
        }
        let Open::Expanded(cursor) = &mut self.open else {
            return self.toggle(c);
        };
        if let Some(row) = cursor.row_at(&c.view(), event.location) {
            cursor.set_index(row);
            self.confirm()?;
        }
        Ok(())
    }

    /// Collapse without changing selection.
    #[command]
    pub fn cancel(&mut self) -> Result<()> {
        self.open = Open::Collapsed;
        debug_assert!(self.selection_invariant_holds());
        Ok(())
    }

    /// Return the unclamped size required to render the current dropdown state.
    fn content_size(&self) -> Size {
        let max_label_width = widest_label(self.items.iter().map(ItemLabel::label));
        let width = max_label_width.saturating_add(2);
        let height = if self.expanded() {
            u32::try_from(self.items.len()).unwrap_or(u32::MAX)
        } else {
            1
        };

        Size::new(width, height)
    }

    /// Return whether selection and highlight indices point at current items.
    fn selection_invariant_holds(&self) -> bool {
        let highlight = match self.open {
            Open::Collapsed => true,
            Open::Expanded(cursor) => cursor.index().is_some_and(|index| index < self.items.len()),
        };
        !self.items.is_empty() && self.selected < self.items.len() && highlight
    }
}

impl<T> Register for Dropdown<T>
where
    T: ItemLabel + 'static,
{
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()
    }
}

impl<T> Widget for Dropdown<T>
where
    T: ItemLabel + 'static,
{
    fn accepts_intent(&self, intent: &str, _ctx: &dyn ViewContext) -> bool {
        self.expanded() && CursorMove::of(intent).is_some()
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

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
            self.handle_click(ctx, *mouse_event)?;
        }
        // Ignore so mouse bindings can also fire.
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("dropdown");
        let view = ctx.view();
        let selection = roles::selection(ctx.is_focused());
        let rect = view.view_rect_local();

        if let Open::Expanded(cursor) = self.open {
            // Render visible items
            for (row, idx) in label_rows(self.items.len(), view.scroll.y, rect.h) {
                let item = &self.items[idx];
                let line_rect = rect.line(row)?;
                let (label, _) =
                    text::slice_by_columns(item.label(), view.scroll.x as usize, rect.w as usize);

                if Some(idx) == cursor.index() {
                    rndr.fill(selection, line_rect.rect(), ' ')?;
                    rndr.text(selection, line_rect, label)?;
                } else if idx == self.selected {
                    rndr.text("chosen", line_rect, label)?;
                } else {
                    rndr.text(roles::TEXT, line_rect, label)?;
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
            rndr.text(roles::TEXT, rect.line(0)?, display)?;
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
    use canopy::testing::harness::Harness;

    use super::*;

    #[test]
    fn test_dropdown_creation() {
        let items = vec!["Option 1".to_string(), "Option 2".to_string()];
        let dropdown = Dropdown::new(items).expect("nonempty dropdown");
        assert_eq!(dropdown.selected_index(), 0);
        assert!(!dropdown.expanded());
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
        assert_eq!(dropdown.selected_index(), 1);
        assert_eq!(dropdown.selected().label(), "Option 2");
    }

    #[test]
    fn open_transitions() -> Result<()> {
        /// An event of the dropdown.
        #[derive(Clone, Copy, Debug)]
        enum Step {
            Toggle,
            Down,
            Confirm,
            Cancel,
        }
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut harness = Harness::builder(Dropdown::new(items)?)
            .register::<Dropdown<String>>()
            .size(20, 6)
            .build()?;
        // From collapsed on 1, and from expanded on 1 with the highlight on
        // 2: the next highlight, or none when collapsed, and the selection.
        for (expanded, step, highlight, selected) in [
            (false, Step::Toggle, Some(1), 1),
            (false, Step::Down, None, 1),
            (false, Step::Confirm, None, 1),
            (false, Step::Cancel, None, 1),
            (true, Step::Toggle, None, 1),
            (true, Step::Down, Some(2), 1),
            (true, Step::Confirm, None, 2),
            (true, Step::Cancel, None, 1),
        ] {
            harness.with_root_widget_context(|dropdown: &mut Dropdown<String>, ctx| {
                dropdown.open = Open::Collapsed;
                dropdown.selected = 1;
                if expanded {
                    dropdown.toggle(ctx)?;
                    dropdown.select_by(ctx, 1)?;
                }
                match step {
                    Step::Toggle => dropdown.toggle(ctx)?,
                    Step::Down => dropdown.select_by(ctx, 1)?,
                    Step::Confirm => dropdown.confirm()?,
                    Step::Cancel => dropdown.cancel()?,
                }
                let shown = match dropdown.open {
                    Open::Collapsed => None,
                    Open::Expanded(cursor) => cursor.index(),
                };
                let from = if expanded { "expanded" } else { "collapsed" };
                assert_eq!(shown, highlight, "{step:?} from {from}");
                assert_eq!(dropdown.selected_index(), selected, "{step:?} from {from}");
                assert!(dropdown.selection_invariant_holds());
                Ok(())
            })?;
        }
        Ok(())
    }

    /// A left or right press at `(x, y)` of the screen.
    fn press(button: mouse::Button, x: i32, y: i32) -> mouse::MouseEvent {
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button,
            modifiers: canopy::input::key::Empty,
            location: canopy::geom::PointI32 { x, y },
        }
    }

    #[test]
    fn a_click_expands_and_a_click_on_a_row_confirms_it() -> Result<()> {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut harness = Harness::builder(Dropdown::new(items)?)
            .register::<Dropdown<String>>()
            .size(20, 6)
            .build()?;
        let state = |harness: &mut Harness| {
            harness.with_root_widget(|dropdown: &mut Dropdown<String>| {
                (dropdown.expanded(), dropdown.selected_index())
            })
        };
        harness.mouse(press(mouse::Button::Right, 0, 0))?;
        assert_eq!(
            state(&mut harness),
            (false, 0),
            "a right press does nothing"
        );
        harness.mouse(press(mouse::Button::Left, 0, 0))?;
        assert_eq!(state(&mut harness), (true, 0), "a click expands");
        harness.render()?;
        harness.mouse(press(mouse::Button::Left, 0, 5))?;
        assert_eq!(state(&mut harness), (true, 0), "a click below the rows");
        harness.mouse(press(mouse::Button::Left, 0, 2))?;
        assert_eq!(state(&mut harness), (false, 2), "a click confirms its row");
        Ok(())
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
            assert!(!dropdown.expanded());
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
            dropdown.toggle(ctx)?;
            dropdown.select_by(ctx, 99)?;
            dropdown.confirm()?;
            assert_eq!(dropdown.selected_index(), 2);
            assert!(!dropdown.expanded());
            assert!(dropdown.selection_invariant_holds());

            dropdown.toggle(ctx)?;
            dropdown.select_by(ctx, -99)?;
            dropdown.cancel()?;
            assert_eq!(dropdown.selected_index(), 2);
            assert!(!dropdown.expanded());
            assert!(dropdown.selection_invariant_holds());
            Ok(())
        })
    }
}
