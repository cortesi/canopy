use std::{env, time::Duration};

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, cursor, derive_commands,
    error::{Error, Result},
    event::{Event, key, mouse},
    geom::Point,
    layout::{Constraint, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{AttrSet, solarized},
    widget::{EventOutcome, Widget},
    widgets::{
        Box, Button, Center, Root, Terminal, TerminalConfig, Text, boxed, frame,
        list::{List, Selectable},
    },
};
use unicode_width::UnicodeWidthStr;

/// Height for each terminal entry row, including borders.
const ENTRY_HEIGHT: u32 = 3;

/// List item widget for the terminal sidebar.
struct TermEntry {
    /// Label text.
    label: String,
    /// Selection state.
    selected: bool,
    /// Mounted box node ID.
    box_id: Option<NodeId>,
    /// Mounted text node ID.
    text_id: Option<NodeId>,
}

#[derive_commands]
impl TermEntry {
    /// Construct a new terminal entry.
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            selected: false,
            box_id: None,
            text_id: None,
        }
    }

    /// Compute the display width of the label text.
    fn label_width(&self) -> u32 {
        UnicodeWidthStr::width(self.label.as_str()).max(1) as u32
    }

    /// Ensure the child widget tree is mounted.
    fn ensure_tree(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.box_id.is_some() && self.text_id.is_some() {
            return Ok(());
        }

        let box_id = ctx.add_orphan(Box::new().with_glyphs(boxed::SINGLE).with_fill());
        let center_id = ctx.add_orphan(Center::new());
        let text_id =
            ctx.add_orphan(Text::new(self.label.clone()).with_wrap_width(self.label_width()));

        ctx.mount_child_to(center_id, text_id)?;
        ctx.mount_child_to(box_id, center_id)?;
        ctx.mount_child_to(ctx.node_id(), box_id)?;

        self.box_id = Some(box_id);
        self.text_id = Some(text_id);

        Ok(())
    }
}

impl Selectable for TermEntry {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Widget for TermEntry {
    fn layout(&self) -> Layout {
        Layout::column()
            .flex_horizontal(1)
            .fixed_height(ENTRY_HEIGHT)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.ensure_tree(ctx)
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("entry");
        if self.selected {
            rndr.push_layer("selected");
        }
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => self.label_width().saturating_add(2),
        };
        c.clamp(Size::new(width, ENTRY_HEIGHT))
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("term_entry")
    }
}

/// Stack container for terminal widgets.
struct TerminalStack;

#[derive_commands]
impl TerminalStack {
    /// Construct a terminal stack container.
    fn new() -> Self {
        Self
    }
}

impl Widget for TerminalStack {
    fn layout(&self) -> Layout {
        Layout::stack().flex_horizontal(1).flex_vertical(1)
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }
}

/// Sidebar container that owns the button and terminal list.
struct Sidebar;

#[derive_commands]
impl Sidebar {
    /// Construct a sidebar container.
    fn new() -> Self {
        Self
    }
}

impl Widget for Sidebar {
    fn layout(&self) -> Layout {
        Layout::column().flex_vertical(1)
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }
}

/// Multi-terminal demo widget.
pub struct TermGym {
    /// Node ID for the terminal list widget.
    list_id: Option<NodeId>,
    /// Node ID for the terminal stack container.
    stack_id: Option<NodeId>,
    /// Node IDs for each terminal instance.
    terminals: Vec<NodeId>,
    /// Index of the active terminal.
    active: usize,
}

impl Default for TermGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl TermGym {
    /// Construct the terminal gym demo.
    pub fn new() -> Self {
        Self {
            list_id: None,
            stack_id: None,
            terminals: Vec::new(),
            active: 0,
        }
    }

    /// Ensure the widget tree is mounted.
    fn ensure_tree(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.list_id.is_some() && self.stack_id.is_some() {
            return Ok(());
        }

        let list_id = c.add_orphan(List::<TermEntry>::new());
        let button_id = c.add_orphan(
            Button::new("+ New terminal").with_command(Self::cmd_new_terminal().call()),
        );
        let sidebar_id = c.add_orphan(Sidebar::new());
        c.mount_child_to(sidebar_id, button_id)?;
        c.mount_child_to(sidebar_id, list_id)?;

        let stack_id = c.add_orphan(TerminalStack::new());
        let term_frame_id = c.add_orphan(
            frame::Frame::new()
                .with_glyphs(boxed::ROUND_THICK)
                .with_title("terminal"),
        );
        c.mount_child_to(term_frame_id, stack_id)?;

        c.set_children(vec![sidebar_id, term_frame_id])?;

        c.with_layout(&mut |layout| {
            *layout = Layout::row().flex_horizontal(1).flex_vertical(1);
        })?;
        c.with_layout_of(sidebar_id, &mut |layout| {
            *layout = Layout::column().fixed_width(24).flex_vertical(1);
        })?;
        c.with_layout_of(button_id, &mut |layout| {
            *layout = Layout::fill().fixed_height(ENTRY_HEIGHT);
        })?;
        c.with_layout_of(list_id, &mut |layout| {
            *layout = Layout::fill();
        })?;
        c.with_layout_of(stack_id, &mut |layout| {
            *layout = Layout::stack().flex_horizontal(1).flex_vertical(1);
        })?;

        self.list_id = Some(list_id);
        self.stack_id = Some(stack_id);
        self.add_terminal(c)?;

        Ok(())
    }

