//! The slot: Canopy's `Editor` in place of the content rows of the marked
//! unit, between the rows of the presentation above and below it.
//!
//! While the slot is open, the reader's body shows a column of three parts in
//! place of its rendered surface. The parts above and below paint rows of the
//! presentation, and do not scroll. The slot holds the editor, with a bar in
//! the margin beside it. The column lies outside the paths of the reader's own
//! bindings, so the reader's keys never reach the editor.

use std::{cell::RefCell, iter, ops::Range, rc::Rc};

use canopy::{
    Context, ContextExt, NodeId, NodeName, TypedId, ViewContext, ViewContextExt, Widget,
    commands::CommandTarget,
    error::Result,
    geom::Rect,
    layout::{Edges, Layout, RevealAlign},
    render::Render,
};
#[cfg(feature = "syntax")]
use canopy_widgets::highlight::SyntectHighlighter;
use canopy_widgets::{
    Answer, Closing, Confirm, ConfirmRequest, Container,
    editor::{Editor, EditorConfig},
};

use super::{BlockEdit, Markdown, invalid};
use crate::{
    presentation::PaintOverlays,
    source_store::Lifecycle,
    surface::{PendingMark, RenderedSurface, SurfaceState},
    units::{EditUnit, line_terminator, unit_at},
};

/// Flex weight of the rows above the slot, far above the weight of the rows
/// below, so the rows above take all the free rows up to their limit, and the
/// rows below take the rest.
const ABOVE_WEIGHT: u32 = 1_000_000;

/// An open slot and the edit in it.
pub(super) struct OpenSlot {
    /// The column that shows in place of the rendered surface.
    column: NodeId,
    /// The rows above the slot.
    above: NodeId,
    /// The slot: the bar and the editor.
    #[cfg_attr(not(test), expect(dead_code, reason = "tests read the parts' heights"))]
    middle: NodeId,
    /// The margin of the slot, with its bar.
    bar: NodeId,
    /// The rows below the slot.
    #[cfg_attr(not(test), expect(dead_code, reason = "tests read the parts' heights"))]
    below: NodeId,
    /// The editor in the slot.
    pub(super) editor: TypedId<Editor>,
    /// The epoch of the text that `range` addresses.
    pub(super) epoch: u64,
    /// The source bytes the edit replaces: the marked unit, or an empty
    /// range at the end of a text with no unit.
    pub(super) range: Range<usize>,
    /// Text a commit puts before the editor's text: the line breaks that
    /// start a new block at the end of the text.
    lead: String,
    /// The line terminator of the unit, which new lines take.
    terminator: &'static str,
    /// The text of `range` when the slot opened.
    original: String,
    /// The text that a discard returns to: the last checkpoint.
    kept: String,
    /// The screen row of the unit's first row when the slot opened, which
    /// the slot keeps when it fits.
    pub(super) row: u32,
    /// Whether a commit's text waits for its presentation, while the slot
    /// still shows it.
    pub(super) committed: bool,
}

impl OpenSlot {
    /// Return the text of the editor with its line breaks in the unit's
    /// terminator.
    fn draft(&self, editor: &Editor) -> String {
        in_terminator(&editor.text(), self.terminator)
    }

    /// Return the edit that puts `text` in place of the range.
    fn edit(&self, text: &str) -> BlockEdit {
        BlockEdit {
            epoch: self.epoch,
            range: self.range.clone(),
            text: if text.is_empty() {
                String::new()
            } else {
                format!("{}{text}", self.lead)
            },
        }
    }
}

/// Return `text` with each of its line breaks in `terminator`. The editor adds
/// lines with LF, and a CRLF unit keeps its own breaks.
fn in_terminator(text: &str, terminator: &str) -> String {
    if terminator == "\n" {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                out.push_str(terminator);
            }
            '\n' | '\r' => out.push_str(terminator),
            c => out.push(c),
        }
    }
    out
}

