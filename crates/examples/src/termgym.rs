use std::env;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ReadContext, Widget, command, derive_commands,
    error::{Error, Result},
    event::key,
    layout::{Constraint, Direction, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{Attr, AttrSet, solarized},
};
use canopy_widgets::{
    Box, Button, Center, Frame, List, ROUND_THICK, Root, SINGLE, Selectable, Terminal,
    TerminalConfig, Text, VStack,
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
}

#[derive_commands]
impl TermEntry {
    /// Construct a new terminal entry.
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            selected: false,
        }
    }

    /// Compute the display width of the label text.
    fn label_width(&self) -> u32 {
        UnicodeWidthStr::width(self.label.as_str()).max(1) as u32
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
        let box_id = ctx.add_child(Box::new().with_glyphs(SINGLE).with_fill())?;
        let center_id = ctx.add_child_to(box_id, Center::new())?;
        ctx.add_child_to(
            center_id,
            Text::new(self.label.clone()).with_wrap_width(self.label_width()),
        )?;
        Ok(())
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
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

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
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
        Layout::fill().direction(Direction::Stack)
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

/// Multi-terminal demo widget.
pub struct TermGym {
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
        Self { active: 0 }
    }

    /// Execute a closure with the terminal list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, mut f: F) -> Result<R>
    where
        F: FnMut(&mut List<TermEntry>, &mut dyn Context) -> Result<R>,
    {
        c.with_unique_descendant::<List<TermEntry>, _>(|list, ctx| f(list, ctx))
    }

    /// Execute a closure with the terminal stack widget.
    fn with_stack<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut TerminalStack, &mut dyn Context) -> Result<R>,
    {
        c.with_unique_descendant::<TerminalStack, _>(f)
    }

    /// Return terminal stack children in order.
    fn terminal_ids(&self, c: &mut dyn Context) -> Result<Vec<NodeId>> {
        self.with_stack(c, |_stack, ctx| {
            Ok(ctx
                .children_of_type::<Terminal>()
                .into_iter()
                .map(|id| id.into())
                .collect())
        })
    }

    /// Create and mount a new terminal instance.
    fn add_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        let cwd = env::current_dir().map_err(|err| Error::Internal(err.to_string()))?;
        self.with_stack(c, |_, ctx| {
            let terminal_id = ctx.add_child(Terminal::new(TerminalConfig {
                cwd: Some(cwd),
                ..TerminalConfig::default()
            }))?;
            ctx.with_layout_of(terminal_id, &mut |layout| {
                *layout = Layout::fill();
            })?;
            Ok(())
        })?;

        let index = self.terminal_ids(c)?.len();
        self.with_list(c, |list, ctx| {
            let entry = TermEntry::new(index.to_string());
            list.append(ctx, entry)?;
            Ok(())
        })?;

        self.set_active(c, index.saturating_sub(1))?;
        Ok(())
    }

    /// Activate a specific terminal by index.
    fn set_active(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        let terminals = self.terminal_ids(c)?;
        if terminals.is_empty() {
            return Ok(());
        }
        let target = index.min(terminals.len() - 1);
        self.active = target;

        for (idx, terminal_id) in terminals.iter().enumerate() {
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

        if let Some(active_id) = terminals.get(self.active).copied() {
            c.set_focus(active_id);
        }

        Ok(())
    }

    /// Move the active terminal selection forward or backward.
    fn shift_terminal(&mut self, c: &mut dyn Context, forward: bool) -> Result<()> {
        let terminals = self.terminal_ids(c)?;
        if terminals.is_empty() {
            return Ok(());
        }
        let next = if forward {
            (self.active + 1) % terminals.len()
        } else {
            (self.active + terminals.len() - 1) % terminals.len()
        };
        self.set_active(c, next)
    }

    /// Rebuild the sidebar list to match the current terminal set.
    fn rebuild_list(&self, c: &mut dyn Context) -> Result<()> {
        let count = self.terminal_ids(c)?.len();
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
        let terminals = self.terminal_ids(c)?;
        if index >= terminals.len() {
            return Ok(());
        }

        let removed_id = terminals[index];
        c.remove_subtree(removed_id)?;

        self.rebuild_list(c)?;

        let remaining = self.terminal_ids(c)?;
        if remaining.is_empty() {
            self.active = 0;
            return Ok(());
        }

        let next_active = if index < self.active {
            self.active.saturating_sub(1)
        } else {
            self.active.min(remaining.len() - 1)
        };

        self.set_active(c, next_active)?;
        Ok(())
    }

    /// Focus the selected sidebar entry if the list exists.
    fn focus_sidebar_list(&self, c: &mut dyn Context) -> Result<()> {
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
        if self.terminal_ids(c)?.is_empty() {
            return Ok(());
        }

        let terminals = self.terminal_ids(c)?;
        let target = self.active.min(terminals.len() - 1);
        self.remove_terminal(c, target)?;
        self.focus_sidebar_list(c)?;
        Ok(())
    }

    #[command]
    /// Focus the terminal list sidebar.
    pub fn focus_sidebar(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus_sidebar_list(c)?;
        Ok(())
    }

    #[command]
    /// Focus the active terminal instance.
    pub fn focus_active_terminal(&mut self, c: &mut dyn Context) -> Result<()> {
        let terminals = self.terminal_ids(c)?;
        if let Some(active_id) = terminals.get(self.active).copied() {
            c.set_focus(active_id);
        }
        Ok(())
    }

    #[command]
    /// Activate a terminal from a sidebar row selection.
    pub fn activate_terminal(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        self.set_active(c, index)?;
        self.focus_sidebar_list(c)?;
        Ok(())
    }
}

