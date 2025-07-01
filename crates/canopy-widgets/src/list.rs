use canopy_core as canopy;

use canopy_core::{
    Context, Layout, Node, NodeState, Render, Result, StatefulNode, derive_commands,
    geom::{Expanse, Rect},
    *,
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
#[derive(canopy_core::StatefulNode)]
pub struct List<N>
where
    N: Node + ListItem,
{
    state: NodeState,

    items: Vec<Item<N>>,
    pub offset: usize,
}

#[derive_commands]
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
            l.items[0].set_selected(true);
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
    }

    /// Insert an item after the current selection.
    pub fn insert_after(&mut self, itm: N) {
        self.items
            .insert((self.offset + 1).clamp(0, self.len()), Item::new(itm));
    }

    /// Append an item to the end of the list.
    pub fn append(&mut self, itm: N) {
        self.items.insert(self.len(), Item::new(itm));
        if self.items.len() == 1 {
            self.offset = 0;
            self.items[0].set_selected(true);
        }
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
    pub fn select(&mut self, offset: usize) -> bool {
        if self.is_empty() {
            return false;
        }
        let new_off = offset.clamp(0, self.items.len() - 1);
        if new_off == self.offset {
            return false;
        }
        self.items[self.offset].set_selected(false);
        self.offset = new_off;
        self.items[self.offset].set_selected(true);
        true
    }

    /// Move selection to the next item in the list, if possible.
    pub fn delete_item(&mut self, core: &mut dyn Context, offset: usize) -> Option<N> {
        if offset >= self.items.len() {
            return None;
        }

        // Clear the previous selection while indices are valid.
        if let Some(itm) = self.items.get_mut(self.offset) {
            itm.set_selected(false);
        }

        let itm = self.items.remove(offset);

        if self.items.is_empty() {
            self.offset = 0;
        } else {
            if self.offset > offset {
                self.offset -= 1;
            } else if self.offset >= self.items.len() {
                self.offset = self.items.len() - 1;
            }
            if let Some(itm) = self.items.get_mut(self.offset) {
                itm.set_selected(true);
            }
            // If the deleted item was above the current view, adjust the scroll
            // position so remaining items stay visible.
            let vp_y = self.vp().view().tl.y;
            if itm.virt.tl.y < vp_y {
                core.scroll_by(self, 0, -(itm.virt.h as i16));
            }
            if self.ensure_selected_in_view(core) {
                core.taint(self);
            }
        }

        core.taint_tree(self);
        Some(itm.itm)
    }

    /// Make sure the selected item is within the view after a change.
    fn ensure_selected_in_view(&mut self, c: &mut dyn Context) -> bool {
        if self.is_empty() {
            return false;
        }
        let virt = self.items[self.offset].virt;
        let view = self.vp().view();
        // Check if the selected item is fully visible
        if let Some(v) = virt.vextent().intersection(&view.vextent()) {
            if v.len == virt.h {
                // Item is fully visible, no need to scroll
                return false;
            }
        }
        let (start, end) = self.view_range();
        // We know there isn't an entire overlap
        if self.offset <= start {
            return c.scroll_to(self, view.tl.x, virt.tl.y);
        } else if self.offset >= end {
            if virt.h >= view.h {
                return c.scroll_to(self, view.tl.x, virt.tl.y);
            } else {
                let y = virt.tl.y - (view.h - virt.h);
                return c.scroll_to(self, view.tl.x, y);
            }
        }
        false
    }

    /// Calculate which items are in the list's vertical window, and return
    /// their offsets and sizes. Items that are offscreen to the side are also
    /// returned, so the returned vector is guaranteed to be a contiguous range.
    fn in_view(&self) -> Vec<usize> {
        let view = self.vp().view();
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

    /// Clear all items.
    #[command(ignore_result)]
    pub fn clear(&mut self) -> Vec<N> {
        self.items.drain(..).map(move |x| x.itm).collect()
    }

    /// Delete the currently selected item.
    #[command(ignore_result)]
    pub fn delete_selected(&mut self, core: &mut dyn Context) -> Option<N> {
        self.delete_item(core, self.offset)
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) {
        if self.is_empty() {
            return;
        }
        let changed = self.select(0);
        // Don't scroll - just ensure the selected item is in view
        let scrolled = self.ensure_selected_in_view(c);
        if changed || scrolled {
            c.taint(self);
        }
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_last(&mut self, c: &mut dyn Context) {
        let changed = self.select(self.len());
        let scrolled = self.ensure_selected_in_view(c);
        if changed || scrolled {
            c.taint(self);
        }
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) {
        let changed = self.select(self.offset.saturating_add(1));
        let scrolled = self.ensure_selected_in_view(c);
        if changed || scrolled {
            c.taint(self);
        }
    }

    /// Move selection to the next previous the list, if possible.
    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) {
        let changed = self.select(self.offset.saturating_sub(1));
        let scrolled = self.ensure_selected_in_view(c);
        if changed || scrolled {
            c.taint(self);
        }
    }

    /// Scroll the viewport down by one line.
    #[command]
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    /// Scroll the viewport up by one line.
    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    /// Scroll the viewport left by one column.
    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    /// Scroll the viewport right by one column.
    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    /// Scroll the viewport down by one page.
    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    /// Scroll the viewport up by one page.
    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
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

    fn layout(&mut self, _l: &Layout, _r: Expanse) -> Result<()> {
        // let mut w = 0;
        // let mut h = 0;
        //
        // let mut voffset: u16 = 0;
        // for itm in &mut self.items {
        //     itm.itm.layout(l, r)?;
        //     let item_view = itm.itm.vp().canvas().rect();
        //     itm.virt = item_view.shift(0, voffset as i16);
        //     voffset += item_view.h;
        // }
        //
        // for i in &mut self.items {
        //     w = w.max(i.virt.w);
        //     h += i.virt.h
        // }
        // l.size(self, Expanse { w, h }, r)?;
        // let vp = self.vp();
        // for itm in self.items.iter_mut() {
        //     if let Some(child_vp) = vp.map(itm.virt)? {
        //         l.set_child_position(
        //             &mut itm.itm,
        //             child_vp.position(),
        //             vp.position(),
        //             vp.canvas().rect(),
        //         )?;
        //         // The item should lay out using its full canvas size so that
        //         // horizontal scrolling only affects the viewport. We set the
        //         // canvas here and expose the entire view for layout.
        //         l.set_canvas(&mut itm.itm, child_vp.canvas());
        //         l.set_view(&mut itm.itm, child_vp.canvas().rect());
        //
        //         itm.itm.layout(l, child_vp.canvas())?;
        //
        //         // After layout, apply the actual visible view and constrain
        //         // the result within the parent.
        //         l.set_view(&mut itm.itm, child_vp.view());
        //         l.constrain_child(&mut itm.itm, vp);
        //
        //         let final_vp = itm.itm.vp();
        //         itm.itm.children(&mut |ch| {
        //             // `ch.vp().position()` returns absolute co-ordinates. We
        //             // want a rectangle relative to the item's canvas, so we
        //             // calculate the offset from the item's position. Use
        //             // `saturating_sub` to avoid panics if the child hasn't been
        //             // repositioned yet and lies above or to the left of the
        //             // item.
        //             let ch_rect = Rect::new(
        //                 ch.vp().position().x.saturating_sub(final_vp.position().x),
        //                 ch.vp().position().y.saturating_sub(final_vp.position().y),
        //                 ch.vp().canvas().w,
        //                 ch.vp().canvas().h,
        //             );
        //             if let Some(ch_vp) = final_vp.map(ch_rect)? {
        //                 l.set_child_position(
        //                     ch,
        //                     ch_vp.position(),
        //                     final_vp.position(),
        //                     final_vp.canvas().rect(),
        //                 )?;
        //                 l.set_canvas(ch, ch_vp.canvas());
        //                 l.set_view(ch, ch_vp.view());
        //             } else {
        //                 // Even if the child is fully clipped, ensure it stays
        //                 // at a valid position relative to the item so that
        //                 // invariants hold.
        //                 l.set_child_position(
        //                     ch,
        //                     final_vp.position(),
        //                     final_vp.position(),
        //                     final_vp.canvas().rect(),
        //                 )?;
        //                 l.set_view(ch, Rect::default());
        //             }
        //             Ok(())
        //         })?;
        //         l.unhide(&mut itm.itm);
        //     } else {
        //         l.hide(&mut itm.itm);
        //         l.set_view(&mut itm.itm, Rect::default());
        //     }
        // }
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
        rndr.fill("", self.vp().canvas().rect(), ' ')?;
        Ok(())
    }
}
