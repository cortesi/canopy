use std::marker::PhantomData;

use crate as canopy;
use crate::{
    error::Result,
    geom::{Point, Rect, Size},
    node::Node,
    state::{NodeState, StatefulNode},
    Canopy,
};

#[derive(StatefulNode)]
pub struct List<S, N>
where
    N: Node<S>,
{
    _marker: PhantomData<S>,
    items: Vec<N>,
    // Offset within the virtual rectangle
    pub offset: Point,
    pub focus: u32,
    state: NodeState,
}

impl<S, N> List<S, N>
where
    N: Node<S>,
{
    pub fn new(c: Vec<N>) -> Self {
        List {
            _marker: PhantomData,
            items: c,
            offset: Point::zero(),
            focus: 0,
            state: NodeState::default(),
        }
    }
}

impl<S, N> Node<S> for List<S, N>
where
    N: Node<S>,
{
    fn children(&self, f: &mut dyn FnMut(&dyn Node<S>) -> Result<()>) -> Result<()> {
        for i in &self.items {
            f(i)?
        }
        Ok(())
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S>) -> Result<()>) -> Result<()> {
        for i in self.items.iter_mut() {
            f(i)?
        }
        Ok(())
    }

    fn fit(&mut self, app: &mut Canopy<S>, r: Size) -> Result<Size> {
        let mut w = 0;
        let mut h = 0;
        for i in &mut self.items {
            let v = i.fit(app, r)?;
            w = w.max(v.w);
            h += v.h
        }
        Ok(Size { w, h })
    }

    fn layout(&mut self, app: &mut Canopy<S>, screen: Rect) -> Result<()> {
        let myvp = self.state().viewport;
        let mut voffset: u16 = 0;
        for itm in &mut self.items {
            let item_virt = itm.fit(app, screen.into())?.rect().shift(0, voffset as i16);
            if let Some(vp) = myvp.map(item_virt)? {
                itm.state_mut().viewport = vp;
                itm.layout(app, vp.screen())?;
                itm.unhide();
            } else {
                itm.hide();
            }
            voffset += itm.outer().h;
        }
        Ok(())
    }

    fn render(&self, app: &mut Canopy<S>) -> Result<()> {
        // FIXME: only paint needed areas
        app.render.fill("", self.view(), ' ')?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::test::TestRender;
    use crate::tutils::utils::{tcanopy, TFixed};

    #[test]
    fn drawnodes() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = tcanopy(&mut tr);
        let mut lst = List::new(vec![TFixed::new(10, 10)]);
        let _ = lst.fit(&mut app, Size::new(20, 20))?;
        // lst.layout(
        //     &mut app,
        //     Rect {
        //         tl: Point::zero(),
        //         w: 10,
        //         h: 10,
        //     },
        // )?;

        Ok(())
    }
}
