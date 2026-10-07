//! Tabbed pages beneath a tab bar, and a tab bar alone for an owner that
//! shows its own pages.

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, TypedId, ViewContext, Widget,
    commands::CommandCall,
    derive_commands,
    error::Result,
    geom::{Line, Point, Rect},
    input::{Event, mouse},
    layout::{Edges, Layout, LayoutOverride},
    render::Render,
    text,
    tree::FocusScope,
};

use crate::border::BoxGlyphs;

/// Columns left blank between tab labels.
const TAB_GAP: u32 = 1;

/// Style paths of a solid tab: inactive, active, and active while focus is
/// within the tabs.
const SOLID: [&str; 3] = ["tabs/tab", "tabs/tab/active", "tabs/tab/active/focused"];
/// Style paths of the label of a bordered tab, by state as in [`SOLID`].
const LABEL: [&str; 3] = [
    "tabs/box/label",
    "tabs/box/label/active",
    "tabs/box/label/active/focused",
];
/// Style paths of the border of a bordered tab, by state as in [`SOLID`].
const BORDER: [&str; 3] = [
    "tabs/box/border",
    "tabs/box/border/active",
    "tabs/box/border/active/focused",
];

/// How the tab bar draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabsLook {
    /// Each label on a filled run of cells, in a bar one row tall.
    #[default]
    Solid,
    /// Each label inside a box border of these glyphs, in a bar three rows
    /// tall.
    Bordered(BoxGlyphs),
}

impl TabsLook {
    /// Return the rows that the bar takes.
    pub const fn rows(self) -> u32 {
        match self {
            Self::Solid => 1,
            Self::Bordered(_) => 3,
        }
    }
}

/// A row of tabs over a set of pages, one page visible at a time.
///
/// Each page is a child node. Tabs hides every page but the active one. When a
/// switch hides the page that held focus, the page remembers the node that
/// held it, and focus moves into the new page: to the node that it remembers,
/// or to its first focusable node when it remembers none, or when that node
/// can no longer take focus.
///
/// The bar occupies the top rows of padding, as many as its [`TabsLook`]
/// takes. A solid bar paints `tabs/bar`, each label `tabs/tab`, and the active
/// label `tabs/tab/active`, or `tabs/tab/active/focused` while focus is within
/// the tabs. A bordered bar paints `tabs/box`, and each tab
/// `tabs/box/label` and `tabs/box/border`, with the same `active` and
/// `active/focused` suffixes.
///
/// An owner that follows the active tab, whether a command or a click changes
/// it, sets [`Tabs::with_on_change`].
pub struct Tabs {
    /// The tabs, in bar order.
    tabs: Vec<Tab>,
    /// Index of the active tab.
    active: usize,
    /// How the bar draws.
    look: TabsLook,
    /// Call posted with the index of the new tab after each change.
    on_change: Option<CommandCall>,
}

/// One tab: its label, its page, and the node that held focus when the page
/// was last left.
struct Tab {
    /// Text of the label in the bar.
    label: String,
    /// The page node.
    page: NodeId,
    /// The node of the page that held focus when a switch hid the page.
    focus: Option<NodeId>,
}

/// Return whether `node` is `root` or lies below it.
fn contains(c: &dyn Context, root: NodeId, node: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == root {
            return true;
        }
        current = c.parent_of(id);
    }
    false
}

/// Return each tab's index, padded label, first column, and width, border
/// included.
fn spans<'a>(
    look: TabsLook,
    labels: impl IntoIterator<Item = &'a str>,
) -> impl Iterator<Item = (usize, String, u32, u32)> {
    let border = match look {
        TabsLook::Solid => 0,
        TabsLook::Bordered(_) => 2,
    };
    let mut next = 0u32;
    labels.into_iter().enumerate().map(move |(index, label)| {
        let text = format!(" {label} ");
        let width = text::width(&text) + border;
        let start = next;
        next = next.saturating_add(width).saturating_add(TAB_GAP);
        (index, text, start, width)
    })
}

