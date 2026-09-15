//! Responsive rows and scrolling behavior for contextual binding help.

use std::mem;

use canopy::{
    Canopy, Context, Loader, NodeName, Render, ViewContext, Widget,
    commands::CommandStatus,
    derive_commands,
    error::Result,
    event::key::{Empty, Key, KeyCode},
    geom::{Line, Size},
    help::{AvailableBinding, BindingSnapshot},
    layout::{CanvasContext, Layout, MeasureOverflow},
};
use unicode_width::UnicodeWidthStr;

/// Widest row of keys for one action before further keys continue below.
const KEY_ROW_WIDTH: usize = 20;

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
///
/// The list uses its full width. An enclosing frame draws its scroll position.
pub struct BindingList {
    /// Captured application context, absent while help is closed.
    snapshot: Option<BindingSnapshot>,
}

#[derive_commands]
impl BindingList {
    /// Construct an empty list.
    pub(crate) const fn new() -> Self {
        Self { snapshot: None }
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
        let bindings = self
            .snapshot
            .as_ref()
            .map_or(&[][..], |snapshot| snapshot.bindings.as_slice());
        display_lines(bindings, width)
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
        Layout::fill().overflow_y(MeasureOverflow::Unbounded)
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        let lines = self.display_lines(view.w);
        Size::new(view.w, u32::try_from(lines.len()).unwrap_or(u32::MAX))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let view = context.view();
        let rect = view.outer_rect_local();
        render.fill("help/panel", rect, ' ')?;
        let width = view.content.w;
        let lines = self.display_lines(width);
        let viewport = view.view_rect();
        for (index, line) in lines
            .iter()
            .enumerate()
            .skip(viewport.tl.y as usize)
            .take(viewport.h as usize)
        {
            let y = u32::try_from(index).unwrap_or(u32::MAX) - viewport.tl.y;
            render_line(render, line, 0, y, width)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("binding_list")
    }
}

/// Bindings that share one action, shown together.
struct BindingGroup {
    /// Distinct key labels, in display order.
    keys: Vec<String>,
    /// Action text shared by every key.
    description: String,
}

/// Build the sorted display lines for `bindings` at `width`.
pub(super) fn display_lines(bindings: &[AvailableBinding], width: u32) -> Vec<DisplayLine> {
    if bindings.is_empty() {
        return vec![DisplayLine {
            key: None,
            text: "No key bindings in this context".to_string(),
            style: "help/label",
        }];
    }

    let groups = binding_groups(bindings)
        .into_iter()
        .map(|group| (key_rows(&group.keys), group.description))
        .collect::<Vec<_>>();
    let max_key_width = groups
        .iter()
        .flat_map(|(rows, _)| rows)
        .map(|row| text_width(row))
        .max()
        .unwrap_or(0);
    binding_lines(&groups, width, max_key_width)
}

/// Return the width that shows every action on one row without wrapping.
pub(super) fn natural_width(bindings: &[AvailableBinding]) -> usize {
    let groups = binding_groups(bindings);
    let keys = groups
        .iter()
        .flat_map(|group| key_rows(&group.keys))
        .map(|row| text_width(&row))
        .max()
        .unwrap_or(0);
    let description = groups
        .iter()
        .map(|group| text_width(&group.description))
        .max()
        .unwrap_or(0);
    keys + 2 + description
}

/// Render one display line from column `x` of row `y`, within `width` cells.
pub(super) fn render_line(
    render: &mut Render,
    line: &DisplayLine,
    x: u32,
    y: u32,
    width: u32,
) -> Result<()> {
    let Some(key) = &line.key else {
        return render.text(line.style, Line::new(x, y, width), &line.text);
    };
    let key_width = text_width(key) as u32;
    render.text("help/key", Line::new(x, y, key_width.min(width)), key)?;
    let start = key_width.saturating_add(2).min(width);
    render.text(
        line.style,
        Line::new(x + start, y, width.saturating_sub(start)),
        &line.text,
    )
}

/// Merge bindings that show the same action into groups, ordered by their
/// first key.
fn binding_groups(bindings: &[AvailableBinding]) -> Vec<BindingGroup> {
    let mut sorted = bindings.iter().collect::<Vec<_>>();
    sorted.sort_by_cached_key(|binding| binding_sort_key(binding));
    let mut groups: Vec<BindingGroup> = Vec::new();
    for binding in sorted {
        let key = key_label(binding.key);
        let description = binding_description(binding);
        match groups
            .iter_mut()
            .find(|group| group.description == description)
        {
            Some(group) if !group.keys.contains(&key) => group.keys.push(key),
            Some(_) => {}
            None => groups.push(BindingGroup {
                keys: vec![key],
                description,
            }),
        }
    }
    groups
}

/// Pack key labels into rows no wider than [`KEY_ROW_WIDTH`], with at least one
/// label on each row.
///
/// A space separates keys. No key label contains one, so the split is never
/// ambiguous.
fn key_rows(keys: &[String]) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for key in keys {
        match rows.last_mut() {
            Some(row) if text_width(row) + 1 + text_width(key) <= KEY_ROW_WIDTH => {
                row.push(' ');
                row.push_str(key);
            }
            _ => rows.push(key.clone()),
        }
    }
    rows
}

