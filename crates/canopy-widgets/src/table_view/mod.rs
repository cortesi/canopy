//! A keyed read-only table with one scrolling body and fixed chrome.

use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    Widget,
    commands::CommandStatus,
    derive_commands,
    error::{Error, Result},
    geom::{Line, Point, Rect, Size},
    input::{
        Event,
        key::{Key, KeyCode},
        mouse,
    },
    layout::{CanvasContext, Edges, Layout, RevealAlign, ScrollOp},
    render::Render,
    runtime::{NodeWakeHandle, WidgetSemantics},
};

use crate::{
    scrollbar::Scrollbar,
    table::{
        CellKey, PreparedTable, TableCopy, TableData, TableEvent, TableGlyphs, TableHit,
        TablePaintStats, TableSelection, TableViewOptions, TableWindow, display_control,
        style_rules,
    },
};

/// Scrollbar targets and their shell-local tracks.
type Tracks = Vec<(NodeId, Rect)>;

/// Immutable data and lightweight selection shared with the sole body node.
struct BodyPaint {
    /// Immutable publication shared with the shell.
    base: Arc<PreparedTable>,
    /// The geometry that shows: the publication, or for a wrapping table the
    /// publication refitted to the body's width.
    table: Arc<PreparedTable>,
    /// The body width the shown geometry was fitted to.
    fitted: Option<u32>,
    /// Stable keys of the current cursor.
    cursor: Option<CellKey>,
    /// Shared immutable axis membership.
    selection: TableSelection,
    /// Counters from the most recent visible body paint.
    stats: TablePaintStats,
}
/// The single scrolling canvas beneath TableView's fixed chrome.
struct TableBody {
    /// State shared with the owning TableView.
    paint: Rc<RefCell<BodyPaint>>,
    /// Whether the body receives keyboard focus.
    interactive: bool,
}
impl Widget for TableBody {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        let mut paint = self.paint.borrow_mut();
        // A wrapping table widens its cut-back columns into the body, so it
        // refits whenever the body's width changes.
        if paint.base.wraps()
            && paint.fitted != Some(view.w)
            && let Ok(fitted) = paint.base.fit(view.w)
        {
            paint.table = Arc::new(fitted);
            paint.fitted = Some(view.w);
        }
        paint.table.size()
    }
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.interactive
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let mut paint = self.paint.borrow_mut();
        let view = ctx.view();
        if view.content.w <= 1 {
            paint.stats = Default::default();
            return Ok(());
        }
        let stats = paint.table.paint(
            render,
            view.view_rect_local(),
            TableWindow {
                offset: view.scroll,
                header: false,
                row_numbers: false,
            },
            paint.cursor.as_ref(),
            &paint.selection,
        )?;
        paint.stats = stats;
        Ok(())
    }
    fn reveal_anchor(&self, _content: Size) -> Option<Rect> {
        let paint = self.paint.borrow();
        let key = paint.cursor.as_ref()?;
        paint.table.cell_rect(
            paint.table.data().row_index(&key.row)?,
            paint.table.data().column_index(&key.column)?,
        )
    }
    fn name(&self) -> NodeName {
        NodeName::convert("table_body")
    }
}

/// A reusable read-only spreadsheet surface. Its body owns both runtime scroll
/// offsets. Header, gutter, and tracks stay outside that body's canvas.
pub struct TableView {
    /// Current atomic data and geometry publication.
    table: Arc<PreparedTable>,
    /// Interaction, fixed chrome, and action limits.
    options: TableViewOptions,
    /// Indexed cursor in the current publication.
    cursor: Option<(usize, usize)>,
    /// Explicit keyed Cartesian selection membership.
    selection: TableSelection,
    /// Indexed origin of the current rectangle gesture.
    anchor: Option<(usize, usize)>,
    /// Whether a cell-selection gesture holds capture.
    drag: bool,
    /// Whether a scrollbar gesture holds capture.
    scroll_drag: bool,
    /// Text of the optional coordinate prompt.
    prompt: Option<String>,
    /// Lightweight immutable paint state shared with the body.
    paint: Rc<RefCell<BodyPaint>>,
    /// The sole child and owner of both scroll offsets.
    body: Option<TypedId<TableBody>>,
    /// Vertical scrollbar track and capture state.
    vertical: Scrollbar,
    /// Horizontal scrollbar track and capture state.
    horizontal: Scrollbar,
    /// Bounded queue of complete host actions.
    events: VecDeque<TableEvent>,
    /// Aggregate key and text bytes retained by the action queue.
    event_bytes: usize,
    /// Whether the host admits copy actions.
    copy_enabled: bool,
    /// Optional host wake endpoint for queued actions.
    event_wake: Option<NodeWakeHandle>,
}

