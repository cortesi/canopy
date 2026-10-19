//! A deterministic standalone table host. No clipboard, worker, or timer is
//! required.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, TypedId, ViewContext, Widget,
    commands::ArgValue,
    derive_commands,
    error::{Error, Result},
    geom::Line,
    layout::{Edges, Layout},
    render::Render,
    runtime::PollLifetime,
    style::themes,
};
use canopy_widgets::{
    TableView,
    table::{
        self, PreparedTable, TableCell, TableColumn, TableData, TableEvent, TableGlyphs,
        TableLayoutOptions, TableRow, TableViewOptions,
    },
};

/// Initial deterministic body row count.
const FIRST_ROWS: usize = 64;
/// Bound on synchronous gym data preparation.
const MAX_ROWS: usize = 256;
/// Maximum bytes recorded in one complete copy payload.
const COPY_LIMIT: usize = 64 * 1024;
/// Table events queued and drained per poll.
const EVENT_CAPACITY: usize = 8;
/// Publication controls, scoped to the gym so plain letters stay local.
/// Presentation toggles share one which-key menu, as in the Markdown gym.
const BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    path = "table_gym",
    { key = "a", description = "Append keyed row", action = command.table_gym.append() },
    { key = "r", description = "Reverse keyed rows", action = command.table_gym.reorder() },
    { key = "p", description = "Presentation", action = canopy.menu("presentation") },
})
canopy.keymap({
    mode = "presentation",
    { key = "g", description = "ASCII / Unicode glyphs", action = command.table_gym.toggle_ascii() },
    { key = "w", description = "Compact / wide columns", action = command.table_gym.toggle_widths() },
    { key = "t", description = "Light / dark theme", action = command.table_gym.theme() },
})
"#;

