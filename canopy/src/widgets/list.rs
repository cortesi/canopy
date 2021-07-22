use std::marker::PhantomData;

use crate as canopy;
use crate::{
    error::Result,
    geom::{Point, Rect},
    layout::ConstrainedWidthLayout,
    node::Node,
    state::{NodeState, StatefulNode},
    widgets::frame::FrameContent,
    Canopy,
};

struct Item<S, N: Node<S> + ConstrainedWidthLayout<S>> {
    _marker: PhantomData<S>,
    itm: N,
}

#[derive(StatefulNode)]
pub struct List<S, N: Node<S> + ConstrainedWidthLayout<S>> {
    _marker: PhantomData<S>,
    items: Vec<Item<S, N>>,
    // Offset within the virtual rectangle
    pub offset: Point,
    pub focus: u32,
    state: NodeState,
}

impl<S, N> List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    pub fn new(c: Vec<N>) -> Self {
        List {
            _marker: PhantomData,
            items: c
                .into_iter()
                .map(|x| Item {
                    itm: x,
                    _marker: PhantomData,
                })
                .collect(),
            offset: Point::zero(),
            focus: 0,
            state: NodeState::default(),
        }
    }
    pub fn scroll_line(&mut self) {}
}

impl<S, N> ConstrainedWidthLayout<S> for List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn constrain(&mut self, app: &mut Canopy<S>, width: u16) -> Result<()> {
        let mut w = 0;
        let mut h = 0;
        for i in &mut self.items {
            i.itm.constrain(app, width)?;
            let v = i.itm.state().view.outer();
            w = w.max(v.w);
            h += v.h
        }
        self.state_mut().view.resize_outer(Rect::new(0, 0, w, h));
        Ok(())
    }

    fn layout_children(&mut self, app: &mut Canopy<S>) -> Result<()> {
        let view = self.state().view;

        let mut voffset = 0;
        for itm in &mut self.items {
            // The virtual item rectangle
            let item_rect = itm.itm.state().view.view().shift(0, voffset as i16);
            if let Some(r) = view.view().intersect(&item_rect) {
                itm.itm.layout(
                    app,
                    // The virtual coords are the intersection translated into
                    // the co-ordinates of the item.
                    item_rect.rebase_rect(&r)?,
                )?;
            } else {
                itm.itm.hide();
            }
            voffset += itm.itm.state().view.view().h;
        }
        Ok(())
    }
}

impl<S, N> FrameContent for List<S, N> where N: Node<S> + ConstrainedWidthLayout<S> {}

impl<S, N> Node<S> for List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn render(&self, _app: &mut Canopy<S>) -> Result<()> {
        Ok(())
    }
    fn children(&self, f: &mut dyn FnMut(&dyn Node<S>) -> Result<()>) -> Result<()> {
        for i in &self.items {
            f(&i.itm)?
        }
        Ok(())
    }
    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S>) -> Result<()>) -> Result<()> {
        for i in self.items.iter_mut() {
            f(&mut i.itm)?
        }
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
        let _ = lst.constrain(&mut app, 20)?;
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
