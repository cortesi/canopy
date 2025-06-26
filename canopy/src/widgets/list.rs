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
        if !self.is_empty() && offset < self.len() {
            let itm = self.items.remove(offset);
            if offset <= self.offset {
                self.select_prev(core);
            }
            Some(itm.itm)
        } else {
            None
        }
    }

    /// Make sure the selected item is within the view after a change.
    fn ensure_selected_in_view(&mut self, c: &mut dyn Context) -> bool {
        let virt = self.items[self.offset].virt;
        let view = self.vp().view();
        if let Some(v) = virt.vextent().intersection(&view.vextent()) {
            if v.len == virt.h {
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
        let changed = self.select(0);
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
        if c.scroll_down(self) {
            c.taint(self);
        }
    }

    /// Scroll the viewport up by one line.
    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        if c.scroll_up(self) {
            c.taint(self);
        }
    }

    /// Scroll the viewport left by one column.
    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        if c.scroll_left(self) {
            c.taint(self);
        }
    }

    /// Scroll the viewport right by one column.
    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        if c.scroll_right(self) {
            c.taint(self);
        }
    }

    /// Scroll the viewport down by one page.
    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        if c.page_down(self) {
            c.taint(self);
        }
    }

    /// Scroll the viewport up by one page.
    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        if c.page_up(self) {
            c.taint(self);
        }
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
        for itm in &mut self.items {
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
                    let ch_rect = Rect::new(
                        ch.vp().position().x - final_vp.position().x,
                        ch.vp().position().y - final_vp.position().y,
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
        backend::test::CanvasRender, cursor, geom::Point, render::RenderBackend, style::Style,
        widgets::frame, widgets::Text, Context,
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
        let bottom = first[(size.h - 1) as usize].clone();

        assert_eq!(first[1][1], 'A');
        assert_eq!(first[2][1], 'B');

        let corner = first[0][0];

        canopy.scroll_down(&mut root.list.child);
        canopy.taint_tree(&mut root);
        canopy.render(&mut cr, &mut root)?;
        let second = buf.lock().unwrap().cells.clone();

        assert_eq!(second[1][1], 'B');
        assert_eq!(second[2][1], 'C');
        assert_eq!(second[0][0], corner);
        assert_eq!(second[(size.h - 1) as usize], bottom);

        canopy.scroll_up(&mut root.list.child);
        canopy.taint_tree(&mut root);
        canopy.render(&mut cr, &mut root)?;
        let third = buf.lock().unwrap().cells.clone();

        assert_eq!(third[1][1], 'A');
        assert_eq!(third[2][1], 'B');
        assert_eq!(third[0][0], corner);
        assert_eq!(third[(size.h - 1) as usize], bottom);

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
        fn show_cursor(&mut self, _c: cursor::Cursor) -> Result<()> {
            Ok(())
        }
        fn hide_cursor(&mut self) -> Result<()> {
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
            for row in painted.iter() {
                assert!(row.iter().all(|&c| c));
            }
        }

        canopy.scroll_down(&mut root.frame.child);
        canopy.taint_tree(&mut root);
        canopy.render(&mut pr, &mut root)?;
        {
            let painted = buf.lock().unwrap();
            for row in painted.iter() {
                assert!(row.iter().all(|&c| c));
            }
        }

        canopy.scroll_up(&mut root.frame.child);
        canopy.taint_tree(&mut root);
        canopy.render(&mut pr, &mut root)?;
        {
            let painted = buf.lock().unwrap();
            for row in painted.iter() {
                assert!(row.iter().all(|&c| c));
            }
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
        canopy.taint_tree(&mut root);
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
        canopy.taint_tree(&mut root);
        canopy.render(&mut cr, &mut root)?;

        {
            let canvas = buf.lock().unwrap();
            assert_eq!(canvas.cells[1][3], '1');
        }

        Ok(())
    }
}
