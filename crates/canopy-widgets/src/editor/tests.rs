use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use canopy::{
    Canopy, Context, FocusScope, Loader, Widget, buf, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::Point,
    layout::{Edges, Layout},
    state::NodeName,
    style::{AttrSet, Color, Paint, PartialStyle, Style, StyleManager},
    testing::harness::Harness,
};

use super::{
    Selection, TextPosition, TextRange,
    search::{PromptState, SearchDirection, find_matches},
    vi::ViMode,
};
use crate::editor::{
    EditMode, Editor, EditorConfig, LineNumbers, WrapMode,
    highlight::{HighlightSpan, Highlighter},
};

canopy::key!(EditorSlot: Editor);

/// Host widget that mounts an editor as its only child.
struct EditorHost {
    /// Initial text contents.
    text: String,
    /// Editor configuration.
    config: EditorConfig,
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
            binding_hits: 0,
        }
    }

    /// Record a binding invocation on the host.
    #[command]
    fn record_binding(&mut self, _ctx: &mut dyn Context) {
        self.binding_hits = self.binding_hits.saturating_add(1);
    }

    /// Return the number of binding hits recorded on the host.
    fn binding_hits(&self) -> usize {
        self.binding_hits
    }
}

impl Widget for EditorHost {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let editor = Editor::with_config(self.text.clone(), self.config.clone());
        let editor_id = c.add_keyed::<EditorSlot>(editor)?;
        c.set_layout(Layout::fill())?;
        c.set_layout_of(editor_id, Layout::fill())?;
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("editor_host")
    }
}

impl Loader for EditorHost {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Editor>()?;
        c.add_commands::<Self>()?;
        Ok(())
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
            ctx.focus_first(FocusScope::Current)?;
            Ok(())
        })
        .expect("Failed to focus editor");
    harness.render().expect("Failed to render");
    harness
}

fn with_editor<R>(harness: &mut Harness, f: impl FnOnce(&mut Editor) -> R) -> R {
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.with_child::<EditorSlot, _>(|editor, _| Ok(f(editor)))
        })
        .expect("editor missing")
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
        <Editor as Widget>::cursor(editor)
            .expect("cursor missing")
            .location
    })
}

fn editor_view_scroll(harness: &mut Harness) -> Point {
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.with_child::<EditorSlot, _>(|_editor, ctx| Ok(ctx.view().tl))
        })
        .expect("editor missing")
}

fn scroll_editor_to(harness: &mut Harness, x: u32, y: u32) {
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.with_child::<EditorSlot, _>(|_editor, ctx| {
                ctx.scroll_to(x, y);
                Ok(())
            })
        })
        .expect("editor missing");
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
fn vi_x_deletes_forward_and_yanks_for_put() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("abc", config, 10, 2);
    harness.key('x').unwrap();
    assert_eq!(editor_text(&mut harness), "bc");
    harness.key('p').unwrap();
    assert_eq!(editor_text(&mut harness), "bac");
}

#[test]
fn soft_wrap_renders_each_segment_once() {
    let config = EditorConfig::new().with_wrap(WrapMode::Soft);
    let mut harness = build_harness("abcdefghij", config, 4, 3);
    harness.render().unwrap();
    harness.tbuf().assert_matches(buf!["abcd" "efgh" "ij  "]);
}

#[test]
fn visual_indent_of_three_wrapped_lines_keeps_layout() {
    let config = EditorConfig::new()
        .with_mode(EditMode::Vi)
        .with_wrap(WrapMode::Soft);
    let mut harness = build_harness("aaaaaa\nbbbbbb\ncccccc", config, 6, 6);
    harness.keys(['V', 'j', 'j', '>']).unwrap();
    harness.render().unwrap();
    harness.tbuf().assert_matches(buf![
        "    aa"
        "aaaa  "
        "    bb"
        "bbbb  "
        "    cc"
        "cccc  "
    ]);
}

#[test]
fn set_text_rebuilds_the_layout_cache() {
    let mut harness = build_harness("a", EditorConfig::new(), 10, 4);
    harness.render().unwrap();
    with_editor(&mut harness, |editor| editor.set_text("one\ntwo\nthree"));
    harness.render().unwrap();
    harness
        .tbuf()
        .assert_matches(buf!["one       " "two       " "three     " "          "]);
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
fn vi_delete_line_continues_at_eof_with_scroll() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("one\ntwo\nthree\n", config, 6, 2);
    harness.key('G').unwrap();
    harness.key('k').unwrap();
    harness.keys(['d', 'd']).unwrap();
    assert_eq!(editor_text(&mut harness), "one\ntwo\n");
    harness.keys(['d', 'd']).unwrap();
    assert_eq!(editor_text(&mut harness), "one\ntwo");
    harness.keys(['d', 'd']).unwrap();
    assert_eq!(editor_text(&mut harness), "one");
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(0, 0));
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

