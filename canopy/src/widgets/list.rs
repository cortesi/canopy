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
    widgets::frame::FrameContent,
    Canopy,
};

struct Item<S, N: Node<S> + ConstrainedWidthLayout<S>> {
    _marker: PhantomData<S>,
    itm: N,
    size: Rect,
}

#[derive(StatefulNode)]
pub struct List<S, N: Node<S> + ConstrainedWidthLayout<S>> {
    _marker: PhantomData<S>,
    items: Vec<Item<S, N>>,
    pub virt_origin: Option<Point>,
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
                    size: Rect::default(),
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
            i.size = r;
            w = w.max(r.w);
            h += r.h
        }
        Ok(Rect {
            tl: Point::zero(),
            w,
            h,
        })
    }

    fn layout_children(
        &mut self,
        app: &mut Canopy<S>,
        virt_rect: Rect,
        _screen_rect: Rect,
    ) -> Result<()> {
        let mut voffset = 0;
        // The virtual screen location
        for itm in &mut self.items {
            // The virtual item rectangle
            let item_rect = itm.size.shift(0, voffset as i16);
            if let Some(r) = virt_rect.intersect(&item_rect) {
                itm.itm.layout(
                    app,
                    // The virtual coords are the intersection translated into
                    // the co-ordinates of the item.
                    item_rect.rebase_rect(&r)?,
                    // The screen rect is the intersection translated into the
                    // target rect
                    virt_rect.rebase_rect(&r)?,
                )?;
            } else {
                itm.itm.hide();
            }
            voffset += itm.size.h;
        }
        Ok(())
    }
}

impl<S, N> FrameContent for List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn bounds(&self) -> Option<(Rect, Rect)> {
        None
        // self.scrollstate.as_ref().map(|ss| (ss.window, ss.virt))
    }
}

impl<S, N> Node<S> for List<S, N>
where
    N: Node<S> + ConstrainedWidthLayout<S>,
{
    fn render(&self, _app: &Canopy<S>, _colors: &mut Style, _w: &mut dyn Write) -> Result<()> {
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
        let _ = lst.constrain(&mut app, 20)?;
        lst.layout(
            &mut app,
            Rect {
                tl: Point::zero(),
                w: 10,
                h: 10,
            },
            Rect {
                tl: Point::zero(),
                w: 10,
                h: 10,
            },
        )?;
        let itms: Vec<u16> = lst.items.iter().map(|i| i.size.w).collect();
        println!("{:?}", itms);

        Ok(())
    }
}