#[derive_commands]
impl TableView {
    /// Construct a view around one atomic prepared publication.
    pub fn new(table: Arc<PreparedTable>, options: TableViewOptions) -> Self {
        let cursor = initial_cursor(table.data());
        let (vertical, horizontal) = scrollbars(&table);
        let paint = Rc::new(RefCell::new(BodyPaint {
            cursor: cursor.and_then(|(r, c)| table.data().cell_key(r, c)),
            base: table.clone(),
            table: table.clone(),
            fitted: None,
            selection: TableSelection::default(),
            stats: Default::default(),
        }));
        Self {
            table,
            options,
            cursor,
            selection: TableSelection::default(),
            anchor: None,
            drag: false,
            scroll_drag: false,
            prompt: None,
            paint,
            body: None,
            vertical,
            horizontal,
            events: VecDeque::new(),
            event_bytes: 0,
            copy_enabled: true,
            event_wake: None,
        }
    }
    /// Display one coherent snapshot. Older revisions of the same dataset
    /// reject without changing data, geometry, cursor, selection, or
    /// offsets.
    pub fn publish(&mut self, ctx: &mut dyn Context, table: Arc<PreparedTable>) -> Result<()> {
        let same = table.data().dataset() == self.table.data().dataset();
        if same && table.data().revision() < self.table.data().revision() {
            return Err(Error::Invalid(
                "table publication would roll back its revision".into(),
            ));
        }
        let old = self.cursor();
        let old_index = self.cursor;
        let offset = self
            .body
            .and_then(|id| ctx.view_of(id.into()))
            .map(|view| view.scroll)
            .unwrap_or(Point::ZERO);
        let top = row_or_above(&self.geometry(), offset.y)
            .and_then(|row| self.table.data().rows().get(row))
            .map(|row| row.key.clone());
        self.end_drag(ctx)?;
        if ctx.has_mouse_capture() {
            ctx.release_mouse()?;
        }
        self.table = table;
        (self.vertical, self.horizontal) = scrollbars(&self.table);
        if same {
            self.cursor = initial_cursor(self.table.data()).map(|_| {
                let (r, c) = old_index.unwrap_or((0, 0));
                let r = old
                    .as_ref()
                    .and_then(|key| self.table.data().row_index(&key.row))
                    .unwrap_or(r.min(self.table.data().rows().len() - 1));
                let c = old
                    .as_ref()
                    .and_then(|key| self.table.data().column_index(&key.column))
                    .unwrap_or(c.min(self.table.data().columns().len() - 1));
                (r, c)
            });
            self.selection.reconcile(self.table.data());
        } else {
            self.cursor = initial_cursor(self.table.data());
            self.selection = TableSelection::default();
        }
        self.anchor = None;
        self.prompt = None;
        self.sync_paint();
        if let Some(body) = self.body {
            let origin = if same {
                let y = top
                    .and_then(|key| self.table.data().row_index(&key))
                    .and_then(|row| self.geometry().row_lines(row))
                    .map_or(offset.y, |(top, _)| top);
                Point::new(offset.x, y)
            } else {
                Point::ZERO
            };
            ctx.scroll_node(body.into(), ScrollOp::To(origin))?;
            ctx.with_widget_mut(body, |_: &mut TableBody, ctx| {
                let width = ctx.view().content.w.max(1);
                ctx.reveal_area(Rect::new(origin.x, origin.y, width, 1), RevealAlign::Top(0));
                Ok(())
            })?;
        }
        Ok(())
    }
    /// Mounted body node that receives focus and owns both scroll offsets.
    pub fn initial_focus(&self) -> Option<NodeId> {
        self.body.map(Into::into)
    }
    /// Enable copy admission when the host provides a clipboard or copy sink.
    pub fn set_copy_enabled(&mut self, enabled: bool) {
        self.copy_enabled = enabled;
    }
    /// Set the host's maximum complete copy payload size in UTF-8 bytes.
    pub fn set_copy_limit(&mut self, bytes: usize) {
        self.options.copy_bytes = bytes;
    }
    /// Show or hide the fixed row-number gutter.
    pub fn set_row_numbers(&mut self, show: bool) {
        self.options.row_numbers = show;
    }
    /// Return whether the row-number gutter shows.
    pub fn row_numbers(&self) -> bool {
        self.options.row_numbers
    }
    /// Displayed geometry and data.
    pub fn table(&self) -> &Arc<PreparedTable> {
        &self.table
    }
    /// Current stable cell keys, or no cursor for an empty axis.
    pub fn cursor(&self) -> Option<CellKey> {
        self.cursor
            .and_then(|(r, c)| self.table.data().cell_key(r, c))
    }
    /// Work performed by the most recent body paint, independent of dataset
    /// size.
    pub fn paint_stats(&self) -> TablePaintStats {
        self.paint.borrow().stats
    }
    /// Indexed cursor position, useful for bounded status reporting.
    pub fn cursor_index(&self) -> Option<(usize, usize)> {
        self.cursor
    }
    /// Move to an existing keyed cell and reveal it.
    pub fn set_cursor(&mut self, ctx: &mut dyn Context, cell: &CellKey) -> Result<()> {
        let row = self
            .table
            .data()
            .row_index(&cell.row)
            .ok_or_else(|| Error::Invalid("unknown table row key".into()))?;
        let column = self
            .table
            .data()
            .column_index(&cell.column)
            .ok_or_else(|| Error::Invalid("unknown table column key".into()))?;
        self.move_to(ctx, row, column, false)
    }
    /// Explicit selection membership, independent of cursor and clipping.
    pub fn selection(&self) -> &TableSelection {
        &self.selection
    }
    /// Drain at most `limit` pending host actions.
    pub fn take_events(&mut self, limit: usize) -> Vec<TableEvent> {
        let events: Vec<_> = self.events.drain(..limit.min(self.events.len())).collect();
        for event in &events {
            self.event_bytes = self.event_bytes.saturating_sub(event_size(event));
        }
        events
    }
    /// Wake an owner whenever a complete host event enters the queue.
    pub fn set_event_wake(&mut self, wake: NodeWakeHandle) {
        self.event_wake = Some(wake);
    }
    /// Pending host event count and aggregate text/key bytes.
    pub fn pending_events(&self) -> (usize, usize) {
        (self.events.len(), self.event_bytes)
    }
    /// Return the cursor's one-based coordinates, for automation.
    /// @return "row,column", or "" when the table is empty.
    #[command]
    #[must_use]
    pub fn cursor_position(&self) -> String {
        self.cursor
            .map_or_else(String::new, |(r, c)| format!("{},{}", r + 1, c + 1))
    }
    /// Move by cells, clamping at every edge. Empty data consumes a no-op.
    #[command]
    pub fn move_cursor(
        &mut self,
        ctx: &mut dyn Context,
        rows: i32,
        columns: i32,
        extend: bool,
    ) -> Result<()> {
        let Some((r, c)) = self.cursor else {
            return Ok(());
        };
        self.move_to(
            ctx,
            r.saturating_add_signed(rows as isize),
            c.saturating_add_signed(columns as isize),
            extend,
        )
    }
    /// Jump to one-based coordinates. Coordinates beyond the data clamp.
    #[command]
    pub fn goto(&mut self, ctx: &mut dyn Context, row: usize, column: usize) -> Result<()> {
        if row == 0 || column == 0 {
            return Err(Error::Invalid("table coordinates start at 1".into()));
        }
        self.move_to(ctx, row - 1, column - 1, false)
    }
    /// Select the current row across all columns.
    #[command]
    pub fn select_row(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        if let Some((r, _)) = self.cursor {
            self.selection = TableSelection::new(
                Arc::from([self.table.data().rows()[r].key.clone()]),
                self.table.data().column_keys(),
            );
            self.sync_paint();
        }
        Ok(())
    }
    /// Select the current column across all rows.
    #[command]
    pub fn select_column(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        if let Some((_, c)) = self.cursor {
            self.selection = TableSelection::new(
                self.table.data().row_keys(),
                Arc::from([self.table.data().columns()[c].key.clone()]),
            );
            self.sync_paint();
        }
        Ok(())
    }
    /// Select all axis keys without allocating a record for each cell.
    #[command]
    pub fn select_all(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.selection = TableSelection::new(
            self.table.data().row_keys(),
            self.table.data().column_keys(),
        );
        self.sync_paint();
        Ok(())
    }
    /// Clear explicit membership.
    #[command]
    pub fn clear_selection(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.selection = TableSelection::default();
        self.anchor = None;
        self.sync_paint();
        Ok(())
    }
    /// Emit one activation carrying the displayed revision and stable keys.
    #[command]
    pub fn activate(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        if let Some(cell) = self.cursor() {
            self.push_event(TableEvent::Activate {
                dataset: self.table.data().dataset().clone(),
                revision: self.table.data().revision(),
                cell,
            })?;
        }
        Ok(())
    }
    /// Copy the full current value, with no TSV framing.
    #[command(enabled = "copy_status")]
    pub fn copy_cell(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.copy(TableCopy::Cell, false)
    }
    /// Copy selected body fields as exact TSV.
    #[command(enabled = "copy_status")]
    pub fn copy_selection(&mut self, _ctx: &mut dyn Context, include_header: bool) -> Result<()> {
        self.copy(TableCopy::Selection, include_header)
    }
    /// Copy selected rows, or the cursor row, across every column.
    #[command(enabled = "copy_status")]
    pub fn copy_rows(&mut self, _ctx: &mut dyn Context, include_header: bool) -> Result<()> {
        self.copy(TableCopy::Rows, include_header)
    }
    /// Copy all labels and body rows as exact TSV.
    #[command(enabled = "copy_status")]
    pub fn copy_all(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.copy(TableCopy::All, true)
    }
    /// Start a one-based `row,column` coordinate prompt.
    #[command]
    pub fn coordinate_prompt(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.prompt = Some(String::new());
        Ok(())
    }

