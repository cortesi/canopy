//! The slot: Canopy's `Editor` in place of the content rows of the marked
//! unit, between the rows of the presentation above and below it.
//!
//! While the slot is open, the reader's body shows a column of three parts in
//! place of its rendered surface. The parts above and below paint rows of the
//! presentation, and do not scroll. The slot holds the editor, with a bar in
//! the margin beside it. The column lies outside the paths of the reader's own
//! bindings, so the reader's keys never reach the editor.

use std::{cell::RefCell, iter, mem, ops::Range, rc::Rc};

use canopy::{
    Context, ContextExt, NodeId, NodeName, TypedId, ViewContext, ViewContextExt, Widget,
    commands::CommandTarget,
    error::Result,
    geom::Rect,
    layout::{Layout, RevealAlign},
    render::Render,
};
#[cfg(feature = "syntax")]
use canopy_widgets::highlight::SyntectHighlighter;
use canopy_widgets::{
    Answer, Closing, Confirm, ConfirmRequest, Container,
    editor::{Editor, EditorConfig, Interaction, ViMode, WrapMode},
};

use super::{BlockEdit, Markdown, invalid};
use crate::{
    presentation::PaintOverlays,
    source_store::Lifecycle,
    surface::{PendingMark, RenderedSurface, SurfaceState},
    units::{EditKind, EditUnit, NewBlock, delete_unit, line_terminator, unit_at},
};

/// Flex weight of the rows above the slot, far above the weight of the rows
/// below, so the rows above take all the free rows up to their limit, and the
/// rows below take the rest.
const ABOVE_WEIGHT: u32 = 1_000_000;

/// The block editor's slot: closed, open, or committed and waiting for its
/// text's layout.
#[derive(Default)]
pub(super) enum Edit {
    /// No slot.
    #[default]
    Idle,
    /// The slot is open, and its editor takes keys.
    Open(OpenSlot),
    /// The slot's text is an edit that waits for its layout. The editor
    /// still shows, but takes no keys: they reach the reader, whose keys
    /// that need the new layout wait for it.
    Committed(OpenSlot),
}

impl Edit {
    /// The slot, open or committed.
    pub(super) fn slot(&self) -> Option<&OpenSlot> {
        match self {
            Self::Open(slot) | Self::Committed(slot) => Some(slot),
            Self::Idle => None,
        }
    }

    /// The open slot, whose editor takes keys.
    pub(super) fn open(&self) -> Option<&OpenSlot> {
        match self {
            Self::Open(slot) => Some(slot),
            Self::Idle | Self::Committed(_) => None,
        }
    }

    /// Whether the slot is open, so its editor takes keys and the reader
    /// refuses its own commands.
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    /// Whether the slot shows in place of the rendered surface, open or
    /// committed.
    pub(super) fn shows(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// Whether the slot is committed and waits for its layout.
    pub(super) fn is_committed(&self) -> bool {
        matches!(self, Self::Committed(_))
    }

    /// Close the slot, and return it.
    fn take(&mut self) -> Option<OpenSlot> {
        match mem::take(self) {
            Self::Open(slot) | Self::Committed(slot) => Some(slot),
            Self::Idle => None,
        }
    }

    /// Commit the open slot, and return it.
    fn commit(&mut self) -> Option<&OpenSlot> {
        if let Self::Open(slot) = mem::take(self) {
            *self = Self::Committed(slot);
        }
        self.slot()
    }
}

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
    /// The start of the unit the slot opened on, or after, or `None` for a
    /// slot at the end of a text without units. The slot keeps its unit
    /// whatever the block cursor marks.
    unit: Option<usize>,
    /// The epoch of the text that `range` addresses.
    pub(super) epoch: u64,
    /// The source bytes the edit replaces: the marked unit, or the empty
    /// range of a new block.
    pub(super) range: Range<usize>,
    /// The new block that the editor's text makes, or `None` for an edit of
    /// the marked unit.
    pub(super) new_block: Option<NewBlock>,
    /// The line terminator of the unit, which new lines take.
    terminator: &'static str,
    /// Whether the unit is prose, which the editor wraps at the reader's
    /// prose width. Code, tables, and the other cards take the body width.
    prose: bool,
    /// The text of `range` when the slot opened.
    original: String,
    /// The text that a discard returns to: the last checkpoint.
    kept: String,
    /// The screen row of the unit's first row when the slot opened, which
    /// the slot keeps when it fits.
    pub(super) row: u32,
}