/// Which rows of the presentation a part shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// The rows that end where the slot's rows start.
    Above,
    /// The rows that start where the slot's rows end.
    Below,
}

/// Return the rows of the presentation that the slot takes the place of:
/// the content rows of the marked unit, or the end of a text with no unit.
fn slot_rows(state: &mut SurfaceState) -> Option<Range<u32>> {
    let height = state.presentation.as_ref()?.height();
    Some(state.marked_unit().map_or(height..height, |(_, rows)| rows))
}

/// Rows of the presentation above or below the slot. They do not scroll.
struct EditRows {
    /// Which rows this part shows.
    part: Part,
    /// The reader's shared state, which holds the presentation.
    shared: Rc<RefCell<SurfaceState>>,
}

impl Widget for EditRows {
    fn name(&self) -> NodeName {
        NodeName::convert(match self.part {
            Part::Above => "markdown_edit_above",
            Part::Below => "markdown_edit_below",
        })
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        render.fill("markdown/text", view.outer_rect_local(), ' ')?;
        let mut state = self.shared.borrow_mut();
        let Some(rows) = slot_rows(&mut state) else {
            return Ok(());
        };
        let Some(presentation) = state.presentation.clone() else {
            return Ok(());
        };
        let height = view.content.h;
        let first = match self.part {
            Part::Above => rows.start.saturating_sub(height),
            Part::Below => rows.end,
        };
        // The images of image blocks are the only overlays the parts show.
        let overlays = PaintOverlays {
            images: state.overlays.images.clone(),
            ..PaintOverlays::default()
        };
        drop(state);
        presentation.paint(
            render,
            Rect::new(0, first, view.content.w, height),
            &overlays,
        )?;
        Ok(())
    }
}

/// The margin beside the editor, with the bar that marks the rows of the
/// slot.
struct SlotBar {
    /// The reader's shared state, which holds the presentation.
    shared: Rc<RefCell<SurfaceState>>,
}

impl Widget for SlotBar {
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_slot_bar")
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        render.fill("markdown/text", view.outer_rect_local(), ' ')?;
        let column = self
            .shared
            .borrow()
            .presentation
            .as_ref()
            .map_or(0, |p| p.bar_column());
        render.fill(
            "markdown/block/editing",
            Rect::new(column, 0, 1, view.content.h),
            '▌',
        )
    }
}

/// The column that shows in place of the rendered surface while the slot is
/// open.
struct EditColumn;

impl Widget for EditColumn {
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_edit")
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        // The parts leave the editor's padding unpainted. The fill keeps the
        // surface beneath from showing through it, although the surface
        // paints nothing there while the slot is open.
        render.fill("markdown/text", ctx.view().outer_rect_local(), ' ')
    }
}

/// Return the layout of the rows above the slot: all the free rows, up to
/// `limit`.
fn above_layout(limit: u32) -> Layout {
    Layout::fill().flex_vertical(ABOVE_WEIGHT).max_height(limit)
}

/// Return the configuration of the slot's editor: the host's, growing with
/// its text, so the slot takes the height of the text.
fn slot_config(config: &EditorConfig) -> EditorConfig {
    config.clone().with_auto_grow(true)
}

/// Return the line breaks that start a new block after `source`.
fn new_block_lead(source: &str, terminator: &str) -> String {
    if source.is_empty() || source.ends_with(&format!("{terminator}{terminator}")) {
        String::new()
    } else if source.ends_with(['\n', '\r']) {
        terminator.to_owned()
    } else {
        terminator.repeat(2)
    }
}