/// Draw the bar of `labels` across the top rows of `outer`, with the tab at
/// `active` marked, and lit while `focused`.
fn draw_bar<'a>(
    rndr: &mut Render,
    outer: Rect,
    look: TabsLook,
    labels: impl IntoIterator<Item = &'a str>,
    active: usize,
    focused: bool,
) -> Result<()> {
    if outer.w == 0 || outer.h == 0 {
        return Ok(());
    }
    let bar = Rect::new(outer.tl.x, outer.tl.y, outer.w, look.rows().min(outer.h));
    let state = |index| match (index == active, focused) {
        (false, _) => 0,
        (true, false) => 1,
        (true, true) => 2,
    };
    match look {
        TabsLook::Solid => {
            rndr.fill("tabs/bar", bar, ' ')?;
            for (index, text, start, width) in spans(look, labels) {
                let line = Line::new(bar.tl.x.saturating_add(start), bar.tl.y, width);
                rndr.text(SOLID[state(index)], line, &text)?;
            }
        }
        TabsLook::Bordered(glyphs) => {
            rndr.fill("tabs/box", bar, ' ')?;
            if bar.h < 3 {
                return Ok(());
            }
            for (index, text, start, width) in spans(look, labels) {
                let x = bar.tl.x.saturating_add(start);
                let (border, label) = (BORDER[state(index)], LABEL[state(index)]);
                let rule = glyphs.horizontal.to_string().repeat(width as usize - 2);
                let vertical = glyphs.vertical.to_string();
                let top = format!("{}{rule}{}", glyphs.topleft, glyphs.topright);
                let bottom = format!("{}{rule}{}", glyphs.bottomleft, glyphs.bottomright);
                rndr.text(border, Line::new(x, bar.tl.y, width), &top)?;
                rndr.text(border, Line::new(x, bar.tl.y + 1, 1), &vertical)?;
                rndr.text(label, Line::new(x + 1, bar.tl.y + 1, width - 2), &text)?;
                rndr.text(border, Line::new(x + width - 1, bar.tl.y + 1, 1), &vertical)?;
                rndr.text(border, Line::new(x, bar.tl.y + 2, width), &bottom)?;
            }
        }
    }
    Ok(())
}

/// Return the tab that a left press on the bar of `labels` lands on, if any.
fn clicked<'a>(
    event: &Event,
    ctx: &dyn Context,
    look: TabsLook,
    labels: impl IntoIterator<Item = &'a str>,
) -> Option<usize> {
    let Event::Mouse(m) = event else {
        return None;
    };
    if m.action != mouse::Action::Down || m.button != mouse::Button::Left {
        return None;
    }
    let point: Point = ctx
        .view()
        .outer_point(m.location)
        .filter(|point| point.y < look.rows())?;
    spans(look, labels)
        .find(|(_, _, start, width)| point.x >= *start && point.x < start + width)
        .map(|(index, ..)| index)
}