impl Widget for TermGym {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let list_id = c.create_detached(
            List::<TermEntry>::new().with_on_activate(Self::cmd_activate_terminal().call()),
        );
        let button_id = c.create_detached(
            Button::new("+ New terminal").with_command(Self::cmd_new_terminal().call()),
        );
        let sidebar_id = c.add_child(
            VStack::new()
                .push_fixed(button_id, ENTRY_HEIGHT)
                .push_flex(list_id, 1),
        )?;

        let stack_id = c.create_detached(TerminalStack::new());
        let term_frame_id =
            c.add_child(Frame::new().with_glyphs(ROUND_THICK).with_title("terminal"))?;
        c.attach(term_frame_id, stack_id)?;

        c.set_layout(Layout::fill().direction(Direction::Row))?;
        c.set_layout_of(
            sidebar_id,
            Layout::column().fixed_width(24).flex_vertical(1),
        )?;

        self.add_terminal(c)?;

        Ok(())
    }

    fn render(&mut self, r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        r.push_layer("termgym");
        Ok(())
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
    use canopy::style::StyleBuilder;

    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };

    let entry_normal = StyleBuilder::new()
        .fg(solarized::BASE0)
        .bg(solarized::BASE03);

    let entry_selected = StyleBuilder::new()
        .fg(solarized::BASE3)
        .bg(solarized::BLUE)
        .attrs(selected_attrs);

    let button_normal = StyleBuilder::new()
        .fg(solarized::BASE3)
        .bg(solarized::BASE02);

    let button_selected = StyleBuilder::new()
        .fg(solarized::BASE3)
        .bg(solarized::BLUE)
        .attrs(selected_attrs);

    cnpy.style
        .rules()
        .prefix("termgym/entry")
        .style_all(&["border", "fill", "text"], entry_normal)
        .style_all(
            &["selected/border", "selected/fill", "selected/text"],
            entry_selected,
        )
        .prefix("termgym/button")
        .style_all(&["border", "fill", "text"], button_normal)
        .style_all(
            &["selected/border", "selected/fill", "selected/text"],
            button_selected,
        )
        .prefix("termgym/frame")
        .fg("", solarized::BASE01)
        .style(
            "focused",
            StyleBuilder::new().fg(solarized::YELLOW).attr(Attr::Bold),
        )
        .fg("active", solarized::ORANGE)
        .style(
            "title",
            StyleBuilder::new().fg(solarized::BASE3).attr(Attr::Bold),
        )
        .apply();

    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("term_gym/*/terminal/")
        .key(
            key::Ctrl + key::KeyCode::Char('a'),
            "term_gym::focus_sidebar()",
        )
        .with_path("term_gym")
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