/// Return the byte, in the unit's text, where the editor opens: the start of
/// the unit's own text, or of the unit. In a tall unit whose first row is
/// above the view, the byte of the first row that shows, when the reader maps
/// it exactly, or else the start of the line at the same fraction of the
/// unit.
fn opening_byte(
    source: &str,
    unit: &EditUnit,
    rows: &Range<u32>,
    top: u32,
    exact: Option<usize>,
) -> usize {
    let start = unit.own.as_ref().map_or(unit.range.start, |own| own.start);
    if rows.start >= top || rows.is_empty() {
        return start - unit.range.start;
    }
    if let Some(byte) = exact.filter(|byte| unit.range.contains(byte)) {
        return byte - unit.range.start;
    }
    let text = &source[unit.range.clone()];
    let lines: Vec<usize> = iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let shown = u64::from(top - rows.start);
    let line = (shown * lines.len() as u64 / u64::from(rows.end - rows.start)) as usize;
    lines[line.min(lines.len() - 1)]
}

/// Open the slot on the marked unit, or, with no unit, an empty one at the
/// end of the text.
pub(super) fn open(reader: &mut Markdown, ctx: &mut dyn Context) -> Result<()> {
    // Without a block editor in its options, the reader cannot edit, and
    // its `e` does nothing.
    let Some(config) = reader.options.block_editor.clone() else {
        return Ok(());
    };
    // The slot opens over the rendered view alone, once at a time.
    if reader.locked || reader.inspector.is_some() || !reader.rendered_actions_available() {
        return Ok(());
    }
    let snapshot = reader.store.snapshot();
    if !matches!(snapshot.lifecycle, Lifecycle::Complete(_)) {
        return Err(invalid(
            "A document that is still arriving cannot be edited",
        ));
    }
    let Some(nodes) = reader.nodes.as_ref() else {
        return Ok(());
    };
    let (body, rendered, search) = (nodes.body, nodes.rendered, nodes.search);
    // The units must be those of the current text, or the edit's range would
    // address another text. A new text that has not shown leaves the rendered
    // actions unavailable too, and an edit's text sets `awaiting`.
    {
        let state = reader.shared.borrow();
        let shown = state
            .presentation
            .as_ref()
            .is_some_and(|p| p.document.source.epoch == snapshot.epoch);
        if !shown || state.awaiting {
            return Ok(());
        }
    }
    // The block cursor marks the unit to edit.
    if reader.shared.borrow().mode != crate::Mode::Interact {
        ctx.with_widget_mut(rendered, |surface: &mut RenderedSurface, ctx| {
            surface.enter_interact(ctx)
        })?;
    }
    let top = ctx.view_of(rendered).map_or(0, |view| view.scroll.y);
    let source = snapshot.text();
    let (range, lead, cursor, row, terminator, tall) = {
        let mut state = reader.shared.borrow_mut();
        let presentation = state.presentation.clone();
        match (state.marked_unit(), presentation) {
            (Some((unit, rows)), Some(p)) => {
                let exact = p.exact_source_at_row(top.max(rows.start));
                let cursor = opening_byte(&source, &unit, &rows, top, exact);
                let row = rows.start.saturating_sub(top);
                let terminator = line_terminator(&source, &unit.range);
                let tall = rows.start < top;
                (unit.range, String::new(), cursor, row, terminator, tall)
            }
            (_, presentation) => {
                let end = source.len();
                let terminator = line_terminator(&source, &(end..end));
                let row = presentation.map_or(0, |p| p.height().saturating_sub(top));
                let lead = new_block_lead(&source, terminator);
                (end..end, lead, 0, row, terminator, false)
            }
        }
    };
    let original = source[range.clone()].to_owned();
    // The reader's search does nothing while the slot is open, so its field
    // closes.
    if !reader.options.external_search {
        ctx.with_widget_mut(search, |bar: &mut canopy_widgets::SearchBar, ctx| {
            bar.close(ctx)
        })?;
    }
    let column = ctx.add_child(body, EditColumn)?;
    let above = ctx.add_child(
        column,
        EditRows {
            part: Part::Above,
            shared: reader.shared.clone(),
        },
    )?;
    let slot = ctx.add_child(
        column,
        Container::new(Layout::row()).with_name("markdown_slot"),
    )?;
    let bar = ctx.add_child(
        slot,
        SlotBar {
            shared: reader.shared.clone(),
        },
    )?;
    let editor = ctx.add_child(
        slot,
        Editor::with_config(original.clone(), slot_config(&config)),
    )?;
    #[cfg(feature = "syntax")]
    ctx.with_widget_mut(editor, |editor: &mut Editor, _| {
        editor.set_highlighter(Some(Box::new(
            SyntectHighlighter::new("md").with_palette_roles(),
        )));
        Ok(())
    })?;
    ctx.set_layout_override(
        editor.into(),
        Layout::column()
            .flex_horizontal(1)
            .padding(Edges::new(0, 1, 0, 0))
            .into(),
    )?;
    let below = ctx.add_child(
        column,
        EditRows {
            part: Part::Below,
            shared: reader.shared.clone(),
        },
    )?;
    // The column covers the rendered surface, which keeps its place: a
    // hidden view would lose its scroll.
    reader.slot = Some(OpenSlot {
        column: column.into(),
        above: above.into(),
        middle: slot.into(),
        bar: bar.into(),
        below: below.into(),
        editor,
        epoch: snapshot.epoch,
        range,
        lead,
        terminator,
        original: original.clone(),
        kept: original,
        row,
        committed: false,
    });
    layout_parts(reader, ctx)?;
    ctx.with_widget_mut(editor, |editor: &mut Editor, ctx| {
        let position = editor.position_of_offset(cursor);
        editor.set_cursor(ctx, position);
        // A tall unit's slot is at the top of the view, where the reader
        // showed the line the editor opens on.
        if tall {
            ctx.reveal_anchor(RevealAlign::Top(0));
        }
        Ok(())
    })?;
    ctx.set_focus(editor.into())?;
    reader.update_chrome(ctx)
}

