//! Contextual help modal widget.
//!
//! Displays bindings and commands available from the current focus context.

use std::cell::RefCell;

// Re-export help types for convenience
pub use canopy::help::{BindingKind, OwnedHelpBinding, OwnedHelpCommand, OwnedHelpSnapshot};
use canopy::{
    Binder, Canopy, Context, Core, DefaultBindings, EventOutcome, Loader, NodeId, ReadContext,
    Widget, command, derive_commands,
    error::Result,
    event::{Event, key::*},
    geom::Line,
    inputmap::InputSpec,
    layout::{CanvasContext, Edges, Layout, Size},
    render::Render,
    state::NodeName,
};
use unicode_width::UnicodeWidthStr;

use crate::{frame, modal::Modal};

/// Help modal widget displaying contextual bindings and commands.
pub struct Help {
    /// Captured help snapshot for display.
    snapshot: Option<OwnedHelpSnapshot>,
}

#[derive_commands]
impl Help {
    /// Create a new Help widget.
    pub fn new() -> Self {
        Self { snapshot: None }
    }

    /// Set the help snapshot to display.
    pub fn set_snapshot(&mut self, snapshot: OwnedHelpSnapshot) {
        self.snapshot = Some(snapshot);
    }

    /// Clear the stored snapshot.
    pub fn clear_snapshot(&mut self) {
        self.snapshot = None;
    }

    /// Get the current snapshot, if any.
    pub fn snapshot(&self) -> Option<&OwnedHelpSnapshot> {
        self.snapshot.as_ref()
    }

    /// Set the snapshot on the HelpContent child widget.
    pub fn set_content_snapshot(c: &mut dyn Context, snapshot: OwnedHelpSnapshot) -> Result<()> {
        c.with_first_descendant::<HelpContent, _>(|content, _ctx| {
            content.set_snapshot(snapshot);
            Ok(())
        })
    }

    /// Build the help subtree and return its node id.
    pub fn install(core: &mut Core) -> Result<NodeId> {
        // Create content widget - uses its own layout() with overflow and padding
        let content_id = core.create_detached(HelpContent::new());

        // Wrap content in Frame for visual boundary
        let frame_id = core.create_detached(frame::Frame::new().with_title("Help"));
        core.set_children(frame_id, vec![content_id])?;
        // Frame has fixed size - this is what gets centered
        core.with_layout_of(frame_id, |layout| {
            layout.min_width = Some(50);
            layout.max_width = Some(50);
            layout.min_height = Some(20);
            layout.max_height = Some(20);
        })?;

        // Wrap frame in Modal for centering
        let modal_id = core.create_detached(Modal::new());
        core.set_children(modal_id, vec![frame_id])?;
        // Modal uses its own layout (Stack with Center alignment), don't override

        // Create the Help widget as the root of this subtree
        let help_id = core.create_detached(Self::new());
        core.set_children(help_id, vec![modal_id])?;
        core.set_layout_of(help_id, Layout::fill())?;

        Ok(help_id)
    }
}

impl Default for Help {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Help {
    fn render(&mut self, r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        r.push_layer("help");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help")
    }
}

impl DefaultBindings for Help {
    fn defaults(b: Binder) -> Binder {
        b.with_path("help/")
            .key(KeyCode::Esc, "root::hide_help()")
            .key('q', "root::hide_help()")
            // Scroll bindings
            .key('j', "help_content::scroll_down()")
            .key('k', "help_content::scroll_up()")
            .key(KeyCode::Down, "help_content::scroll_down()")
            .key(KeyCode::Up, "help_content::scroll_up()")
            .key('g', "help_content::scroll_to_top()")
            .key('G', "help_content::scroll_to_bottom()")
            .key(' ', "help_content::page_down()")
    }
}

impl Loader for Help {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<HelpContent>()?;
        Ok(())
    }
}

/// Cached layout for help content.
struct HelpLayout {
    /// Maximum key width in characters.
    max_key_width: usize,
    /// Layout entries: (key_string, wrapped_label_lines).
    entries: Vec<(String, Vec<String>)>,
    /// Total number of display lines.
    total_lines: usize,
    /// Wrap width used for this layout.
    wrap_width: usize,
}

/// Content widget for the help modal that displays bindings and commands.
pub struct HelpContent {
    /// Captured help snapshot for display.
    snapshot: Option<OwnedHelpSnapshot>,
    /// Cached layout for the current snapshot.
    layout_cache: RefCell<Option<HelpLayout>>,
}

#[derive_commands]
impl HelpContent {
    /// Create a new help content widget.
    pub fn new() -> Self {
        Self {
            snapshot: None,
            layout_cache: RefCell::new(None),
        }
    }

