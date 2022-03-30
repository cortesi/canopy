use crate as canopy;
use crate::{
    error::Result,
    geom::{Rect, Size},
    node::Node,
    state::{NodeState, StatefulNode},
    Outcome, Render, ViewPort,
};

/// ListItem must be implemented by items displayed in a `List`.
pub trait ListItem {
    fn set_selected(&mut self, _state: bool) {}
}

pub struct Item<N>
where
    N: Node + ListItem,
{
    itm: N,
    virt: Rect,
}

impl<N> Item<N>
where
    N: Node + ListItem,
{
    fn new(itm: N) -> Self {
        Item {
            virt: Rect::default(),
            itm,
        }
    }
    fn set_selected(&mut self, state: bool) {
        self.itm.set_selected(state)
    }
}

/// Manage and display a list of items.
#[derive(StatefulNode)]
pub struct List<N>
where
    N: Node + ListItem,
{
    state: NodeState,

    items: Vec<Item<N>>,
    pub selected: usize,

    // Set of rectangles to clear during the next render.
    clear: Vec<Rect>,
}

impl<N> List<N>
where
    N: Node + ListItem,
{
    pub fn new(items: Vec<N>) -> Self {
        let mut l = List {
            items: items.into_iter().map(Item::new).collect(),
            selected: 0,
            state: NodeState::default(),
            clear: vec![],
        };
        if !l.is_empty() {
            l.select(0);
        }
        l
    }

    /// The number of items in the list.
    pub fn is_empty(&self) -> bool {
        self.items.len() == 0
    }

    /// The number of items in the list.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Insert an item at the given index.
    pub fn insert(&mut self, index: usize, itm: N) {
        self.items
            .insert(index.clamp(0, self.len()), Item::new(itm));
        self.fix_selection();
    }

    /// Insert an item after the current selection.
    pub fn insert_after(&mut self, itm: N) {
        self.items
            .insert((self.selected + 1).clamp(0, self.len()), Item::new(itm));
        self.fix_selection();
    }

    /// Append an item to the end of the list.
    pub fn append(&mut self, itm: N) {
        self.items.insert(self.len(), Item::new(itm));
        self.fix_selection();
    }

    /// Clear all items.
    pub fn clear(&mut self) -> Vec<N> {
        self.items.drain(..).map(move |x| x.itm).collect()
    }

    /// Move selection to the next item in the list, if possible.
    pub fn delete_item(&mut self, offset: usize) -> Option<N> {
        if !self.is_empty() && offset < self.len() {
            let itm = self.items.remove(offset);
            if offset <= self.selected {
                self.select_prev();
            }
            Some(itm.itm)
        } else {
            None
        }
    }

    pub fn delete_selected(&mut self) -> Option<N> {
        self.delete_item(self.selected)
    }

    /// Move selection to the next item in the list, if possible.
    pub fn select_first(&mut self) {
        self.select(0)
    }

    /// Move selection to the next item in the list, if possible.
    pub fn select_last(&mut self) {
        self.select(self.len())
    }

    /// Move selection to the next item in the list, if possible.
    pub fn select_next(&mut self) {
        self.select(self.selected.saturating_add(1))
    }

    /// Move selection to the next previous the list, if possible.
    pub fn select_prev(&mut self) {
        self.select(self.selected.saturating_sub(1))
    }

    /// Select an item at a specified offset, clamping the offset to make sure
    /// it lies within the list.
    pub fn select(&mut self, offset: usize) {
        if !self.is_empty() {
            self.selected = self.selected.clamp(0, self.len() - 1);
            self.items[self.selected].set_selected(false);
            self.selected = offset.clamp(0, self.items.len() - 1);
            self.items[self.selected].set_selected(true);
            self.fix_view();
        }
    }

    /// Scroll the viewport to a specified location.
    pub fn scroll_to(&mut self, x: u16, y: u16) {
        self.update_viewport(&|vp| vp.scroll_to(x, y));
        self.fix_selection();
    }

    /// Scroll the viewport down by one line.
    pub fn scroll_down(&mut self) {
        self.update_viewport(&|vp| vp.down());
        self.fix_selection();
    }

    /// Scroll the viewport up by one line.
    pub fn scroll_up(&mut self) {
        self.update_viewport(&|vp| vp.up());
        self.fix_selection();
    }

    /// Scroll the viewport left by one column.
    pub fn scroll_left(&mut self) {
        self.update_viewport(&|vp| vp.left());
        self.fix_selection();
    }

    /// Scroll the viewport right by one column.
    pub fn scroll_right(&mut self) {
        self.update_viewport(&|vp| vp.right());
        self.fix_selection();
    }

    /// Scroll the viewport down by one page.
    pub fn page_down(&mut self) {
        self.update_viewport(&|vp| vp.page_down());
        self.fix_selection();
    }

    /// Scroll the viewport up by one page.
    pub fn page_up(&mut self) {
        self.update_viewport(&|vp| vp.page_up());
        self.fix_selection();
    }

    /// Fix the selected item after a scroll operation.
    fn fix_selection(&mut self) {
        let (start, end) = self.view_range();
        if self.selected < start {
            self.select(start);
        } else if self.selected > end {
            self.select(end);
        } else {
            self.select(self.selected);
        }
    }

    /// Fix the view after a selection change operation.
    fn fix_view(&mut self) {
        let virt = self.items[self.selected].virt;
        let view = self.vp().view_rect();
        if let Some(v) = virt.vextent().intersect(&view.vextent()) {
            if v.len == virt.h {
                return;
            }
        }
        let (start, end) = self.view_range();
        // We know there isn't an entire overlap
        if self.selected <= start {
            self.update_viewport(&|vp| vp.scroll_to(view.tl.x, virt.tl.y));
        } else if self.selected >= end {
            if virt.h >= view.h {
                self.update_viewport(&|vp| vp.scroll_to(view.tl.x, virt.tl.y));
            } else {
                let y = virt.tl.y - (view.h - virt.h);
                self.update_viewport(&|vp| vp.scroll_to(view.tl.x, y));
            }
        }
    }

    /// Calculate which items are in the list's vertical window, and return
    /// their offsets and sizes. Items that are offscreen to the side are also
    /// returned, so the returned vector is guaranteed to be a contiguous range.
    fn in_view(&self) -> Vec<usize> {
        let view = self.vp().view_rect();
        let mut ret = vec![];
        for (idx, itm) in self.items.iter().enumerate() {
            if view.vextent().intersect(&itm.virt.vextent()).is_some() {
                ret.push(idx);
            }
        }
        ret
    }

    /// The first and last items of the view. (0, 0) if the lis empty.
    fn view_range(&self) -> (usize, usize) {
        let v = self.in_view();
        if let (Some(f), Some(l)) = (v.first(), v.last()) {
            (*f, *l)
        } else {
            (0, 0)
        }
    }

    /// Calculate and return the outer viewport rectangles of all items.
    fn refresh_views(&mut self, r: Size) -> Result<()> {
        let mut voffset: u16 = 0;
        for itm in &mut self.items {
            let item_view = itm.itm.fit(r)?.rect();
            itm.virt = item_view.shift(0, voffset as i16);
            voffset += item_view.h;
        }
        Ok(())
    }
}

