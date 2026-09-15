use canopy::{
    BindingId, BindingOwner, BindingPhase, BindingScope, Loader, NodeId, buf,
    error::Result,
    event::{key, mouse},
    geom::{Point, PointI32, Size},
    help::{AvailableBinding, BindingSnapshot},
    path::Path,
    testing::harness::Harness,
};

use super::{binding_list::BindingList, panel::ControlFooter};

impl Loader for ControlFooter {}

fn binding(
    id: u64,
    key: impl Into<key::Key>,
    description: &str,
    phase: BindingPhase,
) -> AvailableBinding {
    AvailableBinding {
        id: BindingId::from_u64(id),
        key: key.into(),
        description: description.to_string(),
        owner: BindingOwner::Application,
        scope: BindingScope::Default,
        path_filter: String::new(),
        route_path: Path::from("/root/editor"),
        phase,
        command: None,
        source: Some("test".to_string()),
    }
}

fn snapshot(focus: NodeId, bindings: Vec<AvailableBinding>) -> BindingSnapshot {
    BindingSnapshot {
        focus,
        focus_path: Path::from("/root/editor"),
        active_modes: vec!["insert".to_string()],
        transient_mode: None,
        exclusive_group: None,
        bindings,
    }
}

fn list_with(bindings: Vec<AvailableBinding>) -> BindingList {
    let mut list = BindingList::new();
    let focus = canopy::Canopy::new().root_id();
    drop(list.replace_snapshot(Some(snapshot(focus, bindings))));
    list
}

fn harness_with(width: u32, height: u32, bindings: Vec<AvailableBinding>) -> Result<Harness> {
    let mut harness = Harness::builder(BindingList::new())
        .size(width, height)
        .build()?;
    let focus = harness.root;
    harness.with_root_widget::<BindingList, _>(|list| {
        drop(list.replace_snapshot(Some(snapshot(focus, bindings))));
    });
    harness.render()?;
    Ok(harness)
}

#[test]
fn wide_keys_align_descriptions_by_display_columns() {
    let list = list_with(vec![
        binding(1, 'a', "ASCII", BindingPhase::BeforeWidget),
        binding(2, '界', "Wide", BindingPhase::BeforeWidget),
        binding(3, key::Ctrl + 'a', "Modified", BindingPhase::BeforeWidget),
    ]);
    let lines = list.display_lines(60);
    let widths = lines
        .iter()
        .filter_map(|line| line.key.as_deref())
        .map(unicode_width::UnicodeWidthStr::width)
        .collect::<Vec<_>>();
    assert_eq!(widths, [6, 6, 6]);
}

#[test]
fn empty_list_has_one_explicit_row() {
    let list = BindingList::new();
    let lines = list.display_lines(40);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "No key bindings in this context");
}

#[test]
fn rows_sort_by_key_category_without_routing_details() {
    let list = list_with(vec![
        binding(1, key::Ctrl + 'a', "Modified", BindingPhase::BeforeWidget),
        binding(
            2,
            key::Key::parse_spec("Down").expect("valid key"),
            "Arrow",
            BindingPhase::BeforeWidget,
        ),
        binding(3, '2', "Digit", BindingPhase::BeforeWidget),
        binding(4, 'B', "Upper", BindingPhase::BeforeWidget),
        binding(5, 'a', "Lower", BindingPhase::BeforeWidget),
        binding(6, 'z', "Fallback", BindingPhase::AfterWidget),
    ]);
    let lines = list.display_lines(60);
    let keys = lines
        .iter()
        .filter_map(|line| line.key.as_deref().map(str::trim))
        .collect::<Vec<_>>();
    assert_eq!(keys, ["a", "z", "B", "2", "↓", "Ctrl+a"]);
    assert_eq!(lines.len(), 6);
}

