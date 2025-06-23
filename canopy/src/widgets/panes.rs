use crate as canopy;
use crate::{
    geom::Expanse,
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
            c.focus_next(self);
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
        c.focus_next(&mut n);
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
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for col in &mut self.children {
            for row in col {
                f(row)?
            }
        }
        Ok(())
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.fill(self, sz)?;
        let vp = self.vp();
        let lst = vp.canvas.rect().split_panes(&self.shape())?;
        for (ci, col) in self.children.iter_mut().enumerate() {
            for (ri, row) in col.iter_mut().enumerate() {
                l.place(row, vp, lst[ci][ri])?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutils::*;

    fn split(len: u16, n: u16) -> Vec<u16> {
        let w = len / n;
        let rem = len % n;
        (0..n).map(|i| if i < rem { w + 1 } else { w }).collect()
    }

    #[test]
    fn tlayout() -> Result<()> {
        let mut c = Canopy::new();
        let tn = Ba::new();
        let mut p: Panes<Ba> = Panes::new(tn);
        let l = Layout {};
        let e = Expanse { w: 100, h: 100 };

        p.layout(&l, e)?;

        assert_eq!(p.shape(), vec![1]);
        let tn = Ba::new();
        p.insert_col(&mut c, tn)?;
        p.layout(&l, e)?;

        assert_eq!(p.shape(), vec![1, 1]);
        c.set_focus(&mut p.children[0][0].a);
        p.layout(&l, e)?;

        let tn = Ba::new();
        assert_eq!(p.focus_coords(&c), Some((0, 0)));
        p.insert_row(&mut c, tn);
        p.layout(&l, e)?;

        assert_eq!(p.shape(), vec![2, 1]);

        c.set_focus(&mut p.children[1][0].a);
        assert_eq!(p.focus_coords(&c), Some((1, 0)));
        Ok(())
    }

    #[derive(StatefulNode)]
    struct Root {
        state: NodeState,
        panes: Panes<TFixed>,
    }

    #[derive_commands]
    impl Root {
        fn new() -> Self {
            Root {
                state: NodeState::default(),
                panes: Panes::new(TFixed::new(1, 1)),
            }
        }
    }

    impl Node for Root {
        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.panes)
        }

        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, sz)?;
            let vp = self.vp();
            let parts = vp.view.split_horizontal(2)?;
            l.place(&mut self.panes, vp, parts[1])?;
            Ok(())
        }
    }

    #[test]
    fn tlayout_offset() -> Result<()> {
        let mut c = Canopy::new();
        let mut root = Root::new();
        let l = Layout {};

        c.set_root_size(Expanse::new(20, 10), &mut root)?;
        root.panes.insert_col(&mut c, TFixed::new(1, 1))?;
        root.layout(&l, Expanse::new(20, 10))?;

        assert_eq!(root.panes.children.len(), 2);
        assert_eq!(root.panes.children[0][0].vp().position.x, 10);
        assert_eq!(root.panes.children[1][0].vp().position.x, 15);
        Ok(())
    }

    fn check_tiles(p: &Panes<TFixed>) -> Result<()> {
        let vp = p.vp();
        let shape = p.shape();
        let col_widths = split(vp.canvas.w, shape.len() as u16);
        let mut x = vp.position.x;
        let mut expect_pos: Vec<(u16, u16)> = vec![];
        for (ci, rows) in shape.iter().enumerate() {
            let heights = split(vp.canvas.h, *rows);
            let mut y = vp.position.y;
            for h in heights {
                expect_pos.push((x, y));
                y += h;
            }
            x += col_widths[ci as usize];
        }
        expect_pos.sort();

        let mut actual_pos: Vec<(u16, u16)> = vec![];
        for col in &p.children {
            for ch in col {
                let r = ch.vp().screen_rect();
                actual_pos.push((r.tl.x, r.tl.y));
            }
        }
        actual_pos.sort();

        assert_eq!(actual_pos, expect_pos);
        Ok(())
    }

    #[test]
    fn tlayout_tiling_and_scroll() -> Result<()> {
        let mut c = Canopy::new();
        let mut p: Panes<TFixed> = Panes::new(TFixed::new(1, 1));
        let l = Layout {};

        p.insert_col(&mut c, TFixed::new(1, 1))?;
        p.insert_col(&mut c, TFixed::new(1, 1))?;
        c.set_focus(&mut p.children[0][0]);
        p.insert_row(&mut c, TFixed::new(1, 1));
        p.layout(&l, Expanse::new(9, 6))?;
        // verify positions without scrolling
        check_tiles(&p)?;
        
        // Scroll the container and insert more splits
        for _ in 0..2 {
            p.__vp_mut().scroll_right();
        }
        p.insert_col(&mut c, TFixed::new(1, 1))?;
        p.insert_row(&mut c, TFixed::new(1, 1));
        p.layout(&l, Expanse::new(9, 6))?;
        check_tiles(&p)?;
        Ok(())
    }
}
