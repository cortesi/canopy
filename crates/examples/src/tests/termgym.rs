use canopy::{BindingPhase, event::key::Key, prelude::*, testing::harness::Harness};
use canopy_widgets::{Frame, Root, SINGLE_THICK, Terminal};

use crate::termgym::{TermGym, setup_bindings};

/// Build the installed TermGym application with its real bindings.
fn termgym_harness() -> Result<(Harness, NodeId)> {
    let mut canopy = Canopy::new();
    Root::load(&mut canopy)?;
    TermGym::load(&mut canopy)?;
    setup_bindings(&mut canopy)?;
    canopy.finalize_api()?;
    let app = Root::install_app(&mut canopy, TermGym::new())?;
    let mut harness = Harness::from_canopy(canopy, Size::new(80, 24))?;
    harness.render()?;
    Ok((harness, app.into()))
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
        context.node_path(
            context.root_id(),
            context.focused_node().expect("TermGym must own focus"),
        )
    })
}

#[test]
fn installed_termgym_keeps_sidebar_beside_terminal() -> Result<()> {
    let (mut harness, app) = termgym_harness()?;
    harness.canopy.with_root_context(|context| {
        context.with_node(app, |_termgym: &mut TermGym, context| {
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
        let sidebar = context.node_view(*sidebar).expect("sidebar view").outer;
        let terminal = context.node_view(*terminal).expect("terminal view").outer;

        assert_eq!(sidebar.top(), terminal.top());
        assert_eq!(sidebar.h, terminal.h);
        assert_eq!(sidebar.right(), terminal.left());
    });

    let frame = harness.canopy.with_root_view(|context| {
        context
            .children_of(app)
            .get(1)
            .copied()
            .expect("termgym terminal frame")
    });
    harness.with_widget::<Frame, _>(frame, |frame| {
        assert_eq!(frame.glyphs(), &SINGLE_THICK);
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
        .find(|binding| binding.key == Key::parse_spec("F6").expect("valid key"))
        .expect("terminal toggle binding");
    assert_eq!(toggle.description, "Toggle terminal list");
    assert_eq!(toggle.phase, BindingPhase::BeforeWidget);
    for removed in ["ctrl-a", "ctrl-F2", "ctrl-F3", "ctrl-F4"] {
        let removed = Key::parse_spec(removed).expect("valid removed key");
        assert!(
            terminal_bindings
                .iter()
                .all(|binding| binding.key != removed),
            "{removed} must remain available to the terminal"
        );
    }

    harness.key(Key::parse_spec("F6").expect("valid key"))?;
    assert!(!terminal_has_focus(&harness));
    let list_path = focus_path(&harness);
    assert!(list_path.to_string().contains("/list/term_entry"));

    let list_bindings = harness.canopy.available_bindings(None)?.bindings;
    for expected in [
        "n", "Down", "j", "Up", "k", "Enter", "Right", "Delete", "d", "F6",
    ] {
        let expected = Key::parse_spec(expected).expect("valid list key");
        assert!(
            list_bindings.iter().any(|binding| binding.key == expected),
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