    /// Set the help snapshot to display.
    pub fn set_snapshot(&mut self, snapshot: OwnedHelpSnapshot) {
        self.snapshot = Some(snapshot);
        self.layout_cache.borrow_mut().take();
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&self, c: &mut dyn Context) {
        c.scroll_up();
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&self, c: &mut dyn Context) {
        c.scroll_down();
    }

    #[command]
    /// Scroll to the top.
    pub fn scroll_to_top(&self, c: &mut dyn Context) {
        c.scroll_to(0, 0);
    }

    #[command]
    /// Scroll to the bottom.
    pub fn scroll_to_bottom(&self, c: &mut dyn Context) {
        let view = c.view();
        let canvas_h = view.canvas.h;
        let view_h = view.view_rect().h;
        if canvas_h > view_h {
            c.scroll_to(0, canvas_h - view_h);
        }
    }

    #[command]
    /// Page down by one screen.
    pub fn page_down(&self, c: &mut dyn Context) {
        c.page_down();
    }

    /// Build or retrieve cached layout for the given wrap width.
    fn with_layout<R>(&self, wrap_width: usize, f: impl FnOnce(&HelpLayout) -> R) -> R {
        let mut cache = self.layout_cache.borrow_mut();
        let rebuild = cache
            .as_ref()
            .is_none_or(|cached| cached.wrap_width != wrap_width);

        if rebuild {
            let layout = self.build_layout(wrap_width);
            *cache = Some(layout);
        }

        f(cache.as_ref().expect("layout cache initialized"))
    }

    /// Build layout for the current snapshot.
    fn build_layout(&self, wrap_width: usize) -> HelpLayout {
        let Some(snapshot) = &self.snapshot else {
            return HelpLayout {
                max_key_width: 0,
                entries: Vec::new(),
                total_lines: 1, // "Loading..." line
                wrap_width,
            };
        };

        // Sort bindings by key groups with stable alphabetical ordering.
        let mut bindings: Vec<_> = snapshot.bindings.iter().collect();
        bindings.sort_by(|a, b| {
            let (a_group, a_key) = binding_sort_key(&a.input);
            let (b_group, b_key) = binding_sort_key(&b.input);
            a_group
                .cmp(&b_group)
                .then_with(|| a_key.cmp(&b_key))
                .then_with(|| binding_kind_rank(a.kind).cmp(&binding_kind_rank(b.kind)))
        });

        // Find the widest key for alignment
        let max_key_width = bindings
            .iter()
            .map(|b| UnicodeWidthStr::width(b.input.to_string().as_str()))
            .max()
            .unwrap_or(0);

        // Calculate label wrap width (total width - key - separator)
        let separator_width = 2;
        let label_wrap_width = wrap_width
            .saturating_sub(max_key_width + separator_width)
            .max(10);

        // Build entries with wrapped labels
        let mut entries = Vec::with_capacity(bindings.len());
        let mut total_lines = 0;

        for binding in bindings {
            let key_str = binding.input.to_string();
            let wrapped_lines: Vec<String> = textwrap::wrap(&binding.label, label_wrap_width)
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            let line_count = wrapped_lines.len().max(1);
            total_lines += line_count;
            entries.push((key_str, wrapped_lines));
        }

        HelpLayout {
            max_key_width,
            entries,
            total_lines,
            wrap_width,
        }
    }
}

/// Sort groups for help bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BindingGroup {
    /// Lowercase letter keys.
    Lowercase,
    /// Uppercase letter keys.
    Uppercase,
    /// Digit keys.
    Number,
    /// Arrow keys.
    Arrow,
    /// Unmodified special keys and mouse inputs.
    Special,
    /// Modifier chords and modified mouse inputs.
    Chord,
}

/// Rank binding kinds for deterministic ordering.
fn binding_kind_rank(kind: BindingKind) -> u8 {
    match kind {
        BindingKind::PreEventOverride => 0,
        BindingKind::PostEventFallback => 1,
    }
}