impl<N> Node for List<N>
where
    N: Node + ListItem,
{
    fn handle_focus(&mut self) -> Result<Outcome> {
        self.set_focus();
        Ok(Outcome::handle())
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node) -> Result<()>) -> Result<()> {
        for i in &self.items {
            f(&i.itm)?
        }
        Ok(())
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for i in self.items.iter_mut() {
            f(&mut i.itm)?
        }
        Ok(())
    }

    fn fit(&mut self, r: Size) -> Result<Size> {
        let mut w = 0;
        let mut h = 0;
        self.refresh_views(r)?;
        for i in &mut self.items {
            w = w.max(i.virt.w);
            h += i.virt.h
        }
        Ok(Size { w, h })
    }

    fn render(&mut self, rndr: &mut Render, myvp: ViewPort) -> Result<()> {
        self.clear = vec![];
        for itm in &mut self.items {
            if let Some(vp) = myvp.map(itm.virt)? {
                itm.itm.set_viewport(vp);
                itm.itm.taint_tree()?;
                itm.itm.unhide();

                // At this point, the item's screen rect has been calculated to
                // be the same size as its view, which may be smaller than our
                // own view. We need to clear anything to the left or to the
                // right of the screen rect in our own view.

                // First, we calculate the area of our view the child will draw
                // on. We know we can unwrap here, because the views intersect
                // by definition.
                let drawn = myvp.view_rect().intersect(&itm.virt).unwrap();

                // Now, if there is space to the left, we clear it. In practice,
                // given map's node positioning, there will never be space to
                // the left, but the reasons are slightly subtle. Ditch this
                // code, or keep it, in case behaviour changes?
                let left = Rect::new(
                    myvp.view_rect().tl.x,
                    drawn.tl.y,
                    drawn.tl.x - myvp.view_rect().tl.x,
                    drawn.h,
                );
                if !left.is_zero() {
                    self.clear.push(left);
                }

                // Now, if there is space to the right, we clear it.
                let right = Rect::new(
                    drawn.tl.x + drawn.w,
                    drawn.tl.y,
                    myvp.view_rect().w - drawn.w - left.w,
                    drawn.h,
                );
                if !right.is_zero() {
                    self.clear.push(right);
                }
            } else if let Some(isect) = myvp.view_rect().vextent().intersect(&itm.virt.vextent()) {
                // There was no intersection of the rects, but the vertical
                // extent of the item overlaps with our view. This means that
                // item is not on screen because it's off to the left of our
                // view, but we still need to clear its full row.
                self.clear.push(myvp.view_rect().vslice(&isect)?);
                itm.itm.hide();
            } else {
                itm.itm.hide();
            }
        }
        for r in self.clear.iter() {
            rndr.fill("", *r, ' ')?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::test::TestRender,
        tutils::utils::{tcanopy, TFixed},
    };

    pub fn views(lst: &List<TFixed>) -> Vec<Rect> {
        let mut v = vec![];
        lst.children(&mut |x: &dyn Node| {
            v.push(if x.is_hidden() {
                Rect::default()
            } else {
                x.vp().view_rect()
            });
            Ok(())
        })
        .unwrap();
        v
    }

    #[test]
    fn select() -> Result<()> {
        // Empty initilization shouldn't fail
        let _: List<TFixed> = List::new(Vec::new());

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
        let (mut r, _) = tcanopy(&mut tr);
        let rw = 20;
        let rh = 10;
        let mut lst = List::new(vec![
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
        ]);

        lst.place(Rect::new(0, 0, 10, 10))?;
        canopy::render(&mut r, &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 5));
        lst.place(Rect::new(0, 0, 10, 10))?;
        lst.taint_tree()?;
        canopy::render(&mut r, &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 5, 10, 5),
                Rect::new(0, 0, 10, 5),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 5));
        lst.place(Rect::new(0, 0, 10, 10))?;
        lst.taint_tree()?;
        canopy::render(&mut r, &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 10));
        lst.place(Rect::new(0, 0, 10, 10))?;
        lst.taint_tree()?;
        canopy::render(&mut r, &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 10));
        lst.place(Rect::new(0, 0, 10, 10))?;
        lst.taint_tree()?;
        canopy::render(&mut r, &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_to(5, 0));
        lst.place(Rect::new(0, 0, 10, 10))?;
        lst.taint_tree()?;
        canopy::render(&mut r, &mut lst)?;
        assert_eq!(
            views(&lst),
            vec![
                Rect::new(5, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 5));
        lst.place(Rect::new(0, 0, 10, 10))?;
        lst.taint_tree()?;
        canopy::render(&mut r, &mut lst)?;
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
