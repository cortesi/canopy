use crate as canopy;
use crate::{
    state::{NodeState, StatefulNode},
    *,
};

/// Panes manages a set of child nodes arranged in a 2d grid.
#[derive(StatefulNode)]
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
    pub fn focus_coords(&mut self, c: &Canopy) -> Option<(usize, usize)> {
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
    pub fn delete_focus(&mut self, c: &mut Canopy) -> Result<()> {
        if let Some((x, y)) = self.focus_coords(c) {
            c.focus_next(self)?;
            self.children[x].remove(y);
            if self.children[x].is_empty() {
                self.children.remove(x);
            }
            c.taint_tree(self);
        }
        Ok(())
    }

    /// Insert a node, splitting vertically. If we have a focused node, the new
    /// node is inserted in a row beneath it. If not, a new column is added.
    pub fn insert_row(&mut self, c: &mut Canopy, n: N)
    where
        N: Node,
    {
        if let Some((x, y)) = self.focus_coords(c) {
            self.children[x].insert(y, n);
        } else {
            self.children.push(vec![n]);
        }
        c.taint_tree(self);
    }

    /// Insert a node in a new column. If we have a focused node, the new node
    /// is added in a new column to the right.
    pub fn insert_col(&mut self, c: &mut Canopy, mut n: N) -> Result<()>
    where
        N: Node,
    {
        let coords = self.focus_coords(c);
        c.focus_next(&mut n)?;
        if let Some((x, _)) = coords {
            self.children.insert(x + 1, vec![n])
        } else {
            self.children.push(vec![n])
        }
        c.taint_tree(self);
        Ok(())
    }

    /// Returns the shape of the current child grid
    fn shape(&self) -> Vec<u16> {
        let mut ret = vec![];
        for i in &self.children {
            ret.push(i.len() as u16)
        }
        ret
    }
}

impl<N: Node> Node for Panes<N> {
    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for col in &mut self.children {
            for row in col {
                f(row)?
            }
        }
        Ok(())
    }

    fn render(&mut self, _: &dyn Core, _rndr: &mut Render) -> Result<()> {
        let l = self.vp().screen_rect().split_panes(&self.shape())?;
        for (ci, col) in self.children.iter_mut().enumerate() {
            for (ri, row) in col.iter_mut().enumerate() {
                place(row, l[ci][ri])?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::{Point, Rect},
        tutils::utils,
    };

    #[test]
    fn tlayout() -> Result<()> {
        let mut c = Canopy::new();
        let tn = utils::TBranch::new("a");
        let mut p: Panes<utils::TBranch> = Panes::new(tn);
        let r = Rect {
            tl: Point::zero(),
            w: 100,
            h: 100,
        };
        place(&mut p, r)?;

        assert_eq!(p.shape(), vec![1]);
        let tn = utils::TBranch::new("b");
        p.insert_col(&mut c, tn)?;
        place(&mut p, r)?;

        assert_eq!(p.shape(), vec![1, 1]);
        c.set_focus(&mut p.children[0][0].a);
        place(&mut p, r)?;

        let tn = utils::TBranch::new("c");
        assert_eq!(p.focus_coords(&c), Some((0, 0)));
        p.insert_row(&mut c, tn);
        place(&mut p, r)?;

        assert_eq!(p.shape(), vec![2, 1]);

        c.set_focus(&mut p.children[1][0].a);
        assert_eq!(p.focus_coords(&c), Some((1, 0)));
        Ok(())
    }
}