#[derive_commands]
impl Tabs {
    /// Construct tabs with no pages.
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            look: TabsLook::default(),
            on_change: None,
        }
    }

    /// Draw the bar in `look`.
    #[must_use]
    pub fn with_look(mut self, look: TabsLook) -> Self {
        self.look = look;
        self
    }

    /// Post `call` with the index of the new tab appended each time the
    /// active tab changes, by a command or a click. The index is the last
    /// positional argument, or `index` among named ones.
    #[must_use]
    pub fn with_on_change(mut self, call: CommandCall) -> Self {
        self.on_change = Some(call);
        self
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
        let id = c.add_child(c.node_id(), page)?;
        let node = NodeId::from(id);
        c.set_layout_override(
            node,
            LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
        )?;
        self.tabs.push(Tab {
            label: label.into(),
            page: node,
            focus: None,
        });
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
        self.sync(c, Some(previous))?;
        if let Some(call) = &self.on_change
            && self.active != previous
        {
            c.post(&call.with_arg("index", self.active))?;
        }
        Ok(())
    }

    /// Move the active tab by a signed offset, wrapping around.
    #[command]
    pub fn cycle(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if self.tabs.is_empty() {
            return Ok(());
        }
        let len = self.tabs.len() as i64;
        let next = (self.active as i64 + i64::from(delta)).rem_euclid(len) as usize;
        self.select(c, next)
    }

    /// Show the active page, hide the rest, and move focus off a hidden page
    /// into the active one.
    fn sync(&mut self, c: &mut dyn Context, previous: Option<usize>) -> Result<()> {
        let mut focus_left = false;
        if let Some(tab) = previous
            .filter(|previous| *previous != self.active)
            .and_then(|previous| self.tabs.get_mut(previous))
        {
            tab.focus = c.focused_within(tab.page);
            focus_left = tab.focus.is_some();
        }
        for (index, tab) in self.tabs.iter().enumerate() {
            c.set_hidden(tab.page, index != self.active)?;
        }
        // A page that was hidden has no view until the next layout, so the
        // target must not depend on one.
        if focus_left && let Some(tab) = self.tabs.get(self.active) {
            let restored = match tab
                .focus
                .filter(|node| c.is_attached(*node) && contains(c, tab.page, *node))
            {
                Some(node) => c.focus_first(FocusScope::Node(node))?.changed(),
                None => false,
            };
            if !restored {
                c.focus_first(FocusScope::Node(tab.page))?;
            }
        }
        Ok(())
    }

    /// Return the tab labels, in bar order.
    fn labels(&self) -> impl Iterator<Item = &str> {
        self.tabs.iter().map(|tab| tab.label.as_str())
    }
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Tabs {
    fn layout(&self) -> Layout {
        Layout::fill().padding(Edges::new(self.look.rows(), 0, 0, 0))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let focused = ctx.is_on_focus_path(ctx.node_id());
        let outer = ctx.view().outer_rect_local();
        draw_bar(rndr, outer, self.look, self.labels(), self.active, focused)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        match clicked(event, ctx, self.look, self.labels()) {
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

/// A row of tabs with no pages of its own.
///
/// An owner that keeps its pages elsewhere and shows them itself draws their
/// tabs with this bar. The owner sets the labels and the active tab with
/// [`TabBar::set_tabs`], and follows clicks through
/// [`TabBar::with_on_select`]. The bar draws as the bar of [`Tabs`] does,
/// with the same style paths. Its active label takes the focused style while
/// focus is within the node that [`TabBar::with_scope`] names.
#[derive(Default)]
pub struct TabBar {
    /// The labels, in bar order.
    labels: Vec<String>,
    /// Index of the active tab.
    active: usize,
    /// How the bar draws.
    look: TabsLook,
    /// The node whose focus lights the active label.
    scope: Option<NodeId>,
    /// Call posted with the index of a clicked tab that is not active.
    on_select: Option<CommandCall>,
}

impl TabBar {
    /// Construct a bar with no tabs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw the bar in `look`.
    #[must_use]
    pub fn with_look(mut self, look: TabsLook) -> Self {
        self.look = look;
        self
    }

    /// Light the active label while focus is within `scope`, such as the
    /// region that shows the pages.
    #[must_use]
    pub fn with_scope(mut self, scope: NodeId) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Post `call` with the index of a clicked tab appended, when that tab is
    /// not the active one. The index is the last positional argument, or
    /// `index` among named ones. The owner then shows that tab and sets it
    /// active with [`TabBar::set_tabs`].
    #[must_use]
    pub fn with_on_select(mut self, call: CommandCall) -> Self {
        self.on_select = Some(call);
        self
    }

    /// Show `labels`, with the tab at `active` marked, clamped to the last.
    pub fn set_tabs(&mut self, labels: Vec<String>, active: usize) {
        self.active = active.min(labels.len().saturating_sub(1));
        self.labels = labels;
    }

    /// Return the labels, in bar order.
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Return the index of the active tab.
    pub fn active(&self) -> usize {
        self.active
    }
}

impl Widget for TabBar {
    fn layout(&self) -> Layout {
        Layout::row()
            .flex_horizontal(1)
            .fixed_height(self.look.rows())
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let focused = self.scope.is_some_and(|scope| ctx.is_on_focus_path(scope));
        let outer = ctx.view().outer_rect_local();
        let labels = self.labels.iter().map(String::as_str);
        draw_bar(rndr, outer, self.look, labels, self.active, focused)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let labels = self.labels.iter().map(String::as_str);
        let Some(index) = clicked(event, ctx, self.look, labels) else {
            return Ok(EventOutcome::Ignore);
        };
        if let Some(call) = &self.on_select
            && index != self.active
        {
            ctx.post(&call.with_arg("index", index))?;
        }
        Ok(EventOutcome::Handle)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("tab_bar")
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{
        Register, Setup, commands::CommandTarget, geom::PointI32, input::key,
        testing::harness::Harness,
    };

    use super::*;
    use crate::Container;

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
            let tabs = c.add_child(c.node_id(), Tabs::new())?;
            c.set_layout_override(tabs.into(), Layout::fill().into())?;
            let pages = c.with_widget_mut(tabs, |tabs: &mut Tabs, c| {
                let one = tabs.add_tab(c, "One", Page)?;
                let two = tabs.add_tab(c, "Two", Page)?;
                Ok([NodeId::from(one), NodeId::from(two)])
            })?;
            *self.nodes.borrow_mut() = vec![tabs.into(), pages[0], pages[1]];
            Ok(())
        }
    }

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

    /// Root that mounts tabs over a page of two focusable fields and a
    /// focusable page.
    struct Fields {
        /// The tabs node, the two fields of the first page, and the second
        /// page.
        nodes: Rc<RefCell<Vec<NodeId>>>,
    }

    impl Widget for Fields {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let tabs = c.add_child(c.node_id(), Tabs::new())?;
            c.set_layout_override(tabs.into(), Layout::fill().into())?;
            let nodes = c.with_widget_mut(tabs, |tabs: &mut Tabs, c| {
                let one = tabs.add_tab(c, "One", Container::new(Layout::column()))?;
                let first = c.add_child(NodeId::from(one), Page)?;
                let second = c.add_child(NodeId::from(one), Page)?;
                let two = tabs.add_tab(c, "Two", Page)?;
                Ok([first.into(), second.into(), two.into()])
            })?;
            *self.nodes.borrow_mut() = vec![tabs.into(), nodes[0], nodes[1], nodes[2]];
            Ok(())
        }
    }

    #[test]
    fn a_page_gets_back_the_focus_that_it_had_when_it_was_left() -> Result<()> {
        let nodes = Rc::new(RefCell::new(Vec::new()));
        let mut harness = Harness::builder(Fields {
            nodes: Rc::clone(&nodes),
        })
        .size(20, 5)
        .build()?;
        harness.render()?;
        let [tabs, first, second, two] = nodes.borrow().clone()[..] else {
            panic!("the scene mounts tabs, two fields, and a page");
        };
        let focused = |harness: &Harness| harness.canopy.with_root_view(|c| c.focused_node());
        let select = |harness: &mut Harness, index| -> Result<()> {
            harness.canopy.with_root_context(|c| {
                c.with_widget_mut(tabs, |tabs: &mut Tabs, c| tabs.select(c, index))
            })?;
            harness.render()
        };
        harness
            .canopy
            .with_root_context(|c| c.set_focus(second).map(|_| ()))?;
        select(&mut harness, 1)?;
        assert_eq!(focused(&harness), Some(two));
        select(&mut harness, 0)?;
        assert_eq!(focused(&harness), Some(second), "the field that was left");

        // A remembered node that can no longer take focus gives way to the
        // first focusable node of the page.
        select(&mut harness, 1)?;
        harness
            .canopy
            .with_root_context(|c| c.set_hidden(second, true).map(|_| ()))?;
        select(&mut harness, 0)?;
        assert_eq!(focused(&harness), Some(first));
        Ok(())
    }

    /// Root that mounts tabs and records each change that they post.
    #[derive(Default)]
    struct Owner {
        /// How the bar draws.
        look: TabsLook,
        /// Indexes that the tabs posted, in order.
        changes: Vec<usize>,
    }

    #[derive_commands]
    impl Owner {
        /// Records one change of the active tab.
        /// @param index The new tab.
        #[command]
        fn changed(&mut self, index: usize) {
            self.changes.push(index);
        }
    }

    impl Widget for Owner {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let owner = CommandTarget::Exact(c.node_id());
            let tabs = c.add_child(
                c.node_id(),
                Tabs::new()
                    .with_look(self.look)
                    .with_on_change(Self::spec_changed().call().with_target(owner)),
            )?;
            c.set_layout_override(tabs.into(), Layout::fill().into())?;
            c.with_widget_mut(tabs, |tabs: &mut Tabs, c| {
                tabs.add_tab(c, "One", Page)?;
                tabs.add_tab(c, "Two", Page)?;
                Ok(())
            })
        }

        fn name(&self) -> NodeName {
            NodeName::convert("owner")
        }
    }

    impl Register for Owner {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Tabs>()?;
            setup.add_commands::<Self>()
        }
    }

    #[test]
    fn a_change_by_command_or_by_click_posts_the_new_index() -> Result<()> {
        let mut harness = Harness::builder(Owner::default())
            .register::<Owner>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        let changes = |harness: &mut Harness| {
            harness.with_root_widget(|owner: &mut Owner| owner.changes.clone())
        };

        harness.script("tabs.select(1)")?;
        assert_eq!(changes(&mut harness), [1]);
        // A tab that already shows changes nothing.
        harness.script("tabs.select(1)")?;
        assert_eq!(changes(&mut harness), [1]);

        harness.render()?;
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 1, y: 0 },
        })?;
        assert_eq!(
            changes(&mut harness),
            [1, 0],
            "a click on a label posts too"
        );
        Ok(())
    }

    #[test]
    fn a_bordered_bar_boxes_each_label_and_a_click_in_a_box_selects_it() -> Result<()> {
        let owner = Owner {
            look: TabsLook::Bordered(BoxGlyphs::ROUND),
            ..Owner::default()
        };
        let mut harness = Harness::builder(owner)
            .register::<Owner>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        let bar = lines[..3]
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>();
        assert_eq!(
            bar,
            ["╭─────╮ ╭─────╮", "│ One │ │ Two │", "╰─────╯ ╰─────╯"]
        );

        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 10, y: 2 },
        })?;
        let changes = harness.with_root_widget(|owner: &mut Owner| owner.changes.clone());
        assert_eq!(changes, [1], "a click on the border of a box selects it");
        Ok(())
    }

    /// Root that mounts a bar over two focusable pages that it shows itself.
    #[derive(Default)]
    struct BarOwner {
        /// The bar, the page its scope names, and the other page.
        nodes: Vec<NodeId>,
        /// Indexes that the bar posted, in order.
        selections: Vec<usize>,
    }

    #[derive_commands]
    impl BarOwner {
        /// Records a click on a tab, and makes that tab active.
        /// @param index The clicked tab.
        #[command]
        fn selected(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
            self.selections.push(index);
            c.with_widget_mut(self.nodes[0], |bar: &mut TabBar, _| {
                let labels = bar.labels().to_vec();
                bar.set_tabs(labels, index);
                Ok(())
            })
        }
    }

    impl Widget for BarOwner {
        fn layout(&self) -> Layout {
            Layout::column()
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let owner = CommandTarget::Exact(c.node_id());
            let scope = c.add_child(c.node_id(), Page)?;
            let other = c.add_child(c.node_id(), Page)?;
            let mut bar = TabBar::new()
                .with_scope(scope.into())
                .with_on_select(Self::spec_selected().call().with_target(owner));
            bar.set_tabs(vec!["view".into(), "terminal".into()], 0);
            let bar = c.add_child(c.node_id(), bar)?;
            c.set_children(c.node_id(), vec![bar.into(), scope.into(), other.into()])?;
            for page in [scope, other] {
                c.set_layout_override(page.into(), LayoutOverride::new().fixed_height(1))?;
            }
            self.nodes = vec![bar.into(), scope.into(), other.into()];
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("bar_owner")
        }
    }

    impl Register for BarOwner {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    #[test]
    fn a_bar_draws_its_labels_lit_by_its_scope_and_posts_a_click() -> Result<()> {
        let mut harness = Harness::builder(BarOwner::default())
            .register::<BarOwner>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        let nodes = harness.with_root_widget(|owner: &mut BarOwner| owner.nodes.clone());
        assert_eq!(harness.tbuf().lines()[0].trim_end(), " view   terminal");

        // The active label takes the focused style only while its scope holds
        // focus.
        let active_style = |harness: &mut Harness, focus: NodeId| -> Result<_> {
            harness
                .canopy
                .with_root_context(|c| c.set_focus(focus).map(|_| ()))?;
            harness.render()?;
            Ok(harness.buf().get(Point { x: 1, y: 0 }).expect("cell").style)
        };
        let lit = active_style(&mut harness, nodes[1])?;
        let unlit = active_style(&mut harness, nodes[2])?;
        assert_ne!(lit, unlit);
        let inactive = harness.buf().get(Point { x: 9, y: 0 }).expect("cell").style;
        assert_ne!(unlit, inactive, "the active label stands out without focus");

        let click = |harness: &mut Harness| {
            harness.mouse(mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: PointI32 { x: 9, y: 0 },
            })
        };
        click(&mut harness)?;
        click(&mut harness)?;
        let selections = harness.with_root_widget(|owner: &mut BarOwner| owner.selections.clone());
        assert_eq!(selections, [1], "a click on the active tab posts nothing");
        Ok(())
    }
}