fn prepare_replace(harness: &mut Harness, query: &str, replacement: &str) {
    with_editor(harness, |editor| {
        editor.prompt = Some(PromptState::ReplaceConfirm {
            query: query.to_string(),
            replacement: replacement.to_string(),
            matches: find_matches(&editor.buffer, query),
            index: 0,
            replace_all: false,
        });
    });
}

#[test]
fn replace_all_consumes_original_occurrences_once() {
    for (text, query, replacement, expected) in [
        ("a", "a", "aa", "aa"),
        ("aaaa", "aa", "a", "aa"),
        ("aaa", "a", "", ""),
        ("aa", "a", "a", "aa"),
        ("éééé", "éé", "é", "éé"),
        ("aa", "a", "x\ny", "x\nyx\ny"),
        ("abc", "missing", "x", "abc"),
        ("abc", "", "x", "abc"),
    ] {
        let config = EditorConfig::new().with_mode(EditMode::Vi);
        let mut harness = build_harness(text, config, 20, 4);
        prepare_replace(&mut harness, query, replacement);
        harness.key('a').unwrap();
        assert_eq!(editor_text(&mut harness), expected, "{text:?} / {query:?}");
        assert!(with_editor(&mut harness, |editor| editor.prompt.is_none()));
    }
}

#[test]
fn replace_confirmation_preserves_skips_and_forward_order() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("a a a", config, 20, 2);
    prepare_replace(&mut harness, "a", "aa");
    harness.keys(['n', 'y']).unwrap();
    assert_eq!(editor_text(&mut harness), "a aa a");
    assert!(with_editor(&mut harness, |editor| editor.prompt.is_some()));
    harness.key('y').unwrap();
    assert_eq!(editor_text(&mut harness), "a aa aa");
    assert!(with_editor(&mut harness, |editor| editor.prompt.is_none()));

    with_editor(&mut harness, |editor| editor.set_text("a a a"));
    prepare_replace(&mut harness, "a", "");
    harness.keys(['n', 'a']).unwrap();
    assert_eq!(editor_text(&mut harness), "a  ");
    assert!(with_editor(&mut harness, |editor| editor.prompt.is_none()));
}

#[test]
fn replace_confirmation_preserves_read_only_contents_and_selection() {
    for response in ['y', 'a'] {
        let config = EditorConfig::new()
            .with_mode(EditMode::Vi)
            .with_read_only(true);
        let mut harness = build_harness("aaa", config, 10, 2);
        prepare_replace(&mut harness, "a", "aa");
        let selection = editor_selection(&mut harness);
        harness.key(response).unwrap();
        assert_eq!(editor_text(&mut harness), "aaa");
        assert_eq!(editor_selection(&mut harness), selection);
        assert!(with_editor(&mut harness, |editor| editor.prompt.is_none()));
    }
}

#[test]
fn replacement_keeps_single_line_normalization() {
    let config = EditorConfig::new()
        .with_mode(EditMode::Vi)
        .with_multiline(false);
    let mut harness = build_harness("aa", config, 20, 1);
    prepare_replace(&mut harness, "a", "x\ny");
    harness.key('a').unwrap();
    assert_eq!(editor_text(&mut harness), "x yx y");
}

#[test]
fn read_only_history_commands_preserve_text_selection_and_history() {
    for command_path in [false, true] {
        for redo in [false, true] {
            let config = EditorConfig::new().with_mode(EditMode::Vi);
            let mut harness = build_harness("abc", config, 10, 2);
            with_editor(&mut harness, |editor| {
                editor.buffer.insert_text("X");
                if redo {
                    assert!(editor.buffer.undo());
                }
                editor.set_config(editor.config().clone().with_read_only(true));
            });
            let text = editor_text(&mut harness);
            let selection = editor_selection(&mut harness);
            for read_only in [true, false] {
                with_editor(&mut harness, |editor| {
                    editor.set_config(editor.config().clone().with_read_only(read_only));
                });
                if command_path {
                    harness
                        .canopy
                        .eval_script(if redo {
                            "editor.redo()"
                        } else {
                            "editor.undo()"
                        })
                        .unwrap();
                } else if redo {
                    harness.key(key::Ctrl + 'r').unwrap();
                } else {
                    harness.key('u').unwrap();
                }
                if read_only {
                    assert_eq!(editor_text(&mut harness), text);
                    assert_eq!(editor_selection(&mut harness), selection);
                } else {
                    assert_eq!(editor_text(&mut harness), if redo { "Xabc" } else { "abc" });
                    assert_eq!(
                        editor_cursor(&mut harness),
                        TextPosition::new(0, usize::from(redo))
                    );
                }
            }
        }
    }
}

