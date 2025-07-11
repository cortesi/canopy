use canopy_core as canopy;

use canopy_core::{
    Context, Layout, Node, NodeState, Result, StatefulNode, derive_commands, geom::Expanse,
};

/// Panes manages a set of child nodes arranged in a 2d grid.
#[derive(canopy_core::StatefulNode)]
pub struct Panes<N: Node> {
    pub children: Vec<Vec<N>>,
    pub state: NodeState,
}

#[derive_commands]
impl<N> Panes<N>
where
    N: Node,
{
    pub fn new(n: N) -> Self {
        Panes {
            children: vec![vec![n]],
            state: NodeState::default(),
        }
    }

    /// Get the offset of the current focus in the children vector.
    pub fn focus_coords(&mut self, c: &dyn Context) -> Option<(usize, usize)> {
        for (x, col) in self.children.iter_mut().enumerate() {
            for (y, row) in col.iter_mut().enumerate() {
                if c.is_on_focus_path(row) {
                    return Some((x, y));
                }
            }
        }
        None
    }

    /// Delete the focus node. If a column ends up empty, it is removed.
    pub fn delete_focus(&mut self, c: &mut dyn Context) -> Result<()> {
        if let Some((x, y)) = self.focus_coords(c) {
            c.focus_next(self);
            self.children[x].remove(y);
            if self.children[x].is_empty() {
                self.children.remove(x);
            }
        }
        Ok(())
    }

    /// Insert a node, splitting vertically. If we have a focused node, the new
    /// node is inserted in a row beneath it. If not, a new column is added.
    pub fn insert_row(&mut self, c: &mut dyn Context, n: N)
    where
        N: Node,
    {
        if let Some((x, y)) = self.focus_coords(c) {
            self.children[x].insert(y, n);
        } else {
            self.children.push(vec![n]);
        }
    }

    /// Insert a node in a new column. If we have a focused node, the new node
    /// is added in a new column to the right.
    pub fn insert_col(&mut self, c: &mut dyn Context, mut n: N) -> Result<()>
    where
        N: Node,
    {
        let coords = self.focus_coords(c);
        c.focus_next(&mut n);
        if let Some((x, _)) = coords {
            self.children.insert(x + 1, vec![n])
        } else {
            self.children.push(vec![n])
        }
        Ok(())
    }

    /// Returns the shape of the current child grid
    fn shape(&self) -> Vec<u32> {
        let mut ret = vec![];
        for i in &self.children {
            ret.push(i.len() as u32)
        }
        ret
    }
}

impl<N: Node> Node for Panes<N> {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for col in &mut self.children {
            for row in col {
                f(row)?
            }
        }
        Ok(())
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.fill(sz)?;
        let vp = self.vp();
        let lst = vp.view().split_panes(&self.shape())?;
        for (ci, col) in self.children.iter_mut().enumerate() {
            for (ri, row) in col.iter_mut().enumerate() {
                l.place(row, lst[ci][ri])?;
            }
        }
        Ok(())
    }
}
