use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use crate::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, buf, command,
    core::context::{CoreContext, CoreViewContext},
    derive_commands,
    editor::{Selection, TextPosition, TextRange},
    error::Result,
    event::{key, mouse},
    geom::Point,
    layout::Layout,
    render::Render,
    state::NodeName,
    style::{AttrSet, Color, PartialStyle, Style, StyleManager},
    testing::harness::Harness,
    widget::Widget,
    widgets::editor::{
        EditMode, Editor, EditorConfig, LineNumbers, WrapMode,
        highlight::{HighlightSpan, Highlighter},
    },
};

/// Host widget that mounts an editor as its only child.
struct EditorHost {
    /// Initial text contents.
    text: String,
    /// Editor configuration.
    config: EditorConfig,
    /// Cached editor node id.
    editor_id: Option<NodeId>,
    /// Number of times the host command was triggered.
    binding_hits: usize,
}

#[derive_commands]
impl EditorHost {
    /// Construct a new host for the editor.
    fn new(text: &str, config: EditorConfig) -> Self {
        Self {
            text: text.to_string(),
            config,
            editor_id: None,
            binding_hits: 0,
        }
    }

    /// Record a binding invocation on the host.
    #[command]
    fn record_binding(&mut self, _ctx: &mut dyn Context) {
        self.binding_hits = self.binding_hits.saturating_add(1);
    }

    /// Ensure the editor node exists and is laid out.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.editor_id.is_some() {
            return;
        }
        let editor = Editor::with_config(self.text.clone(), self.config.clone());
        let editor_id = c.add_orphan(editor);
        c.set_children(vec![editor_id])
            .expect("Failed to mount editor");
        c.with_layout(&mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to layout host");
        c.with_layout_of(editor_id, &mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to layout editor");
        self.editor_id = Some(editor_id);
    }

    /// Return the editor node id.
    fn editor_id(&self) -> NodeId {
        self.editor_id.expect("editor id missing")
    }

    /// Return the number of binding hits recorded on the host.
    fn binding_hits(&self) -> usize {
        self.binding_hits
    }
}

impl Widget for EditorHost {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
    }

    fn name(&self) -> NodeName {
        NodeName::convert("editor_host")
    }
}

impl Loader for EditorHost {
    fn load(c: &mut Canopy) {
        c.add_commands::<Editor>();
        c.add_commands::<Self>();
    }
}

fn build_harness(text: &str, config: EditorConfig, width: u32, height: u32) -> Harness {
    let host = EditorHost::new(text, config);
    let mut harness = Harness::builder(host)
        .size(width, height)
        .build()
        .expect("Failed to build harness");
    harness.render().expect("Failed to render");
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.focus_first();
            Ok(())
        })
        .expect("Failed to focus editor");
    harness.render().expect("Failed to render");
    harness
}

fn editor_id(harness: &mut Harness) -> NodeId {
    harness.with_root_widget(|root: &mut EditorHost| root.editor_id())
}

fn with_editor<R>(harness: &mut Harness, f: impl FnOnce(&mut Editor) -> R) -> R {
    let id = editor_id(harness);
    harness.with_widget(id, f)
}

fn host_binding_hits(harness: &mut Harness) -> usize {
    harness.with_root_widget(|root: &mut EditorHost| root.binding_hits())
}

fn editor_text(harness: &mut Harness) -> String {
    with_editor(harness, |editor| editor.buffer().text())
}

fn editor_cursor(harness: &mut Harness) -> TextPosition {
    with_editor(harness, |editor| editor.buffer().cursor())
}

fn editor_selection(harness: &mut Harness) -> Selection {
    with_editor(harness, |editor| editor.buffer().selection())
}

fn editor_cursor_location(harness: &mut Harness) -> Point {
    with_editor(harness, |editor| {
        editor.cursor().expect("cursor missing").location
    })
}

fn editor_view_scroll(harness: &mut Harness) -> Point {
    let id = editor_id(harness);
    let ctx = CoreViewContext::new(&harness.canopy.core, id);
    ctx.view().tl
}

fn scroll_editor_to(harness: &mut Harness, x: u32, y: u32) {
    let id = editor_id(harness);
    harness.canopy.core.with_widget_mut(id, |_widget, core| {
        let mut ctx = CoreContext::new(core, id);
        ctx.scroll_to(x, y);
    });
}

fn mouse_event(action: mouse::Action, x: u32, y: u32) -> mouse::MouseEvent {
    mouse::MouseEvent {
        action,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: Point { x, y },
    }
}

#[test]
fn render_with_line_numbers() {
    let config = EditorConfig::new().with_line_numbers(LineNumbers::Absolute);
    let mut harness = build_harness("hi\nok", config, 6, 2);
    harness.render().unwrap();
    harness.tbuf().assert_matches(buf!["1 hi  " "2 ok  "]);
}

