use canopy::{
    Context, NodeId, ViewContext, Widget, command,
    commands::{CommandNode, CommandSpec},
    derive_commands,
    error::Result,
    layout::{Layout, Sizing},
    state::NodeName,
};

/// Internal column container for panes.
struct PaneColumn;

impl CommandNode for PaneColumn {
    fn commands() -> &'static [&'static CommandSpec] {
        &[]
    }
}

impl Widget for PaneColumn {
    fn layout(&self) -> Layout {
        Layout::column().flex_horizontal(1).flex_vertical(1)
    }

    fn render(&mut self, _rndr: &mut canopy::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
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

    /// Return the active column container node IDs in order.
    pub fn column_nodes(&self) -> Vec<NodeId> {
        self.column_nodes
            .iter()
            .copied()
            .take(self.columns.len())
            .collect()
    }

    /// Return the focused column index, if any.
    pub fn focused_column_index(&self, c: &dyn Context) -> Option<usize> {
        self.focus_coords(c).map(|(x, _)| x)
    }

    /// Move focus by a signed column offset (wraps around).
    #[command]
    pub fn focus_column(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let columns = self.column_nodes();
        if columns.is_empty() {
            return Ok(());
        }
        let current = self.focused_column_index(c).unwrap_or(0);
        let len = columns.len() as i32;
        let next = ((current as i32 + delta).rem_euclid(len)) as usize;
        focus_column_node(c, columns[next]);
        Ok(())
    }

    /// Move focus to the next column.
    #[command]
    pub fn next_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus_column(c, 1)
    }

    /// Move focus to the previous column.
    #[command]
    pub fn prev_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus_column(c, -1)
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
            self.columns[x].remove(y);
            let mut focus_idx = x;
            if self.columns[x].is_empty() {
                self.columns.remove(x);
                if x < self.column_nodes.len() {
                    self.column_nodes.remove(x);
                }
                if focus_idx >= self.columns.len() && !self.columns.is_empty() {
                    focus_idx = self.columns.len() - 1;
                }
            }
            self.sync_layout(c)?;
            if let Some(column_node) = self.column_nodes.get(focus_idx).copied() {
                focus_column_node(c, column_node);
            }
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
        let target_idx = if let Some((x, _)) = coords {
            while self.column_nodes.len() < self.columns.len() {
                let column_node = c.create_detached(PaneColumn);
                self.column_nodes.push(column_node);
            }
            self.columns.insert(x + 1, vec![n]);
            let column_node = c.create_detached(PaneColumn);
            self.column_nodes.insert(x + 1, column_node);
            x + 1
        } else {
            self.columns.push(vec![n]);
            self.columns.len() - 1
        };
        self.sync_layout(c)?;
        if let Some(column_node) = self.column_nodes.get(target_idx).copied() {
            focus_column_node(c, column_node);
        }
        Ok(())
    }

    /// Sync child layout and grid placement styles.
    fn sync_layout(&mut self, c: &mut dyn Context) -> Result<()> {
        while self.column_nodes.len() < self.columns.len() {
            let column_node = c.create_detached(PaneColumn);
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
                    layout.width = Sizing::Flex(1);
                    layout.height = Sizing::Flex(1);
                })?;
            }
        }

        Ok(())
    }
}

/// Focus the first focusable leaf under a column, falling back to the first leaf.
fn focus_column_node(c: &mut dyn Context, column_node: NodeId) {
    let focusables = c.focusable_leaves(column_node);
    if let Some(target) = focusables
        .first()
        .copied()
        .or_else(|| first_leaf(c, column_node))
    {
        c.set_focus(target);
    }
}

/// Return the first leaf node under a root using pre-order traversal.
fn first_leaf(ctx: &dyn Context, root: NodeId) -> Option<NodeId> {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let children = ctx.children_of(id);
        if children.is_empty() {
            return Some(id);
        }
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

impl Default for Panes {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Panes {
    fn render(&mut self, _rndr: &mut canopy::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        self.sync_layout(c)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("panes")
    }
}