#[test]
fn narrow_and_long_rows_use_indented_wrapped_continuations() {
    let list = list_with(vec![binding(
        1,
        key::Ctrl + key::Shift + 'x',
        "A deliberately long description that must wrap safely",
        BindingPhase::BeforeWidget,
    )]);
    let lines = list.display_lines(12);
    assert_eq!(lines[0].text, "Ctrl+Shift+x");
    assert!(lines[1].text.starts_with("  "));
    assert!(lines.len() > 3);
    assert!(lines.iter().all(|line| line.key.is_none()));
}

#[test]
fn normal_and_empty_buffers_are_stable() -> Result<()> {
    let normal = harness_with(
        32,
        5,
        vec![
            binding(1, 'a', "Alpha", BindingPhase::BeforeWidget),
            binding(2, 'b', "Beta", BindingPhase::AfterWidget),
        ],
    )?;
    normal.tbuf().assert_matches(buf![
        "a  Alpha"
        "b  Beta"
        ""
        ""
        ""
    ]);

    let empty = harness_with(32, 2, Vec::new())?;
    empty
        .tbuf()
        .assert_matches(buf!["No key bindings in this context" ""]);
    Ok(())
}

#[test]
fn tiny_and_wide_key_buffers_do_not_overflow() -> Result<()> {
    let tiny = harness_with(
        1,
        1,
        vec![binding(1, 'a', "Alpha", BindingPhase::BeforeWidget)],
    )?;
    tiny.tbuf().assert_matches(buf!["█"]);

    let wide = harness_with(
        12,
        3,
        vec![binding(1, '界', "Wide key", BindingPhase::BeforeWidget)],
    )?;
    wide.tbuf().assert_matches(buf!["界X" "  Wide key" ""]);
    Ok(())
}

#[test]
fn narrow_long_buffer_wraps_to_exact_rows() -> Result<()> {
    let narrow = harness_with(
        12,
        4,
        vec![binding(
            1,
            key::Ctrl + key::Shift + 'x',
            "Long text wraps here",
            BindingPhase::BeforeWidget,
        )],
    )?;
    narrow
        .tbuf()
        .assert_matches(buf!["Ctrl+Shift+x" "  Long text" "  wraps here" ""]);
    Ok(())
}

#[test]
fn scrolled_and_resized_buffers_have_exact_rows() -> Result<()> {
    let bindings = vec![
        binding(1, 'a', "Alpha", BindingPhase::BeforeWidget),
        binding(2, 'b', "Beta", BindingPhase::BeforeWidget),
        binding(3, 'c', "Gamma", BindingPhase::BeforeWidget),
        binding(4, 'd', "Delta", BindingPhase::BeforeWidget),
    ];
    let mut harness = harness_with(16, 3, bindings)?;
    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::ScrollDown,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: PointI32 { x: 0, y: 0 },
    })?;
    harness
        .tbuf()
        .assert_matches(buf!["  Alpha        █" "b              █" "  Beta"]);

    harness.canopy.set_root_size(Size::new(16, 8))?;
    harness.render()?;
    harness.tbuf().assert_matches(buf![
        "a"
        "  Alpha"
        "b"
        "  Beta"
        "c"
        "  Gamma"
        "d"
        "  Delta"
    ]);
    Ok(())
}

#[test]
fn wheel_indicator_and_resize_keep_scroll_within_the_exact_canvas() -> Result<()> {
    let bindings = (0..12)
        .map(|index| {
            binding(
                index + 1,
                char::from(b'a' + index as u8),
                "A wrapped description",
                BindingPhase::BeforeWidget,
            )
        })
        .collect();
    let mut harness = harness_with(10, 4, bindings)?;

    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::ScrollDown,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: PointI32 { x: 0, y: 0 },
    })?;
    let after_wheel = harness
        .canopy
        .with_root_view(|context| context.view_of(harness.root).expect("list view"));
    assert_eq!(after_wheel.scroll.y, 1);

    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: PointI32 { x: 9, y: 3 },
    })?;
    let after_click = harness
        .canopy
        .with_root_view(|context| context.view_of(harness.root).expect("list view"));
    assert_eq!(
        after_click.scroll.y,
        after_click.canvas.h.saturating_sub(after_click.content.h)
    );

    harness.canopy.set_root_size(Size::new(10, 40))?;
    harness.render()?;
    let resized = harness
        .canopy
        .with_root_view(|context| context.view_of(harness.root).expect("list view"));
    assert_eq!(
        resized.scroll.y,
        resized.canvas.h.saturating_sub(resized.content.h)
    );
    Ok(())
}

