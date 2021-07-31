use duplicate::duplicate;
use std::marker::PhantomData;

use crate as canopy;
use crate::{
    fit_and_update,
    state::{NodeState, StatefulNode},
    Actions, Canopy, Node, Result,
};

/// Panes manages a set of child nodes arranged in a 2d grid.
#[derive(StatefulNode)]
pub struct Panes<S, A: Actions, N: Node<S, A>> {
    _marker: PhantomData<(S, A)>,
    pub children: Vec<Vec<N>>,
    pub state: NodeState,
}

impl<S, A: Actions, N> Panes<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new(n: N) -> Self {
        Panes {
            children: vec![vec![n]],
            state: NodeState::default(),
            _marker: PhantomData,
        }
    }
    /// Get the offset of the current focus in the children vector.
    pub fn focus_coords(&mut self, app: &Canopy<S, A>) -> Option<(usize, usize)> {
        for (x, col) in self.children.iter_mut().enumerate() {
            for (y, row) in col.iter_mut().enumerate() {
                if app.on_focus_path(row) {
                    return Some((x, y));
                }
            }
        }
        None
    }
    /// Delete the focus node. If a column ends up empty, it is removed.
    pub fn delete_focus(&mut self, app: &mut Canopy<S, A>) -> Result<()> {
        if let Some((x, y)) = self.focus_coords(app) {
            app.focus_next(self)?;
            self.children[x].remove(y);
            if self.children[x].is_empty() {
                self.children.remove(x);
            }
            self.layout(app)?;
            app.taint_tree(self)?;
        }
        Ok(())
    }
    /// Insert a node, splitting vertically. If we have a focused node, the new
    /// node is inserted in a row beneath it. If not, a new column is added.
    pub fn insert_row(&mut self, app: &Canopy<S, A>, n: N) -> Result<()>
    where
        N: Node<S, A>,
    {
        if let Some((x, y)) = self.focus_coords(app) {
            self.children[x].insert(y, n);
        } else {
            self.children.push(vec![n]);
        }
        app.taint_tree(self)
    }
    /// Insert a node in a new column. If we have a focused node, the new node
    /// is added in a new column to the right.
    pub fn insert_col(&mut self, app: &mut Canopy<S, A>, mut n: N) -> Result<()>
    where
        N: Node<S, A>,
    {
        let coords = self.focus_coords(app);
        app.focus_next(&mut n)?;
        if let Some((x, _)) = coords {
            self.children.insert(x + 1, vec![n])
        } else {
            self.children.push(vec![n])
        }
        app.taint_tree(self)
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

impl<S, A: Actions, N: Node<S, A>> Node<S, A> for Panes<S, A, N> {
    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<S, A>])) -> Result<()>,
    ) -> Result<()> {
        for col in reference([self.children]) {
            for row in col {
                f(row)?
            }
        }
        Ok(())
    }

    fn render(&self, _: &mut Canopy<S, A>) -> Result<()> {
        // FIXME - this should probably clear the area if the last node is
        // deleted.
        Ok(())
    }

    fn layout(&mut self, app: &mut Canopy<S, A>) -> Result<()> {
        let l = self.screen().split_panes(&self.shape())?;
        for (ci, col) in self.children.iter_mut().enumerate() {
            for (ri, row) in col.iter_mut().enumerate() {
                fit_and_update(app, l[ci][ri], row)?;
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
        render::test::TestRender,
        tutils::utils,
    };

    #[test]
    fn tlayout() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        let tn = utils::TBranch::new("a");
        let mut p: Panes<utils::State, utils::TActions, utils::TBranch> = Panes::new(tn);
        let r = Rect {
            tl: Point::zero(),
            w: 100,
            h: 100,
        };
        fit_and_update(&mut app, r, &mut p)?;

        assert_eq!(p.shape(), vec![1]);
        let tn = utils::TBranch::new("b");
        p.insert_col(&mut app, tn)?;
        fit_and_update(&mut app, r, &mut p)?;

        assert_eq!(p.shape(), vec![1, 1]);
        app.set_focus(&mut p.children[0][0].a)?;
        fit_and_update(&mut app, r, &mut p)?;

        let tn = utils::TBranch::new("c");
        assert_eq!(p.focus_coords(&mut app), Some((0, 0)));
        p.insert_row(&mut app, tn)?;
        fit_and_update(&mut app, r, &mut p)?;

        assert_eq!(p.shape(), vec![2, 1]);

        app.set_focus(&mut p.children[1][0].a)?;
        assert_eq!(p.focus_coords(&mut app), Some((1, 0)));
        Ok(())
    }
}