/// Recorded host events make the public surface testable without touching a
/// clipboard.
pub struct TableGym {
    /// Mounted public table widget.
    table: Option<TypedId<TableView>>,
    /// Stable row identities in current display order.
    order: Vec<usize>,
    /// Identity assigned to the next appended row.
    next_row: usize,
    /// Current dataset revision.
    revision: u64,
    /// Whether the current geometry uses ASCII glyphs.
    ascii: bool,
    /// Whether alternate explicit widths are selected.
    compact: bool,
    /// Whether the live palette uses the light theme.
    light: bool,
    /// Number of complete copy events recorded.
    copies: usize,
    /// Number of cell activation events recorded.
    activations: usize,
    /// Most recent exact bounded copy payload.
    last_copy: String,
    /// Shared full value of the most recently activated cell.
    last_detail: Arc<str>,
    /// Bounded host feedback painted below the table.
    status: String,
}
impl Default for TableGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl TableGym {
    /// Start with 64 keyed rows, six columns, and a deliberately long full
    /// value.
    pub fn new() -> Self {
        Self {
            table: None,
            order: (0..FIRST_ROWS).collect(),
            next_row: FIRST_ROWS,
            revision: 1,
            ascii: false,
            compact: false,
            light: false,
            copies: 0,
            activations: 0,
            last_copy: String::new(),
            last_detail: "".into(),
            status: String::new(),
        }
    }
    /// Append one stable row, preserving cursor and selection keys.
    #[command]
    pub fn append(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.order.len() >= MAX_ROWS {
            return Err(Error::Invalid(format!(
                "table gym is limited to {MAX_ROWS} rows"
            )));
        }
        self.order.push(self.next_row);
        self.next_row += 1;
        self.revision += 1;
        self.publish(c)?;
        self.status = "Appended one keyed row".into();
        Ok(())
    }
    /// Reverse display order while preserving surviving keyed state.
    #[command]
    pub fn reorder(&mut self, c: &mut dyn Context) -> Result<()> {
        self.order.reverse();
        self.revision += 1;
        self.publish(c)?;
        self.status = "Reversed rows; stable cursor and selection keys survived".into();
        Ok(())
    }
    /// Republish geometry at the same data revision with the other glyph
    /// preset.
    #[command]
    pub fn toggle_ascii(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ascii = !self.ascii;
        self.publish(c)?;
        self.status = if self.ascii {
            "ASCII separators and scrollbar tracks"
        } else {
            "Unicode separators and scrollbar tracks"
        }
        .into();
        Ok(())
    }
    /// Republish alternate explicit widths at the same data revision.
    #[command]
    pub fn toggle_widths(&mut self, c: &mut dyn Context) -> Result<()> {
        self.compact = !self.compact;
        self.publish(c)?;
        self.status = "Changed prepared column widths without changing the data revision".into();
        Ok(())
    }
    /// Toggle the live palette between dark and light.
    #[command]
    pub fn theme(&mut self, c: &mut dyn Context) -> Result<()> {
        self.light = !self.light;
        c.set_theme(if self.light {
            themes::solarized_light()
        } else {
            themes::default_dark()
        });
        self.status = if self.light {
            "Light theme"
        } else {
            "Dark theme"
        }
        .into();
        Ok(())
    }
    /// Move to one-based display coordinates through TableView's public API.
    #[command]
    pub fn cursor(&self, c: &mut dyn Context, row: usize, column: usize) -> Result<()> {
        let table = self.table_id()?;
        c.with_widget_mut(table, |table: &mut TableView, c| table.goto(c, row, column))
    }
    /// Read bounded revision-addressed state and exact recorded copy bytes.
    #[command]
    pub fn state(&mut self, c: &mut dyn Context) -> Result<ArgValue> {
        self.record(c)?;
        let table = self.table_id()?;
        let mut map = c.with_widget_mut(table, |table: &mut TableView, c| {
            let mut map = BTreeMap::new();
            let cursor = table.cursor();
            let index = table.cursor_index();
            map.insert(
                "revision".into(),
                ArgValue::UInt(table.table().data().revision()),
            );
            map.insert(
                "rows".into(),
                ArgValue::UInt(table.table().data().rows().len() as u64),
            );
            map.insert(
                "geometry".into(),
                ArgValue::UInt(table.table().geometry_key()),
            );
            map.insert(
                "row_key".into(),
                cursor
                    .as_ref()
                    .map_or(ArgValue::Null, |key| ArgValue::String(key.row.to_string())),
            );
            map.insert(
                "column_key".into(),
                cursor.as_ref().map_or(ArgValue::Null, |key| {
                    ArgValue::String(key.column.to_string())
                }),
            );
            map.insert(
                "row".into(),
                index.map_or(ArgValue::Null, |(r, _)| ArgValue::UInt((r + 1) as u64)),
            );
            map.insert(
                "column".into(),
                index.map_or(ArgValue::Null, |(_, col)| ArgValue::UInt((col + 1) as u64)),
            );
            map.insert(
                "selected_rows".into(),
                ArgValue::UInt(table.selection().rows().len() as u64),
            );
            map.insert(
                "selected_columns".into(),
                ArgValue::UInt(table.selection().columns().len() as u64),
            );
            if let Some(view) = table.initial_focus().and_then(|body| c.view_of(body)) {
                map.insert("scroll_x".into(), ArgValue::UInt(u64::from(view.scroll.x)));
                map.insert("scroll_y".into(), ArgValue::UInt(u64::from(view.scroll.y)));
            }
            Ok(map)
        })?;
        map.insert("ascii".into(), ArgValue::Bool(self.ascii));
        map.insert("compact".into(), ArgValue::Bool(self.compact));
        map.insert("copies".into(), ArgValue::UInt(self.copies as u64));
        map.insert(
            "activations".into(),
            ArgValue::UInt(self.activations as u64),
        );
        map.insert("copy".into(), ArgValue::String(self.last_copy.clone()));
        map.insert(
            "detail".into(),
            ArgValue::String(self.last_detail.to_string()),
        );
        Ok(ArgValue::Map(map))
    }
    /// Return the mounted table node or a clear lifecycle error.
    fn table_id(&self) -> Result<TypedId<TableView>> {
        self.table
            .ok_or_else(|| Error::NotFound("table gym body".into()))
    }
    /// Build the bounded deterministic snapshot and its explicit geometry.
    fn prepared(&self) -> Result<Arc<PreparedTable>> {
        let columns = [
            ("id", "ID"),
            ("name", "Name"),
            ("empty", "Empty"),
            ("lines", "Multiline"),
            ("quotes", "Quoted / tabs"),
            ("detail", "Full value"),
        ]
        .into_iter()
        .map(|(key, label)| TableColumn::new(key, label))
        .collect();
        let rows = self
            .order
            .iter()
            .map(|row| TableRow::new(format!("row-{row:03}"), values(*row)))
            .collect();
        let data = Arc::new(TableData::new("table", self.revision, columns, rows)?);
        Ok(Arc::new(table::prepare(
            data,
            TableLayoutOptions {
                widths: if self.compact {
                    vec![5, 16, 5, 12, 18, 100]
                } else {
                    vec![12, 20, 10, 16, 18, 200]
                },
                glyphs: if self.ascii {
                    TableGlyphs::ASCII
                } else {
                    TableGlyphs::UNICODE
                },
                ..Default::default()
            },
        )?))
    }
    /// Publish the gym's current keyed model through the public table API.
    fn publish(&self, c: &mut dyn Context) -> Result<()> {
        let prepared = self.prepared()?;
        c.with_widget_mut(self.table_id()?, |table: &mut TableView, c| {
            table.publish(c, prepared)
        })
    }
    /// Drain bounded public table events into the recording host, resolving
    /// each activated cell's full value against the published data.
    fn record(&mut self, c: &mut dyn Context) -> Result<()> {
        let events = c.with_widget_mut(self.table_id()?, |table: &mut TableView, _| {
            Ok(table
                .take_events(EVENT_CAPACITY)
                .into_iter()
                .map(|event| {
                    let detail = match &event {
                        TableEvent::Activate { cell, .. } => table.table().data().cell(cell),
                        TableEvent::Copy { .. } => None,
                    }
                    .map_or_else(|| Arc::from(""), TableCell::shared_text);
                    (event, detail)
                })
                .collect::<Vec<_>>())
        })?;
        for (event, detail) in events {
            match event {
                TableEvent::Copy { revision, text, .. } => {
                    self.copies += 1;
                    self.status = format!(
                        "Recorded copy {}: {} exact bytes at revision {revision}",
                        self.copies,
                        text.len()
                    );
                    self.last_copy = text;
                }
                TableEvent::Activate { revision, cell, .. } => {
                    self.activations += 1;
                    self.last_detail = detail;
                    self.status = format!(
                        "Cell detail {} / {}: {} full bytes at revision {revision}",
                        cell.row,
                        cell.column,
                        self.last_detail.len()
                    );
                }
            }
        }
        Ok(())
    }
}
/// Generate one deterministic row, including the long-cell fixture.
fn values(row: usize) -> Vec<TableCell> {
    let name = match row {
        0 => "Alpha 界".into(),
        1 => "Beta e\u{301}".into(),
        _ => format!("Report {row:03} · 界"),
    };
    let detail = if row == 0 {
        "Full value: Unicode 界, emoji 🦀, combining e\u{301}; tabs\tand newlines\nremain exact. "
            .repeat(40)
    } else {
        format!("Deterministic row {row}: score {}", (row * 17) % 101)
    };
    vec![
        row.to_string().into(),
        name.into(),
        "".into(),
        "line\nnext".into(),
        "say \"yes\"\tthen".into(),
        detail.into(),
    ]
}
impl Register for TableGym {
    fn register(setup: &mut Setup) -> Result<()> {
        TableView::register(setup)?;
        setup.add_commands::<Self>()
    }
}
impl Widget for TableGym {
    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::new(1, 0, 1, 0))
    }
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let mut table = TableView::new(
            self.prepared()?,
            TableViewOptions {
                event_capacity: EVENT_CAPACITY,
                event_bytes: COPY_LIMIT * 2,
                copy_bytes: COPY_LIMIT,
                ..Default::default()
            },
        );
        table.set_event_wake(c.wake_handle(PollLifetime::Node)?);
        self.table = Some(c.add_child(c.node_id(), table)?);
        Ok(())
    }
    fn render(&mut self, r: &mut Render, c: &dyn ViewContext) -> Result<()> {
        let area = c.view().outer_rect_local();
        r.text(
            "table/gutter",
            Line::new(0, 0, area.w),
            &format!(
                "{} rows · revision {} · {} · {} widths",
                self.order.len(),
                self.revision,
                if self.ascii { "ASCII" } else { "Unicode" },
                if self.compact { "compact" } else { "wide" }
            ),
        )?;
        if area.h > 0 {
            r.text(
                "table/gutter",
                Line::new(0, area.h - 1, area.w),
                &self.status,
            )?;
        }
        Ok(())
    }
    fn poll(&mut self, c: &mut dyn Context) -> Result<Option<Duration>> {
        self.record(c)?;
        Ok(None)
    }
    fn name(&self) -> NodeName {
        NodeName::convert("table_gym")
    }
}

/// Keep the real table input active while exposing the gym's publication
/// controls.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.script("table", BINDINGS)
}