#[test]
fn visual_change_enters_insert_and_undoes_as_one_edit() {
    for (text, keys, expected) in [
        ("abc", vec!['v', 'l'], "Xbc"),
        ("one\ntwo", vec!['V'], "Xtwo"),
    ] {
        let config = EditorConfig::new().with_mode(EditMode::Vi);
        let mut harness = build_harness(text, config, 20, 3);
        harness.keys(keys).unwrap();
        harness.key('c').unwrap();
        assert_eq!(
            with_editor(&mut harness, |editor| editor.vi.mode()),
            ViMode::Insert
        );
        harness.type_text("X").unwrap();
        harness.key(key::KeyCode::Esc).unwrap();
        assert_eq!(editor_text(&mut harness), expected);
        assert_eq!(
            with_editor(&mut harness, |editor| editor.vi.mode()),
            ViMode::Normal
        );
        harness.key('u').unwrap();
        assert_eq!(editor_text(&mut harness), text);
    }
}

#[test]
fn open_below_enters_the_new_line() {
    for (text, expected) in [
        ("one\ntwo", "one\nX\ntwo"),
        ("one", "one\nX"),
        ("one\n", "one\nX\n"),
    ] {
        let config = EditorConfig::new().with_mode(EditMode::Vi);
        let mut harness = build_harness(text, config, 20, 4);
        harness.key('o').unwrap();
        assert_eq!(editor_cursor(&mut harness), TextPosition::new(1, 0));
        harness.type_text("X").unwrap();
        harness.key(key::KeyCode::Esc).unwrap();
        assert_eq!(editor_text(&mut harness), expected);
        harness.key('u').unwrap();
        assert_eq!(editor_text(&mut harness), text);
    }
}

#[test]
fn repeat_empty_open_below_enters_another_new_line() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("one\ntwo", config, 20, 5);
    harness.key('o').unwrap();
    harness.key(key::KeyCode::Esc).unwrap();
    harness.key('.').unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(2, 0));
    harness.type_text("X").unwrap();
    harness.key(key::KeyCode::Esc).unwrap();
    assert_eq!(editor_text(&mut harness), "one\n\nX\ntwo");
}

#[test]
fn linewise_put_preserves_line_boundaries_and_history() {
    for (text, yank, line, before, expected) in [
        ("one", "one", 0, false, "one\none"),
        ("one", "one", 0, true, "one\none"),
        ("one", "two\n", 0, false, "one\ntwo\n"),
        ("one\nthree", "two", 0, false, "one\ntwo\nthree"),
        ("one\nthree", "two\n", 1, true, "one\ntwo\nthree"),
        ("one\n", "two", 1, false, "one\ntwo"),
        ("one\n", "two\n", 1, false, "one\ntwo\n"),
        ("one\n", "two", 1, true, "one\ntwo\n"),
    ] {
        let config = EditorConfig::new().with_mode(EditMode::Vi);
        let mut harness = build_harness(text, config, 20, 5);
        with_editor(&mut harness, |editor| {
            editor.yank = yank.to_string();
            editor.yank_linewise = true;
            editor.buffer.set_cursor(TextPosition::new(line, 0));
        });
        let before_selection = editor_selection(&mut harness);
        harness.key(if before { 'P' } else { 'p' }).unwrap();
        assert_eq!(editor_text(&mut harness), expected);
        let after_selection = editor_selection(&mut harness);
        harness.key('u').unwrap();
        assert_eq!(editor_text(&mut harness), text);
        assert_eq!(editor_selection(&mut harness), before_selection);
        harness.key(key::Ctrl + 'r').unwrap();
        assert_eq!(editor_text(&mut harness), expected);
        assert_eq!(editor_selection(&mut harness), after_selection);
    }
}

