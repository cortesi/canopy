use std::io::Write;
use std::marker::PhantomData;

use crate as canopy;
use crate::{
    error::Result,
    geom::{Point, Rect},
    layout::ConstrainedWidthLayout,
    node::Node,
    state::{NodeState, StatefulNode},
    style::Style,
    Canopy,
};

struct Item<S, N: Node<S> + ConstrainedWidthLayout<S>> {
    _marker: PhantomData<S>,
    itm: N,
    width: u16,
}

#[derive(StatefulNode)]
pub struct List<S, N: Node<S> + ConstrainedWidthLayout<S>> {
    _marker: PhantomData<S>,
    items: Vec<Item<S, N>>,
    pub virt_origin: Option<Point>,
    // Offset within the virtual rectangle
    offset: Point,
    focus: u32,
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
                    width: 0,
                    _marker: PhantomData,
                })
                .collect(),
            offset: Point::zero(),
            virt_origin: None,
            focus: 0,
            state: NodeState::default(),
        }
    }
}

impl<S, N> ConstrainedWidthLayout<S> for List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn constrain(&mut self, app: &mut Canopy<S>, width: u16) -> Result<Rect> {
        let mut w = 0;
        let mut h = 0;
        for i in &mut self.items {
            let r = i.itm.constrain(app, width)?;
            i.width = r.w;
            w = w.max(r.w);
            h += r.h
        }
        Ok(Rect {
            tl: Point::zero(),
            w,
            h,
        })
    }

    fn layout(&mut self, app: &mut Canopy<S>, virt_origin: Point, rect: Rect) -> Result<()> {
        self.set_area(rect);

        let view = rect.rebase(virt_origin);

        self.virt_origin = Some(virt_origin);
        Ok(())
    }
}

impl<S, N> Node<S> for List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn render(
        &self,
        _app: &Canopy<S>,
        colors: &mut Style,
        _: Rect,
        w: &mut dyn Write,
    ) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutils::utils::{State, TFixed};
    use crate::Canopy;
    #[test]
    fn drawnodes() -> Result<()> {
        let mut app: Canopy<State> = Canopy::new();
        let mut lst = List::new(vec![TFixed::new(10, 10)]);
        let r = lst.constrain(&mut app, 20)?;
        lst.layout(
            &mut app,
            Point::zero(),
            Rect {
                tl: Point::zero(),
                w: 10,
                h: 10,
            },
        )?;
        let itms: Vec<u16> = lst.items.iter().map(|i| i.width).collect();
        println!("{:?}", itms);

        Ok(())
    }
}