/// Build the grouping and alphabetic sort key for a binding input.
fn binding_sort_key(input: &InputSpec) -> (BindingGroup, String) {
    use canopy::event::mouse;

    match input {
        InputSpec::Key(key) => {
            if key.mods != Empty {
                return (BindingGroup::Chord, key.to_string());
            }
            match key.key {
                KeyCode::Char(c) if c.is_ascii_lowercase() => {
                    (BindingGroup::Lowercase, c.to_string())
                }
                KeyCode::Char(c) if c.is_ascii_uppercase() => {
                    (BindingGroup::Uppercase, c.to_string())
                }
                KeyCode::Char(c) if c.is_ascii_digit() => (BindingGroup::Number, c.to_string()),
                KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                    (BindingGroup::Arrow, key.key.to_string())
                }
                _ => (BindingGroup::Special, key.key.to_string()),
            }
        }
        InputSpec::Mouse(m) => {
            let has_mods = m.modifiers.ctrl || m.modifiers.alt || m.modifiers.shift;
            let group = if has_mods {
                BindingGroup::Chord
            } else {
                BindingGroup::Special
            };
            let action = format!("{:?}", m.action);
            let key_label = if matches!(m.button, mouse::Button::None) {
                action
            } else {
                let button = format!("{:?}", m.button);
                format!("{button} {action}")
            };
            let mut key = String::new();
            if has_mods {
                let mut parts = Vec::new();
                if m.modifiers.ctrl {
                    parts.push("Ctrl");
                }
                if m.modifiers.alt {
                    parts.push("Alt");
                }
                if m.modifiers.shift {
                    parts.push("Shift");
                }
                key.push_str(&format!("{}+", parts.join("+")));
            }
            key.push_str(&key_label);
            (group, key)
        }
    }
}

impl Default for HelpContent {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for HelpContent {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        Layout::fill().overflow_y().padding(Edges::all(1))
    }

    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        let wrap_width = view.width.max(1) as usize;
        // If no snapshot yet, assume large content to ensure scrollbar appears
        // on first render. The actual content size will be used after snapshot loads.
        let total_lines = if self.snapshot.is_some() {
            self.with_layout(wrap_width, |layout| layout.total_lines)
        } else {
            100 // Reasonable default for help content
        };
        Size::new(view.width, total_lines as u32)
    }

    fn on_event(&mut self, _event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        // Always check for pending snapshot (overwrites old if present)
        if let Some(snapshot) = ctx.take_help_snapshot() {
            self.snapshot = Some(snapshot);
            self.layout_cache.borrow_mut().take();
        }
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        // Check for pending snapshot and copy to local state if present
        if let Some(pending) = ctx.pending_help_snapshot() {
            self.snapshot = Some(pending.clone());
            self.layout_cache.borrow_mut().take();
        }

        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();

        // Fill visible area with background
        let local_rect = view.outer_rect_local();
        r.fill("help/content", local_rect, ' ')?;

        if self.snapshot.is_none() {
            // Center "Loading..." both vertically and horizontally
            let msg = "Loading...";
            let v_offset = local_rect.h / 2;
            r.text(
                "help/content",
                Line::new(content_origin.x, content_origin.y + v_offset, local_rect.w),
                msg,
            )?;
            return Ok(());
        }

        let wrap_width = view_rect.w.max(1) as usize;
        self.with_layout(wrap_width, |layout| -> Result<()> {
            let max_key_width = layout.max_key_width;
            let separator_width = 2;
            let label_start = (max_key_width + separator_width) as u32;

            // Track which content line we're on
            let mut content_line: u32 = 0;

            for (key_str, wrapped_lines) in &layout.entries {
                let entry_height = wrapped_lines.len().max(1) as u32;

                // Check if any part of this entry is visible
                let entry_end = content_line + entry_height;
                if entry_end <= view_rect.tl.y {
                    content_line = entry_end;
                    continue;
                }
                if content_line >= view_rect.tl.y + view_rect.h {
                    break;
                }

                // Render each line of this entry
                for (line_idx, label_line) in wrapped_lines.iter().enumerate() {
                    let abs_line = content_line + line_idx as u32;

                    // Skip lines above the view
                    if abs_line < view_rect.tl.y {
                        continue;
                    }
                    // Stop if below the view
                    if abs_line >= view_rect.tl.y + view_rect.h {
                        break;
                    }

                    let local_y = content_origin.y + (abs_line - view_rect.tl.y);

                    // Render key only on first line of entry
                    if line_idx == 0 {
                        let padded_key = format!("{:>width$}", key_str, width = max_key_width);
                        r.text(
                            "help/key",
                            Line::new(content_origin.x, local_y, max_key_width as u32),
                            &padded_key,
                        )?;
                        r.text(
                            "help/content",
                            Line::new(content_origin.x + max_key_width as u32, local_y, 2),
                            "  ",
                        )?;
                    }

                    // Render label line
                    let label_width = view_rect.w.saturating_sub(label_start);
                    r.text(
                        "help/label",
                        Line::new(content_origin.x + label_start, local_y, label_width),
                        label_line,
                    )?;
                }

                content_line = entry_end;
            }

            Ok(())
        })?;

        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_content")
    }
}
