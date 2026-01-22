//! Terminal widget demo with tabs.

use std::env;

use canopy::{
    Context, NodeId, ReadContext, TypedId, Widget, command, derive_commands,
    error::{Error, Result},
    layout::{Direction, Edges, Layout},
    render::Render,
    state::NodeName,
    style::{Color, Paint, StyleMap},
};
use canopy_widgets::{Button, Frame, ROUND, Terminal, TerminalConfig};

/// Tab labels shown in the demo.
const TAB_LABELS: [&str; 3] = ["claude", "codex", "gemini"];
/// Height of each tab widget.
const TAB_HEIGHT: u32 = 3;
/// Button style prefix for tab widgets.
const TAB_STYLE_PREFIX: &str = "term_demo/button";

/// Row container for tab widgets.
struct TabBar;

impl TabBar {
    /// Construct a tab bar.
    fn new() -> Self {
        Self
    }
}

impl Widget for TabBar {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Row)
            .gap(1)
            .fixed_height(TAB_HEIGHT)
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("term_tab_bar")
    }
}

/// Stack container for terminal widgets.
struct TerminalStack;

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

    fn name(&self) -> NodeName {
        NodeName::convert("term_stack")
    }
}

/// Terminal demo widget with three commands.
pub struct TermDemo {
    /// Current active index.
    active: usize,
    /// Tab button node ids.
    tab_ids: Vec<TypedId<Button>>,
    /// Terminal frame node ids.
    frame_ids: Vec<NodeId>,
    /// Terminal node ids.
    terminal_ids: Vec<NodeId>,
}

impl Default for TermDemo {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl TermDemo {
    /// Construct a terminal demo.
    pub fn new() -> Self {
        Self {
            active: 0,
            tab_ids: Vec::new(),
            frame_ids: Vec::new(),
            terminal_ids: Vec::new(),
        }
    }

    /// Activate a tab/terminal index.
    fn set_active(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {
        if self.terminal_ids.is_empty() || self.tab_ids.is_empty() || self.frame_ids.is_empty() {
            return Ok(());
        }

        let target = index.min(self.terminal_ids.len() - 1);
        self.active = target;

        for (idx, tab_id) in self.tab_ids.iter().enumerate() {
            let active = idx == self.active;
            ctx.with_typed(*tab_id, |tab: &mut Button, _ctx| {
                tab.set_active(active);
                Ok(())
            })?;
        }

        for (idx, frame_id) in self.frame_ids.iter().enumerate() {
            let active = idx == self.active;
            ctx.with_layout_of(*frame_id, &mut |layout| {
                *layout = if active {
                    Layout::fill().padding(Edges::all(1))
                } else {
                    Layout::fill().none()
                };
            })?;
        }

        if let Some(active_id) = self.terminal_ids.get(self.active).copied() {
            ctx.set_focus(active_id);
        }

        Ok(())
    }

    /// Cycle to the next tab.
    #[command]
    pub fn next_tab(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.terminal_ids.is_empty() {
            return Ok(());
        }
        let next = (self.active + 1) % self.terminal_ids.len();
        self.set_active(ctx, next)
    }
}

impl Widget for TermDemo {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let mut style = StyleMap::new();
        style
            .rules()
            .prefix(TAB_STYLE_PREFIX)
            .fg("active/border", Paint::solid(Color::rgb("#FF8C00")))
            .fg("inactive/border", Paint::solid(Color::rgb("#6B6B6B")))
            .fg("active/text", Paint::solid(Color::White))
            .fg("inactive/text", Paint::solid(Color::White))
            .apply();
        ctx.set_style(style);

        let tab_bar_id = ctx.add_child(TabBar::new())?;
        for label in TAB_LABELS {
            let tab_id = ctx.add_child_to(
                tab_bar_id,
                Button::new(label.to_string()).with_glyphs(ROUND),
            )?;
            ctx.set_layout_of(
                tab_id,
                Layout::fill().fixed_height(TAB_HEIGHT).flex_horizontal(1),
            )?;
            self.tab_ids.push(tab_id);
        }

        let stack_id = ctx.add_child(TerminalStack::new())?;

        let cwd = env::current_dir().map_err(|err| Error::Internal(err.to_string()))?;
        for label in TAB_LABELS {
            let frame_id = ctx.add_child_to(stack_id, Frame::new())?;
            ctx.set_layout_of(frame_id, Layout::fill().padding(Edges::all(1)))?;
            let terminal_id = ctx.add_child_to(
                frame_id,
                Terminal::new(TerminalConfig {
                    command: Some(vec![label.to_string()]),
                    cwd: Some(cwd.clone()),
                    ..TerminalConfig::default()
                }),
            )?;
            ctx.set_layout_of(terminal_id, Layout::fill())?;
            self.frame_ids.push(frame_id.into());
            self.terminal_ids.push(terminal_id.into());
        }

        self.set_active(ctx, 0)?;
        Ok(())
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        rndr.push_layer("term_demo");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("term_demo")
    }
}