    /// Execute a closure with the terminal list widget.
    fn with_list<F, R>(&mut self, c: &mut dyn Context, mut f: F) -> Result<R>
    where
        F: FnMut(&mut List<TermEntry>, &mut dyn Context) -> Result<R>,
    {
        self.ensure_tree(c)?;
        let list_id = self
            .list_id
            .ok_or_else(|| Error::Internal("list not initialized".into()))?;
        c.with_widget(list_id, |list: &mut List<TermEntry>, ctx| f(list, ctx))
    }

    /// Create and mount a new terminal instance.
    fn add_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        let stack_id = self
            .stack_id
            .ok_or_else(|| Error::Internal("stack not initialized".into()))?;
        let cwd = env::current_dir().map_err(|err| Error::Internal(err.to_string()))?;
        let terminal_id = c.add_orphan(Terminal::new(TerminalConfig {
            cwd: Some(cwd),
            ..TerminalConfig::default()
        }));
        c.mount_child_to(stack_id, terminal_id)?;
        c.with_layout_of(terminal_id, &mut |layout| {
            *layout = Layout::fill();
        })?;

        self.terminals.push(terminal_id);
        let index = self.terminals.len();
        self.with_list(c, |list, ctx| {
            let entry = TermEntry::new(index.to_string());
            list.append(ctx, entry)?;
            Ok(())
        })?;

        self.set_active(c, self.terminals.len().saturating_sub(1))?;
        Ok(())
    }

    /// Activate a specific terminal by index.
    fn set_active(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        if self.terminals.is_empty() {
            return Ok(());
        }
        let target = index.min(self.terminals.len() - 1);
        self.active = target;

        for (idx, terminal_id) in self.terminals.iter().enumerate() {
            let active = self.active;
            c.with_layout_of(*terminal_id, &mut |node_layout| {
                *node_layout = if idx == active {
                    Layout::fill()
                } else {
                    Layout::fill().none()
                };
            })?;
        }

        let active = self.active;
        self.with_list(c, |list, ctx| {
            list.select(ctx, active);
            Ok(())
        })?;

        if let Some(active_id) = self.terminals.get(self.active).copied() {
            c.set_focus(active_id);
        }

        Ok(())
    }

    /// Move the active terminal selection forward or backward.
    fn shift_terminal(&mut self, c: &mut dyn Context, forward: bool) -> Result<()> {
        if self.terminals.is_empty() {
            return Ok(());
        }
        let next = if forward {
            (self.active + 1) % self.terminals.len()
        } else {
            (self.active + self.terminals.len() - 1) % self.terminals.len()
        };
        self.set_active(c, next)
    }

    /// Rebuild the sidebar list to match the current terminal set.
    fn rebuild_list(&mut self, c: &mut dyn Context) -> Result<()> {
        let count = self.terminals.len();
        self.with_list(c, |list, ctx| {
            list.clear(ctx)?;
            for index in 1..=count {
                let entry = TermEntry::new(index.to_string());
                list.append(ctx, entry)?;
            }
            Ok(())
        })
    }

    /// Remove a terminal at a specific index.
    fn remove_terminal(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        self.ensure_tree(c)?;

        if index >= self.terminals.len() {
            return Ok(());
        }

        let stack_id = self
            .stack_id
            .ok_or_else(|| Error::Internal("stack not initialized".into()))?;
        let removed_id = self.terminals.remove(index);
        c.detach_child_from(stack_id, removed_id)?;

        self.rebuild_list(c)?;

        if self.terminals.is_empty() {
            self.active = 0;
            return Ok(());
        }

        let next_active = if index < self.active {
            self.active.saturating_sub(1)
        } else {
            self.active.min(self.terminals.len() - 1)
        };

        self.set_active(c, next_active)?;
        Ok(())
    }

    /// Focus the selected sidebar entry if the list exists.
    fn focus_sidebar_list(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            if let Some(selected) = list.selected_item() {
                ctx.set_focus(selected.into());
            } else {
                ctx.focus_first_in(ctx.node_id());
            }
            Ok(())
        })
    }

    /// Shift terminal selection and keep focus on the sidebar list.
    fn shift_terminal_in_sidebar(&mut self, c: &mut dyn Context, forward: bool) -> Result<()> {
        self.shift_terminal(c, forward)?;
        self.focus_sidebar_list(c)?;
        Ok(())
    }

    #[command]
    /// Create a new terminal instance.
    pub fn new_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        self.add_terminal(c)
    }

    #[command]
    /// Create a new terminal instance while keeping focus on the sidebar.
    pub fn new_terminal_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {
        self.add_terminal(c)?;
        self.focus_sidebar_list(c)?;
        Ok(())
    }

    #[command]
    /// Switch to the next terminal instance.
    pub fn next_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        self.shift_terminal(c, true)
    }

    #[command]
    /// Switch to the previous terminal instance.
    pub fn prev_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        self.shift_terminal(c, false)
    }

    #[command]
    /// Switch to the next terminal while keeping focus on the sidebar.
    pub fn next_terminal_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {
        self.shift_terminal_in_sidebar(c, true)
    }

    #[command]
    /// Switch to the previous terminal while keeping focus on the sidebar.
    pub fn prev_terminal_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {
        self.shift_terminal_in_sidebar(c, false)
    }

    #[command]
    /// Delete the active terminal and keep focus on the sidebar.
    pub fn delete_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.terminals.is_empty() {
            return Ok(());
        }

        let target = self.active.min(self.terminals.len() - 1);
        self.remove_terminal(c, target)?;
        self.focus_sidebar_list(c)?;
        Ok(())
    }

    #[command]
    /// Focus the terminal list sidebar.
    pub fn focus_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        self.focus_sidebar_list(c)?;
        Ok(())
    }

    #[command]
    /// Focus the active terminal instance.
    pub fn focus_active_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        if let Some(active_id) = self.terminals.get(self.active).copied() {
            c.set_focus(active_id);
        }
        Ok(())
    }

    /// Handle a mouse click within the sidebar list.
    fn handle_list_click(&mut self, c: &mut dyn Context, location: Point) -> bool {
        let Some(list_id) = self.list_id else {
            return false;
        };
        let Some(list_view) = c.node_view(list_id) else {
            return false;
        };
        let view = c.view();
        let screen_x = view.content.tl.x as i64 + location.x as i64;
        let screen_y = view.content.tl.y as i64 + location.y as i64;
        let left = list_view.content.tl.x as i64;
        let top = list_view.content.tl.y as i64;
        let right = left + list_view.content.w as i64;
        let bottom = top + list_view.content.h as i64;

        if screen_x < left || screen_x >= right || screen_y < top || screen_y >= bottom {
            return false;
        }

        let local_y = (screen_y - top).max(0) as u32;
        let content_y = local_y.saturating_add(list_view.tl.y);
        let index = (content_y / ENTRY_HEIGHT) as usize;
        if index < self.terminals.len() && self.set_active(c, index).is_err() {
            return false;
        }
        if self.focus_sidebar_list(c).is_err() {
            return false;
        }
        true
    }
}