#[test]
fn final_line_yank_put_keeps_a_separate_line() {
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut harness = build_harness("one", config, 10, 3);
    harness.keys(['y', 'y', 'p']).unwrap();
    assert_eq!(editor_text(&mut harness), "one\none");
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
fn nested_padding_scroll_and_captured_pointer_agree_on_wide_grapheme() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let text = ["ab界cdefghijklmnopqrstuvwxyz"; 10].join("\n");
    let mut harness = build_harness(&text, config, 18, 8);
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.set_layout(Layout::fill().padding(Edges::all(1)))?;
            ctx.with_child::<EditorSlot, _>(|_editor, ctx| {
                ctx.set_layout(Layout::fill().padding(Edges::all(1)))
            })
        })
        .unwrap();
    harness.render().unwrap();
    scroll_editor_to(&mut harness, 1, 2);
    harness.render().unwrap();
    assert_eq!(editor_view_scroll(&mut harness), Point { x: 1, y: 2 });
    assert_eq!(harness.buf().get(Point { x: 2, y: 2 }).unwrap().ch, 'b');
    assert_eq!(harness.buf().get(Point { x: 3, y: 2 }).unwrap().ch, '界');
    assert!(
        harness
            .buf()
            .get(Point { x: 4, y: 2 })
            .unwrap()
            .continuation
    );
    assert_eq!(harness.buf().get(Point { x: 5, y: 2 }).unwrap().ch, 'c');

    // Screen x=3 is content x=2, the first cell of the wide grapheme.
    harness
        .mouse(mouse_event(mouse::Action::Down, 3, 2))
        .unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(2, 2));
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.with_child::<EditorSlot, _>(|_editor, ctx| ctx.capture_mouse())
        })
        .unwrap();
    // Captured input still uses viewport-local coordinates at the trailing cell.
    harness
        .mouse(mouse_event(mouse::Action::Drag, 4, 2))
        .unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(2, 2));
    // Outside input clamps at the legacy unsigned viewport boundary.
    harness
        .mouse(mouse_event(mouse::Action::Drag, 0, 2))
        .unwrap();
    assert_eq!(editor_cursor(&mut harness), TextPosition::new(2, 1));
    harness.mouse(mouse_event(mouse::Action::Up, 0, 2)).unwrap();
    harness
        .with_root_context(|_root: &mut EditorHost, ctx| {
            ctx.with_child::<EditorSlot, _>(|_editor, ctx| ctx.release_mouse())
        })
        .unwrap();
}