impl OpenSlot {
    /// Return the text of the editor with its line breaks in the unit's
    /// terminator.
    fn draft(&self, editor: &Editor) -> String {
        in_terminator(&editor.text(), self.terminator)
    }

    /// Return the edit that puts `text` in place of the range: the text
    /// itself, or the source of a new block of it.
    fn edit(&self, text: &str) -> BlockEdit {
        BlockEdit {
            epoch: self.epoch,
            range: self.range.clone(),
            text: self
                .new_block
                .as_ref()
                .map_or_else(|| text.to_owned(), |block| block.text(text)),
        }
    }

    /// Return the source byte where the first unit of an edit of `text`
    /// starts: the start of the range, or of a new block's first line.
    fn first_byte(&self, text: &str) -> usize {
        self.new_block
            .as_ref()
            .map_or(self.range.start, |block| block.start(text))
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

/// The rows of the presentation around the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SlotRows {
    /// The row after the last row above the slot.
    above: u32,
    /// The first row below the slot.
    below: u32,
}

/// Return the rows of the presentation around the slot of the unit that
/// starts at `unit`. An edit of the unit takes the place of its content
/// rows. A new block at source byte `new_at` lies between the last unit
/// that starts before it and the next unit, with the rows between them on
/// both sides, so it shows as a block of its own: right after the unit, or
/// after the nested blocks of a list item. With no unit, the slot lies at
/// the end of the text.
fn slot_rows(
    state: &mut SurfaceState,
    unit: Option<usize>,
    new_at: Option<usize>,
) -> Option<SlotRows> {
    let height = state.presentation.as_ref()?.height();
    let end = SlotRows {
        above: height,
        below: height,
    };
    let (units, all) = state.cursor_map()?;
    let Some(opened) = unit.and_then(|byte| unit_at(&units, byte)) else {
        return Some(end);
    };
    let Some(rows) = all.get(opened).cloned() else {
        return Some(end);
    };
    let Some(at) = new_at else {
        return Some(SlotRows {
            above: rows.start,
            below: rows.end,
        });
    };
    let index = units
        .iter()
        .rposition(|unit| unit.range.start < at)
        .unwrap_or(opened)
        .max(opened);
    let rows = all.get(index).cloned().unwrap_or(rows);
    let next = all
        .get(index + 1..)
        .and_then(|after| after.iter().find(|rows| !rows.is_empty()))
        .map_or(height, |rows| rows.start);
    Some(SlotRows {
        above: next.max(rows.end),
        below: rows.end,
    })
}

/// Rows of the presentation above or below the slot. They do not scroll.
struct EditRows {
    /// Which rows this part shows.
    part: Part,
    /// The start of the unit the slot opened on, or after.
    unit: Option<usize>,
    /// The source byte of the new block that the slot holds, or `None` for
    /// an edit of the unit.
    new_at: Option<usize>,
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
        let Some(rows) = slot_rows(&mut state, self.unit, self.new_at) else {
            return Ok(());
        };
        let Some(presentation) = state.presentation.clone() else {
            return Ok(());
        };
        let height = view.content.h;
        let first = match self.part {
            Part::Above => rows.above.saturating_sub(height),
            Part::Below => rows.below,
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
/// its text, so the slot takes the height of the text, and breaking rows
/// only after blanks, as the reader breaks them.
fn slot_config(config: &EditorConfig) -> EditorConfig {
    config
        .clone()
        .with_auto_grow(true)
        .with_wrap(WrapMode::Spaces)
}

/// Return whether a unit of `kind` is prose, which the reader wraps at its
/// prose width, rather than a card, which takes the body width.
fn is_prose(kind: EditKind) -> bool {
    !matches!(
        kind,
        EditKind::FencedCode
            | EditKind::IndentedCode
            | EditKind::Table
            | EditKind::Html
            | EditKind::FrontMatter
            | EditKind::Source
    )
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

/// What the slot opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opening {
    /// The marked unit.
    Unit,
    /// A new block after the marked unit.
    After,
}

/// Return whether the user can change a block now. The reader needs a block
/// editor in its options, no lock, no inspector or slot, and the layout of
/// its current text. A lock shows its notice.
fn editable(reader: &Markdown, ctx: &mut dyn Context) -> Result<bool> {
    // Without a block editor in its options, the reader cannot edit, and
    // its `e`, `a`, and `d` do nothing.
    if reader.options.block_editor.is_none() || reader.refused_by_lock(ctx)? {
        return Ok(false);
    }
    // A change works on the rendered view alone, once at a time.
    if reader.inspector.is_some() || reader.slot.shows() || !reader.rendered_actions_available() {
        return Ok(false);
    }
    let snapshot = reader.store.snapshot();
    if !matches!(snapshot.lifecycle, Lifecycle::Complete(_)) {
        return Err(invalid(
            "A document that is still arriving cannot be edited",
        ));
    }
    // The units must be those of the current text, or the edit's range would
    // address another text. A new text that has not shown leaves the rendered
    // actions unavailable too, and an edit's text sets `awaiting`.
    let state = reader.shared.borrow();
    let shown = state
        .presentation
        .as_ref()
        .is_some_and(|p| p.document.source.epoch == snapshot.epoch);
    Ok(reader.nodes.is_some() && shown && !state.awaiting())
}

/// Open the slot on the marked unit, or empty for a new block after it, as
/// `opening` says. With no unit, open it empty at the end of the text.
pub(super) fn open(reader: &mut Markdown, ctx: &mut dyn Context, opening: Opening) -> Result<()> {
    if !editable(reader, ctx)? {
        return Ok(());
    }
    let (Some(config), Some(nodes)) = (reader.options.block_editor.clone(), reader.nodes.as_ref())
    else {
        return Ok(());
    };
    let snapshot = reader.store.snapshot();
    let (body, rendered, search) = (nodes.body, nodes.rendered, nodes.search);
    // The block cursor marks the unit to edit.
    if reader.shared.borrow().mode() != crate::Mode::Interact {
        ctx.with_widget_mut(rendered, |surface: &mut RenderedSurface, ctx| {
            surface.enter_interact(ctx)
        })?;
    }
    let top = ctx.view_of(rendered).map_or(0, |view| view.scroll.y);
    let source = snapshot.text();
    let (unit, range, new_block, cursor, row, terminator, tall, prose) = {
        let mut state = reader.shared.borrow_mut();
        let presentation = state.presentation.clone();
        let marked = state.marked_unit();
        let anchor = marked.as_ref().map(|(unit, _)| unit.range.start);
        let (range, new_block, cursor, row, terminator, tall, prose) = match (marked, presentation)
        {
            (Some((unit, rows)), Some(p)) if opening == Opening::Unit => {
                let exact = p.exact_source_at_row(top.max(rows.start));
                let cursor = opening_byte(&source, &unit, &rows, top, exact);
                let row = rows.start.saturating_sub(top);
                let terminator = line_terminator(&source, &unit.range);
                let tall = rows.start < top;
                let prose = is_prose(unit.kind);
                (unit.range, None, cursor, row, terminator, tall, prose)
            }
            (Some((unit, _)), Some(p)) => {
                // After a list item's text, the new block is the next item.
                let block = NewBlock::item_after(&p.document, &unit)
                    .unwrap_or_else(|| NewBlock::after(&p.document, &unit));
                let terminator = line_terminator(&source, &unit.range);
                let above =
                    slot_rows(&mut state, anchor, Some(block.at)).map_or(0, |rows| rows.above);
                let row = above.saturating_sub(top);
                let cursor = block.seed.len();
                (
                    block.at..block.at,
                    Some(block),
                    cursor,
                    row,
                    terminator,
                    false,
                    true,
                )
            }
            (_, presentation) => {
                let block = NewBlock::at_end(&source);
                let end = source.len();
                let terminator = line_terminator(&source, &(end..end));
                let row = presentation.map_or(0, |p| p.height().saturating_sub(top));
                (end..end, Some(block), 0, row, terminator, false, true)
            }
        };
        (
            anchor, range, new_block, cursor, row, terminator, tall, prose,
        )
    };
    let insert = opening == Opening::After;
    let original = source[range.clone()].to_owned();
    // The editor opens on the unit's text, or on a new block's seed, which a
    // discard returns to.
    let opening_text = new_block
        .as_ref()
        .map_or_else(|| original.clone(), |block| block.seed.clone());
    let new_at = new_block.as_ref().map(|block| block.at);
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
            unit,
            new_at,
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
        Editor::with_config(opening_text.clone(), slot_config(&config)),
    )?;
    #[cfg(feature = "syntax")]
    ctx.with_widget_mut(editor, |editor: &mut Editor, _| {
        editor.set_highlighter(Some(Box::new(
            SyntectHighlighter::new("md").with_palette_roles(),
        )));
        Ok(())
    })?;
    let below = ctx.add_child(
        column,
        EditRows {
            part: Part::Below,
            unit,
            new_at,
            shared: reader.shared.clone(),
        },
    )?;
    // The column covers the rendered surface, which keeps its place: a
    // hidden view would lose its scroll.
    reader.slot = Edit::Open(OpenSlot {
        column: column.into(),
        above: above.into(),
        middle: slot.into(),
        bar: bar.into(),
        below: below.into(),
        editor,
        unit,
        epoch: snapshot.epoch,
        range,
        new_block,
        terminator,
        prose,
        original,
        kept: opening_text,
        row,
    });
    layout_parts(reader, ctx)?;
    ctx.with_widget_mut(editor, |editor: &mut Editor, ctx| {
        let position = editor.position_of_offset(cursor);
        editor.set_cursor(ctx, position);
        // A new block takes its text at once.
        if insert {
            editor.set_vi_mode(ViMode::Insert);
        }
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

/// Delete the marked unit, as one edit for the host. The block cursor then
/// marks the unit after it, on the deleted unit's screen row, or else the
/// unit before it.
pub(super) fn delete(reader: &mut Markdown, ctx: &mut dyn Context) -> Result<()> {
    if !editable(reader, ctx)? {
        return Ok(());
    }
    let Some(rendered) = reader.nodes.as_ref().map(|nodes| nodes.rendered) else {
        return Ok(());
    };
    let view = ctx
        .view_of(rendered)
        .map(|view| (view.scroll.y, view.content.h));
    let (edit, first, row) = {
        let mut state = reader.shared.borrow_mut();
        let (Some((_, rows)), Some(index), Some(p)) = (
            state.marked_unit(),
            state.marked_index(),
            state.presentation.clone(),
        ) else {
            return Ok(());
        };
        let Some((units, _)) = state.cursor_map() else {
            return Ok(());
        };
        let Some(splice) = delete_unit(&p.document, &units, index) else {
            return Ok(());
        };
        let (first, row) = match units.get(index + 1) {
            Some(next) => (
                next.range.start - splice.range.len() + splice.text.len(),
                view.and_then(|(top, height)| screen_row(&rows, top, height)),
            ),
            None => (
                index
                    .checked_sub(1)
                    .and_then(|before| units.get(before))
                    .map_or(0, |before| before.range.start),
                None,
            ),
        };
        let edit = BlockEdit {
            epoch: p.document.source.epoch,
            range: splice.range,
            text: splice.text,
        };
        (edit, first, row)
    };
    reader.edit_text(ctx, edit, first, row)
}

/// Lay out the parts for the current presentation: fit the rows above the
/// slot to the slot's screen row and to the rows above the unit, and the
/// slot's margin to the presentation's.
pub(super) fn layout_parts(reader: &Markdown, ctx: &mut dyn Context) -> Result<()> {
    let Some(slot) = reader.slot.slot() else {
        return Ok(());
    };
    let (above, bar, row) = (slot.above, slot.bar, slot.row);
    let new_at = slot.new_block.as_ref().map(|block| block.at);
    let (rows, inset, width) = {
        let mut state = reader.shared.borrow_mut();
        let presentation = state.presentation.as_ref();
        let inset = presentation.map_or(0, |p| p.inset());
        let width = presentation.map(|p| {
            if slot.prose {
                p.prose_width()
            } else {
                p.body_width()
            }
        });
        let rows = slot_rows(&mut state, slot.unit, new_at).map_or(0, |rows| rows.above);
        (rows, inset, width)
    };
    ctx.set_layout_override(above, above_layout(row.min(rows)).into())?;
    // The editor wraps its text where the reader wraps it, and keeps one more
    // column for the caret, where the reader keeps its scrollbar's column,
    // so the text keeps its rows as the block opens.
    let editor = width.map_or_else(
        || Layout::column().flex_horizontal(1),
        |width| Layout::column().fixed_width(width.saturating_add(1)),
    );
    ctx.set_layout_override(slot.editor.into(), editor.into())?;
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
    let target = match reader.slot.open() {
        Some(slot) => ctx.with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))?,
        None => return Ok(()),
    };
    if reader.refused_by_lock(ctx)? {
        return Ok(());
    }
    finish(reader, ctx, &target)
}

/// Discard the changes since the last checkpoint and close the slot. With
/// changes, ask first unless `confirmed`.
pub(super) fn discard(reader: &mut Markdown, ctx: &mut dyn Context, confirmed: bool) -> Result<()> {
    let (kept, changed) = match reader.slot.open() {
        Some(slot) => {
            let draft = ctx.with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))?;
            (slot.kept.clone(), draft != slot.kept)
        }
        None => return Ok(()),
    };
    if reader.refused_by_lock(ctx)? {
        return Ok(());
    }
    if changed && !confirmed {
        return ask_discard(reader, ctx);
    }
    finish(reader, ctx, &kept)
}

/// End the slot with `target` as its text: an edit when it differs from the
/// text before the edit, which keeps the slot on screen until its
/// presentation shows, or else a close.
fn finish(reader: &mut Markdown, ctx: &mut dyn Context, target: &str) -> Result<()> {
    let Some(slot) = reader.slot.open() else {
        return Ok(());
    };
    let edit = slot.edit(target);
    if edit.text == slot.original {
        return close(reader, ctx);
    }
    // The slot's screen row now, which it left to fit its text.
    let row = ctx
        .view_of(slot.above)
        .map_or(slot.row, |view| view.outer.h);
    let first = slot.first_byte(target);
    reader.edit_text(ctx, edit, first, Some(row))?;
    if let Some(slot) = reader.slot.commit() {
        // The committed editor shows until the new layout does, and takes
        // no more keys: they go to the reader, whose keys that need the new
        // layout wait for it.
        let (editor, column) = (slot.editor, slot.column);
        ctx.with_widget_mut(editor, |editor: &mut Editor, _| {
            let config = editor
                .config()
                .clone()
                .with_interaction(Interaction::Display);
            editor.set_config(config);
            Ok(())
        })?;
        if ctx.is_on_focus_path(column)
            && let Some(nodes) = reader.nodes.as_ref()
        {
            ctx.set_focus(nodes.rendered)?;
        }
    }
    reader.update_chrome(ctx)
}

/// Mark the editor's text as kept: a later discard returns to it.
pub(super) fn checkpoint(reader: &mut Markdown, ctx: &dyn ViewContext) -> Result<()> {
    let Some(slot) = reader.slot.open() else {
        return Ok(());
    };
    let draft = ctx.with_widget(slot.editor, |editor: &Editor| Ok(slot.draft(editor)))?;
    if let Edit::Open(slot) = &mut reader.slot {
        slot.kept = draft;
    }
    Ok(())
}

/// Return the edit that a commit of the slot would make now, with the
/// editor's text in place of its range.
pub(super) fn draft(reader: &Markdown, ctx: &dyn ViewContext) -> Option<BlockEdit> {
    let slot = reader.slot.open()?;
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
        mode: state.mode(),
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
    let slot = reader.slot.slot()?;
    let height = |node: NodeId| ctx.view_of(node).map_or(0, |view| view.outer.h);
    Some((height(slot.above), height(slot.middle), height(slot.below)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::parse_units;

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
