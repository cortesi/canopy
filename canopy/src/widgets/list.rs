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
    pub fn select(&mut self, offset: usize) {
        if !self.is_empty() {
            self.offset = self.offset.clamp(0, self.len() - 1);
            self.items[self.offset].set_selected(false);
            self.offset = offset.clamp(0, self.items.len() - 1);
            self.items[self.offset].set_selected(true);
        }
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
    fn ensure_selected_in_view(&mut self, c: &dyn Context) {
        let virt = self.items[self.offset].virt;
        let view = self.vp().view;
        if let Some(v) = virt.vextent().intersection(&view.vextent()) {
            if v.len == virt.h {
                return;
            }
        }
        let (start, end) = self.view_range();
        // We know there isn't an entire overlap
        if self.offset <= start {
            c.scroll_to(self, view.tl.x, virt.tl.y);
        } else if self.offset >= end {
            if virt.h >= view.h {
                c.scroll_to(self, view.tl.x, virt.tl.y);
            } else {
                let y = virt.tl.y - (view.h - virt.h);
                c.scroll_to(self, view.tl.x, y);
            }
        }
    }

    /// Calculate which items are in the list's vertical window, and return
    /// their offsets and sizes. Items that are offscreen to the side are also
    /// returned, so the returned vector is guaranteed to be a contiguous range.
    fn in_view(&self) -> Vec<usize> {
        let view = self.vp().view;
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
    pub fn select_first(&mut self, c: &dyn Context) {
        self.select(0);
        self.ensure_selected_in_view(c);
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_last(&mut self, c: &dyn Context) {
        self.select(self.len());
        self.ensure_selected_in_view(c);
    }

    /// Move selection to the next item in the list, if possible.
    #[command]
    pub fn select_next(&mut self, c: &dyn Context) {
        self.select(self.offset.saturating_add(1));
        self.ensure_selected_in_view(c);
    }

    /// Move selection to the next previous the list, if possible.
    #[command]
    pub fn select_prev(&mut self, c: &dyn Context) {
        self.select(self.offset.saturating_sub(1));
        self.ensure_selected_in_view(c);
    }

    /// Scroll the viewport down by one line.
    #[command]
    pub fn scroll_down(&mut self, c: &dyn Context) {
        c.scroll_down(self);
    }

    /// Scroll the viewport up by one line.
    #[command]
    pub fn scroll_up(&mut self, c: &dyn Context) {
        c.scroll_up(self);
    }

    /// Scroll the viewport left by one column.
    #[command]
    pub fn scroll_left(&mut self, c: &dyn Context) {
        c.scroll_left(self);
    }

    /// Scroll the viewport right by one column.
    #[command]
    pub fn scroll_right(&mut self, c: &dyn Context) {
        c.scroll_right(self);
    }

    /// Scroll the viewport down by one page.
    #[command]
    pub fn page_down(&mut self, c: &dyn Context) {
        c.page_down(self);
    }

    /// Scroll the viewport up by one page.
    #[command]
    pub fn page_up(&mut self, c: &dyn Context) {
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
            let item_view = itm.itm.vp().canvas.rect();
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
                *itm.itm.__vp_mut() = child_vp;
                itm.itm.unhide();
            } else {
                itm.itm.hide();
                itm.itm.__vp_mut().view = Rect::default();
            }
        }
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
        rndr.fill("", self.vp().canvas.rect(), ' ')?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use crate::{
        backend::test::TestRender,
        backend::test::CanvasRender,
        tutils::{DummyContext, TFixed},
        Context,
        widgets::Text,
        widgets::frame,
        cursor,
        geom::Point,
        render::RenderBackend,
        style::Style,
    };

    impl ListItem for Text {}

    pub fn views(lst: &mut List<TFixed>) -> Vec<Rect> {
        let mut v = vec![];
        lst.children(&mut |x: &mut dyn Node| {
            v.push(if x.is_hidden() {
                Rect::default()
            } else {
                x.vp().view
            });
            Ok(())
        })
        .unwrap();
        v
    }

    #[test]
    fn select() -> Result<()> {
        let dc = DummyContext {};

        // Empty initilization shouldn't fail
        let _: List<TFixed> = List::new(Vec::new());

        let mut lst = List::new(vec![
            TFixed::new(10, 10),
            TFixed::new(10, 10),
            TFixed::new(10, 10),
        ]);
        assert_eq!(lst.offset, 0);
        lst.select_prev(&dc);
        assert_eq!(lst.offset, 0);
        lst.select_next(&dc);
        assert_eq!(lst.offset, 1);
        lst.select_next(&dc);
        lst.select_next(&dc);
        assert_eq!(lst.offset, 2);

        Ok(())
    }

    #[test]
    fn drawnodes() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut c = Canopy::new();

        let rw = 20;
        let rh = 10;
        let mut lst = List::new(vec![
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
            TFixed::new(rw, rh),
        ]);

        let l = Layout {};

        lst.layout(&l, Expanse::new(10, 10))?;
        tr.render(&mut c, &mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        c.scroll_by(&mut lst, 0, 5);
        lst.layout(&l, Expanse::new(10, 10))?;
        c.taint_tree(&mut lst);
        tr.render(&mut c, &mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 5, 10, 5),
                Rect::new(0, 0, 10, 5),
                Rect::new(0, 0, 0, 0),
            ]
        );

        c.scroll_by(&mut lst, 0, 5);
        lst.layout(&l, Expanse::new(10, 10))?;
        c.taint_tree(&mut lst);
        tr.render(&mut c, &mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
            ]
        );

        c.scroll_by(&mut lst, 0, 10);
        lst.layout(&l, Expanse::new(10, 10))?;
        c.taint_tree(&mut lst);
        tr.render(&mut c, &mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        c.scroll_by(&mut lst, 0, 10);
        lst.layout(&l, Expanse::new(10, 10))?;
        c.taint_tree(&mut lst);
        tr.render(&mut c, &mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 10, 10),
            ]
        );

        c.scroll_to(&mut lst, 5, 0);
        lst.layout(&l, Expanse::new(10, 10))?;
        c.taint_tree(&mut lst);
        tr.render(&mut c, &mut lst)?;
        assert_eq!(
            views(&mut lst),
            vec![
                Rect::new(5, 0, 10, 10),
                Rect::new(0, 0, 0, 0),
                Rect::new(0, 0, 0, 0),
            ]
        );

        c.scroll_by(&mut lst, 0, 5);
        lst.layout(&l, Expanse::new(10, 10))?;
        c.taint_tree(&mut lst);
        tr.render(&mut c, &mut lst)?;
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
                l.place(&mut self.list, vp, vp.view)?;
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

        let corner = first[0][0];

        canopy.scroll_down(&mut root.list.child);
        canopy.taint_tree(&mut root);
        canopy.render(&mut cr, &mut root)?;
        let second = buf.lock().unwrap().cells.clone();

        assert_eq!(second[1][1], 'B');
        assert_eq!(second[2][1], 'C');
        assert_eq!(second[0][0], corner);

        canopy.scroll_up(&mut root.list.child);
        canopy.taint_tree(&mut root);
        canopy.render(&mut cr, &mut root)?;
        let third = buf.lock().unwrap().cells.clone();

        assert_eq!(third[1][1], 'A');
        assert_eq!(third[2][1], 'B');
        assert_eq!(third[0][0], corner);

        Ok(())
    }


    struct PaintTracker {
        painted: Arc<Mutex<Vec<Vec<bool>>>>,
        size: Expanse,
    }

    impl PaintTracker {
        fn create(size: Expanse) -> (Arc<Mutex<Vec<Vec<bool>>>>, Self) {
            let buf = Arc::new(Mutex::new(vec![vec![false; size.w as usize]; size.h as usize]));
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

        fn flush(&mut self) -> Result<()> { Ok(()) }
        fn show_cursor(&mut self, _c: cursor::Cursor) -> Result<()> { Ok(()) }
        fn hide_cursor(&mut self) -> Result<()> { Ok(()) }
        fn style(&mut self, _s: Style) -> Result<()> { Ok(()) }

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

        fn exit(&mut self, _code: i32) -> ! { unreachable!() }
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
                Block { state: NodeState::default(), text: Text::new(SAMPLE).with_fixed_width(w) }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse { w: vp.canvas.w, h: vp.canvas.h };
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
                l.place(&mut self.frame, vp, vp.view)?;
                Ok(())
            }
        }

        let mut canopy = Canopy::new();
        let size = Expanse::new(20, 20);
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;

        let list_rect = root.frame.child.vp().view;
        let mut rects = Vec::new();
        root.frame.child.children(&mut |n| {
            if !n.is_hidden() {
                rects.push(n.vp().view);
            }
            Ok(())
        })?;
        for r in &rects {
            assert!(list_rect.contains_rect(r));
        }

        Ok(())
    }

    #[test]
    fn canvas_background_painted() -> Result<()> {
        const SAMPLE: &str = "aaa bbb ccc\nddd";

        #[derive(StatefulNode)]
        struct Block {
            state: NodeState,
            text: Text,
        }

        #[derive_commands]
        impl Block {
            fn new(w: u16) -> Self {
                Block { state: NodeState::default(), text: Text::new(SAMPLE).with_fixed_width(w) }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse { w: vp.canvas.w, h: vp.canvas.h };
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
                l.place(&mut self.frame, vp, vp.view)?;
                Ok(())
            }
        }

        let size = Expanse::new(20, 8);
        let (buf, mut pr) = PaintTracker::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut pr, &mut root)?;

        let painted = buf.lock().unwrap();
        let rect = root.vp().screen_rect();
        assert!(painted[0][0]);
        assert!(painted[(rect.h - 1) as usize][(rect.w - 1) as usize]);

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
                Block { state: NodeState::default(), text: Text::new(SAMPLE).with_fixed_width(w) }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse { w: vp.canvas.w, h: vp.canvas.h };
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
                l.place(&mut self.frame, vp, vp.view)?;
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
                Block { state: NodeState::default(), text: Text::new(SAMPLE).with_fixed_width(w) }
            }
        }

        impl ListItem for Block {}

        impl Node for Block {
            fn layout(&mut self, l: &Layout, s: Expanse) -> Result<()> {
                l.fill(self, s)?;
                let vp = self.vp();
                l.place(&mut self.text, vp, Rect::new(0, 0, s.w, s.h))?;
                let vp = self.text.vp();
                let sz = Expanse { w: vp.canvas.w, h: vp.canvas.h };
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
                l.place(&mut self.frame, vp, vp.view)?;
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

}
