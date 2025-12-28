use std::{env, time::Duration};

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, cursor, derive_commands,
    error::{Error, Result},
    event::{Event, key, mouse},
    geom::{Expanse, Line, Point, Rect},
    layout::Layout,
    render::Render,
    style::{AttrSet, solarized},
    widget::{EventOutcome, Widget},
    widgets::{
        Root, Terminal, TerminalConfig, frame,
        list::{List, ListItem},
    },
};

/// Height for each terminal entry row, including borders.
const ENTRY_HEIGHT: u32 = 3;
/// Minimum width for a boxed entry.
const ENTRY_MIN_WIDTH: usize = 3;

/// Build box-drawing lines for a centered label.
fn box_lines(width: usize, label: &str) -> (String, String, String) {
    let full_width = width.max(ENTRY_MIN_WIDTH);
    let inner_width = full_width.saturating_sub(2).max(1);
    let top = format!("┌{}┐", "─".repeat(inner_width));
    let middle = format!("│{:^inner_width$}│", label, inner_width = inner_width);
    let bottom = format!("└{}┘", "─".repeat(inner_width));
    (top, middle, bottom)
}

/// List item representing a terminal instance.
struct TermItem {
    /// One-based terminal index.
    index: usize,
}

impl TermItem {
    /// Construct a list item for a terminal.
    fn new(index: usize) -> Self {
        Self { index }
    }

    /// Render the label for this terminal.
    fn label(&self) -> String {
        self.index.to_string()
    }
}

impl ListItem for TermItem {
    fn measure(&self, available_width: u32) -> Expanse {
        Expanse::new(available_width.max(ENTRY_MIN_WIDTH as u32), ENTRY_HEIGHT)
    }

    fn render(
        &mut self,
        rndr: &mut Render,
        area: Rect,
        selected: bool,
        offset: Point,
        full_size: Expanse,
    ) -> Result<()> {
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }

        let style = if selected {
            "termgym/list_selected"
        } else {
            "termgym/list"
        };

        rndr.fill(style, area, ' ')?;

        let label = self.label();
        let (top, middle, bottom) = box_lines(full_size.w as usize, &label);

        for row in 0..area.h {
            let local_y = offset.y.saturating_add(row);
            let line = match local_y {
                0 => &top,
                1 => &middle,
                2 => &bottom,
                _ => continue,
            };
            let skip = offset.x as usize;
            let visible = area.w as usize;
            let text: String = line.chars().skip(skip).take(visible).collect();
            rndr.text(style, area.line(row), &text)?;
        }

        Ok(())
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

/// Clickable button used to create new terminals.
struct ActionButton {
    /// Button label.
    label: String,
    /// TermGym node ID to invoke.
    target: NodeId,
}

#[derive_commands]
impl ActionButton {
    /// Construct a new action button.
    fn new(label: impl Into<String>, target: NodeId) -> Self {
        Self {
            label: label.into(),
            target,
        }
    }
}

impl Widget for ActionButton {
    fn layout(&self) -> Layout {
        Layout::fill().fixed_height(ENTRY_HEIGHT)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Mouse(m)
                if m.button == mouse::Button::Left && m.action == mouse::Action::Down =>
            {
                let target = self.target;
                if ctx
                    .with_widget(target, |gym: &mut TermGym, ctx| gym.add_terminal(ctx))
                    .is_err()
                {
                    return EventOutcome::Handle;
                }
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();
        if rect.w == 0 || rect.h == 0 {
            return Ok(());
        }

        let style = "termgym/button";
        rndr.fill(style, rect, ' ')?;

        let label = format!("+ {}", self.label);
        let (top, middle, bottom) = box_lines(rect.w as usize, &label);
        let lines = [top, middle, bottom];
        for (row, line_text) in lines.into_iter().enumerate() {
            if row as u32 >= rect.h {
                break;
            }
            let line = Line::new(rect.tl.x, rect.tl.y + row as u32, rect.w);
            rndr.text(style, line, &line_text)?;
        }

        Ok(())
    }
}

/// Multi-terminal demo widget.
pub struct TermGym {
    /// Node ID for the terminal list widget.
    list_id: Option<NodeId>,
    /// Node ID for the add-terminal button.
    button_id: Option<NodeId>,
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
            button_id: None,
            stack_id: None,
            terminals: Vec::new(),
            active: 0,
        }
    }

    /// Ensure the widget tree is mounted.
    fn ensure_tree(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.list_id.is_some() && self.button_id.is_some() && self.stack_id.is_some() {
            return Ok(());
        }

        let termgym_id = c.node_id();
        let list_id = c.add_orphan(List::new(Vec::<TermItem>::new()));
        let button_id = c.add_orphan(ActionButton::new("New terminal", termgym_id));
        let sidebar_id = c.add_orphan(Sidebar::new());
        c.mount_child_to(sidebar_id, button_id)?;
        c.mount_child_to(sidebar_id, list_id)?;

        let stack_id = c.add_orphan(TerminalStack::new());
        let term_frame_id = c.add_orphan(frame::Frame::new().with_title("terminal"));
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
        c.with_layout_of(term_frame_id, &mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        c.with_layout_of(list_id, &mut |layout| {
            *layout = Layout::fill().flex_vertical(1);
        })?;
        c.with_layout_of(stack_id, &mut |layout| {
            *layout = Layout::stack().flex_horizontal(1).flex_vertical(1);
        })?;

        self.list_id = Some(list_id);
        self.button_id = Some(button_id);
        self.stack_id = Some(stack_id);
        self.add_terminal(c)?;

        Ok(())
    }

    /// Execute a closure with the terminal list widget.
    fn with_list<F>(&mut self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut List<TermItem>) -> Result<()>,
    {
        self.ensure_tree(c)?;
        let list_id = self
            .list_id
            .ok_or_else(|| Error::Internal("list not initialized".into()))?;
        c.with_widget(list_id, |list: &mut List<TermItem>, _| f(list))
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
        self.with_list(c, |list| {
            list.append(TermItem::new(index));
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
        self.with_list(c, |list| {
            list.select(active);
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
        self.with_list(c, |list| {
            list.clear();
            for index in 1..=count {
                list.append(TermItem::new(index));
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

    /// Ensure focus returns to the sidebar list if it exists.
    fn focus_sidebar_list(&self, c: &mut dyn Context) {
        if let Some(list_id) = self.list_id {
            c.set_focus(list_id);
        }
    }

    /// Shift terminal selection and keep focus on the sidebar list.
    fn shift_terminal_in_sidebar(&mut self, c: &mut dyn Context, forward: bool) -> Result<()> {
        self.shift_terminal(c, forward)?;
        self.focus_sidebar_list(c);
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
        self.focus_sidebar_list(c);
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
        self.focus_sidebar_list(c);
        Ok(())
    }

    #[command]
    /// Focus the terminal list sidebar.
    pub fn focus_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        if let Some(list_id) = self.list_id {
            c.set_focus(list_id);
        }
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
        c.set_focus(list_id);
        true
    }
}

impl Widget for TermGym {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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
    }
}

/// Install key bindings and styles for the terminal gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };

    cnpy.style.add(
        "termgym/list",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "termgym/list_selected",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "termgym/button",
        Some(solarized::BASE3),
        Some(solarized::BASE02),
        Some(AttrSet::default()),
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