/// Lay out the parts for the current presentation: fit the rows above the
/// slot to the slot's screen row and to the rows above the unit, and the
/// slot's margin to the presentation's.
pub(super) fn layout_parts(reader: &Markdown, ctx: &mut dyn Context) -> Result<()> {
    let Some(slot) = reader.slot.as_ref() else {
        return Ok(());
    };
    let (above, bar, row) = (slot.above, slot.bar, slot.row);
    let (rows, inset) = {
        let mut state = reader.shared.borrow_mut();
        let inset = state.presentation.as_ref().map_or(0, |p| p.inset());
        (slot_rows(&mut state).unwrap_or(0..0), inset)
    };
    ctx.set_layout_override(above, above_layout(row.min(rows.start)).into())?;
    ctx.set_layout_override(bar, Layout::fill().fixed_width(inset).into())?;
    ctx.set_hidden(bar, inset == 0)?;
    Ok(())
}

/// Close the slot and show the rendered surface again.
pub(super) fn close(reader: &mut Markdown, ctx: &mut dyn Context) -> Result<()> {
    let Some(slot) = reader.slot.take() else {
        return Ok(());
    };
    let focused = ctx.is_on_focus_path(slot.column);
    ctx.remove_subtree(slot.column)?;
    if focused && let Some(nodes) = reader.nodes.as_ref() {
        ctx.set_focus(nodes.rendered)?;
    }
    reader.update_chrome(ctx)
}

/// Commit the slot: record its text as an edit when it differs from the text
/// before the edit, and otherwise close it.
pub(super) fn commit(reader: &mut Markdown, ctx: &mut dyn Context) -> Result<()> {
    let target = match reader.slot.as_ref() {
        Some(slot) if !slot.committed && !reader.locked => {
            ctx.with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))?
        }
        _ => return Ok(()),
    };
    finish(reader, ctx, &target)
}

