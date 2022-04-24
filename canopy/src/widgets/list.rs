use crate as canopy;
use crate::{
    derive_actions,
    error::Result,
    geom::{Expanse, Rect},
    node::Node,
    state::{NodeState, StatefulNode},
    Render,
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
    pub offset: usize,
}

#[derive_actions]
impl<N> List<N>
where
    N: Node + ListItem,
{
    pub fn new(items: Vec<N>) -> Self {
        let mut l = List {
            items: items.into_iter().map(Item::new).collect(),
            offset: 0,
            state: NodeState::default(),
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
            .insert((self.offset + 1).clamp(0, self.len()), Item::new(itm));
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
            if offset <= self.offset {
                self.select_prev();
            }
            Some(itm.itm)
        } else {
            None
        }
    }

    pub fn delete_selected(&mut self) -> Option<N> {
        self.delete_item(self.offset)
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
        self.select(self.offset.saturating_add(1))
    }

    /// Move selection to the next previous the list, if possible.
    pub fn select_prev(&mut self) {
        self.select(self.offset.saturating_sub(1))
    }

    /// The current selected item, if any
    pub fn selected(&self) -> Option<&N> {
        if !self.is_empty() {
            Some(&self.items[self.offset].itm)
        } else {
            None
        }
    }

    /// Select an item at a specified offset, clamping the offset to make sure
    /// it lies within the list.
    pub fn select(&mut self, offset: usize) {
        if !self.is_empty() {
            self.offset = self.offset.clamp(0, self.len() - 1);
            self.items[self.offset].set_selected(false);
            self.offset = offset.clamp(0, self.items.len() - 1);
            self.items[self.offset].set_selected(true);
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
        if self.offset < start {
            self.select(start);
        } else if self.offset > end {
            self.select(end);
        } else {
            self.select(self.offset);
        }
    }

    /// Fix the view after a selection change operation.
    fn fix_view(&mut self) {
        let virt = self.items[self.offset].virt;
        let view = self.vp().view_rect();
        if let Some(v) = virt.vextent().intersection(&view.vextent()) {
            if v.len == virt.h {
                return;
            }
        }
        let (start, end) = self.view_range();
        // We know there isn't an entire overlap
        if self.offset <= start {
            self.update_viewport(&|vp| vp.scroll_to(view.tl.x, virt.tl.y));
        } else if self.offset >= end {
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
            if view.vextent().intersection(&itm.virt.vextent()).is_some() {
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
    fn refresh_views(&mut self, r: Expanse) -> Result<()> {
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
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for i in self.items.iter_mut() {
            f(&mut i.itm)?
        }
        Ok(())
    }

    fn fit(&mut self, r: Expanse) -> Result<Expanse> {
        let mut w = 0;
        let mut h = 0;
        self.refresh_views(r)?;
        for i in &mut self.items {
            w = w.max(i.virt.w);
            h += i.virt.h
        }
        Ok(Expanse { w, h })
    }

    fn render(&mut self, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        for itm in &mut self.items {
            if let Some(vp) = vp.map(itm.virt)? {
                itm.itm.set_viewport(vp);
                canopy::taint_tree(&mut itm.itm);
                itm.itm.unhide();
            } else {
                itm.itm.hide();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::test::TestRender, place, tutils::utils::TFixed};

    pub fn views(lst: &mut List<TFixed>) -> Vec<Rect> {
        let mut v = vec![];
        lst.children(&mut |x: &mut dyn Node| {
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
        assert_eq!(lst.offset, 0);
        lst.select_prev();
        assert_eq!(lst.offset, 0);
        lst.select_next();
        assert_eq!(lst.offset, 1);
        lst.select_next();
        lst.select_next();
        assert_eq!(lst.offset, 2);

        Ok(())
    }

    #[test]
    fn drawnodes() -> Result<()> {
        let (_, mut tr) = TestRender::create();

        let rw = 20;
        let rh = 10;
        let mut lst = List::new(vec![
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
        ]);

        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 5));
        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        canopy::taint_tree(&mut lst);
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 5, 10, 5),
                Rect::new(0, 0, 10, 5),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 5));
        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        canopy::taint_tree(&mut lst);
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 10));
        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        canopy::taint_tree(&mut lst);
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 10));
        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        canopy::taint_tree(&mut lst);
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_to(5, 0));
        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        canopy::taint_tree(&mut lst);
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(5, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        lst.update_viewport(&|vp| vp.scroll_by(0, 5));
        place(&mut lst, Rect::new(0, 0, 10, 10))?;
        canopy::taint_tree(&mut lst);
        tr.render(&mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(5, 5, 10, 5),
                Rect::new(5, 0, 10, 5),
                Rect::new(0, 0, 0, 0),
            ]
        );

        Ok(())
    }
}
