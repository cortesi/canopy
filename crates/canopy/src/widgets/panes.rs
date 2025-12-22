use taffy::{
    geometry::Line,
    style::{Display, GridPlacement, Style, TrackSizingFunction},
    style_helpers::{FromFlex, line},
};

use crate::{
    Context, NodeId, ViewContext, derive_commands,
    error::Result,
    event::Event,
    geom::Rect,
    state::NodeName,
    widget::{EventOutcome, Widget},
};

/// Panes manages a set of child nodes arranged in a 2d grid.
pub struct Panes {
    /// Child nodes arranged by column.
    columns: Vec<Vec<NodeId>>,
}

#[derive_commands]
impl Panes {
    /// Construct panes with no children.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
        }
    }

    /// Construct panes with a single child.
    pub fn with_child(child: NodeId) -> Self {
        Self {
            columns: vec![vec![child]],
        }
    }

    /// Get the offset of the current focus in the children vector.
    pub fn focus_coords(&self, c: &dyn Context) -> Option<(usize, usize)> {
        for (x, col) in self.columns.iter().enumerate() {
            for (y, row) in col.iter().enumerate() {
                if c.node_is_on_focus_path(*row) {
                    return Some((x, y));
                }
            }
        }
        None
    }

    /// Delete the focus node. If a column ends up empty, it is removed.
    pub fn delete_focus(&mut self, c: &mut dyn Context) -> Result<()> {
        if let Some((x, y)) = self.focus_coords(c) {
            c.focus_next(c.root_id());
            self.columns[x].remove(y);
            if self.columns[x].is_empty() {
                self.columns.remove(x);
            }
            self.sync_layout(c)?;
        }
        Ok(())
    }

    /// Insert a node, splitting vertically.
    pub fn insert_row(&mut self, c: &mut dyn Context, n: NodeId) -> Result<()> {
        if let Some((x, y)) = self.focus_coords(c) {
            self.columns[x].insert(y, n);
        } else {
            self.columns.push(vec![n]);
        }
        self.sync_layout(c)
    }

    /// Insert a node in a new column.
    pub fn insert_col(&mut self, c: &mut dyn Context, n: NodeId) -> Result<()> {
        let coords = self.focus_coords(c);
        if let Some((x, _)) = coords {
            self.columns.insert(x + 1, vec![n]);
        } else {
            self.columns.push(vec![n]);
        }
        self.sync_layout(c)
    }

    /// Sync child layout and grid placement styles.
    fn sync_layout(&self, c: &mut dyn Context) -> Result<()> {
        let node_id = c.node_id();
        let mut children = Vec::new();
        let mut rows = 0usize;
        for col in &self.columns {
            rows = rows.max(col.len());
            children.extend(col.iter().copied());
        }

        c.set_children(node_id, children)?;

        let cols = self.columns.len().max(1);
        let rows = rows.max(1);

        let mut col_tracks = Vec::new();
        for _ in 0..cols {
            col_tracks.push(TrackSizingFunction::from_flex(1.0));
        }

        let mut row_tracks = Vec::new();
        for _ in 0..rows {
            row_tracks.push(TrackSizingFunction::from_flex(1.0));
        }

        let mut update_panes = |style: &mut Style| {
            style.display = Display::Grid;
            style.grid_template_columns = col_tracks.clone();
            style.grid_template_rows = row_tracks.clone();
        };
        c.with_style(node_id, &mut update_panes)?;

        for (col_idx, col) in self.columns.iter().enumerate() {
            for (row_idx, child) in col.iter().enumerate() {
                let mut update_child = |style: &mut Style| {
                    style.grid_column = line::<Line<GridPlacement>>((col_idx + 1) as i16);
                    style.grid_row = line::<Line<GridPlacement>>((row_idx + 1) as i16);
                };
                c.with_style(*child, &mut update_child)?;
            }
        }

        Ok(())
    }
}

impl Default for Panes {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Panes {
    fn render(
        &mut self,
        _rndr: &mut crate::render::Render,
        _area: Rect,
        _ctx: &dyn ViewContext,
    ) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn name(&self) -> NodeName {
        NodeName::convert("panes")
    }
}
