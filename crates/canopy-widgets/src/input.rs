use std::iter::repeat_n;

use canopy::{
    Context, EventOutcome, NodeName, Register, Setup, ViewContext, Widget,
    commands::CommandCall,
    derive_commands,
    error::Result,
    geom::{Line, Point, Size},
    input::{Event, IntentSpec, key},
    layout::{Layout, MeasureConstraints, Measurement},
    render::{
        Render,
        cursor::{self, CursorRequest},
    },
    runtime::WidgetSemantics,
    style::{WidgetState, roles},
    text,
};

use crate::text_buffer::{TextBuffer, TextPosition, single_line};

/// What one key does in a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputKey {
    /// Insert a character.
    Text(char),
    /// Move the cursor left.
    Left,
    /// Move the cursor right.
    Right,
    /// Move the cursor to the start.
    Home,
    /// Move the cursor past the end.
    End,
    /// Delete before the cursor.
    Backspace,
    /// Delete under the cursor.
    Delete,
    /// Run the submit call.
    Submit,
    /// Run the cancel call.
    Cancel,
}

/// Default tab stop width for single-line inputs.
const DEFAULT_TAB_STOP: usize = 4;

/// A single-line text buffer with horizontal scrolling.
#[derive(Debug, Clone)]
struct InputBuffer {
    /// Rope-backed text storage.
    buffer: TextBuffer,
    /// Cached buffer contents for easy slicing.
    value: String,
    /// Column offset of the visible window.
    scroll: usize,
    /// Visible window width in columns.
    view_width: usize,
    /// Tab stop width in columns.
    tab_stop: usize,
}

impl InputBuffer {
    /// Construct a new input buffer with initial content.
    fn new(start: impl Into<String>) -> Self {
        let raw = start.into();
        let value = single_line(&raw);
        let buffer = TextBuffer::new(value.clone());
        let mut out = Self {
            buffer,
            value,
            scroll: 0,
            view_width: 0,
            tab_stop: DEFAULT_TAB_STOP,
        };
        out.ensure_cursor_visible();
        out
    }

    /// Set the visible window width.
    fn set_display_width(&mut self, width: usize) {
        self.view_width = width;
        self.ensure_cursor_visible();
    }

    /// The location of the displayed cursor along the x axis.
    fn cursor_display(&self) -> u32 {
        let cursor_col = self.cursor_column();
        cursor_col.saturating_sub(self.scroll) as u32
    }

    /// Return the raw input value.
    fn value(&self) -> &str {
        &self.value
    }

    /// Return the visible text for rendering.
    fn render_text(&self) -> String {
        if self.view_width == 0 {
            return String::new();
        }
        let expanded = text::expand_tabs(&self.value, self.tab_stop);
        let (out, _) = text::slice_by_columns(&expanded, self.scroll, self.view_width);
        out.to_string()
    }

    /// Insert text at the cursor position, with line breaks as spaces and
    /// other control characters dropped.
    fn insert(&mut self, text: &str) {
        let insert = single_line(text)
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect::<String>();
        if insert.is_empty() {
            return;
        }
        self.buffer.insert_text(&insert);
        self.sync_value();
        self.ensure_cursor_visible();
    }

