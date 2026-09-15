//! Tabbed pages beneath a one-row tab bar.

use canopy::{
    Context, ContextExt, EventOutcome, FocusScope, NodeId, NodeName, Render, TypedId, ViewContext,
    Widget, derive_commands,
    error::Result,
    event::{Event, mouse},
    geom::{Line, Point},
    layout::{Edges, Layout, Sizing},
};
use unicode_width::UnicodeWidthStr;

/// Columns left blank between tab labels.
const TAB_GAP: u32 = 1;

/// A row of tabs over a set of pages, one page visible at a time.
///
/// Each page is a child node. Tabs hides every page but the active one. When a
/// switch hides the page that held focus, focus moves to the first focusable
/// node of the new page.
///
/// The bar occupies the top row of padding. It paints `tabs/bar`, each label
/// `tabs/tab`, and the active label `tabs/tab/active`, or
/// `tabs/tab/active/focused` while focus is within the tabs.
pub struct Tabs {
    /// Labels and page nodes, in bar order.
    tabs: Vec<(String, NodeId)>,
    /// Index of the active tab.
    active: usize,
}

#[derive_commands]
impl Tabs {
    /// Construct tabs with no pages.
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    /// Add `page` under a new tab at the end of the bar and return its node.
    ///
    /// The first page added is active. Later pages start hidden.
    pub fn add_tab<W: Widget + 'static>(
        &mut self,
        c: &mut dyn Context,
        label: impl Into<String>,
        page: W,
    ) -> Result<TypedId<W>> {
        let id = c.add_child(page)?;
        let node = NodeId::from(id);
        c.with_layout_of(node, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })?;
        self.tabs.push((label.into(), node));
        self.sync(c, None)?;
        Ok(id)
    }

    /// Return the index of the active tab.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Activate the tab at `index`, clamped to the last tab.
    #[command]
    pub fn select(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        let Some(last) = self.tabs.len().checked_sub(1) else {
            return Ok(());
        };
        let previous = self.active;
        self.active = index.min(last);
        self.sync(c, Some(previous))
    }

    /// Move the active tab by a signed offset, wrapping around.
    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if self.tabs.is_empty() {
            return Ok(());
        }
        let len = self.tabs.len() as i64;
        let next = (self.active as i64 + i64::from(delta)).rem_euclid(len) as usize;
        self.select(c, next)
    }

    /// Show the active page, hide the rest, and move focus off a hidden page.
    fn sync(&self, c: &mut dyn Context, previous: Option<usize>) -> Result<()> {
        let focus_left = previous
            .filter(|previous| *previous != self.active)
            .and_then(|previous| self.tabs.get(previous))
            .is_some_and(|(_, page)| c.is_on_focus_path_of(*page));
        for (index, (_, page)) in self.tabs.iter().enumerate() {
            c.set_hidden_of(*page, index != self.active)?;
        }
        // A page that was hidden has no view until the next layout, so the
        // target must not depend on one.
        if focus_left && let Some((_, page)) = self.tabs.get(self.active) {
            c.focus_first(FocusScope::Node(*page))?;
        }
        Ok(())
    }

    /// Return each tab's index, padded label, first column, and width.
    fn spans(&self) -> impl Iterator<Item = (usize, String, u32, u32)> + '_ {
        let mut next = 0u32;
        self.tabs
            .iter()
            .enumerate()
            .map(move |(index, (label, _))| {
                let text = format!(" {label} ");
                let width = UnicodeWidthStr::width(text.as_str()) as u32;
                let start = next;
                next = next.saturating_add(width).saturating_add(TAB_GAP);
                (index, text, start, width)
            })
    }
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Tabs {
    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::new(1, 0, 0, 0))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let outer = ctx.view().outer_rect_local();
        if outer.w == 0 || outer.h == 0 {
            return Ok(());
        }
        let bar = outer.line(0)?;
        rndr.fill("tabs/bar", bar.rect(), ' ')?;
        let focused = ctx.is_on_focus_path();
        for (index, text, start, width) in self.spans() {
            let style = if index != self.active {
                "tabs/tab"
            } else if focused {
                "tabs/tab/active/focused"
            } else {
                "tabs/tab/active"
            };
            let line = Line {
                tl: Point {
                    x: bar.tl.x.saturating_add(start),
                    y: bar.tl.y,
                },
                w: width,
            };
            rndr.text(style, line, &text)?;
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(m) = event else {
            return Ok(EventOutcome::Ignore);
        };
        if m.action != mouse::Action::Down || m.button != mouse::Button::Left {
            return Ok(EventOutcome::Ignore);
        }
        let Some(point) = ctx
            .view()
            .outer_point(m.location)
            .filter(|point| point.y == 0)
        else {
            return Ok(EventOutcome::Ignore);
        };
        let hit = self
            .spans()
            .find(|(_, _, start, width)| point.x >= *start && point.x < start + width)
            .map(|(index, ..)| index);
        match hit {
            Some(index) => {
                self.select(ctx, index)?;
                Ok(EventOutcome::Handle)
            }
            None => Ok(EventOutcome::Ignore),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("tabs")
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{Loader, testing::harness::Harness};

    use super::*;

    /// A page that accepts focus.
    struct Page;

    impl Widget for Page {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }
    }

    /// Root that mounts tabs over two focusable pages.
    struct Scene {
        /// The tabs node, then each page node.
        nodes: Rc<RefCell<Vec<NodeId>>>,
    }

    impl Widget for Scene {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let tabs = c.add_child(Tabs::new())?;
            c.set_layout_of(tabs, Layout::fill())?;
            let pages = c.with_widget_mut(tabs, |tabs: &mut Tabs, c| {
                let one = tabs.add_tab(c, "One", Page)?;
                let two = tabs.add_tab(c, "Two", Page)?;
                Ok([NodeId::from(one), NodeId::from(two)])
            })?;
            *self.nodes.borrow_mut() = vec![tabs.into(), pages[0], pages[1]];
            Ok(())
        }
    }

    impl Loader for Scene {}

    #[test]
    fn switching_away_from_the_focused_page_focuses_the_new_page() -> Result<()> {
        let nodes = Rc::new(RefCell::new(Vec::new()));
        let scene = Scene {
            nodes: Rc::clone(&nodes),
        };
        let mut harness = Harness::builder(scene).size(20, 5).build()?;
        harness.render()?;
        let [tabs, one, two] = nodes.borrow().clone()[..] else {
            panic!("the scene mounts tabs and two pages");
        };
        let focused = |harness: &Harness| harness.canopy.with_root_view(|c| c.focused_node());
        assert_eq!(focused(&harness), Some(one));

        // The second page has never been laid out, so it has no view yet.
        let tabs = |index| {
            move |c: &mut dyn Context| {
                c.with_widget_mut(tabs, |tabs: &mut Tabs, c| tabs.select(c, index))
            }
        };
        harness.canopy.with_root_context(tabs(1))?;
        assert_eq!(focused(&harness), Some(two));
        harness.render()?;
        assert_eq!(focused(&harness), Some(two));

        harness.canopy.with_root_context(tabs(0))?;
        harness.render()?;
        assert_eq!(focused(&harness), Some(one));
        Ok(())
    }
}