#[test]
fn padded_editor_mouse_uses_content_coordinates_with_gutter_and_scroll() {
    for (numbers, scroll_x, scroll_y) in [
        (LineNumbers::None, 0, 0),
        (LineNumbers::Absolute, 0, 2),
        (LineNumbers::None, 3, 2),
    ] {
        let config = EditorConfig::new()
            .with_wrap(WrapMode::None)
            .with_line_numbers(numbers);
        let text = ["abcdefghijklmnopqrstuvwxyz"; 10].join("\n");
        let mut harness = build_harness(&text, config, 18, 6);
        harness
            .with_root_context(|_root: &mut EditorHost, ctx| {
                ctx.with_child::<EditorSlot, _>(|_editor, ctx| {
                    ctx.set_layout(Layout::fill().padding(Edges::all(1)))
                })
            })
            .unwrap();
        harness.render().unwrap();
        scroll_editor_to(&mut harness, scroll_x, scroll_y);
        harness.render().unwrap();
        let gutter = with_editor(&mut harness, |editor| editor.gutter_width());
        let first_x = 1 + gutter;
        harness
            .mouse(mouse_event(mouse::Action::Down, first_x, 1))
            .unwrap();
        assert_eq!(
            editor_cursor(&mut harness),
            TextPosition::new(scroll_y as usize, scroll_x as usize)
        );
        harness
            .mouse(mouse_event(mouse::Action::Up, first_x, 1))
            .unwrap();
        harness
            .mouse(mouse_event(mouse::Action::Down, first_x + 3, 1))
            .unwrap();
        assert_eq!(
            editor_cursor(&mut harness),
            TextPosition::new(scroll_y as usize, scroll_x as usize + 3)
        );
        harness
            .mouse(mouse_event(mouse::Action::Drag, first_x + 5, 1))
            .unwrap();
        assert_eq!(
            editor_selection(&mut harness).range(),
            TextRange::new(
                TextPosition::new(scroll_y as usize, scroll_x as usize + 3),
                TextPosition::new(scroll_y as usize, scroll_x as usize + 5),
            )
        );
    }
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
fn mouse_wheel_scrolls_editor() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let text = (0..20)
        .map(|idx| format!("line{idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut harness = build_harness(&text, config, 10, 3);
    let start = editor_view_scroll(&mut harness);

    harness
        .mouse(mouse::MouseEvent {
            action: mouse::Action::ScrollDown,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: Point { x: 1, y: 1 },
        })
        .unwrap();
    let after = editor_view_scroll(&mut harness);
    assert!(after.y > start.y);

    harness
        .mouse(mouse::MouseEvent {
            action: mouse::Action::ScrollUp,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: Point { x: 1, y: 1 },
        })
        .unwrap();
    let end = editor_view_scroll(&mut harness);
    assert_eq!(end.y, start.y);
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
    harness
        .canopy
        .eval_script(
            r#"
canopy.bind("x", { path = "editor", description = "Cursor left" }, function()
    editor.cursor("Left")
end)
"#,
        )
        .unwrap();
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
        fg: Paint::solid(Color::Red),
        bg: Paint::solid(Color::Black),
        attrs: AttrSet::default(),
    };
    with_editor(&mut harness, |editor| {
        editor.buffer.set_cursor(TextPosition::new(0, 2));
        editor.set_highlighter(Some(Box::new(TestHighlighter {
            style: style.clone(),
        })));
    });
    harness.render().unwrap();
    let partial = PartialStyle::fg(Color::Red);
    assert!(harness.tbuf().contains_text_style("hi", &partial));
}

#[test]
fn search_current_other_matches_and_syntax_keep_separate_styles() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let mut harness = build_harness("hi a a", config, 10, 1);
    with_editor(&mut harness, |editor| {
        editor.set_highlighter(Some(Box::new(TestHighlighter {
            style: Style {
                fg: Paint::solid(Color::Red),
                bg: Paint::solid(Color::Black),
                attrs: AttrSet::default(),
            },
        })));
        editor
            .search
            .set_query(&editor.buffer, "a", SearchDirection::Forward);
        editor.buffer.set_cursor(TextPosition::new(0, 6));
    });
    harness.render().unwrap();
    let styles = StyleManager::default();
    let current = styles.get(harness.canopy.style(), "editor/search/current");
    let other = styles.get(harness.canopy.style(), "editor/search/match");
    let selected = styles.get(harness.canopy.style(), "editor/selection");
    let buffer = harness.buf();
    assert_eq!(
        buffer.get(Point { x: 0, y: 0 }).unwrap().style.fg,
        Color::Red
    );
    assert_eq!(
        buffer.get(Point { x: 1, y: 0 }).unwrap().style.fg,
        Color::Red
    );
    assert_eq!(
        buffer.get(Point { x: 3, y: 0 }).unwrap().style.bg,
        current.bg.solid_color().unwrap()
    );
    assert_eq!(
        buffer.get(Point { x: 5, y: 0 }).unwrap().style.bg,
        other.bg.solid_color().unwrap()
    );
    assert_ne!(current.bg, other.bg);

    with_editor(&mut harness, |editor| {
        editor.buffer.set_selection(Selection::new(
            TextPosition::new(0, 0),
            TextPosition::new(0, 6),
        ));
    });
    harness.render().unwrap();
    for x in [0, 1, 3, 5] {
        let style = harness.buf().get(Point { x, y: 0 }).unwrap().style;
        assert_eq!(style.fg, selected.fg.solid_color().unwrap());
        assert_eq!(style.bg, selected.bg.solid_color().unwrap());
    }
}

#[test]
fn highlight_spans_inherit_editor_background() {
    let config = EditorConfig::new().with_wrap(WrapMode::None);
    let mut harness = build_harness("hi\nok", config, 4, 2);
    let highlight_style = Style {
        fg: Paint::solid(Color::Green),
        bg: Paint::solid(Color::Red),
        attrs: AttrSet::default(),
    };
    with_editor(&mut harness, |editor| {
        editor.set_highlighter(Some(Box::new(TestHighlighter {
            style: highlight_style,
        })));
    });
    harness.key(key::KeyCode::Down).unwrap();

    let base_bg = StyleManager::default()
        .get(harness.canopy.style(), "editor/text")
        .bg
        .solid_color()
        .expect("editor text background is solid");
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
    harness
        .canopy
        .eval_script(
            r#"
canopy.bind("q", { path = "editor_host", description = "Record binding" }, function()
    editor_host.record_binding()
end)
"#,
        )
        .unwrap();
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
