use crate::{
    Context, NodeId, ViewContext,
    commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
    derive_commands,
    error::{Error, Result},
    layout::Layout,
    state::NodeName,
    widget::Widget,
};

/// Internal column container for panes.
struct PaneColumn;

impl CommandNode for PaneColumn {
    fn commands() -> Vec<CommandSpec> {
        Vec::new()
    }

    fn dispatch(&mut self, _c: &mut dyn Context, cmd: &CommandInvocation) -> Result<ReturnValue> {
        Err(Error::UnknownCommand(cmd.command.clone()))
    }
}

impl Widget for PaneColumn {
    fn layout(&self) -> Layout {
        Layout::column().flex_horizontal(1).flex_vertical(1)
    }

    fn render(&mut self, _rndr: &mut crate::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("pane_column")
    }
}

/// Panes manages a set of child nodes arranged in a 2d grid.
pub struct Panes {
    /// Child nodes arranged by column.
    columns: Vec<Vec<NodeId>>,
    /// Column container nodes.
    column_nodes: Vec<NodeId>,
}

#[derive_commands]
impl Panes {
    /// Construct panes with no children.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            column_nodes: Vec::new(),
        }
    }

    /// Construct panes with a single child.
    pub fn with_child(child: NodeId) -> Self {
        Self {
            columns: vec![vec![child]],
            column_nodes: Vec::new(),
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
            c.focus_next_global();
            self.columns[x].remove(y);
            if self.columns[x].is_empty() {
                self.columns.remove(x);
                if x < self.column_nodes.len() {
                    self.column_nodes.remove(x);
                }
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
            while self.column_nodes.len() < self.columns.len() {
                let column_node = c.add(Box::new(PaneColumn));
                self.column_nodes.push(column_node);
            }
            self.columns.insert(x + 1, vec![n]);
            let column_node = c.add(Box::new(PaneColumn));
            self.column_nodes.insert(x + 1, column_node);
        } else {
            self.columns.push(vec![n]);
        }
        self.sync_layout(c)
    }

    /// Sync child layout and grid placement styles.
    fn sync_layout(&mut self, c: &mut dyn Context) -> Result<()> {
        while self.column_nodes.len() < self.columns.len() {
            let column_node = c.add(Box::new(PaneColumn));
            self.column_nodes.push(column_node);
        }

        let active_columns: Vec<NodeId> = self
            .column_nodes
            .iter()
            .copied()
            .take(self.columns.len())
            .collect();

        c.set_children(active_columns.clone())?;

        c.with_layout(&mut |layout| {
            *layout = Layout::row().flex_horizontal(1).flex_vertical(1);
        })?;

        for (idx, column_node) in active_columns.iter().enumerate() {
            let pane_nodes = self.columns.get(idx).cloned().unwrap_or_default();
            c.set_children_of(*column_node, pane_nodes.clone())?;
            c.with_layout_of(*column_node, &mut |layout| {
                *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
            })?;
            for pane in pane_nodes {
                c.with_layout_of(pane, &mut |layout| {
                    *layout = Layout::fill();
                })?;
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
    fn render(&mut self, _rndr: &mut crate::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("panes")
    }
}