#[test]
fn text_entry_inserts_and_backspaces() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("", config, 10, 2);
    harness.type_text("hi").unwrap();
    harness.key(key::KeyCode::Backspace).unwrap();
    assert_eq!(editor_text(&mut harness), "h");
}

#[test]
fn vi_insert_mode_inserts_text() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("", config, 10, 2);
    harness.key('i').unwrap();
    harness.type_text("hi").unwrap();
    harness.key(key::KeyCode::Esc).unwrap();
    assert_eq!(editor_text(&mut harness), "hi");
}

#[test]
fn vi_word_motions_cross_lines() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("one\ntwo", config, 10, 3);
    harness.keys(['g', 'g']).unwrap();
    harness.key('w').unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(1, 0));
    harness.key('b').unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(0, 0));
}

#[test]
fn vi_yank_put_linewise() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("one\ntwo", config, 10, 3);
    harness.keys(['g', 'g', 'y', 'y', 'p']).unwrap();
    assert_eq!(editor_text(&mut harness), "one\none\ntwo");
}

#[test]
fn preferred_column_survives_vertical_moves() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("abcd\na\nabcd", config, 10, 3);
    harness
        .keys([
            key::KeyCode::Right,
            key::KeyCode::Right,
            key::KeyCode::Right,
        ])
        .unwrap();
    harness.key(key::KeyCode::Down).unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(1, 1));
    harness.key(key::KeyCode::Down).unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(2, 3));
}

#[test]
fn visual_line_delete_removes_lines() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("one\ntwo\nthree", config, 10, 3);
    harness.keys(['g', 'g', 'V', 'j', 'd']).unwrap();
    assert_eq!(editor_text(&mut harness), "three");
}

#[test]
fn search_replace_all() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("foo bar foo", config, 20, 2);
    harness.key('R').unwrap();
    harness.type_text("foo").unwrap();
    harness.key(key::KeyCode::Enter).unwrap();
    harness.type_text("baz").unwrap();
    harness.key(key::KeyCode::Enter).unwrap();
    harness.key('a').unwrap();
    assert_eq!(editor_text(&mut harness), "baz bar baz");
}

#[test]
fn mouse_double_click_selects_word() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("hello", config, 10, 1);
    harness
        .mouse(mouse_event(mouse::Action::Down, 1, 0))
        .unwrap();
    harness
        .mouse(mouse_event(mouse::Action::Down, 1, 0))
        .unwrap();
    let selection = editor_selection(&mut harness);
    assert_eq!(
        selection.range(),
        TextRange::new(TextPosition::new(0, 0), TextPosition::new(0, 5))
    );
}

#[test]
fn mouse_click_moves_cursor() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("hello", config, 10, 1);
    harness
        .mouse(mouse_event(mouse::Action::Down, 2, 0))
        .unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(0, 2));
}

#[test]
fn mouse_drag_extends_selection() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("hello", config, 10, 1);
    harness
        .mouse(mouse_event(mouse::Action::Down, 1, 0))
        .unwrap();
    harness
        .mouse(mouse_event(mouse::Action::Drag, 4, 0))
        .unwrap();
    let selection = editor_selection(&mut harness);
    assert_eq!(
        selection.range(),
        TextRange::new(TextPosition::new(0, 1), TextPosition::new(0, 4))
    );
}

#[test]
fn mouse_triple_click_selects_line() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("hello", config, 10, 1);
    harness
        .mouse(mouse_event(mouse::Action::Down, 1, 0))
        .unwrap();
    harness
        .mouse(mouse_event(mouse::Action::Down, 1, 0))
        .unwrap();
    harness
        .mouse(mouse_event(mouse::Action::Down, 1, 0))
        .unwrap();
    let selection = editor_selection(&mut harness);
    assert_eq!(
        selection.range(),
        TextRange::new(TextPosition::new(0, 0), TextPosition::new(0, 5))
    );
}

