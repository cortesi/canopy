use canopy::{
    BindingId, BindingOwner, BindingPhase, BindingScope, Loader, NodeId, Path, buf,
    error::Result,
    event::{key, mouse},
    geom::{Point, Size},
    help::{AvailableBinding, BindingSnapshot},
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
        source: Some("test".to_string()),
    }
}

fn snapshot(focus: NodeId, bindings: Vec<AvailableBinding>) -> BindingSnapshot {
    BindingSnapshot {
        focus,
        focus_path: Path::from("/root/editor"),
        active_modes: vec!["insert".to_string()],
        exclusive_group: None,
        bindings,
    }
}

fn list_with(bindings: Vec<AvailableBinding>) -> BindingList {
    let mut list = BindingList::new();
    let focus = canopy::Canopy::new().root_id();
    list.set_snapshot(snapshot(focus, bindings));
    list
}

fn harness_with(width: u32, height: u32, bindings: Vec<AvailableBinding>) -> Result<Harness> {
    let mut harness = Harness::builder(BindingList::new())
        .size(width, height)
        .build()?;
    let focus = harness.root;
    harness.with_root_widget::<BindingList, _>(|list| {
        list.set_snapshot(snapshot(focus, bindings));
    });
    harness.render()?;
    Ok(harness)
}

#[test]
fn empty_list_has_one_explicit_row() {
    let list = BindingList::new();
    let lines = list.display_lines(40);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "No key bindings in this context");
}

#[test]
fn rows_sort_by_key_category_and_split_fallbacks() {
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
        binding(6, 'z', "Fallback", BindingPhase::AfterIgnore),
    ]);
    let lines = list.display_lines(60);
    let keys = lines
        .iter()
        .filter_map(|line| line.key.as_deref().map(str::trim))
        .collect::<Vec<_>>();
    assert_eq!(keys, ["a", "B", "2", "Down", "Ctrl+a", "z"]);
    assert!(lines.iter().any(|line| {
        line.text == "When the focused widget does not handle the key"
            && line.style == "help/fallback"
    }));
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
            binding(2, 'b', "Beta", BindingPhase::AfterIgnore),
        ],
    )?;
    normal.tbuf().assert_matches(buf![
        "a  Alpha"
        ""
        "When the focused widget does not"
        "handle the key"
        "b  Beta"
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
        location: Point { x: 0, y: 0 },
    })?;
    harness
        .tbuf()
        .assert_matches(buf!["  Alpha        █" "b" "  Beta"]);

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
        location: Point { x: 0, y: 0 },
    })?;
    let after_wheel = harness
        .canopy
        .with_root_view(|context| context.node_view(harness.root).expect("list view"));
    assert_eq!(after_wheel.tl.y, 1);

    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: Point { x: 9, y: 3 },
    })?;
    let after_click = harness
        .canopy
        .with_root_view(|context| context.node_view(harness.root).expect("list view"));
    assert_eq!(
        after_click.tl.y,
        after_click.canvas.h.saturating_sub(after_click.content.h)
    );

    harness.canopy.set_root_size(Size::new(10, 40))?;
    harness.render()?;
    let resized = harness
        .canopy
        .with_root_view(|context| context.node_view(harness.root).expect("list view"));
    assert_eq!(
        resized.tl.y,
        resized.canvas.h.saturating_sub(resized.content.h)
    );
    Ok(())
}

#[test]
fn footer_groups_navigation_and_keeps_close_guide_visible() -> Result<()> {
    let mut wide = Harness::builder(ControlFooter::new()).size(70, 1).build()?;
    wide.render()?;
    assert!(wide.tbuf().contains_text("Up/k Down/j scroll"));
    assert!(wide.tbuf().contains_text("PgUp/PgDn page"));
    assert!(wide.tbuf().contains_text("Home/End jump"));
    assert!(wide.tbuf().contains_text("?/Esc close"));
    let key_style = wide
        .buf()
        .get(Point { x: 0, y: 0 })
        .expect("first footer key")
        .style;
    let label_style = wide
        .buf()
        .get(Point { x: 12, y: 0 })
        .expect("first footer label")
        .style;
    assert!(key_style.attrs.bold);
    assert!(!label_style.attrs.bold);
    assert_ne!(key_style.fg, label_style.fg);

    let mut narrow = Harness::builder(ControlFooter::new()).size(20, 1).build()?;
    narrow.render()?;
    narrow.tbuf().assert_matches(buf!["         ?/Esc close"]);
    Ok(())
}
