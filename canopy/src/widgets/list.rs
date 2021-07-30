use std::marker::PhantomData;

use crate as canopy;
use crate::{
    error::Result,
    geom::{Point, Rect, Size},
    node::Node,
    state::{NodeState, StatefulNode},
    Actions, Canopy,
};

/// ListItem should be implemented by items to be displayed in a `List`.
pub trait ListItem {
    fn set_selected(&mut self, _state: bool) {}
}

#[derive(StatefulNode)]
pub struct List<S, A: Actions, N>
where
    N: Node<S, A> + ListItem,
{
    _marker: PhantomData<(S, A)>,
    items: Vec<N>,

    // Offset within the virtual rectangle
    pub offset: Point,
    pub selected: usize,
    state: NodeState,

    // Cached set of rectangles to clear during rendering
    clear: Vec<Rect>,
}

impl<S, A: Actions, N> List<S, A, N>
where
    N: Node<S, A> + ListItem,
{
    pub fn new(items: Vec<N>) -> Self {
        let mut l = List {
            _marker: PhantomData,
            items: items,
            offset: Point::zero(),
            selected: 0,
            state: NodeState::default(),
            clear: vec![],
        };
        if l.items.len() > 0 {
            l.select(0);
        }
        l
    }
    pub fn select_next(&mut self) {
        self.select(self.selected.saturating_add(1))
    }
    pub fn select_prev(&mut self) {
        self.select(self.selected.saturating_sub(1))
    }
    pub fn select(&mut self, offset: usize) {
        self.items[self.selected].set_selected(false);
        self.selected = offset.clamp(0, self.items.len() - 1);
        self.items[self.selected].set_selected(true);
    }
}

impl<S, A: Actions, N> Node<S, A> for List<S, A, N>
where
    N: Node<S, A> + ListItem,
{
    fn can_focus(&self) -> bool {
        true
    }

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
                // item is not on screen because it's off to the left of our
                // view, but we still need to clear its full row.
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
    use crate::{
        fit_and_update,
        render::test::TestRender,
        tutils::utils::{tcanopy, State, TActions, TFixed},
    };

    pub fn views(lst: &List<State, TActions, TFixed>) -> Vec<Rect> {
        let mut v = vec![];
        lst.children(&mut |x: &dyn Node<State, TActions>| {
            v.push(if x.is_hidden() {
                Rect::default()
            } else {
                x.view()
            });
            Ok(())
        })
        .unwrap();
        v
    }

    #[test]
    fn select() -> Result<()> {
        // Empty initilization shouldn't fail
        let _: List<State, TActions, TFixed> = List::new(Vec::new());

        let mut lst = List::new(vec![
            TFixed::new(10, 10),
            TFixed::new(10, 10),
            TFixed::new(10, 10),
        ]);
        assert_eq!(lst.selected, 0);
        lst.select_prev();
        assert_eq!(lst.selected, 0);
        lst.select_next();
        assert_eq!(lst.selected, 1);
        lst.select_next();
        lst.select_next();
        assert_eq!(lst.selected, 2);

        Ok(())
    }

    #[test]
    fn drawnodes() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = tcanopy(&mut tr);
        let rw = 20;
        let rh = 10;
        let mut lst = List::new(vec![
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
        ]);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;

        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );
        lst.state_mut().viewport.scroll_by(0, 5);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 5, 10, 5),
                Rect::new(0, 0, 10, 5),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.state_mut().viewport.scroll_by(0, 5);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.state_mut().viewport.scroll_by(0, 10);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        lst.state_mut().viewport.scroll_by(0, 10);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        lst.state_mut().viewport.scroll_to(5, 0);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(5, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.state_mut().viewport.scroll_by(0, 5);
        fit_and_update(&mut app, Rect::new(0, 0, 10, 10), &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(5, 5, 10, 5),
                Rect::new(5, 0, 10, 5),
                Rect::new(0, 0, 0, 0),
            ]
        );

        Ok(())
    }
}