impl Widget for TermGym {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("termgym");
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Mouse(m)
                if m.button == mouse::Button::Left && m.action == mouse::Action::Down =>
            {
                if self.handle_list_click(ctx, m.location) {
                    return EventOutcome::Handle;
                }
            }
            _ => {}
        }
        EventOutcome::Ignore
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c).ok()?;
        None
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }
}

impl Loader for TermGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<List<TermEntry>>();
    }
}

/// Install key bindings and styles for the terminal gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };

    cnpy.style.add(
        "termgym/entry/border",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/entry/fill",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/entry/text",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/entry/selected/border",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/entry/selected/fill",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/entry/selected/text",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/button/border",
        Some(solarized::BASE3),
        Some(solarized::BASE02),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/button/selected/border",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/button/fill",
        Some(solarized::BASE3),
        Some(solarized::BASE02),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/button/selected/fill",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/button/text",
        Some(solarized::BASE3),
        Some(solarized::BASE02),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/button/selected/text",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/frame",
        Some(solarized::BASE01),
        None,
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/frame/focused",
        Some(solarized::YELLOW),
        None,
        Some(AttrSet {
            bold: true,
            ..AttrSet::default()
        }),
    );
    cnpy.style.add(
        "termgym/frame/active",
        Some(solarized::ORANGE),
        None,
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/frame/title",
        Some(solarized::BASE3),
        None,
        Some(AttrSet {
            bold: true,
            ..AttrSet::default()
        }),
    );

    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("term_gym")
        .key(
            key::Ctrl + key::KeyCode::Char('a'),
            "term_gym::focus_sidebar()",
        )
        .key(key::Ctrl + key::KeyCode::F(2), "term_gym::new_terminal()")
        .key(key::Ctrl + key::KeyCode::F(3), "term_gym::next_terminal()")
        .key(key::Ctrl + key::KeyCode::F(4), "term_gym::prev_terminal()")
        .with_path("term_gym/*/list")
        .key('n', "term_gym::new_terminal_sidebar()")
        .key('j', "term_gym::next_terminal_sidebar()")
        .key('k', "term_gym::prev_terminal_sidebar()")
        .key(key::KeyCode::Enter, "term_gym::focus_active_terminal()")
        .key('d', "term_gym::delete_terminal()");
}