#[test]
fn scrolling_reserves_a_gutter_instead_of_overwriting_action_text() -> Result<()> {
    let harness = harness_with(
        32,
        2,
        vec![
            binding(
                1,
                'a',
                "12345678901234567890123456789",
                BindingPhase::BeforeWidget,
            ),
            binding(2, 'b', "Another action", BindingPhase::BeforeWidget),
            binding(3, 'c', "Last action", BindingPhase::BeforeWidget),
        ],
    )?;
    for y in 0..2 {
        assert_eq!(
            harness
                .buf()
                .get(Point { x: 30, y })
                .unwrap()
                .rendered_text(),
            " "
        );
    }
    harness.tbuf().assert_matches(buf![
        "a  123456789012345678901234567 █"
        "   89"
    ]);
    Ok(())
}

#[test]
fn footer_groups_navigation_and_keeps_close_guide_visible() -> Result<()> {
    let mut wide = Harness::builder(ControlFooter::new()).size(70, 1).build()?;
    wide.render()?;
    assert!(wide.tbuf().contains_text("↑/↓ Scroll"));
    assert!(wide.tbuf().contains_text("PgUp/PgDn Page"));
    assert!(wide.tbuf().contains_text("Home/End First/last"));
    assert!(wide.tbuf().contains_text("Esc Close"));
    let key_style = wide
        .buf()
        .get(Point { x: 0, y: 0 })
        .expect("first footer key")
        .style;
    let label_style = wide
        .buf()
        .get(Point { x: 5, y: 0 })
        .expect("first footer label")
        .style;
    assert!(key_style.attrs.bold);
    assert!(!label_style.attrs.bold);
    assert_ne!(key_style.fg, label_style.fg);

    let mut narrow = Harness::builder(ControlFooter::new()).size(20, 1).build()?;
    narrow.render()?;
    narrow.tbuf().assert_matches(buf!["           Esc Close"]);
    for x in 0..70 {
        assert_eq!(
            wide.buf().get(Point { x, y: 0 }).unwrap().style.bg,
            key_style.bg,
            "footer background must cover gaps at column {x}"
        );
    }
    Ok(())
}

#[test]
fn command_help_keeps_user_feedback_without_command_diagnostics() {
    use canopy::{
        commands::{
            ArgValue, CommandAction, CommandArgs, CommandId, CommandInvocation, CommandRequirement,
            CommandStatus, CommandTarget,
        },
        help::BindingCommand,
    };

    let mut binding = binding(1, 'd', "Delete selection", BindingPhase::BeforeWidget);
    binding.command = Some(BindingCommand {
        action: CommandAction {
            invocation: CommandInvocation {
                id: CommandId("todo::delete_item"),
                args: CommandArgs::Positional(vec![ArgValue::Int(7)]),
            },
            target: Some(CommandTarget::Focus),
        },
        resolution: None,
        status: Some(CommandStatus::Disabled("no selection".into())),
        missing_requirements: vec![CommandRequirement::ListRow],
    });
    let list = list_with(vec![binding.clone()]);
    let text = list
        .display_lines(100)
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(text, "Delete selection — Unavailable: no selection");
    // Input dispatch supplies event/row context that passive discovery lacks.
    binding.command.as_mut().unwrap().status = Some(CommandStatus::Enabled);
    let list = list_with(vec![binding]);
    assert_eq!(list.display_lines(100)[0].text, "Delete selection");
}

