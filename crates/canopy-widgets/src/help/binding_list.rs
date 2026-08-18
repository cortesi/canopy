//! Responsive rows and scrolling behavior for contextual binding help.

use std::mem;

use canopy::{
    BindingPhase, Canopy, Context, EventOutcome, Loader, ViewContext, Widget, command,
    derive_commands,
    error::Result,
    event::{
        Event,
        key::{Empty, KeyCode},
        mouse,
    },
    geom::{Line, Rect},
    help::{AvailableBinding, BindingSnapshot},
    layout::{CanvasContext, Layout, Size},
    render::Render,
    state::NodeName,
};
use unicode_width::UnicodeWidthStr;

/// One prepared display line.
pub(super) struct DisplayLine {
    /// Optional aligned key column.
    pub(super) key: Option<String>,
    /// Visible row text.
    pub(super) text: String,
    /// Style path for the visible row text.
    pub(super) style: &'static str,
}

/// Scrollable list of effective key bindings.
pub struct BindingList {
    /// Captured application context, absent while help is closed.
    snapshot: Option<BindingSnapshot>,
}

impl Default for BindingList {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl BindingList {
    /// Construct an empty list.
    pub const fn new() -> Self {
        Self { snapshot: None }
    }

    /// Install a captured snapshot.
    pub fn set_snapshot(&mut self, snapshot: BindingSnapshot) {
        self.snapshot = Some(snapshot);
    }

    /// Replace the captured snapshot and return the prior value.
    pub(crate) fn replace_snapshot(
        &mut self,
        snapshot: Option<BindingSnapshot>,
    ) -> Option<BindingSnapshot> {
        mem::replace(&mut self.snapshot, snapshot)
    }

    /// Return the installed snapshot.
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Option<&BindingSnapshot> {
        self.snapshot.as_ref()
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&self, context: &mut dyn Context) {
        context.scroll_up();
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&self, context: &mut dyn Context) {
        context.scroll_down();
    }

    #[command]
    /// Scroll up by one viewport.
    pub fn page_up(&self, context: &mut dyn Context) {
        context.page_up();
    }

    #[command]
    /// Scroll down by one viewport.
    pub fn page_down(&self, context: &mut dyn Context) {
        context.page_down();
    }

    #[command]
    /// Scroll to the first row.
    pub fn scroll_to_top(&self, context: &mut dyn Context) {
        context.scroll_to(0, 0);
    }

    #[command]
    /// Scroll to the last row.
    pub fn scroll_to_bottom(&self, context: &mut dyn Context) {
        let view = context.view();
        context.scroll_to(0, view.canvas.h.saturating_sub(view.view_rect().h));
    }

    /// Build the exact vertical canvas for one viewport width.
    pub(super) fn display_lines(&self, width: u32) -> Vec<DisplayLine> {
        let Some(snapshot) = &self.snapshot else {
            return vec![DisplayLine {
                key: None,
                text: "No key bindings in this context".to_string(),
                style: "help/label",
            }];
        };
        let mut primary = snapshot
            .bindings
            .iter()
            .filter(|binding| binding.phase == BindingPhase::BeforeWidget)
            .collect::<Vec<_>>();
        let mut fallback = snapshot
            .bindings
            .iter()
            .filter(|binding| binding.phase == BindingPhase::AfterIgnore)
            .collect::<Vec<_>>();
        primary.sort_by_key(|left| binding_sort_key(left));
        fallback.sort_by_key(|left| binding_sort_key(left));

        if primary.is_empty() && fallback.is_empty() {
            return vec![DisplayLine {
                key: None,
                text: "No key bindings in this context".to_string(),
                style: "help/label",
            }];
        }

        let max_key_width = primary
            .iter()
            .chain(&fallback)
            .map(|binding| UnicodeWidthStr::width(binding.key.to_string().as_str()))
            .max()
            .unwrap_or(0);
        let mut lines = binding_lines(&primary, width, max_key_width, "help/label");
        if !fallback.is_empty() {
            if !lines.is_empty() {
                lines.push(DisplayLine {
                    key: None,
                    text: String::new(),
                    style: "help/fallback",
                });
            }
            lines.extend(
                textwrap::wrap(
                    "When the focused widget does not handle the key",
                    (width as usize).max(1),
                )
                .into_iter()
                .map(|text| DisplayLine {
                    key: None,
                    text: text.to_string(),
                    style: "help/fallback",
                }),
            );
            lines.extend(binding_lines(
                &fallback,
                width,
                max_key_width,
                "help/fallback",
            ));
        }
        lines
    }
}

impl Loader for BindingList {
    fn load(canopy: &mut Canopy) -> Result<()> {
        canopy.add_commands::<Self>()
    }
}

impl Widget for BindingList {
    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        Layout::fill().overflow_y()
    }