    /// Report the host copy capability to command discovery.
    fn copy_status(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if self.copy_enabled {
            CommandStatus::Enabled
        } else {
            CommandStatus::Disabled("Host copy is unavailable".into())
        })
    }
    /// Serialize one displayed operation and queue its complete host payload.
    fn copy(&mut self, operation: TableCopy, include_header: bool) -> Result<()> {
        if !self.copy_enabled {
            return Err(Error::Invalid("host copy is unavailable".into()));
        }
        let data = self.table.data();
        let text = if operation == TableCopy::Cell {
            let Some(cell) = self.cursor().and_then(|key| data.cell(&key)) else {
                return Ok(());
            };
            if cell.text().len() > self.options.copy_bytes {
                return Err(Error::Invalid(
                    "table cell exceeds the copy byte limit".into(),
                ));
            }
            cell.text().to_owned()
        } else {
            let selection = match operation {
                TableCopy::All => TableSelection::new(data.row_keys(), data.column_keys()),
                TableCopy::Rows => TableSelection::new(
                    if self.selection.rows().is_empty() {
                        self.cursor
                            .map(|(r, _)| Arc::from([data.rows()[r].key.clone()]))
                            .unwrap_or_default()
                    } else {
                        self.selection.rows().to_vec().into()
                    },
                    data.column_keys(),
                ),
                _ => self.selection.clone(),
            };
            self.table
                .tsv(&selection, include_header, self.options.copy_bytes)?
        };
        self.push_event(TableEvent::Copy {
            dataset: data.dataset().clone(),
            revision: data.revision(),
            text,
        })
    }
    /// Admit one complete event under both queue limits and wake its host.
    fn push_event(&mut self, event: TableEvent) -> Result<()> {
        let bytes = event_size(&event);
        if self.events.len() >= self.options.event_capacity
            || bytes > self.options.event_bytes.saturating_sub(self.event_bytes)
        {
            return Err(Error::Invalid("table host event queue is full".into()));
        }
        self.event_bytes += bytes;
        self.events.push_back(event);
        if let Some(wake) = &self.event_wake {
            wake.wake()?;
        }
        Ok(())
    }
    /// Return the geometry that shows, which a wrapping table refits to the
    /// body's width.
    fn geometry(&self) -> Arc<PreparedTable> {
        self.paint.borrow().table.clone()
    }
    /// Return the lines the fixed header takes.
    fn header_lines(&self) -> u32 {
        if self.options.header {
            self.geometry().header_lines()
        } else {
            0
        }
    }
    /// Publish lightweight cursor and membership state to the body.
    fn sync_paint(&self) {
        let mut paint = self.paint.borrow_mut();
        if !Arc::ptr_eq(&paint.base, &self.table) {
            paint.base = self.table.clone();
            // A new table fits the body's width at once, so scrolling and
            // hits agree with what the next paint shows.
            let fitted = paint
                .fitted
                .filter(|_| self.table.wraps())
                .and_then(|width| Some((width, Arc::new(self.table.fit(width).ok()?))));
            (paint.table, paint.fitted) = match fitted {
                Some((width, table)) => (table, Some(width)),
                None => (self.table.clone(), None),
            };
        }
        paint.cursor = self.cursor();
        paint.selection = self.selection.clone();
    }
    /// Clamp indexed coordinates, update rectangle membership, and reveal the
    /// cursor.
    fn move_to(
        &mut self,
        ctx: &mut dyn Context,
        row: usize,
        column: usize,
        extend: bool,
    ) -> Result<()> {
        let Some((old_r, old_c)) = self.cursor else {
            return Ok(());
        };
        let row = row.min(self.table.data().rows().len() - 1);
        let column = column.min(self.table.data().columns().len() - 1);
        if extend {
            let (r, c) = *self.anchor.get_or_insert((old_r, old_c));
            self.selection = TableSelection::new(
                self.table.data().row_keys()[r.min(row)..=r.max(row)]
                    .to_vec()
                    .into(),
                self.table.data().column_keys()[c.min(column)..=c.max(column)]
                    .to_vec()
                    .into(),
            );
        } else {
            self.anchor = None;
            self.selection = TableSelection::default();
        }
        self.cursor = Some((row, column));
        self.sync_paint();
        self.reveal(ctx)
    }
    /// Schedule cursor reveal after final body geometry settles.
    fn reveal(&self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(body) = self.body {
            ctx.with_widget_mut(body, |_: &mut TableBody, ctx| {
                ctx.reveal_anchor(RevealAlign::Nearest);
                Ok(())
            })?;
        }
        Ok(())
    }
    /// End either pointer gesture and release this shell's capture.
    fn end_drag(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.drag || self.scroll_drag {
            self.drag = false;
            self.scroll_drag = false;
            if ctx.has_mouse_capture() {
                ctx.release_mouse()?;
            }
        }
        Ok(())
    }
    /// Resolve scrollbar tracks around the sole body viewport.
    fn tracks(&self, ctx: &dyn ViewContext) -> (Tracks, Tracks) {
        let Some(body) = self.body else {
            return (vec![], vec![]);
        };
        let outer = ctx.view().outer_rect_local();
        let Some(view) = ctx.view_of(body.into()) else {
            return (vec![], vec![]);
        };
        if self.narrow(ctx) {
            return (vec![], vec![]);
        }
        let x = self.gutter();
        let y = self.header_lines();
        (
            vec![(
                body.into(),
                Rect::new(outer.w.saturating_sub(1), y, 1, view.content.h),
            )],
            vec![(
                body.into(),
                Rect::new(x, outer.h.saturating_sub(1), view.content.w, 1),
            )],
        )
    }
    /// Return the fixed row-number gutter width.
    fn gutter(&self) -> u32 {
        if self.options.row_numbers {
            self.table.gutter_width()
        } else {
            0
        }
    }
    /// Return whether fixed chrome leaves no usable grid area.
    fn narrow(&self, ctx: &dyn ViewContext) -> bool {
        let area = ctx.view().outer;
        area.w <= self.gutter() + 2 || area.h <= self.header_lines() + 1
    }
    /// Predict raw-key admission without changing table state.
    fn key_action(&self, key: Key) -> bool {
        if !self.options.interactive {
            return false;
        }
        if self.prompt.is_some() {
            return !key.mods.ctrl
                && !key.mods.alt
                && matches!(
                    key.key,
                    KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Enter | KeyCode::Esc
                );
        }
        if key.key == KeyCode::Esc {
            return self.drag || self.scroll_drag || !self.selection.is_empty();
        }
        if !self.copy_enabled
            && ((key.mods.ctrl && matches!(key.key, KeyCode::Char('c' | 'C')))
                || (!key.mods.ctrl && matches!(key.key, KeyCode::Char('y' | 'Y'))))
        {
            return false;
        }
        if key.mods.alt {
            return false;
        }
        if key.mods.ctrl {
            return matches!(
                key.key,
                KeyCode::Home | KeyCode::End | KeyCode::Char('a' | 'A' | 'c' | 'C')
            );
        }
        matches!(
            key.key,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Enter
                | KeyCode::Char(
                    'h' | 'j'
                        | 'k'
                        | 'l'
                        | 'H'
                        | 'J'
                        | 'K'
                        | 'L'
                        | 'g'
                        | 'G'
                        | '0'
                        | '$'
                        | ':'
                        | 'y'
                        | 'Y'
                        | 'v'
                        | 'V'
                        | ' '
                )
        )
    }
    /// Perform one admitted navigation, selection, prompt, or host action.
    fn handle_key(&mut self, ctx: &mut dyn Context, key: Key) -> Result<()> {
        if let Some(prompt) = self.prompt.as_mut() {
            match key.key {
                KeyCode::Esc => {
                    self.prompt = None;
                }
                KeyCode::Backspace => {
                    prompt.pop();
                }
                KeyCode::Enter => {
                    let value = prompt.clone();
                    let coords = value
                        .split_once(',')
                        .or_else(|| value.split_once(':'))
                        .and_then(|(r, c)| {
                            Some((
                                r.trim().parse::<usize>().ok()?,
                                c.trim().parse::<usize>().ok()?,
                            ))
                        });
                    let Some((r, c)) = coords else {
                        return Err(Error::Invalid(
                            "enter table coordinates as row,column".into(),
                        ));
                    };
                    self.goto(ctx, r, c)?;
                    self.prompt = None;
                }
                KeyCode::Char(c)
                    if (c.is_ascii_digit() || [',', ':', ' '].contains(&c))
                        && prompt.len() < 64 =>
                {
                    prompt.push(c)
                }
                _ => {}
            }
            return Ok(());
        }
        if key.key == KeyCode::Esc {
            self.end_drag(ctx)?;
            return self.clear_selection(ctx);
        }
        if key.mods.ctrl {
            return match key.key {
                KeyCode::Char('a' | 'A') => self.select_all(ctx),
                KeyCode::Char('c' | 'C') => {
                    if self.selection.is_empty() {
                        self.copy_cell(ctx)
                    } else {
                        self.copy_selection(ctx, false)
                    }
                }
                KeyCode::Home => {
                    self.move_to(ctx, 0, self.cursor.map_or(0, |(_, c)| c), key.mods.shift)
                }
                KeyCode::End => self.move_to(
                    ctx,
                    usize::MAX,
                    self.cursor.map_or(0, |(_, c)| c),
                    key.mods.shift,
                ),
                _ => Ok(()),
            };
        }
        let (r, c) = self.cursor.unwrap_or((0, 0));
        let lines = self
            .body
            .and_then(|id| ctx.view_of(id.into()))
            .map_or(1, |v| v.content.h.max(1));
        let geometry = self.geometry();
        let page_down = i32::try_from(geometry.rows_within(r, lines)).unwrap_or(i32::MAX);
        let page_up = i32::try_from(geometry.rows_before(r, lines)).unwrap_or(i32::MAX);
        match key.key {
            KeyCode::Left | KeyCode::Char('h') => self.move_cursor(ctx, 0, -1, key.mods.shift),
            KeyCode::Right | KeyCode::Char('l') => self.move_cursor(ctx, 0, 1, key.mods.shift),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(ctx, -1, 0, key.mods.shift),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(ctx, 1, 0, key.mods.shift),
            // Shifted letters extend the selection, like shifted arrows.
            KeyCode::Char('H') => self.move_cursor(ctx, 0, -1, true),
            KeyCode::Char('L') => self.move_cursor(ctx, 0, 1, true),
            KeyCode::Char('K') => self.move_cursor(ctx, -1, 0, true),
            KeyCode::Char('J') => self.move_cursor(ctx, 1, 0, true),
            KeyCode::PageUp => self.move_cursor(ctx, -page_up, 0, key.mods.shift),
            // Space pages down, as in the other read-only views.
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.move_cursor(ctx, page_down, 0, key.mods.shift)
            }
            KeyCode::Home => self.move_to(ctx, r, 0, key.mods.shift),
            KeyCode::End => self.move_to(ctx, r, usize::MAX, key.mods.shift),
            KeyCode::Char('0') => self.move_to(ctx, r, 0, false),
            KeyCode::Char('$') => self.move_to(ctx, r, usize::MAX, false),
            KeyCode::Char('g') => self.move_to(ctx, 0, c, false),
            KeyCode::Char('G') => self.move_to(ctx, usize::MAX, c, false),
            KeyCode::Char(':') => self.coordinate_prompt(ctx),
            KeyCode::Tab | KeyCode::BackTab => {
                let columns = self.table.data().columns().len();
                if columns == 0 {
                    return Ok(());
                }
                let index = r.saturating_mul(columns).saturating_add(c);
                let previous = key.mods.shift || key.key == KeyCode::BackTab;
                let index = if previous {
                    index.saturating_sub(1)
                } else {
                    index.saturating_add(1).min(
                        self.table
                            .data()
                            .rows()
                            .len()
                            .saturating_mul(columns)
                            .saturating_sub(1),
                    )
                };
                self.move_to(ctx, index / columns, index % columns, false)
            }
            KeyCode::Enter => self.activate(ctx),
            KeyCode::Char('v') => self.select_column(ctx),
            KeyCode::Char('V') => self.select_row(ctx),
            KeyCode::Char('y') => {
                if self.selection.is_empty() {
                    self.copy_cell(ctx)
                } else {
                    self.copy_selection(ctx, false)
                }
            }
            KeyCode::Char('Y') => self.copy_rows(ctx, false),
            _ => Ok(()),
        }
    }
}
/// Construct scrollbar chrome for the prepared glyph preset.
fn scrollbars(table: &PreparedTable) -> (Scrollbar, Scrollbar) {
    let ascii = table.glyphs() == TableGlyphs::ASCII;
    (
        Scrollbar::vertical("table/scrollbar/thumb", if ascii { '#' } else { '█' })
            .with_track("table/scrollbar", if ascii { '|' } else { '│' }),
        Scrollbar::horizontal("table/scrollbar/thumb", if ascii { '#' } else { '▄' })
            .with_track("table/scrollbar", if ascii { '-' } else { '─' }),
    )
}
/// Bound and sanitize text for the narrow current-cell fallback.
fn safe_prefix(value: &str, width: u32) -> String {
    value
        .chars()
        .take((width as usize).saturating_mul(4))
        .map(|c| if display_control(c) { ' ' } else { c })
        .collect()
}
/// Return the body row at canvas line `y`. A rule between wrapped rows
/// belongs to the row above it.
fn row_or_above(geometry: &PreparedTable, y: u32) -> Option<usize> {
    geometry
        .row_at(y)
        .or_else(|| geometry.row_at(y.saturating_sub(1)))
}
/// Select the first body cell when both data axes are nonempty.
fn initial_cursor(data: &TableData) -> Option<(usize, usize)> {
    (!data.rows().is_empty() && !data.columns().is_empty()).then_some((0, 0))
}
/// Count retained action key and payload bytes for queue admission.
fn event_size(event: &TableEvent) -> usize {
    match event {
        TableEvent::Copy { dataset, text, .. } => dataset.len().saturating_add(text.len()),
        TableEvent::Activate { dataset, cell, .. } => dataset
            .len()
            .saturating_add(cell.row.len())
            .saturating_add(cell.column.len()),
    }
}
impl Register for TableView {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.widget_styles(style_rules);
        setup.add_commands::<Self>()
    }
}
impl Widget for TableView {
    fn semantics(&self, _ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("table".into()),
            label: Some("Read-only table".into()),
            value: Some(format!(
                "{} rows, {} columns; revision {}; cursor {}",
                self.table.data().rows().len(),
                self.table.data().columns().len(),
                self.table.data().revision(),
                self.cursor
                    .map_or_else(|| "none".into(), |(r, c)| format!("{},{}", r + 1, c + 1))
            )),
            ..Default::default()
        })
    }
    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::new(self.header_lines(), 1, 1, self.gutter()))
    }
    fn owns_scrollbars(&self) -> bool {
        true
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let body = ctx.add_child(
            ctx.node_id(),
            TableBody {
                paint: self.paint.clone(),
                interactive: self.options.interactive,
            },
        )?;
        self.body = Some(body);
        Ok(())
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let outer = ctx.view().outer_rect_local();
        render.fill("table", outer, ' ')?;
        if !ctx.has_mouse_capture() {
            self.drag = false;
            self.scroll_drag = false;
        }
        if self.narrow(ctx) {
            if let Some((r, c)) = self.cursor {
                let col = &self.table.data().columns()[c];
                let cell = &self.table.data().rows()[r].cells[c];
                if outer.h > 1 {
                    let label = safe_prefix(col.label.text(), outer.w);
                    render.text(
                        "table/header",
                        Line::new(0, 0, outer.w),
                        &format!("{},{} {label}", r + 1, c + 1),
                    )?;
                }
                let value = safe_prefix(cell.text(), outer.w);
                render.text(
                    "table/cell",
                    Line::new(0, u32::from(outer.h > 1), outer.w),
                    &value,
                )?;
            } else {
                render.text(
                    "table/cell",
                    Line::new(outer.tl.x, outer.tl.y, outer.w),
                    "Empty table",
                )?;
            }
        } else if let Some(body) = self.body.and_then(|id| ctx.view_of(id.into())) {
            let geometry = self.geometry();
            let header = self.header_lines();
            let window = TableWindow {
                offset: body.scroll,
                header: self.options.header,
                row_numbers: false,
            };
            if self.options.header {
                // The band runs across the whole view, over the gutter and
                // the scrollbar column too, so it reads as one bar.
                render.fill("table/header", Rect::new(0, 0, outer.w, header), ' ')?;
                geometry.paint(
                    render,
                    Rect::new(self.gutter(), 0, body.content.w, header),
                    window,
                    None,
                    &TableSelection::default(),
                )?;
            }
            if self.options.row_numbers {
                geometry.paint(
                    render,
                    Rect::new(0, header, self.gutter(), body.content.h),
                    TableWindow {
                        offset: body.scroll,
                        header: false,
                        row_numbers: true,
                    },
                    None,
                    &TableSelection::default(),
                )?;
            }
            let (vertical, horizontal) = self.tracks(ctx);
            self.vertical.render(render, ctx, &vertical)?;
            self.horizontal.render(render, ctx, &horizontal)?;
        }
        if let Some(prompt) = &self.prompt {
            render.text(
                "table/prompt",
                Line::new(0, 0, outer.w),
                &format!("Go to row,column: {prompt}"),
            )?;
        }
        Ok(())
    }
    fn key_outcome(&self, key: Key, _ctx: &dyn ViewContext) -> EventOutcome {
        if self.key_action(key) {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Key(key) = event {
            if self.key_action(*key) {
                self.handle_key(ctx, *key)?;
                return Ok(EventOutcome::Handle);
            }
            return Ok(EventOutcome::Ignore);
        }
        let Event::Mouse(mouse) = event else {
            return Ok(EventOutcome::Ignore);
        };
        if !ctx.has_mouse_capture() {
            self.drag = false;
            self.scroll_drag = false;
        }
        if self.drag && mouse.action == mouse::Action::Up {
            self.end_drag(ctx)?;
            return Ok(EventOutcome::Handle);
        }
        let (vertical, horizontal) = self.tracks(ctx);
        if self.vertical.handle_mouse(ctx, mouse, &vertical)? == EventOutcome::Handle
            || self.horizontal.handle_mouse(ctx, mouse, &horizontal)? == EventOutcome::Handle
        {
            if matches!(mouse.action, mouse::Action::Down | mouse::Action::Drag) {
                self.scroll_drag = true;
            }
            if mouse.action == mouse::Action::Up {
                self.scroll_drag = false;
            }
            return Ok(EventOutcome::Handle);
        }
        if !self.options.interactive {
            return Ok(EventOutcome::Ignore);
        }
        let Some(body) = self.body else {
            return Ok(EventOutcome::Ignore);
        };
        let Some(view) = ctx.view_of(body.into()) else {
            return Ok(EventOutcome::Ignore);
        };
        // Header, gutter, and body hits share the painted grid area, which
        // excludes the scrollbar row and column.
        let header = self.header_lines();
        let geometry = self.geometry();
        if mouse.action == mouse::Action::Down
            && mouse.button == mouse::Button::Left
            && let Some(point) = ctx.view().outer_point(mouse.location)
            && let Some(hit) = geometry.hit_test(
                Rect::new(
                    0,
                    0,
                    self.gutter().saturating_add(view.content.w),
                    header.saturating_add(view.content.h),
                ),
                TableWindow {
                    offset: view.scroll,
                    header: self.options.header,
                    row_numbers: self.options.row_numbers,
                },
                point,
            )
        {
            ctx.set_focus(body.into())?;
            match hit {
                TableHit::Cell { row, column } => {
                    let extend = mouse.modifiers.shift;
                    self.move_to(ctx, row, column, extend)?;
                    // A shift-click extends from the existing anchor.
                    if !extend {
                        self.anchor = Some((row, column));
                    }
                    self.drag = true;
                    ctx.capture_mouse()?;
                }
                TableHit::Header(column) => {
                    self.move_to(ctx, self.cursor.map_or(0, |(r, _)| r), column, false)?;
                    self.select_column(ctx)?;
                }
                TableHit::Row(row) => {
                    self.move_to(ctx, row, self.cursor.map_or(0, |(_, c)| c), false)?;
                    self.select_row(ctx)?;
                }
            }
            return Ok(EventOutcome::Handle);
        }
        if self.drag && mouse.action == mouse::Action::Drag {
            let point = view.viewport_to_outer(mouse.location)?.clamped_point();
            let x = point.x.saturating_add(view.scroll.x);
            let y = point.y.saturating_add(view.scroll.y);
            let row = row_or_above(&geometry, y).unwrap_or(usize::MAX);
            let column = geometry
                .column_at(x)
                .or_else(|| geometry.column_at(x.saturating_sub(1)))
                .unwrap_or(self.table.data().columns().len().saturating_sub(1));
            self.move_to(ctx, row, column, true)?;
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }
    fn on_unmount(&mut self, _ctx: &mut dyn Context) {
        self.body = None;
        self.drag = false;
        self.scroll_drag = false;
        self.events.clear();
        self.event_bytes = 0;
    }
    fn name(&self) -> NodeName {
        NodeName::convert("table_view")
    }
}

#[cfg(test)]
mod tests;