    /// Delete the character under the cursor.
    fn delete(&mut self) {
        if self.buffer.delete_forward(false) {
            self.sync_value();
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor to the start of the value.
    fn home(&mut self) {
        self.buffer.move_line_start();
        self.ensure_cursor_visible();
    }

    /// Move the cursor past the end of the value.
    fn end(&mut self) {
        self.buffer.move_line_end();
        self.ensure_cursor_visible();
    }

    /// Delete the character before the cursor.
    fn backspace(&mut self) {
        if self.buffer.delete_backward(false) {
            self.sync_value();
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor left by one character.
    fn left(&mut self) {
        if self.buffer.move_left(false) {
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor right by one character.
    fn right(&mut self) {
        if self.buffer.move_right(false) {
            self.ensure_cursor_visible();
        }
    }

    /// Return the display width of the full buffer.
    fn display_width(&self) -> u32 {
        self.line_width() as u32
    }

    /// Update the cached value string from the rope.
    fn sync_value(&mut self) {
        self.value = self.buffer.line_text(0);
    }

    /// Compute the cursor column in display coordinates.
    fn cursor_column(&self) -> usize {
        self.buffer
            .column_for_position(self.buffer.cursor(), self.tab_stop)
    }

    /// Compute the display width of the line.
    fn line_width(&self) -> usize {
        let len = self.buffer.line_char_len(0);
        self.buffer
            .column_for_position(TextPosition::new(0, len), self.tab_stop)
    }

    /// Ensure the cursor stays within the visible window.
    fn ensure_cursor_visible(&mut self) {
        if self.view_width == 0 {
            self.scroll = 0;
            return;
        }
        let cursor_col = self.cursor_column();
        if cursor_col < self.scroll {
            self.scroll = cursor_col;
        } else {
            let window_end = self.scroll.saturating_add(self.view_width);
            if cursor_col >= window_end {
                let delta = cursor_col.saturating_sub(window_end).saturating_add(1);
                self.scroll = self.scroll.saturating_add(delta);
            }
        }

        let text_width = self.line_width();
        let max_scroll = text_width.saturating_add(1).saturating_sub(self.view_width);
        self.scroll = self.scroll.min(max_scroll);
    }
}

/// Intent that clears the focused field, or resets what the focused widget
/// shows.
///
/// Every widget that accepts it registers it with [`register_clear_intent`],
/// and an application binds a key to it. The route offers the intent to the
/// first accepting widget, which clears or resets its own state.
pub const CLEAR_INTENT: &str = "canopy.clear";

/// Register [`CLEAR_INTENT`]. Each widget type that accepts the intent calls
/// this from its `Register` impl; registering it again is harmless.
pub fn register_clear_intent(setup: &mut Setup) -> Result<()> {
    setup.register_intent(IntentSpec::new(
        CLEAR_INTENT,
        "Clear the focused field, or reset what the focused widget shows",
    )?)
}

/// Returns `text` with a dot in place of each character, as wide as the
/// character, so the caret keeps its column.
fn masked(text: &str) -> String {
    let mut masked = String::with_capacity(text.len());
    let mut buffer = [0; 4];
    for character in text.chars() {
        let width = text::width(character.encode_utf8(&mut buffer)) as usize;
        masked.extend(repeat_n('•', width));
    }
    masked
}

/// Single-line text input widget.
///
/// The whole row changes style while the field takes keys, even when empty. A
/// visible prompt can name the field independently of its editable value.
///
/// The field edits itself: text and paste insert, and Left, Right, Home, End,
/// Backspace, and Delete move and delete. It consumes [`CLEAR_INTENT`]. Other
/// keys reach its owner and the application's bindings. An owner learns of
/// the field through stored command calls: [`Input::with_on_change`] runs with
/// the new value appended after every edit, and [`Input::with_on_submit`] and
/// [`Input::with_on_cancel`] take Enter and Esc. The field posts each call with
/// [`Context::post`], so the call runs once the field's handler has returned,
/// and the owner may read, change, or remove the field.
pub struct Input {
    /// Text buffer for the input.
    buffer: InputBuffer,
    /// Visible, noneditable prefix, separate from the value and semantic label.
    prompt: String,
    /// Optional semantic label independent of the text value.
    label: Option<String>,
    /// Policy for exposing the value in semantic snapshots.
    value_exposure: ValueExposure,
    /// Whether the field shows as taking keys, overriding its focus.
    active: Option<bool>,
    /// Call posted with the value appended after every edit.
    on_change: Option<CommandCall>,
    /// Call posted by Enter.
    on_submit: Option<CommandCall>,
    /// Call posted by Esc.
    on_cancel: Option<CommandCall>,
    /// Path segment for this node.
    name: NodeName,
}

/// Policy for publishing an input value in semantic snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueExposure {
    /// Omit the value from semantics.
    #[default]
    Omit,
    /// Publish the raw single-line value.
    Public,
    /// Mark the value as sensitive: always omit it from semantics, and draw
    /// a dot in place of each character on screen.
    Sensitive,
}

#[derive_commands]
impl Input {
    /// Construct a new input with initial text.
    pub fn new(txt: impl Into<String>) -> Self {
        Self {
            buffer: InputBuffer::new(txt),
            prompt: String::new(),
            label: None,
            value_exposure: ValueExposure::Omit,
            active: None,
            on_change: None,
            on_submit: None,
            on_cancel: None,
            name: NodeName::convert("input"),
        }
    }

    /// Name this node's path segment, so scripts and bindings can tell
    /// fields apart. The style layer stays `input`.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = NodeName::convert(name);
        self
    }

    /// Post `call` with the new value appended after every edit.
    ///
    /// The value is appended as the last positional argument, or as `value`
    /// among named ones. [`Input::set_value`] is not an edit. Each call carries
    /// the value of its own edit, so several edits in one dispatch arrive in
    /// order, each with its value. An owner that reads the field instead sees
    /// its latest value.
    #[must_use]
    pub fn with_on_change(mut self, call: CommandCall) -> Self {
        self.on_change = Some(call);
        self
    }

    /// Post `call` when Enter is pressed in the field.
    #[must_use]
    pub fn with_on_submit(mut self, call: CommandCall) -> Self {
        self.on_submit = Some(call);
        self
    }

    /// Post `call` when Esc is pressed in the field.
    #[must_use]
    pub fn with_on_cancel(mut self, call: CommandCall) -> Self {
        self.on_cancel = Some(call);
        self
    }

    /// Replace the visible prompt, keeping the value and caret.
    pub fn set_prompt(&mut self, prompt: impl Into<String>) {
        self.prompt = single_line(&prompt.into());
    }

    /// Replace the semantic label.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
    }

    /// Show the field as taking keys, or not, whatever its focus.
    ///
    /// A composite that keeps focus elsewhere and writes into the field, such
    /// as a picker whose list takes the keys, uses this to light it up.
    /// `None` follows focus again.
    pub fn set_active(&mut self, active: Option<bool>) {
        self.active = active;
    }

    /// Return whether the field shows as taking keys.
    fn is_active(&self, ctx: &dyn ViewContext) -> bool {
        self.active.unwrap_or_else(|| ctx.is_focused())
    }

    /// Post the change call with the current value.
    fn changed(&self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(call) = &self.on_change {
            ctx.post(&call.with_arg("value", self.value()))?;
        }
        Ok(())
    }

    /// Set the semantic label without changing the displayed value.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Add a visible prompt before the editable value, for example `" Glob: "`.
    /// Its width participates in measurement, scrolling, and cursor placement.
    /// The input automatically emphasizes the whole row while it holds focus.
    #[must_use]
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = single_line(&prompt.into());
        self
    }

    /// Display columns reserved before the editable value.
    fn prompt_width(&self) -> u32 {
        text::width(&self.prompt)
    }

    /// Configure whether semantic snapshots expose this input's value.
    #[must_use]
    pub fn with_value_exposure(mut self, exposure: ValueExposure) -> Self {
        self.value_exposure = exposure;
        self
    }

    /// Return the raw input value without padding.
    pub fn value(&self) -> &str {
        self.buffer.value()
    }

    /// Replace the input value.
    ///
    /// The cursor goes to the end of the new value, where typing continues.
    /// This is not an edit, so the change call does not run.
    pub fn set_value(&mut self, value: impl Into<String>) {
        let value = value.into();
        if value == self.value() {
            return;
        }
        self.buffer = InputBuffer::new(value);
        self.buffer.end();
    }

    /// Move the cursor left.
    #[command]
    pub fn left(&mut self, _c: &mut dyn Context) {
        self.buffer.left();
    }

    /// Move the cursor right.
    #[command]
    pub fn right(&mut self, _c: &mut dyn Context) {
        self.buffer.right();
    }

    /// Move the cursor to the start of the value.
    #[command]
    pub fn home(&mut self, _c: &mut dyn Context) {
        self.buffer.home();
    }

    /// Move the cursor past the end of the value.
    #[command]
    pub fn end(&mut self, _c: &mut dyn Context) {
        self.buffer.end();
    }

    /// Delete the character before the cursor.
    #[command]
    pub fn backspace(&mut self, c: &mut dyn Context) -> Result<()> {
        self.edit(c, InputBuffer::backspace)
    }

    /// Delete the character under the cursor.
    #[command]
    pub fn delete(&mut self, c: &mut dyn Context) -> Result<()> {
        self.edit(c, InputBuffer::delete)
    }

    /// Insert `text` at the cursor, as a paste does.
    #[command]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "command arguments are owned values"
    )]
    pub fn insert(&mut self, c: &mut dyn Context, text: String) -> Result<()> {
        self.edit(c, |buffer| buffer.insert(&text))
    }