    fn canvas(&self, view: Size<u32>, _context: &CanvasContext) -> Size<u32> {
        let lines = self.display_lines(view.w);
        Size::new(view.w, u32::try_from(lines.len()).unwrap_or(u32::MAX))
    }

    fn on_event(&mut self, event: &Event, context: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(mouse) = event else {
            return Ok(EventOutcome::Ignore);
        };
        match mouse.action {
            mouse::Action::ScrollUp => {
                context.scroll_up();
                Ok(EventOutcome::Consume)
            }
            mouse::Action::ScrollDown => {
                context.scroll_down();
                Ok(EventOutcome::Consume)
            }
            mouse::Action::Down if mouse.button == mouse::Button::Left => {
                let view = context.view();
                if view.content.w > 0 && mouse.location.x + 1 >= view.content.w {
                    let viewport = view.view_rect().h;
                    let maximum = view.canvas.h.saturating_sub(viewport);
                    let denominator = view.content.h.saturating_sub(1).max(1);
                    let target =
                        maximum.saturating_mul(mouse.location.y.min(denominator)) / denominator;
                    context.scroll_to(0, target);
                    return Ok(EventOutcome::Consume);
                }
                Ok(EventOutcome::Ignore)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let view = context.view();
        let rect = view.outer_rect_local();
        render.fill("help/panel", rect, ' ')?;
        let lines = self.display_lines(view.content.w);
        let viewport = view.view_rect();
        for (index, line) in lines
            .iter()
            .enumerate()
            .skip(viewport.tl.y as usize)
            .take(viewport.h as usize)
        {
            let y = u32::try_from(index).unwrap_or(u32::MAX) - viewport.tl.y;
            let width = view.content.w;
            if let Some(key) = &line.key {
                let key_width = UnicodeWidthStr::width(key.as_str()) as u32;
                render.text("help/key", Line::new(0, y, key_width.min(width)), key)?;
                let start = key_width.saturating_add(2).min(width);
                render.text(
                    line.style,
                    Line::new(start, y, width.saturating_sub(start)),
                    &line.text,
                )?;
            } else {
                render.text(line.style, Line::new(0, y, width), &line.text)?;
            }
        }

        if view.content.w > 0 && view.content.h > 0 && view.canvas.h > viewport.h {
            let maximum = view.canvas.h.saturating_sub(viewport.h).max(1);
            let indicator_y = viewport
                .tl
                .y
                .saturating_mul(view.content.h.saturating_sub(1))
                / maximum;
            render.fill(
                "help/indicator",
                Rect::new(view.content.w - 1, indicator_y, 1, 1),
                '█',
            )?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("binding_list")
    }
}

/// Build wrapped display lines for one phase section.
fn binding_lines(
    bindings: &[&AvailableBinding],
    width: u32,
    max_key_width: usize,
    style: &'static str,
) -> Vec<DisplayLine> {
    let width = width as usize;
    let narrow = width < max_key_width.saturating_add(12) || width < 28;
    let mut lines = Vec::new();
    for binding in bindings {
        let key = binding.key.to_string();
        if narrow {
            lines.push(DisplayLine {
                key: None,
                text: key,
                style: "help/key",
            });
            let wrap_width = width.saturating_sub(2).max(1);
            for text in textwrap::wrap(&binding.description, wrap_width) {
                lines.push(DisplayLine {
                    key: None,
                    text: format!("  {text}"),
                    style,
                });
            }
        } else {
            let wrap_width = width.saturating_sub(max_key_width + 2).max(1);
            let mut wrapped = textwrap::wrap(&binding.description, wrap_width).into_iter();
            lines.push(DisplayLine {
                key: Some(format!("{key:>max_key_width$}")),
                text: wrapped
                    .next()
                    .map_or_else(String::new, |text| text.to_string()),
                style,
            });
            for text in wrapped {
                lines.push(DisplayLine {
                    key: None,
                    text: format!("{}  {text}", " ".repeat(max_key_width)),
                    style,
                });
            }
        }
    }
    lines
}

/// Sort bindings by requested key category and display string.
fn binding_sort_key(binding: &AvailableBinding) -> (u8, String) {
    let key = binding.key;
    let group = if key.mods != Empty {
        5
    } else {
        match key.key {
            KeyCode::Char(character) if character.is_ascii_lowercase() => 0,
            KeyCode::Char(character) if character.is_ascii_uppercase() => 1,
            KeyCode::Char(character) if character.is_ascii_digit() => 2,
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => 3,
            _ => 4,
        }
    };
    (group, key.to_string())
}
