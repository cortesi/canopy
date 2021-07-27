use std::marker::PhantomData;

use crate as canopy;
use crate::{
    error::Result,
    geom::{Point, Rect, Size},
    node::Node,
    state::{NodeState, StatefulNode},
    Actions, Canopy,
};

#[derive(StatefulNode)]
pub struct List<S, A: Actions, N>
where
    N: Node<S, A>,
{
    _marker: PhantomData<(S, A)>,
    items: Vec<N>,
    // Offset within the virtual rectangle
    pub offset: Point,
    pub focus: u32,
    state: NodeState,

    // Cached set of rectangles to clear during rendering
    clear: Vec<Rect>,
}

impl<S, A: Actions, N> List<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new(c: Vec<N>) -> Self {
        List {
            _marker: PhantomData,
            items: c,
            offset: Point::zero(),
            focus: 0,
            state: NodeState::default(),
            clear: vec![],
        }
    }
}

impl<S, A: Actions, N> Node<S, A> for List<S, A, N>
where
    N: Node<S, A>,
{
    fn children(&self, f: &mut dyn FnMut(&dyn Node<S, A>) -> Result<()>) -> Result<()> {
        for i in &self.items {
            f(i)?
        }
        Ok(())
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<()>) -> Result<()> {
        for i in self.items.iter_mut() {
            f(i)?
        }
        Ok(())
    }

    fn fit(&mut self, app: &mut Canopy<S, A>, r: Size) -> Result<Size> {
        let mut w = 0;
        let mut h = 0;
        for i in &mut self.items {
            let v = i.fit(app, r)?;
            w = w.max(v.w);
            h += v.h
        }
        Ok(Size { w, h })
    }

    fn layout(&mut self, app: &mut Canopy<S, A>, screen: Rect) -> Result<()> {
        let myvp = self.state().viewport;
        let mut voffset: u16 = 0;
        self.clear = vec![];
        for itm in &mut self.items {
            let item_view = itm.fit(app, screen.into())?.rect();
            let item_virt = item_view.shift(0, voffset as i16);
            if let Some(vp) = myvp.map(item_virt)? {
                itm.state_mut().viewport = vp;
                itm.layout(app, vp.screen())?;
                itm.unhide();

                // At this point, the item's screen rect has been calculated to
                // be the same size as its view, which may be smaller than our
                // own view. We need to clear anything to the left or to the
                // right of the screen rect in our own view.

                // First, we calculate the area of our view the child will draw
                // on. We know we can unwrap here, because the views intersect
                // by definition.
                let drawn = myvp.view().intersect(&item_virt).unwrap();

                // Now, if there is space to the left, we clear it. In practice,
                // given map's node positioning, there will never be space to
                // the left, but the reasons are slightly subtle. Ditch this
                // code, or keep it, in case behaviour changes?
                let left = Rect::new(
                    myvp.view().tl.x,
                    drawn.tl.y,
                    drawn.tl.x - myvp.view().tl.x,
                    drawn.h,
                );
                if !left.is_empty() {
                    self.clear.push(left);
                }

                // Now, if there is space to the right, we clear it.
                let right = Rect::new(
                    drawn.tl.x + drawn.w,
                    drawn.tl.y,
                    myvp.view().w - drawn.w - left.w,
                    drawn.h,
                );
                if !right.is_empty() {
                    self.clear.push(right);
                }
            } else if let Some(isect) = myvp.view().vextent().intersect(&item_virt.vextent()) {
                // There was no intersection of the rects, but the vertical
                // extent of the item overlaps with our view. This means that
                // item is not on screen because it's off to the left of us, but
                // we still need to clear its full row.
                self.clear.push(myvp.view().vslice(&isect)?);
                itm.hide();
            } else {
                itm.hide();
            }
            voffset += itm.outer().h;
        }
        Ok(())
    }

    fn render(&self, app: &mut Canopy<S, A>) -> Result<()> {
        for r in self.clear.iter() {
            app.render.fill("", *r, ' ')?;
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