    /// Apply one edit, and tell the owner when it changed the value.
    fn edit(&mut self, c: &mut dyn Context, edit: impl FnOnce(&mut InputBuffer)) -> Result<()> {
        let before = self.buffer.value().to_owned();
        edit(&mut self.buffer);
        if self.buffer.value() != before {
            self.changed(c)?;
        }
        Ok(())
    }

    /// Classify one key without running its effect.
    fn classify_key(&self, key: key::Key) -> Option<InputKey> {
        if let Some(character) = key.text_char() {
            return Some(InputKey::Text(character));
        }
        if key.mods.ctrl || key.mods.alt {
            return None;
        }
        Some(match key.key {
            key::KeyCode::Left => InputKey::Left,
            key::KeyCode::Right => InputKey::Right,
            key::KeyCode::Home => InputKey::Home,
            key::KeyCode::End => InputKey::End,
            key::KeyCode::Backspace => InputKey::Backspace,
            key::KeyCode::Delete => InputKey::Delete,
            key::KeyCode::Enter if self.on_submit.is_some() => InputKey::Submit,
            key::KeyCode::Esc if self.on_cancel.is_some() => InputKey::Cancel,
            _ => return None,
        })
    }
}

impl Register for Input {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        register_clear_intent(setup)
    }
}

