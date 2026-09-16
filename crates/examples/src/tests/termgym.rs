use canopy::{
    BindingPhase, ContextExt, NodeId, RoutePhase, ViewContextExt, error::Result, event::key::Key,
    geom::Size, path::Path, testing::harness::Harness,
};
use canopy_widgets::terminal::Terminal;

use super::{Mount, root_harness};
use crate::termgym::{TermGym, binding_setup};

/// Build the installed TermGym application with its real bindings.
fn termgym_harness() -> Result<(Harness, NodeId)> {
    let harness = root_harness(
        TermGym::new(),
        binding_setup,
        Size::new(80, 24),
        Mount::Replace,
    )?;
    let app = harness.canopy.with_root_view(|context| context.root_id());
    Ok((harness, app))
}

/// Return true when a terminal emulator owns focus.
fn terminal_has_focus(harness: &Harness) -> bool {
    harness
        .canopy
        .with_root_view(|context| context.focused_descendant::<Terminal>().is_some())
}

/// Return the number of terminal emulators in the stack.
fn terminal_count(harness: &Harness) -> usize {
    harness
        .canopy
        .with_root_view(|context| context.descendants_of_type::<Terminal>().len())
}

/// Return the current root-relative focus path.
fn focus_path(harness: &Harness) -> Path {
    harness.canopy.with_root_view(|context| {
        context.path_of(
            context.root_id(),
            context.focused_node().expect("TermGym must own focus"),
        )
    })
}

#[test]
fn installed_termgym_keeps_sidebar_beside_terminal() -> Result<()> {
    let (mut harness, app) = termgym_harness()?;
    harness.canopy.with_root_context(|context| {
        context.with_widget_mut(app, |_termgym: &mut TermGym, context| {
            context.invalidate_layout();
            Ok(())
        })
    })?;
    harness.render()?;

    harness.canopy.with_root_view(|context| {
        let children = context.children_of(app);
        let [sidebar, terminal] = children.as_slice() else {
            panic!("termgym must have sidebar and terminal children");
        };
        let sidebar = context.view_of(*sidebar).expect("sidebar view").outer;
        let terminal = context.view_of(*terminal).expect("terminal view").outer;

        assert_eq!(sidebar.top(), terminal.top());
        assert_eq!(sidebar.h, terminal.h);
        assert_eq!(sidebar.right(), terminal.left());
    });

    Ok(())
}

#[test]
fn f6_toggles_terminal_focus_without_stealing_shell_shortcuts() -> Result<()> {
    let (mut harness, _app) = termgym_harness()?;
    assert!(
        terminal_has_focus(&harness),
        "initial focus: {}",
        focus_path(&harness)
    );

    let terminal_bindings = harness.canopy.available_bindings(None)?.bindings;
    let toggle = terminal_bindings
        .iter()
        .find(|binding| binding.input == Key::parse_spec("F6").expect("valid key"))
        .expect("terminal toggle binding");
    assert_eq!(toggle.description, "Toggle terminal list");
    assert_eq!(toggle.phase, BindingPhase::BeforeWidget);
    assert_eq!(
        toggle
            .command
            .as_ref()
            .expect("declarative toggle")
            .action
            .invocation
            .id
            .0,
        "term_gym::toggle_terminal_focus"
    );
    for removed in ["ctrl-a", "ctrl-F2", "ctrl-F3", "ctrl-F4"] {
        let removed = Key::parse_spec(removed).expect("valid removed key");
        assert!(
            terminal_bindings
                .iter()
                .all(|binding| binding.input != removed),
            "{removed} must remain available to the terminal"
        );
    }

    harness.key(Key::parse_spec("F6").expect("valid key"))?;
    assert!(!terminal_has_focus(&harness));
    assert!(
        harness
            .canopy
            .route_trace()
            .iter()
            .any(|entry| entry.phase == RoutePhase::PreEventBinding)
    );
    assert!(
        !harness
            .canopy
            .route_trace()
            .iter()
            .any(|entry| entry.phase == RoutePhase::WidgetEvent)
    );
    let list_path = focus_path(&harness);
    assert!(list_path.to_string().contains("/list/term_entry"));

    let list_bindings = harness.canopy.available_bindings(None)?.bindings;
    for expected in [
        "n", "Down", "j", "Up", "k", "Enter", "Right", "Delete", "d", "F6",
    ] {
        let expected = Key::parse_spec(expected).expect("valid list key");
        assert!(
            list_bindings
                .iter()
                .any(|binding| binding.input == expected),
            "{expected} must be available in the terminal list"
        );
    }

    harness.key(Key::parse_spec("Delete").expect("valid key"))?;
    assert_eq!(terminal_count(&harness), 1);
    assert!(!terminal_has_focus(&harness));
    harness.key('n')?;
    assert_eq!(terminal_count(&harness), 2);
    assert!(!terminal_has_focus(&harness));
    harness.key(Key::parse_spec("Delete").expect("valid key"))?;
    assert_eq!(terminal_count(&harness), 1);
    assert!(!terminal_has_focus(&harness));

    harness.key(Key::parse_spec("Right").expect("valid key"))?;
    assert!(terminal_has_focus(&harness));
    harness.key(Key::parse_spec("F6").expect("valid key"))?;
    assert!(!terminal_has_focus(&harness));
    harness.key(Key::parse_spec("Enter").expect("valid key"))?;
    assert!(terminal_has_focus(&harness));
    Ok(())
}