#[test]
fn cursor_location_tracks_vertical_scroll() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let text = (0..12)
        .map(|idx| format!("line{idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut harness = build_harness(&text, config, 10, 4);
    for _ in 0..5 {
        harness.key(key::KeyCode::Down).unwrap();
    }
    harness.render().unwrap();

    let cursor = editor_cursor(&mut harness);
    let scroll = editor_view_scroll(&mut harness);
    assert!(scroll.y > 0);
    let location = editor_cursor_location(&mut harness);
    assert_eq!(location.y, (cursor.line as u32).saturating_sub(scroll.y));
    assert_eq!(location.x, (cursor.column as u32).saturating_sub(scroll.x));
}

#[test]
fn cursor_location_updates_after_manual_scroll() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let text = (0..10)
        .map(|idx| format!("row{idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut harness = build_harness(&text, config, 10, 4);
    for _ in 0..3 {
        harness.key(key::KeyCode::Down).unwrap();
    }
    let scroll_before = editor_view_scroll(&mut harness);
    assert_eq!(scroll_before.y, 0);

    scroll_editor_to(&mut harness, 0, 2);
    harness.render().unwrap();

    let cursor = editor_cursor(&mut harness);
    let scroll = editor_view_scroll(&mut harness);
    assert_eq!(scroll.y, 2);
    let location = editor_cursor_location(&mut harness);
    assert_eq!(location.y, (cursor.line as u32).saturating_sub(scroll.y));
    assert_eq!(location.x, (cursor.column as u32).saturating_sub(scroll.x));
}

#[test]
fn cursor_location_tracks_horizontal_scroll() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let text = "abcdefghijklmnopqrstuvwxyz";
    let mut harness = build_harness(text, config, 6, 1);
    for _ in 0..12 {
        harness.key(key::KeyCode::Right).unwrap();
    }
    harness.render().unwrap();

    let cursor = editor_cursor(&mut harness);
    let scroll = editor_view_scroll(&mut harness);
    assert!(scroll.x > 0);
    let location = editor_cursor_location(&mut harness);
    assert_eq!(location.x, (cursor.column as u32).saturating_sub(scroll.x));
    assert_eq!(location.y, (cursor.line as u32).saturating_sub(scroll.y));
}

#[test]
fn binding_precedence_blocks_text_entry() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("", config, 10, 1);
    Binder::new(&mut harness.canopy)
        .with_path("editor")
        .key('x', "editor::cursor_left()");
    harness.key('x').unwrap();
    assert_eq!(editor_text(&mut harness), "");
}

#[test]
fn highlight_spans_apply_styles() {
    let config = EditorConfig::new()
        .with_mode(EditMode::Text)
        .with_wrap(WrapMode::None);
    let mut harness = build_harness("hi", config, 5, 1);
    let style = Style {
        fg: Color::Red,
        bg: Color::Black,
        attrs: AttrSet::default(),
    };
    with_editor(&mut harness, |editor| {
        editor.set_highlighter(Some(Box::new(TestHighlighter {
            style: style.clone(),
        })));
    });
    harness.render().unwrap();
    let partial = PartialStyle::fg(Color::Red);
    assert!(harness.tbuf().contains_text_style("hi", &partial));
}

#[test]
fn highlight_spans_inherit_editor_background() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let mut harness = build_harness("hi\nok", config, 4, 2);
    let highlight_style = Style {
        fg: Color::Green,
        bg: Color::Red,
        attrs: AttrSet::default(),
    };
    with_editor(&mut harness, |editor| {
        editor.set_highlighter(Some(Box::new(TestHighlighter {
            style: highlight_style,
        })));
    });
    harness.key(key::KeyCode::Down).unwrap();

    let base_bg = StyleManager::default()
        .get(&harness.canopy.style, "editor/text")
        .bg;
    let buf = harness.buf();
    let first = buf.get(Point { x: 0, y: 0 }).expect("cell missing");
    let second = buf.get(Point { x: 1, y: 0 }).expect("cell missing");
    assert_eq!(first.style.bg, base_bg);
    assert_eq!(second.style.bg, base_bg);
}

#[test]
fn highlight_spans_cached_by_revision() {
    let config = EditorConfig::new()
        .with_mode(EditMode::Text)
        .with_wrap(WrapMode::None);
    let mut harness = build_harness("hi", config, 5, 1);
    let counter = Arc::new(AtomicUsize::new(0));
    let highlighter = CountingHighlighter {
        count: counter.clone(),
    };
    with_editor(&mut harness, |editor| {
        editor.set_highlighter(Some(Box::new(highlighter)));
    });
    harness.render().unwrap();
    let first = counter.load(Ordering::SeqCst);
    assert!(first > 0);
    harness.render().unwrap();
    let second = counter.load(Ordering::SeqCst);
    assert_eq!(first, second);
}

#[test]
fn root_binding_does_not_override_text_entry() {
    let config = EditorConfig::new().with_mode(EditMode::Text);
    let mut harness = build_harness("", config, 6, 1);
    Binder::new(&mut harness.canopy)
        .with_path("editor_host")
        .key('q', "editor_host::record_binding()");
    harness.key('q').unwrap();
    assert_eq!(editor_text(&mut harness), "q");
    assert_eq!(host_binding_hits(&mut harness), 0);
}

#[derive(Clone)]
struct TestHighlighter {
    style: Style,
}

impl Highlighter for TestHighlighter {
    fn highlight_line(&self, line: usize, text: &str) -> Vec<HighlightSpan> {
        if line == 0 && text.len() >= 2 {
            vec![HighlightSpan {
                range: 0..2,
                style: self.style.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone)]
struct CountingHighlighter {
    count: Arc<AtomicUsize>,
}

impl Highlighter for CountingHighlighter {
    fn highlight_line(&self, _line: usize, _text: &str) -> Vec<HighlightSpan> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }
}