impl Widget for Input {
    fn semantics(&self, _ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("input".into()),
            label: self.label.clone(),
            value: (self.value_exposure == ValueExposure::Public).then(|| self.value().to_owned()),
            ..WidgetSemantics::default()
        })
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("input");
        let active = self.is_active(ctx);
        if active {
            r.push_layer(WidgetState::Focused.layer());
        }
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        r.fill(roles::BACKGROUND, view.view_rect_local(), ' ')?;
        if view_rect.h == 0 {
            return Ok(());
        }
        let prompt_width = self.prompt_width().min(view_rect.w);
        if prompt_width > 0 {
            r.text(
                roles::PROMPT,
                Line::new(content_origin.x, content_origin.y, prompt_width),
                &self.prompt,
            )?;
        }
        let width = view_rect.w.saturating_sub(prompt_width);
        self.buffer.set_display_width(width as usize);
        if width == 0 {
            return Ok(());
        }
        let text_x = content_origin.x.saturating_add(prompt_width);
        let line = Line::new(text_x, content_origin.y, width);
        let content = self.buffer.render_text();
        let content = if self.value_exposure == ValueExposure::Sensitive {
            masked(&content)
        } else {
            content
        };
        r.text(roles::TEXT, line, &content)?;
        let caret = Point {
            x: text_x + self.buffer.cursor_display(),
            y: content_origin.y,
        };
        if self.buffer.cursor_display() < width {
            if ctx.is_focused() {
                r.cursor(caret, CursorRequest::new(cursor::TEXT));
            } else if active {
                // An active field without focus still shows where typing
                // lands.
                r.cursor(caret, CursorRequest::new(cursor::INACTIVE));
            }
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let key = match event {
            Event::Key(key) => *key,
            Event::Paste(text) => {
                let text = text.clone();
                self.edit(ctx, |buffer| buffer.insert(&text))?;
                return Ok(EventOutcome::Handle);
            }
            _ => return Ok(EventOutcome::Ignore),
        };
        let Some(action) = self.classify_key(key) else {
            return Ok(EventOutcome::Ignore);
        };
        match action {
            InputKey::Text(character) => {
                self.edit(ctx, |buffer| {
                    buffer.insert(character.encode_utf8(&mut [0; 4]));
                })?;
            }
            InputKey::Left => self.buffer.left(),
            InputKey::Right => self.buffer.right(),
            InputKey::Home => self.buffer.home(),
            InputKey::End => self.buffer.end(),
            InputKey::Backspace => self.edit(ctx, InputBuffer::backspace)?,
            InputKey::Delete => self.edit(ctx, InputBuffer::delete)?,
            InputKey::Submit => {
                if let Some(call) = &self.on_submit {
                    ctx.post(call)?;
                }
            }
            InputKey::Cancel => {
                if let Some(call) = &self.on_cancel {
                    ctx.post(call)?;
                }
            }
        }
        Ok(EventOutcome::Handle)
    }

    fn key_outcome(&self, key: key::Key, _context: &dyn ViewContext) -> EventOutcome {
        if self.classify_key(key).is_some() {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn accepts_intent(&self, intent: &str, _context: &dyn ViewContext) -> bool {
        intent == CLEAR_INTENT
    }

    fn on_intent(&mut self, intent: &str, context: &mut dyn Context) -> Result<EventOutcome> {
        if intent != CLEAR_INTENT {
            return Ok(EventOutcome::Ignore);
        }
        if !self.value().is_empty() {
            self.set_value("");
            self.changed(context)?;
        }
        Ok(EventOutcome::Handle)
    }

    fn layout(&self) -> Layout {
        // A text field keeps a stable width while its value changes, so it
        // fills the width it is given. Its height is the one measured row.
        Layout::column().flex_horizontal(1)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        // The cell after the text holds the caret at the end of the value.
        let width = self
            .prompt_width()
            .saturating_add(self.buffer.display_width())
            .saturating_add(1);
        c.clamp(Size::new(width, 1))
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        CanopyBuilder, EventOutcome, NodeId, Setup, ViewContextExt, Widget,
        error::Result,
        input::{Event, key},
        runtime::TurnInput,
        testing::harness::Harness,
        text,
    };

    use super::{CLEAR_INTENT, Input, InputBuffer, ValueExposure};

    #[test]
    fn semantic_values_require_explicit_public_exposure() {
        for exposure in [
            ValueExposure::Omit,
            ValueExposure::Public,
            ValueExposure::Sensitive,
        ] {
            let mut input = Input::new("secret")
                .with_label("Account")
                .with_value_exposure(exposure);
            input.set_value("updated");
            let semantics = CanopyBuilder::new()
                .build()
                .expect("an empty application builds")
                .with_root_view(|ctx| input.semantics(ctx))
                .expect("input semantics");
            assert_eq!(semantics.role.as_deref(), Some("input"));
            assert_eq!(semantics.label.as_deref(), Some("Account"));
            assert_eq!(
                semantics.value.as_deref(),
                (exposure == ValueExposure::Public).then_some("updated")
            );
        }
        assert_eq!(
            CanopyBuilder::new()
                .build()
                .expect("an empty application builds")
                .with_root_view(|ctx| Input::new("private").semantics(ctx))
                .expect("default semantics")
                .value,
            None
        );
    }

    #[test]
    fn a_sensitive_value_shows_as_dots() -> Result<()> {
        let mut harness =
            Harness::builder(Input::new("").with_value_exposure(ValueExposure::Sensitive))
                .size(20, 1)
                .build()?;
        harness.render()?;
        harness.type_text("sk-123")?;
        harness.render()?;
        let screen = harness.tbuf().lines().join("");
        assert!(screen.starts_with("••••••"), "{screen:?}");
        assert!(!screen.contains("sk-123"), "{screen:?}");
        assert_eq!(super::masked("a界"), "•••");
        Ok(())
    }

    #[test]
    fn input_caret_stays_inside_nonempty_viewports() {
        for width in [1, 3] {
            for text in ["", "a", "abc", "abcdef", "界a", "e\u{301}ab"] {
                let mut buf = InputBuffer::new(text);
                buf.set_display_width(width);
                assert!(
                    buf.cursor_display() < width as u32,
                    "{text:?}, width {width}"
                );
                for _ in 0..text.chars().count() {
                    buf.left();
                    assert!(buf.cursor_display() < width as u32);
                }
                assert_eq!(buf.scroll, 0);
                for _ in 0..text.chars().count() {
                    buf.right();
                    assert!(buf.cursor_display() < width as u32);
                }
                for _ in 0..text.chars().count() {
                    buf.backspace();
                    assert!(buf.cursor_display() < width as u32);
                }
                assert_eq!(buf.value(), "");
            }
        }
    }

    #[test]
    fn input_buffer_handles_multibyte_chars() {
        let mut buf = InputBuffer::new("a");
        buf.set_display_width(10);
        let accent = '\u{00e9}';
        buf.insert(&accent.to_string());
        let expected = format!("a{accent}");
        assert_eq!(buf.value(), expected);
        assert_eq!(buf.cursor_display(), text::width(&expected));
        buf.left();
        assert_eq!(buf.cursor_display(), text::width("a"));
        buf.backspace();
        assert_eq!(buf.value(), accent.to_string());
    }

    #[test]
    fn key_outcome_matches_event_handling_for_every_spec() -> Result<()> {
        let specs = [
            "a",
            "Z",
            "2",
            "space",
            "ctrl-a",
            "ctrl-x",
            "ctrl-alt-x",
            "alt-x",
            "ctrl-alt-z",
            "enter",
            "backspace",
            "left",
            "f1",
            "esc",
            "tab",
            "shift-a",
        ];
        let mut app = CanopyBuilder::new().build()?;
        for spec in specs {
            let mut input = Input::new("");
            let key = key::Key::parse_spec(spec).expect("valid key spec");
            let (predicted, actual) = app.with_root_context(|ctx| {
                let predicted = input.key_outcome(key, ctx);
                Ok((predicted, input.on_event(&Event::Key(key), ctx)?))
            })?;
            assert_eq!(
                predicted, actual,
                "prediction must match handling for {spec}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_clear_action_resets_the_value() -> Result<()> {
        let mut input = Input::new("typed text");
        CanopyBuilder::new().build()?.with_root_context(|ctx| {
            assert!(input.accepts_intent(CLEAR_INTENT, ctx));
            assert!(!input.accepts_intent("canopy.text.other", ctx));
            assert_eq!(input.on_intent(CLEAR_INTENT, ctx)?, EventOutcome::Handle);
            assert_eq!(input.value(), "");

            // An unknown action is ignored and changes nothing.
            input.set_value("kept");
            assert_eq!(
                input.on_intent("canopy.text.other", ctx)?,
                EventOutcome::Ignore
            );
            assert_eq!(input.value(), "kept");
            Ok(())
        })
    }

    #[test]
    fn input_ignores_ctrl_and_alt_chords() -> Result<()> {
        let mut input = Input::new("");
        CanopyBuilder::new().build()?.with_root_context(|ctx| {
            for mods in [key::Ctrl, key::Alt] {
                let event = Event::Key(key::Key {
                    key: key::KeyCode::Char('a'),
                    mods,
                });
                assert_eq!(input.on_event(&event, ctx)?, EventOutcome::Ignore);
                assert_eq!(input.value(), "");
            }

            let event = Event::Key(key::Key {
                key: key::KeyCode::Char('a'),
                mods: key::Empty,
            });
            assert_eq!(input.on_event(&event, ctx)?, EventOutcome::Handle);
            assert_eq!(input.value(), "a");
            Ok(())
        })
    }

    #[test]
    fn input_buffer_handles_grapheme_clusters() {
        let astronaut = "\u{1f469}\u{200d}\u{1f680}";
        let mut buf = InputBuffer::new(format!("a{astronaut}b"));
        buf.set_display_width(10);
        let expected = format!("a{astronaut}b");
        assert_eq!(buf.cursor_display(), text::width(&expected));
        buf.left();
        let expected = format!("a{astronaut}");
        assert_eq!(buf.cursor_display(), text::width(&expected));
        buf.backspace();
        assert_eq!(buf.value(), "ab");
    }

    /// Records what its field tells it.
    #[derive(Default)]
    struct Owner {
        /// Values the change call carried, in order.
        values: Vec<String>,
        /// Submit and cancel calls, in order.
        calls: Vec<&'static str>,
    }

    #[canopy::derive_commands]
    impl Owner {
        /// Record a changed value.
        #[command]
        fn changed(&mut self, _c: &mut dyn canopy::Context, value: String) {
            self.values.push(value);
        }

        /// Record a submit.
        #[command]
        fn submit(&mut self, _c: &mut dyn canopy::Context) {
            self.calls.push("submit");
        }

        /// Record a cancel.
        #[command]
        fn cancel(&mut self, _c: &mut dyn canopy::Context) {
            self.calls.push("cancel");
        }
    }

    impl Widget for Owner {
        fn on_mount(&mut self, c: &mut dyn canopy::Context) -> Result<()> {
            use canopy::{ContextExt, commands::CommandTarget};
            let owner = CommandTarget::Exact(c.node_id());
            let field = c.add_child(
                c.node_id(),
                Input::new("")
                    .with_on_change(Self::spec_changed().call().with_target(owner))
                    .with_on_submit(Self::call_submit().with_target(owner))
                    .with_on_cancel(Self::call_cancel().with_target(owner)),
            )?;
            c.set_focus(field.into())?;
            Ok(())
        }
    }

    #[test]
    fn a_field_edits_itself_and_notifies_its_owner() -> Result<()> {
        use canopy::testing::harness::Harness;
        let mut harness = Harness::builder(Owner::default())
            .configure(|setup| setup.add_commands::<Owner>())
            .register::<Input>()
            .size(20, 1)
            .build()?;
        harness.render()?;
        harness.type_text("abc")?;
        harness.key(key::KeyCode::Home)?;
        harness.key(key::KeyCode::Delete)?;
        harness.key(key::KeyCode::End)?;
        harness.key(key::KeyCode::Backspace)?;
        harness
            .canopy
            .turn(TurnInput::Events(vec![Event::Paste("x\ny".into())]))?;
        harness.key(key::KeyCode::Enter)?;
        harness.key(key::KeyCode::Esc)?;
        let (values, calls) = harness
            .with_root_widget(|owner: &mut Owner| (owner.values.clone(), owner.calls.clone()));
        assert_eq!(values, ["a", "ab", "abc", "bc", "b", "bx y"]);
        assert_eq!(calls, ["submit", "cancel"]);
        Ok(())
    }

    /// Build a harness whose root is `owner`, holding its field.
    fn owned<W: Widget + 'static>(
        owner: W,
        configure: impl FnOnce(&mut Setup) -> Result<()> + 'static,
    ) -> Result<Harness> {
        let mut harness = Harness::builder(owner)
            .configure(configure)
            .register::<Input>()
            .size(20, 1)
            .build()?;
        harness.render()?;
        Ok(harness)
    }

    /// Return the field under the harness root.
    fn the_field(harness: &Harness) -> NodeId {
        harness
            .canopy
            .with_root_view(|ctx| ctx.unique_descendant::<Input>(ctx.node_id()))
            .expect("field lookup")
            .expect("field mounted")
            .into()
    }

    #[test]
    fn edits_in_one_dispatch_arrive_in_order_with_their_own_values() -> Result<()> {
        use canopy::{ContextExt, ViewContextExt};
        let mut harness = owned(Owner::default(), |setup| setup.add_commands::<Owner>())?;
        let field = the_field(&harness);
        let root = harness.canopy.root_id();
        harness.canopy.with_context(field, |ctx| {
            ctx.with_widget_mut(field, |input: &mut Input, ctx| {
                for key in [
                    key::Key::from('a'),
                    key::Key::from('b'),
                    key::KeyCode::Enter.into(),
                ] {
                    input.on_event(&Event::Key(key), ctx)?;
                }
                Ok(())
            })?;
            // The owner hears nothing until the dispatch completes, and the
            // field already holds its latest value.
            let heard = ctx.with_widget(root, |owner: &Owner| Ok(owner.values.len()))?;
            assert_eq!(heard, 0);
            let value = ctx.with_widget(field, |input: &Input| Ok(input.value().to_owned()))?;
            assert_eq!(value, "ab");
            Ok(())
        })?;
        let (values, calls) = harness
            .with_root_widget(|owner: &mut Owner| (owner.values.clone(), owner.calls.clone()));
        assert_eq!(
            values,
            ["a", "ab"],
            "each change carries its own edit's value"
        );
        assert_eq!(calls, ["submit"]);
        Ok(())
    }

    /// Removes its field when the field submits.
    #[derive(Default)]
    struct Closer {
        /// The field, until a submit removes it.
        field: Option<canopy::NodeId>,
    }

    #[canopy::derive_commands]
    impl Closer {
        /// Remove the field that submitted.
        #[command]
        fn submit(&mut self, c: &mut dyn canopy::Context) -> Result<()> {
            if let Some(field) = self.field.take() {
                c.remove_subtree(field)?;
            }
            Ok(())
        }
    }

    impl Widget for Closer {
        fn on_mount(&mut self, c: &mut dyn canopy::Context) -> Result<()> {
            use canopy::{ContextExt, commands::CommandTarget};
            let owner = CommandTarget::Exact(c.node_id());
            let field = c.add_child(
                c.node_id(),
                Input::new("").with_on_submit(Self::call_submit().with_target(owner)),
            )?;
            c.set_focus(field.into())?;
            self.field = Some(field.into());
            Ok(())
        }
    }

    #[test]
    fn an_owner_can_remove_the_field_that_submitted() -> Result<()> {
        let mut harness = owned(Closer::default(), |setup| setup.add_commands::<Closer>())?;
        let field = the_field(&harness);
        harness.key(key::KeyCode::Enter)?;
        let gone = harness
            .canopy
            .with_root_view(|ctx| ctx.type_id_of(field).is_none());
        assert!(gone, "the owner removed the field from its submit");
        assert!(harness.canopy.notices().is_empty());
        Ok(())
    }
}