/// Discard the changes since the last checkpoint and close the slot. With
/// changes, ask first unless `confirmed`.
pub(super) fn discard(reader: &mut Markdown, ctx: &mut dyn Context, confirmed: bool) -> Result<()> {
    let (kept, changed) = match reader.slot.as_ref() {
        Some(slot) if !slot.committed && !reader.locked => {
            let draft = ctx.with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))?;
            (slot.kept.clone(), draft != slot.kept)
        }
        _ => return Ok(()),
    };
    if changed && !confirmed {
        return ask_discard(reader, ctx);
    }
    finish(reader, ctx, &kept)
}

/// End the slot with `target` as its text: an edit when it differs from the
/// text before the edit, which keeps the slot on screen until its
/// presentation shows, or else a close.
fn finish(reader: &mut Markdown, ctx: &mut dyn Context, target: &str) -> Result<()> {
    let Some(slot) = reader.slot.as_ref() else {
        return Ok(());
    };
    if target == slot.original {
        return close(reader, ctx);
    }
    let edit = slot.edit(target);
    // The slot's screen row now, which it left to fit its text.
    let row = ctx
        .view_of(slot.above)
        .map_or(slot.row, |view| view.outer.h);
    reader.edit_text(ctx, edit, Some(row))?;
    if let Some(slot) = reader.slot.as_mut() {
        slot.committed = true;
    }
    reader.update_chrome(ctx)
}

/// Mark the editor's text as kept: a later discard returns to it.
pub(super) fn checkpoint(reader: &mut Markdown, ctx: &dyn ViewContext) -> Result<()> {
    let Some(slot) = reader.slot.as_ref().filter(|slot| !slot.committed) else {
        return Ok(());
    };
    let draft = ctx.with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))?;
    if let Some(slot) = reader.slot.as_mut() {
        slot.kept = draft;
    }
    Ok(())
}

/// Return the edit that a commit of the slot would make now, with the
/// editor's text in place of its range.
pub(super) fn draft(reader: &Markdown, ctx: &dyn ViewContext) -> Option<BlockEdit> {
    let slot = reader.slot.as_ref().filter(|slot| !slot.committed)?;
    let draft = ctx
        .with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))
        .ok()?;
    Some(slot.edit(&draft))
}

/// Ask whether to drop the changes in the slot.
fn ask_discard(reader: &mut Markdown, ctx: &mut dyn Context) -> Result<()> {
    let owner = ctx.node_id();
    let Some(layers) = reader.nodes.as_ref().map(|nodes| nodes.layers) else {
        return Ok(());
    };
    let confirm = match reader.confirm {
        Some(confirm) => confirm,
        None => {
            let confirm = ctx.add_child(layers, Confirm::new())?;
            reader.confirm = Some(confirm);
            confirm
        }
    };
    if ctx.with_widget(confirm, |dialog: &Confirm| Ok(dialog.is_open(ctx)))? {
        return Ok(());
    }
    let target = CommandTarget::Exact(owner);
    let request = ConfirmRequest::new(
        "Discard changes",
        "Drop the changes to this block since it was last kept?",
    )
    .answer(Answer::new(
        "discard",
        Markdown::call_discard_block(true).with_target(target),
    ))
    .declining(Answer::new(
        "keep editing",
        Markdown::call_focus_block().with_target(target),
    ))
    .closing(Closing::BeforeCall);
    Confirm::open(ctx, confirm, owner, request)?;
    Ok(())
}

/// Return the edit transition's mark for an edit that starts at `byte`, on
/// screen row `row`.
pub(super) fn pending_mark(
    state: &SurfaceState,
    epoch: u64,
    byte: usize,
    row: Option<u32>,
) -> PendingMark {
    PendingMark {
        epoch,
        byte,
        row,
        mode: state.mode,
    }
}

/// Return the screen row of the first row of the unit that holds `byte`, if
/// the unit shows.
pub(super) fn unit_row(
    state: &mut SurfaceState,
    byte: usize,
    top: u32,
    height: u32,
) -> Option<u32> {
    let (units, rows) = state.cursor_map()?;
    let rows = rows.get(unit_at(&units, byte)?)?.clone();
    screen_row(&rows, top, height)
}