/// Return the label help shows for `key`, with arrow keys drawn as arrows.
///
/// Arrows are ambiguous-width characters, and many terminal fonts draw them
/// wider than their one cell. Each arrow keeps a blank cell after it for the
/// glyph to spill into, so it never covers the next key.
fn key_label(key: Key) -> String {
    let code = match key.key {
        KeyCode::Left => "← ".to_string(),
        KeyCode::Right => "→ ".to_string(),
        KeyCode::Up => "↑ ".to_string(),
        KeyCode::Down => "↓ ".to_string(),
        code => code.to_string(),
    };
    if key.mods == Empty {
        code
    } else {
        format!("{}+{code}", key.mods)
    }
}

/// Return the terminal-cell width of text.
fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Build aligned shortcut rows, stacking keys above actions on narrow screens.
///
/// A group's extra key rows pair with its wrapped action lines, so a long run
/// of keys continues in the key column below its action.
fn binding_lines(
    groups: &[(Vec<String>, String)],
    width: u32,
    max_key_width: usize,
) -> Vec<DisplayLine> {
    let width = width as usize;
    let narrow = width < max_key_width.saturating_add(12) || width < 28;
    let mut lines = Vec::new();
    for (keys, description) in groups {
        if narrow {
            for row in keys {
                lines.extend(textwrap::wrap(row, width.max(1)).into_iter().map(|text| {
                    DisplayLine {
                        key: None,
                        text: text.into_owned(),
                        style: "help/key",
                    }
                }));
            }
            let wrap_width = width.saturating_sub(2).max(1);
            for text in textwrap::wrap(description, wrap_width) {
                lines.push(DisplayLine {
                    key: None,
                    text: format!("  {text}"),
                    style: "help/label",
                });
            }
        } else {
            let wrap_width = width.saturating_sub(max_key_width + 2).max(1);
            let wrapped = textwrap::wrap(description, wrap_width);
            for index in 0..keys.len().max(wrapped.len()) {
                let text = wrapped
                    .get(index)
                    .map_or_else(String::new, ToString::to_string);
                lines.push(match keys.get(index) {
                    Some(key) => DisplayLine {
                        key: Some(format!(
                            "{}{key}",
                            " ".repeat(max_key_width.saturating_sub(text_width(key)))
                        )),
                        text,
                        style: "help/label",
                    },
                    None => DisplayLine {
                        key: None,
                        text: format!("{}  {text}", " ".repeat(max_key_width)),
                        style: "help/label",
                    },
                });
            }
        }
    }
    lines
}

/// Show the action and useful availability feedback, leaving diagnostics to
/// inspection APIs.
pub(super) fn binding_description(binding: &AvailableBinding) -> String {
    let Some(command) = &binding.command else {
        return binding.description.clone();
    };
    let mut description = binding.description.clone();
    match &command.status {
        Some(CommandStatus::Disabled(reason)) => {
            description.push_str(&format!(" — Unavailable: {reason}"));
        }
        None => description.push_str(" — Unavailable here"),
        Some(CommandStatus::Enabled) => {}
    }
    description
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