/// Return each display line as its trimmed key and its text.
fn rows(list: &BindingList, width: u32) -> Vec<(Option<String>, String)> {
    list.display_lines(width)
        .into_iter()
        .map(|line| (line.key.map(|key| key.trim().to_string()), line.text))
        .collect()
}

#[test]
fn bindings_with_one_action_share_a_row() {
    let down = key::Key::parse_spec("Down").expect("valid key");
    let list = list_with(vec![
        binding(1, 'j', "Next entry", BindingPhase::BeforeWidget),
        binding(2, down, "Next entry", BindingPhase::BeforeWidget),
        binding(3, 'j', "Next entry", BindingPhase::AfterWidget),
        binding(4, 'k', "Previous entry", BindingPhase::BeforeWidget),
    ]);
    assert_eq!(
        rows(&list, 60),
        [
            (Some("j ↓".to_string()), "Next entry".to_string()),
            (Some("k".to_string()), "Previous entry".to_string()),
        ]
    );
}

#[test]
fn long_key_runs_continue_on_rows_below_the_action() {
    let spec = |text: &str| key::Key::parse_spec(text).expect("valid key");
    let list = list_with(vec![
        binding(1, spec("PageDown"), "Page down", BindingPhase::BeforeWidget),
        binding(2, ' ', "Page down", BindingPhase::BeforeWidget),
        binding(
            3,
            spec("shift-PageDown"),
            "Page down",
            BindingPhase::BeforeWidget,
        ),
    ]);
    assert_eq!(
        rows(&list, 60),
        [
            (Some("PageDown Space".to_string()), "Page down".to_string()),
            (Some("Shift+PageDown".to_string()), String::new()),
        ]
    );
}

#[test]
fn arrow_keys_show_as_single_arrows() {
    let list = list_with(vec![
        binding(
            1,
            key::Key::parse_spec("Left").expect("valid key"),
            "Back",
            BindingPhase::BeforeWidget,
        ),
        binding(
            2,
            key::Ctrl + key::KeyCode::Right,
            "Forward",
            BindingPhase::BeforeWidget,
        ),
    ]);
    let keys = rows(&list, 60)
        .into_iter()
        .filter_map(|(key, _)| key)
        .collect::<Vec<_>>();
    assert_eq!(keys, ["←", "Ctrl+→"]);
}

#[test]
fn arrows_keep_a_blank_cell_for_glyphs_wider_than_one() {
    let left = key::Key::parse_spec("Left").expect("valid key");
    let list = list_with(vec![
        binding(1, 'h', "Previous tab", BindingPhase::BeforeWidget),
        binding(2, left, "Previous tab", BindingPhase::BeforeWidget),
        binding(3, '[', "Previous tab", BindingPhase::BeforeWidget),
    ]);
    let lines = list.display_lines(60);
    assert_eq!(lines[0].key.as_deref(), Some("h ←  ["));
}

#[test]
fn scroll_thumb_spans_the_visible_fraction() -> Result<()> {
    let bindings = (0..8)
        .map(|index| {
            binding(
                index + 1,
                char::from(b'a' + index as u8),
                &format!("Action {index}"),
                BindingPhase::BeforeWidget,
            )
        })
        .collect();
    let mut harness = harness_with(40, 4, bindings)?;
    let thumb_rows = |harness: &Harness| {
        (0..4)
            .filter(|&y| harness.buf().get(Point { x: 39, y }).unwrap().ch == '█')
            .collect::<Vec<_>>()
    };
    // Eight rows through a four-row view: the thumb covers half the track.
    assert_eq!(thumb_rows(&harness), [0, 1]);

    harness.with_root_context(|list: &mut BindingList, context| {
        list.scroll_to_bottom(context);
        Ok(())
    })?;
    harness.render()?;
    assert_eq!(thumb_rows(&harness), [2, 3]);
    Ok(())
}
