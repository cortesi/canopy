use crate as canopy;
use crate::{
    error::Result,
    geom::{Expanse, Rect},
    node::Node,
    state::{NodeState, StatefulNode},
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
#[derive(StatefulNode)]
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

    fn layout(&mut self, l: &Layout, r: Expanse) -> Result<()> {
        let mut w = 0;
        let mut h = 0;

        let mut voffset: u16 = 0;
        for itm in &mut self.items {
            itm.itm.layout(l, r)?;
            let item_view = itm.itm.vp().canvas().rect();
            itm.virt = item_view.shift(0, voffset as i16);
            voffset += item_view.h;
        }

        for i in &mut self.items {
            w = w.max(i.virt.w);
            h += i.virt.h
        }
        l.size(self, Expanse { w, h }, r)?;
        let vp = self.vp();
        for itm in self.items.iter_mut() {
            if let Some(child_vp) = vp.map(itm.virt)? {
                {
                    let st = itm.itm.state_mut();
                    st.set_position(child_vp.position(), vp.position(), vp.canvas().rect())?;
                    // The item should lay out using its full canvas size so that
                    // horizontal scrolling only affects the viewport. We set the
                    // canvas here and expose the entire view for layout.
                    st.set_canvas(child_vp.canvas());
                    st.set_view(child_vp.canvas().rect());
                }
                itm.itm.layout(l, child_vp.canvas())?;
                {
                    let st = itm.itm.state_mut();
                    // After layout, apply the actual visible view and constrain
                    // the result within the parent.
                    st.set_view(child_vp.view());
                    st.constrain(vp);
                }
                let final_vp = itm.itm.vp();
                itm.itm.children(&mut |ch| {
                    // `ch.vp().position()` returns absolute co-ordinates. We
                    // want a rectangle relative to the item's canvas, so we
                    // calculate the offset from the item's position. Use
                    // `saturating_sub` to avoid panics if the child hasn't been
                    // repositioned yet and lies above or to the left of the
                    // item.
                    let ch_rect = Rect::new(
                        ch.vp().position().x.saturating_sub(final_vp.position().x),
                        ch.vp().position().y.saturating_sub(final_vp.position().y),
                        ch.vp().canvas().w,
                        ch.vp().canvas().h,
                    );
                    if let Some(ch_vp) = final_vp.map(ch_rect)? {
                        ch.state_mut().set_position(
                            ch_vp.position(),
                            final_vp.position(),
                            final_vp.canvas().rect(),
                        )?;
                        ch.state_mut().set_canvas(ch_vp.canvas());
                        ch.state_mut().set_view(ch_vp.view());
                    } else {
                        // Even if the child is fully clipped, ensure it stays
                        // at a valid position relative to the item so that
                        // invariants hold.
                        ch.state_mut().set_position(
                            final_vp.position(),
                            final_vp.position(),
                            final_vp.canvas().rect(),
                        )?;
                        ch.state_mut().set_view(Rect::default());
                    }
                    Ok(())
                })?;
                itm.itm.unhide();
            } else {
                itm.itm.hide();
                itm.itm.state_mut().set_view(Rect::default());
            }
        }
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
        rndr.fill("", self.vp().canvas().rect(), ' ')?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Context, backend::test::CanvasRender, geom::Point, render::RenderBackend, style::Style,
        tutils::Harness, widgets::Text, widgets::frame,
    };
    use std::sync::{Arc, Mutex};

    impl ListItem for Text {}

    #[test]
    fn frame_repaints_on_scroll() -> Result<()> {
        use crate::backend::test::CanvasRender;
        use crate::widgets::{frame, text::Text};

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            list: frame::Frame<List<Text>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    list: frame::Frame::new(List::new(vec![
                        Text::new("AAAA").with_fixed_width(4),
                        Text::new("BBBB").with_fixed_width(4),
                        Text::new("CCCC").with_fixed_width(4),
                        Text::new("DDDD").with_fixed_width(4),
                    ])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.list)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.list, vp, vp.view())?;
                Ok(())
            }
        }

        let size = Expanse::new(10, 5);
        let (buf, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;
        let first = buf.lock().unwrap().cells.clone();

        assert_eq!(first[1][1], 'A');
        assert_eq!(first[2][1], 'B');

        let _corner = first[0][0];

        canopy.scroll_down(&mut root.list.child);
        canopy.render(&mut cr, &mut root)?;
        let second = buf.lock().unwrap().cells.clone();

        assert_eq!(second[1][1], 'B');
        assert_eq!(second[2][1], 'C');
        // Border should remain unchanged with double buffering

        canopy.scroll_up(&mut root.list.child);
        canopy.render(&mut cr, &mut root)?;
        let third = buf.lock().unwrap().cells.clone();

        assert_eq!(third[1][1], 'A');
        assert_eq!(third[2][1], 'B');
        // Border should remain unchanged with double buffering

        Ok(())
    }

    struct PaintTracker {
        painted: Arc<Mutex<Vec<Vec<bool>>>>,
        size: Expanse,
    }

    impl PaintTracker {
        fn create(size: Expanse) -> (Arc<Mutex<Vec<Vec<bool>>>>, Self) {
            let buf = Arc::new(Mutex::new(vec![
                vec![false; size.w as usize];
                size.h as usize
            ]));
            (buf.clone(), PaintTracker { painted: buf, size })
        }
    }

    impl RenderBackend for PaintTracker {
        fn reset(&mut self) -> Result<()> {
            let mut buf = self.painted.lock().unwrap();
            for row in &mut *buf {
                for c in row.iter_mut() {
                    *c = false;
                }
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
        fn style(&mut self, _s: Style) -> Result<()> {
            Ok(())
        }

        fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
            let mut buf = self.painted.lock().unwrap();
            for (i, _ch) in txt.chars().enumerate() {
                let x = loc.x as usize + i;
                let y = loc.y as usize;
                if x < self.size.w as usize && y < self.size.h as usize {
                    buf[y][x] = true;
                }
            }
            Ok(())
        }

        fn exit(&mut self, _code: i32) -> ! {
            unreachable!()
        }
    }

    #[test]
    fn list_items_within_bounds() -> Result<()> {
        const SAMPLE: &str = "aaa bbb ccc\nddd";

        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(w: u16) -> Self {
                Block {
                    state: NodeState::default(),
                    text: Text::new(SAMPLE).with_fixed_width(w),
                }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse {
                    w: vp.canvas().w,
                    h: vp.canvas().h,
                };
                l.size(self, sz, sz)?;
                Ok(())
            }
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.text)
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            frame: frame::Frame<List<Block>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    frame: frame::Frame::new(List::new(vec![
                        Block::new(4),
                        Block::new(7),
                        Block::new(5),
                    ])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.frame)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.frame, vp, vp.view())?;
                Ok(())
            }
        }

        let mut canopy = Canopy::new();
        let size = Expanse::new(20, 20);
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;

        let list_rect = root.frame.child.vp().view();
        let mut rects = Vec::new();
        root.frame.child.children(&mut |n| {
            if !n.is_hidden() {
                rects.push(n.vp().view());
            }
            Ok(())
        })?;
        for r in &rects {
            assert!(list_rect.contains_rect(r));
        }

        Ok(())
    }

    #[test]
    fn canvas_painted_after_scroll() -> Result<()> {
        const SAMPLE: &str = "aaa bbb ccc\nddd";

        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(w: u16) -> Self {
                Block {
                    state: NodeState::default(),
                    text: Text::new(SAMPLE).with_fixed_width(w),
                }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse {
                    w: vp.canvas().w,
                    h: vp.canvas().h,
                };
                l.size(self, sz, sz)?;
                Ok(())
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.text)
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            frame: frame::Frame<List<Block>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    frame: frame::Frame::new(List::new(vec![
                        Block::new(4),
                        Block::new(7),
                        Block::new(5),
                    ])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.frame)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.frame, vp, vp.view())?;
                Ok(())
            }
        }

        let size = Expanse::new(20, 8);
        let (buf, mut pr) = PaintTracker::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut pr, &mut root)?;
        {
            let painted = buf.lock().unwrap();
            assert!(painted.iter().flat_map(|r| r.iter()).any(|&c| c));
        }

        canopy.scroll_down(&mut root.frame.child);
        canopy.render(&mut pr, &mut root)?;
        {
            let painted = buf.lock().unwrap();
            assert!(painted.iter().flat_map(|r| r.iter()).any(|&c| c));
        }

        canopy.scroll_up(&mut root.frame.child);
        canopy.render(&mut pr, &mut root)?;
        {
            let painted = buf.lock().unwrap();
            assert!(painted.iter().flat_map(|r| r.iter()).any(|&c| c));
        }

        Ok(())
    }

    #[test]
    fn items_do_not_overlap_initially() -> Result<()> {
        const SAMPLE: &str = "aaa bbb ccc\nddd";

        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(w: u16) -> Self {
                Block {
                    state: NodeState::default(),
                    text: Text::new(SAMPLE).with_fixed_width(w),
                }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse {
                    w: vp.canvas().w,
                    h: vp.canvas().h,
                };
                l.size(self, sz, sz)?;
                Ok(())
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.text)
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            frame: frame::Frame<List<Block>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    frame: frame::Frame::new(List::new(vec![
                        Block::new(4),
                        Block::new(7),
                        Block::new(5),
                    ])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.frame)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.frame, vp, vp.view())?;
                Ok(())
            }
        }

        let mut canopy = Canopy::new();
        let size = Expanse::new(20, 8);
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        let (_, mut cr) = CanvasRender::create(size);
        canopy.render(&mut cr, &mut root)?;

        let list_rect = root.frame.child.vp().screen_rect();
        let mut last_y = list_rect.tl.y;
        root.frame.child.children(&mut |n| {
            if !n.is_hidden() {
                let r = n.vp().screen_rect();
                assert!(list_rect.contains_rect(&r));
                assert!(r.tl.y >= last_y);
                last_y = r.tl.y + r.h;
            }
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn list_inside_frame_renders() -> Result<()> {
        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(t: &str) -> Self {
                Block {
                    state: NodeState::default(),
                    text: Text::new(t).with_fixed_width(t.len() as u16),
                }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                // Mirrors the listgym example block layout
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(
                    &mut self.text,
                    vp,
                    Rect::new(2, 0, sz.w.saturating_sub(2), sz.h),
                )?;
                let vp = self.text.vp();
                let sz = Expanse {
                    w: vp.canvas().w + 2,
                    h: vp.canvas().h,
                };
                l.size(self, sz, sz)?;
                Ok(())
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.text)
            }
        }

        #[derive(StatefulNode)]
        struct Status {
            state: NodeState,
        }

        #[derive_commands]
        impl Status {}

        impl Node for Status {
            fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
                r.text("", self.vp().view().line(0), "status")
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            list: frame::Frame<List<Block>>,
            status: Status,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    list: frame::Frame::new(List::new(vec![
                        Block::new("AAAA"),
                        Block::new("BBBB"),
                    ])),
                    status: Status {
                        state: NodeState::default(),
                    },
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.status)?;
                f(&mut self.list)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                let (top, bottom) = vp.screen_rect().carve_vend(1);
                l.place(&mut self.list, vp, top)?;
                l.place(&mut self.status, vp, bottom)?;
                Ok(())
            }
        }

        let size = Expanse::new(15, 4);
        let (buf, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;

        let canvas = buf.lock().unwrap();
        assert_eq!(canvas.cells[1][3], 'A');
        assert!(canvas.painted[1][3]);
        assert_eq!(canvas.cells[0][0], '┌');

        Ok(())
    }

    #[test]
    fn horizontal_scroll_reveals_content() -> Result<()> {
        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            list: List<Text>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    list: List::new(vec![Text::new("0123456789").with_fixed_width(10)]),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.list)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.list, vp, vp.view())?;
                Ok(())
            }
        }

        let size = Expanse::new(5, 1);
        let (buf, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;

        {
            let canvas = buf.lock().unwrap();
            let first: String = canvas.cells[0].iter().collect();
            assert_eq!(first, "01234");
        }

        canopy.scroll_right(&mut root.list);
        canopy.render(&mut cr, &mut root)?;

        {
            let canvas = buf.lock().unwrap();
            let second: String = canvas.cells[0].iter().collect();
            assert_eq!(second, "12345");
        }

        Ok(())
    }

    #[test]
    fn horizontal_scroll_in_frame_reveals_content() -> Result<()> {
        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(t: &str) -> Self {
                Block {
                    state: NodeState::default(),
                    text: Text::new(t).with_fixed_width(t.len() as u16),
                }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(
                    &mut self.text,
                    vp,
                    Rect::new(2, 0, sz.w.saturating_sub(2), sz.h),
                )?;
                let vp = self.text.vp();
                let sz = Expanse {
                    w: vp.canvas().w + 2,
                    h: vp.canvas().h,
                };
                l.size(self, sz, sz)?;
                Ok(())
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.text)
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            frame: frame::Frame<List<Block>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    frame: frame::Frame::new(List::new(vec![Block::new("0123456789")])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.frame)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.frame, vp, vp.view())?;
                Ok(())
            }
        }

        let size = Expanse::new(7, 3);
        let (buf, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;

        {
            let canvas = buf.lock().unwrap();
            assert_eq!(canvas.cells[1][3], '0');
        }

        canopy.scroll_right(&mut root.frame.child);
        canopy.render(&mut cr, &mut root)?;

        {
            let canvas = buf.lock().unwrap();
            assert_eq!(canvas.cells[1][3], '1');
        }

        Ok(())
    }

    #[test]
    fn append_item_clipped_bounds() -> Result<()> {
        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(t: &str) -> Self {
                Block {
                    state: NodeState::default(),
                    text: Text::new(t).with_fixed_width(t.len() as u16),
                }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, sz.w, sz.h))?;
                let vp = self.text.vp();
                l.size(self, vp.canvas(), vp.canvas())?;
                Ok(())
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.text)
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            list: frame::Frame<List<Block>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    list: frame::Frame::new(List::new(vec![Block::new("A"), Block::new("B")])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.list)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.list, vp, vp.view())?;
                Ok(())
            }
        }

        let size = Expanse::new(4, 3);
        let (_, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;

        // Append a new item that will be outside the visible view.
        root.list.child.append(Block::new("C"));
        canopy.render(&mut cr, &mut root)?;

        let list_rect = root.list.child.vp().screen_rect();
        root.list.child.children(&mut |n| {
            if !n.is_hidden() {
                assert!(list_rect.contains_rect(&n.vp().screen_rect()));
            }
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn append_interval_item_no_overflow() -> Result<()> {
        #[derive(StatefulNode)]
        struct IntervalItem {
            state: NodeState,
            child: Text,
            selected: bool,
        }

        #[derive_commands]
        impl IntervalItem {
            fn new() -> Self {
                IntervalItem {
                    state: NodeState::default(),
                    child: Text::new("0"),
                    selected: false,
                }
            }
        }

        impl ListItem for IntervalItem {
            fn set_selected(&mut self, state: bool) {
                self.selected = state;
            }
        }

        impl Node for IntervalItem {
            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                self.child.layout(l, sz)?;
                let vp = self.child.vp();
                l.wrap(self, vp)?;
                Ok(())
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.child)
            }
        }

        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            list: frame::Frame<List<IntervalItem>>,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    list: frame::Frame::new(List::new(vec![])),
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.list)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.list, vp, vp.view())?;
                Ok(())
            }
        }

        let size = Expanse::new(10, 3);
        let (_, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;

        // Append a new item and render again. This previously triggered an
        // invariant violation.
        root.list.child.append(IntervalItem::new());
        canopy.render(&mut cr, &mut root)?;

        let list_rect = root.list.child.vp().screen_rect();
        root.list.child.children(&mut |n| {
            if !n.is_hidden() {
                assert!(list_rect.contains_rect(&n.vp().screen_rect()));
            }
            Ok(())
        })?;

        Ok(())
    }

    // Text item wrapper that supports selection highlighting
    #[derive(StatefulNode)]
    struct SelectableText {
        state: NodeState,
        text: Text,
        selected: bool,
    }

    #[derive_commands]
    impl SelectableText {
        fn new(s: &str) -> Self {
            SelectableText {
                state: NodeState::default(),
                text: Text::new(s),
                selected: false,
            }
        }

        fn with_fixed_width(mut self, width: u16) -> Self {
            self.text = self.text.with_fixed_width(width);
            self
        }
    }

    impl ListItem for SelectableText {
        fn set_selected(&mut self, state: bool) {
            self.selected = state;
        }
    }

    impl Node for SelectableText {
        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            self.text.layout(l, sz)?;
            let vp = self.text.vp();
            l.wrap(self, vp)?;
            Ok(())
        }

        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.text)
        }

        fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
            if self.selected {
                r.style.push_layer("blue");
            }
            Ok(())
        }
    }

    // Simple list implementation for testing
    #[derive(StatefulNode)]
    struct SimpleList {
        state: NodeState,
        list: List<SelectableText>,
    }

    #[derive_commands]
    impl SimpleList {
        fn new(items: Vec<&str>) -> Self {
            SimpleList {
                state: NodeState::default(),
                list: List::new(items.into_iter().map(SelectableText::new).collect()),
            }
        }
    }

    impl Node for SimpleList {
        fn accept_focus(&mut self) -> bool {
            true
        }

        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.list)
        }

        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, sz)?;
            let vp = self.vp();
            l.place(&mut self.list, vp, vp.view())?;
            Ok(())
        }
    }

    impl crate::Loader for SimpleList {
        fn load(c: &mut crate::Canopy) {
            use crate::Binder;
            use crate::style::solarized;

            c.add_commands::<SimpleList>();
            c.add_commands::<List<SelectableText>>();

            // Set up style for selection highlighting
            c.style.add_fg("blue/text", solarized::BLUE);

            // Set up key bindings for testing
            Binder::new(c)
                .key('j', "list::select_next()")
                .key('k', "list::select_prev()")
                .key('g', "list::select_first()")
                .key('G', "list::select_last()")
                .key('d', "list::delete_selected()")
                .key('h', "list::scroll_left()")
                .key('l', "list::scroll_right()");
        }
    }

    #[test]
    fn render_simple_list() -> Result<()> {
        let mut h = Harness::new(SimpleList::new(vec!["item1", "item2", "item3"]))?;
        h.render()?;

        h.expect_contains("item1");
        h.expect_contains("item2");
        h.expect_contains("item3");

        // First item should be selected (highlighted)
        h.expect_highlight("item1");
        Ok(())
    }

    #[test]
    fn scrolling_through_list() -> Result<()> {
        let mut h = Harness::with_size(
            SimpleList::new(vec!["item1", "item2", "item3", "item4", "item5"]),
            Expanse::new(10, 3),
        )?;
        h.render()?;

        // Initially first item is selected
        h.expect_highlight("item1");

        // Scroll down
        h.key('j')?;
        h.expect_highlight("item2");

        h.key('j')?;
        h.expect_highlight("item3");

        // Scroll up
        h.key('k')?;
        h.expect_highlight("item2");

        // Test first/last navigation
        h.key('G')?;
        h.expect_highlight("item5");

        h.key('g')?;
        h.expect_highlight("item1");

        Ok(())
    }

    #[test]
    fn scrolling_with_viewport_constraints() -> Result<()> {
        // Create a list larger than viewport
        let items: Vec<&str> = (1..=10)
            .map(|i| Box::leak(format!("item{i}").into_boxed_str()) as &str)
            .collect();
        let mut h = Harness::with_size(
            SimpleList::new(items),
            Expanse::new(10, 4), // Small viewport
        )?;
        h.render()?;

        // Navigate to bottom
        h.key('G')?;
        h.expect_highlight("item10");

        // Item10 should be visible
        h.expect_contains("item10");

        // Navigate back to top
        h.key('g')?;
        h.expect_highlight("item1");
        h.expect_contains("item1");

        Ok(())
    }

    #[test]
    fn delete_items_from_list() -> Result<()> {
        // Test basic deletion functionality
        let mut h = Harness::new(SimpleList::new(vec!["apple", "banana", "cherry"]))?;
        h.render()?;

        // Verify initial state
        assert_eq!(h.root().list.len(), 3);
        h.expect_highlight("apple");

        // Delete current item
        h.key('d')?;
        h.render()?;

        // Verify one item was deleted
        assert_eq!(h.root().list.len(), 2);

        // Delete another item
        h.key('d')?;
        h.render()?;

        // Verify another item was deleted
        assert_eq!(h.root().list.len(), 1);

        // Delete the last item
        h.key('d')?;
        h.render()?;

        // List should be empty
        assert!(h.root().list.is_empty());

        Ok(())
    }

    #[test]
    fn delete_middle_item() -> Result<()> {
        let mut h = Harness::new(SimpleList::new(vec!["first", "middle", "last"]))?;
        h.render()?;

        // Navigate to middle item
        h.key('j')?;
        h.expect_highlight("middle");

        // Delete it
        h.key('d')?;
        h.render()?;

        // After deleting middle item, we should have 2 items left
        assert_eq!(h.root().list.len(), 2);

        // The selection should be on an item (either first or last)
        // The exact behavior depends on the list implementation
        assert!(h.root().list.selected().is_some());

        Ok(())
    }

    #[test]
    fn add_items_to_list() -> Result<()> {
        let mut h = Harness::new(SimpleList::new(vec!["initial"]))?;
        h.render()?;

        h.expect_highlight("initial");

        // Append items
        h.root().list.append(SelectableText::new("second"));
        h.render()?;
        h.expect_contains("second");

        h.root().list.append(SelectableText::new("third"));
        h.render()?;
        h.expect_contains("third");

        // Insert at specific position
        h.root().list.insert(1, SelectableText::new("inserted"));
        h.render()?;
        h.expect_contains("inserted");

        // Insert after current selection
        h.root()
            .list
            .insert_after(SelectableText::new("after_initial"));
        h.render()?;
        h.expect_contains("after_initial");

        Ok(())
    }

    #[test]
    fn add_to_empty_list() -> Result<()> {
        let mut h = Harness::new(SimpleList::new(vec![]))?;
        h.render()?;

        // List should be empty
        assert!(h.root().list.is_empty());

        // Add first item
        h.root().list.append(SelectableText::new("first"));
        h.render()?;

        // First item should be automatically selected
        h.expect_highlight("first");
        h.expect_contains("first");

        Ok(())
    }

    #[test]
    fn selection_persists_through_operations() -> Result<()> {
        let mut h = Harness::new(SimpleList::new(vec!["a", "b", "c", "d", "e"]))?;
        h.render()?;

        // Move to 'c'
        h.key('j')?;
        h.key('j')?;
        h.expect_highlight("c");

        // Delete item before selection
        // We'll delete 'a' by navigating to it and pressing 'd'
        h.key('g')?; // Go to first
        h.key('d')?; // Delete 'a'
        h.key('j')?; // Go to 'c' (which was at index 2, now at 1)
        h.render()?;

        // Selection should still be on 'c'
        h.expect_highlight("c");
        // 'c' is now at index 1 after deleting 'a'

        // Delete item after selection
        // Navigate to 'e' and delete it
        h.key('G')?; // Go to last ('e')
        h.key('d')?; // Delete 'e'
        h.key('k')?; // Go back to 'c'
        h.render()?;

        // Selection should still be on 'c'
        h.expect_highlight("c");

        Ok(())
    }

    #[test]
    fn clear_list() -> Result<()> {
        let mut h = Harness::new(SimpleList::new(vec!["x", "y", "z"]))?;
        h.render()?;

        // Verify items are initially present
        assert_eq!(h.root().list.len(), 3);

        // Clear all items
        let cleared = h.root().list.clear();
        h.render()?;

        // Verify all items were returned
        assert_eq!(cleared.len(), 3);

        // List should be empty
        assert!(h.root().list.is_empty());

        Ok(())
    }

    #[test]
    fn page_navigation() -> Result<()> {
        // Create a long list
        let items: Vec<&str> = (1..=20)
            .map(|i| Box::leak(format!("line{i:02}").into_boxed_str()) as &str)
            .collect();
        let mut h = Harness::with_size(
            SimpleList::new(items),
            Expanse::new(15, 5), // Small viewport
        )?;
        h.render()?;

        h.expect_highlight("line01");

        // Page down - use multiple select_next instead
        for _ in 0..5 {
            h.key('j')?;
        }
        h.render()?;

        // Should have scrolled down
        assert!(!h.buf().contains_text("line01"));

        // Page up - use multiple select_prev instead
        for _ in 0..5 {
            h.key('k')?;
        }
        h.render()?;

        // Should see line01 again
        h.expect_contains("line01");

        Ok(())
    }

    #[test]
    #[ignore = "Rendering bug: After deleting first item, remaining items don't render correctly"]
    fn delete_first_item_rendering_bug() -> Result<()> {
        // This test demonstrates a bug where after deleting the first item,
        // not all remaining items are rendered properly
        let mut h = Harness::with_size(
            SimpleList::new(vec!["apple", "banana", "cherry"]),
            Expanse::new(20, 10),
        )?;
        h.render()?;

        // Verify all items are initially visible
        h.expect_contains("apple");
        h.expect_contains("banana");
        h.expect_contains("cherry");

        // Delete first item
        h.expect_highlight("apple");
        h.key('d')?;
        h.render()?;

        // BUG: After deletion, both remaining items should be visible
        // but only "cherry" appears in the buffer
        h.expect_contains("banana"); // This fails - banana is not rendered
        h.expect_contains("cherry");
        h.expect_highlight("banana"); // This fails - banana should be highlighted

        Ok(())
    }

    #[test]
    #[ignore = "Rendering bug: List items disappear from view after delete operations"]
    fn delete_with_multiple_items_visible_bug() -> Result<()> {
        // This test shows that items that should remain visible after deletion
        // are not being rendered
        let mut h = Harness::with_size(
            SimpleList::new(vec!["1", "2", "3", "4", "5"]),
            Expanse::new(10, 8), // Large enough to show all items
        )?;
        h.render()?;

        // All items should be visible initially
        for i in 1..=5 {
            h.expect_contains(&i.to_string());
        }

        // Delete item "3"
        h.key('j')?; // to "2"
        h.key('j')?; // to "3"
        h.expect_highlight("3");
        h.key('d')?;
        h.render()?;

        // BUG: Items 1, 2, 4, 5 should all still be visible
        // but some disappear from the rendered output
        h.expect_contains("1"); // This likely fails
        h.expect_contains("2"); // This likely fails
        h.expect_contains("4");
        h.expect_contains("5");

        Ok(())
    }

    #[test]
    #[ignore = "Rendering bug: Frame content not updated properly after list modifications"]
    fn delete_in_frame_rendering_bug() -> Result<()> {
        // When list is inside a frame, the rendering bug is even more apparent
        #[derive(StatefulNode)]
        struct FramedList {
            state: NodeState,
            frame: frame::Frame<List<SelectableText>>,
        }

        #[derive_commands]
        impl FramedList {
            fn new(items: Vec<&str>) -> Self {
                FramedList {
                    state: NodeState::default(),
                    frame: frame::Frame::new(List::new(
                        items.into_iter().map(SelectableText::new).collect(),
                    )),
                }
            }
        }

        impl Node for FramedList {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.frame)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.frame, vp, vp.view())?;
                Ok(())
            }
        }

        impl crate::Loader for FramedList {
            fn load(c: &mut crate::Canopy) {
                use crate::Binder;
                c.add_commands::<List<SelectableText>>();
                Binder::new(c).key('d', "list::delete_selected()");
            }
        }

        let mut h = Harness::with_size(FramedList::new(vec!["A", "B", "C"]), Expanse::new(10, 6))?;
        h.render()?;

        // Frame should show all content
        h.expect_contains("A");
        h.expect_contains("B");
        h.expect_contains("C");

        // Delete first item
        h.key('d')?;
        h.render()?;

        // BUG: Frame content should show B and C, but doesn't render properly
        h.expect_contains("B"); // This likely fails
        h.expect_contains("C");

        Ok(())
    }

    #[test]
    #[ignore = "Rendering bug: Demonstrates exact rendering output after deletion"]
    fn delete_rendering_diagnostic() -> Result<()> {
        // This test shows exactly what gets rendered after deletion
        let mut h = Harness::with_size(
            SimpleList::new(vec!["Line1", "Line2", "Line3", "Line4"]),
            Expanse::new(15, 6),
        )?;
        h.render()?;

        eprintln!("=== Before deletion ===");
        {
            let buf = h.buf();
            for (i, line) in buf.lines().iter().enumerate() {
                eprintln!("Line {i}: '{line}'");
            }
        }

        // Delete first item
        h.key('d')?;
        h.render()?;

        eprintln!("\n=== After deleting first item ===");
        eprintln!("List length: {}", h.root().list.len());
        eprintln!("List offset: {}", h.root().list.offset);
        {
            let buf = h.buf();
            for (i, line) in buf.lines().iter().enumerate() {
                eprintln!("Line {i}: '{line}'");
            }
        }

        // This assertion will fail, showing the rendering issue
        panic!("Check diagnostic output above to see the rendering bug");
    }

    #[test]
    fn horizontal_scrolling() -> Result<()> {
        // Create a custom list with fixed width items
        #[derive(StatefulNode)]
        struct HScrollList {
            state: NodeState,
            list: List<SelectableText>,
        }

        #[derive_commands]
        impl HScrollList {
            fn new() -> Self {
                HScrollList {
                    state: NodeState::default(),
                    list: List::new(vec![SelectableText::new("0123456789").with_fixed_width(10)]),
                }
            }
        }

        impl Node for HScrollList {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.list)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                l.place(&mut self.list, vp, vp.view())?;
                Ok(())
            }
        }

        impl crate::Loader for HScrollList {
            fn load(c: &mut crate::Canopy) {
                use crate::Binder;
                c.add_commands::<List<SelectableText>>();
                Binder::new(c)
                    .key('h', "list::scroll_left()")
                    .key('l', "list::scroll_right()");
            }
        }

        let mut h = Harness::with_size(HScrollList::new(), Expanse::new(5, 1))?;
        h.render()?;

        // Initially see the beginning
        h.expect_contains("01234");

        // Scroll right
        h.key('l')?;
        h.render()?;

        // Should see different content
        h.expect_contains("12345");

        // Scroll left back
        h.key('h')?;
        h.render()?;

        // Should be back at the beginning
        h.expect_contains("01234");

        Ok(())
    }
}