/// Return the screen row of the first of `rows` in a view of `height` rows
/// from `top`, if any of them shows. Rows that start above the view give the
/// top row, as the slot opens there on such a unit.
fn screen_row(rows: &Range<u32>, top: u32, height: u32) -> Option<u32> {
    let shows = rows.start < top.saturating_add(height) && (rows.start >= top || rows.end > top);
    shows.then(|| rows.start.saturating_sub(top))
}

/// Return the heights of the rows above the slot, the slot, and the rows
/// below it, as the last layout gave them.
#[cfg(test)]
pub(super) fn parts(reader: &Markdown, ctx: &dyn ViewContext) -> Option<(u32, u32, u32)> {
    let slot = reader.slot.as_ref()?;
    let height = |node: NodeId| ctx.view_of(node).map_or(0, |view| view.outer.h);
    Some((height(slot.above), height(slot.middle), height(slot.below)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::parse_units;

    #[test]
    fn a_new_block_starts_after_a_blank_line() {
        assert_eq!(new_block_lead("", "\n"), "");
        assert_eq!(new_block_lead("a", "\n"), "\n\n");
        assert_eq!(new_block_lead("a\n", "\n"), "\n");
        assert_eq!(new_block_lead("a\n\n", "\n"), "");
        assert_eq!(new_block_lead("a\r\n", "\r\n"), "\r\n");
    }

    #[test]
    fn every_break_takes_the_units_terminator() {
        assert_eq!(in_terminator("a\nb\rc", "\n"), "a\nb\rc");
        assert_eq!(
            in_terminator("a\nb\r\nc\rd\r", "\r\n"),
            "a\r\nb\r\nc\r\nd\r\n"
        );
    }

    #[test]
    fn a_tall_unit_opens_on_its_exact_row_or_at_the_same_fraction() {
        let source = "intro\n\n```\nl1\nl2\nl3\nl4\n```\n";
        let unit = parse_units(source).pop().unwrap();
        let start = unit.range.start;
        assert_eq!(&source[unit.range.clone()], "```\nl1\nl2\nl3\nl4\n```");
        // The unit's lines start at 0, 4, 7, 10, 13, and 16, and its six
        // rows are 2..8. A unit that starts in view opens at its own text.
        let rows = 2..8;
        assert_eq!(opening_byte(source, &unit, &rows, 2, Some(start + 9)), 4);
        // An exact row in the unit opens there.
        assert_eq!(opening_byte(source, &unit, &rows, 5, Some(start + 7)), 7);
        // Otherwise, three rows down six opens three lines down six, as it
        // does for an exact byte outside the unit.
        assert_eq!(opening_byte(source, &unit, &rows, 5, None), 10);
        assert_eq!(opening_byte(source, &unit, &rows, 5, Some(0)), 10);
        assert_eq!(opening_byte(source, &unit, &(2..14), 5, None), 4);
        // Rows past the unit open on its last line.
        assert_eq!(opening_byte(source, &unit, &(2..4), 10, None), 16);
    }

    #[test]
    fn a_units_screen_row_is_its_first_row_in_view() {
        assert_eq!(screen_row(&(3..5), 2, 10), Some(1));
        assert_eq!(screen_row(&(2..2), 2, 10), Some(0));
        assert_eq!(screen_row(&(11..14), 2, 10), Some(9));
        // A unit that starts above the view and ends in it takes the top row.
        assert_eq!(screen_row(&(0..3), 2, 10), Some(0));
        // A unit wholly above or below the view has no row.
        assert_eq!(screen_row(&(0..2), 2, 10), None);
        assert_eq!(screen_row(&(12..14), 2, 10), None);
        assert_eq!(screen_row(&(20..22), 2, 10), None);
    }
}
