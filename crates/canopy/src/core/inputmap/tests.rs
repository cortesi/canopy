use super::*;
use crate::{
    commands::{CommandArgs, CommandId, CommandInvocation, CommandTarget},
    core::id::testing_node_id,
    error::Result,
    event::key,
};

const HELP: FrameworkBindingGroup = FrameworkBindingGroup::new("root.help");
const OTHER: FrameworkBindingGroup = FrameworkBindingGroup::new("other.modal");

fn script(id: u64) -> LuauFunctionId {
    LuauFunctionId::for_test(id)
}

fn command(id: &'static str) -> CommandInvocation {
    CommandInvocation {
        id: CommandId(id),
        args: CommandArgs::default(),
    }
}

fn bind_framework(
    map: &mut InputMap,
    group: FrameworkBindingGroup,
    input: impl Into<InputSpec>,
    path: &str,
    description: &str,
    command: CommandInvocation,
) -> Result<BindingId> {
    map.bind_framework(
        group,
        input,
        BindingOptions {
            path: Some(path.parse()?),
            scope: BindingScope::Exclusive(group),
            description: description.to_string(),
            source: None,
            phase: BindingPhase::AfterWidget,
        },
        CommandAction {
            invocation: command,
            target: None,
        },
    )
}

fn bind(
    map: &mut InputMap,
    scope: BindingScope,
    key: impl Into<Key>,
    path: &str,
    description: &str,
    target: u64,
) -> Result<BindingId> {
    map.replace_application_action(
        InputSpec::Key(key.into()),
        BindingOptions {
            scope,
            path: if path.is_empty() {
                None
            } else {
                Some(path.parse()?)
            },
            description: description.to_string(),
            source: Some("test:1".to_string()),
            phase: BindingPhase::AfterWidget,
        },
        BindingTarget::Script(script(target)),
    )
    .map(|(id, _)| id)
}

fn target(map: &InputMap, path: &str, key: impl Into<Key>) -> Option<BindingTarget> {
    map.resolve_match(&Path::from(path), InputSpec::Key(key.into()))
        .map(|binding| binding.target)
}

#[test]
fn normalized_keys_share_one_registry_slot() -> Result<()> {
    let mut map = InputMap::new();
    bind(
        &mut map,
        BindingScope::Default,
        key::Shift + 'a',
        "",
        "Shifted A",
        1,
    )?;
    assert_eq!(
        target(&map, "/root", 'A'),
        Some(BindingTarget::Script(script(1)))
    );
    assert_eq!(
        target(&map, "/root", key::Shift + 'A'),
        Some(BindingTarget::Script(script(1)))
    );
    Ok(())
}

#[test]
fn resolution_uses_global_then_newest_mode_then_default() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'a', "", "Default", 1)?;
    bind(
        &mut map,
        BindingScope::Mode("normal".to_string()),
        'a',
        "",
        "Normal",
        2,
    )?;
    bind(
        &mut map,
        BindingScope::Mode("modal".to_string()),
        'a',
        "",
        "Modal",
        3,
    )?;
    bind(&mut map, BindingScope::Global, '?', "/root/**/", "Help", 4)?;

    map.push_mode("normal");
    map.push_mode("modal");
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(3)))
    );
    assert_eq!(
        target(&map, "/root/editor", '?'),
        Some(BindingTarget::Script(script(4)))
    );
    assert_eq!(map.active_modes(), vec!["modal", "normal"]);

    map.pop_mode();
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(2)))
    );
    map.pop_mode();
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(1)))
    );
    Ok(())
}

#[test]
fn path_score_then_latest_insertion_selects_the_winner() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'a', "editor", "Loose", 1)?;
    bind(
        &mut map,
        BindingScope::Default,
        'a',
        "editor/",
        "Anchored",
        2,
    )?;
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(2)))
    );

    bind(&mut map, BindingScope::Default, 'a', "editor/", "Latest", 3)?;
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(3)))
    );
    Ok(())
}

#[test]
fn global_bindings_require_both_path_anchors() {
    for path in ["root/**/", "/root/**", "root/**"] {
        let mut map = InputMap::new();
        assert!(bind(&mut map, BindingScope::Global, '?', path, "Help", 1).is_err());
    }
}

#[test]
fn framework_registration_is_idempotent_and_rejects_conflicts() -> Result<()> {
    let mut map = InputMap::new();
    let first = bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Scroll down",
        command("binding_list::scroll_down"),
    )?;
    let second = bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Scroll down",
        command("binding_list::scroll_down"),
    )?;
    assert_eq!(first, second);
    assert!(
        bind_framework(
            &mut map,
            HELP,
            InputSpec::Key('j'.into()),
            "/root/help/**/",
            "Different",
            command("binding_list::scroll_down"),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn modal_bindings_block_all_application_tiers() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'j', "", "Application", 1)?;
    bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    bind_framework(
        &mut map,
        OTHER,
        InputSpec::Key('x'.into()),
        "/root/other/**/",
        "Other",
        command("other::close"),
    )?;

    map.set_modal_bindings(Some(ModalBindings::Framework(HELP)));
    assert_eq!(
        target(&map, "/root/help/binding_list", 'j'),
        Some(BindingTarget::Command(CommandAction {
            invocation: command("binding_list::scroll_down"),
            target: None
        }))
    );
    assert_eq!(target(&map, "/root/help/binding_list", 'x'), None);
    map.set_modal_bindings(Some(ModalBindings::Framework(OTHER)));
    assert_eq!(target(&map, "/root/help/binding_list", 'j'), None);
    map.set_modal_bindings(None);
    assert_eq!(map.active_exclusive_group(), None);
    assert_eq!(
        target(&map, "/root/help/binding_list", 'j'),
        Some(BindingTarget::Script(script(1)))
    );
    Ok(())
}

#[test]
fn a_transient_mode_hides_older_modes_and_the_default_scope() -> Result<()> {
    let mut map = InputMap::new();
    let default = bind(&mut map, BindingScope::Default, 'a', "", "Default", 1)?;
    bind(
        &mut map,
        BindingScope::Mode("normal".to_string()),
        'b',
        "",
        "Normal",
        2,
    )?;
    bind(
        &mut map,
        BindingScope::Mode("prefix".to_string()),
        'c',
        "",
        "Prefix",
        3,
    )?;
    bind(&mut map, BindingScope::Global, '?', "/root/**/", "Help", 4)?;

    map.push_mode("normal");
    let generation = map.mode_generation();
    map.push_transient_mode("prefix");
    assert_ne!(map.mode_generation(), generation);
    assert_eq!(map.transient_mode(), Some("prefix"));
    assert_eq!(map.current_mode(), "prefix");
    assert_eq!(
        target(&map, "/root/editor", 'c'),
        Some(BindingTarget::Script(script(3)))
    );
    assert_eq!(
        target(&map, "/root/editor", '?'),
        Some(BindingTarget::Script(script(4))),
        "global bindings still win"
    );
    assert_eq!(target(&map, "/root/editor", 'b'), None);
    assert_eq!(target(&map, "/root/editor", 'a'), None);
    assert_eq!(
        map.diagnostic_state(default, &[Path::from("/root/editor")]),
        "blocked by transient mode prefix"
    );

    map.pop_mode();
    assert_eq!(map.transient_mode(), None);
    assert_eq!(
        target(&map, "/root/editor", 'b'),
        Some(BindingTarget::Script(script(2)))
    );
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(1)))
    );
    Ok(())
}

#[test]
fn application_mutation_cannot_remove_framework_records() -> Result<()> {
    let mut map = InputMap::new();
    let app = bind(&mut map, BindingScope::Default, 'a', "", "App", 1)?;
    let framework = bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    assert_eq!(map.unbind(app)?, Some(BindingTarget::Script(script(1))));
    assert!(map.unbind(framework).is_err());
    map.clear_application();
    assert_eq!(map.bindings().len(), 1);
    assert_eq!(map.bindings()[0].id, framework);
    Ok(())
}

#[test]
fn startup_restore_preserves_framework_records_and_modal_bindings() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'a', "", "Before", 1)?;
    let snapshot = map.snapshot_application();
    bind(&mut map, BindingScope::Default, 'b', "", "Transient", 2)?;
    bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    map.set_modal_bindings(Some(ModalBindings::Framework(HELP)));

    map.restore_application(snapshot);
    assert_eq!(map.bindings().len(), 2);
    assert_eq!(map.active_exclusive_group(), Some(HELP));
    Ok(())
}

#[test]
fn identifier_exhaustion_does_not_replace_an_existing_binding() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'a', "", "Existing", 1)?;
    map.next_id = u64::MAX;
    assert!(bind(&mut map, BindingScope::Default, 'a', "", "New", 2).is_err());
    assert_eq!(
        target(&map, "/root", 'a'),
        Some(BindingTarget::Script(script(1)))
    );
    Ok(())
}

#[test]
fn replacement_is_scoped_by_input_scope_and_exact_path() -> Result<()> {
    let mut map = InputMap::new();
    let old = bind(&mut map, BindingScope::Default, 'a', "editor/", "Old", 1)?;
    let mode = bind(
        &mut map,
        BindingScope::Mode("insert".to_string()),
        'a',
        "editor/",
        "Mode",
        2,
    )?;
    let new = bind(&mut map, BindingScope::Default, 'a', "editor/", "New", 3)?;

    assert!(map.binding(old).is_none());
    assert!(map.binding(mode).is_some());
    assert!(map.binding(new).is_some());
    assert_eq!(map.bindings().len(), 2);
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingTarget::Script(script(3)))
    );
    Ok(())
}

#[test]
fn diagnostics_distinguish_scope_path_insertion_route_and_exclusive_causes() -> Result<()> {
    let mut map = InputMap::new();
    let earlier_route = bind(
        &mut map,
        BindingScope::Default,
        'a',
        "/root/editor/",
        "Earlier route",
        1,
    )?;
    let later_route = bind(
        &mut map,
        BindingScope::Default,
        'a',
        "/root/",
        "Later route",
        2,
    )?;
    let insertion_loser = bind(
        &mut map,
        BindingScope::Default,
        'b',
        "*/editor/",
        "First",
        3,
    )?;
    let insertion_winner = bind(
        &mut map,
        BindingScope::Default,
        'b',
        "/root/*/",
        "Second",
        4,
    )?;
    let path_loser = bind(&mut map, BindingScope::Default, 'c', "*/", "Loose", 5)?;
    let path_winner = bind(
        &mut map,
        BindingScope::Default,
        'c',
        "editor/",
        "Anchored",
        6,
    )?;
    let default = bind(&mut map, BindingScope::Default, 'd', "", "Default", 7)?;
    let global = bind(
        &mut map,
        BindingScope::Global,
        'd',
        "/root/**/",
        "Global",
        8,
    )?;
    let unmatched = bind(&mut map, BindingScope::Default, 'e', "other/", "Other", 9)?;
    let route = [Path::from("/root/editor"), Path::from("/root")];

    assert_eq!(map.diagnostic_state(earlier_route, &route), "effective");
    assert_eq!(
        map.diagnostic_state(later_route, &route),
        "shadowed at an earlier route node"
    );
    assert_eq!(
        map.diagnostic_state(insertion_loser, &route),
        "shadowed by later insertion"
    );
    assert_eq!(map.diagnostic_state(insertion_winner, &route), "effective");
    assert_eq!(
        map.diagnostic_state(path_loser, &route),
        "shadowed by a more specific path"
    );
    assert_eq!(map.diagnostic_state(path_winner, &route), "effective");
    assert_eq!(
        map.diagnostic_state(default, &route),
        "shadowed by a higher-priority scope"
    );
    assert_eq!(map.diagnostic_state(global, &route), "effective");
    assert_eq!(
        map.diagnostic_state(unmatched, &route),
        "path does not match route"
    );

    bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    map.set_modal_bindings(Some(ModalBindings::Framework(HELP)));
    assert_eq!(
        map.diagnostic_state(global, &route),
        "blocked by exclusive group root.help"
    );
    map.set_modal_bindings(None);
    Ok(())
}

fn options(path: &str, phase: BindingPhase) -> BindingOptions {
    BindingOptions {
        path: if path.is_empty() {
            None
        } else {
            Some(path.parse().expect("valid path filter"))
        },
        scope: BindingScope::Default,
        description: "Test action".to_string(),
        source: None,
        phase,
    }
}

#[test]
fn explicit_phase_is_independent_of_selector() -> Result<()> {
    let mut map = InputMap::new();
    let input = InputSpec::Key('x'.into());
    for path in ["editor", "editor/"] {
        for phase in [BindingPhase::BeforeWidget, BindingPhase::AfterWidget] {
            map.clear_application();
            let (id, _) = map.replace_application_action(
                input,
                options(path, phase),
                BindingTarget::Script(script(1)),
            )?;
            assert_eq!(map.binding(id).unwrap().phase, phase);
            let resolved = map
                .resolve_match(&Path::from("/root/editor"), input)
                .unwrap();
            assert_eq!(resolved.phase, phase);
        }
    }
    Ok(())
}

#[test]
fn omitted_phase_is_after_widget() -> Result<()> {
    let mut map = InputMap::new();
    for (path, route) in [
        ("editor", "/root/editor"),
        ("editor", "/root/editor/child"),
        ("editor/", "/root/editor"),
        ("", "/root/editor"),
    ] {
        map.clear_application();
        bind(
            &mut map,
            BindingScope::Default,
            'x',
            path,
            "Default phase",
            1,
        )?;
        let resolved = map
            .resolve_match(&Path::from(route), InputSpec::Key('x'.into()))
            .unwrap();
        assert_eq!(resolved.phase, BindingPhase::AfterWidget);
    }
    Ok(())
}

#[test]
fn a_mouse_binding_takes_either_phase() -> Result<()> {
    let mut map = InputMap::new();
    let input = InputSpec::Mouse(Mouse::parse_spec("ScrollUp").unwrap());
    for phase in [BindingPhase::BeforeWidget, BindingPhase::AfterWidget] {
        map.clear_application();
        map.replace_application_action(
            input,
            options("editor/", phase),
            BindingTarget::Script(script(1)),
        )?;
        let resolved = map
            .resolve_match(&Path::from("/root/editor"), input)
            .unwrap();
        assert_eq!(resolved.phase, phase);
    }

    // A framework group takes the early phase on the same terms.
    let mut options = options("/root/dialog/**/", BindingPhase::BeforeWidget);
    options.scope = BindingScope::Exclusive(HELP);
    let id = map.bind_framework(
        HELP,
        Mouse::parse_spec("LeftDown").unwrap(),
        options,
        CommandAction {
            invocation: command("button::press"),
            target: None,
        },
    )?;
    assert_eq!(
        map.binding(id).unwrap().phase,
        BindingPhase::BeforeWidget,
        "a framework mouse binding keeps the phase it declared"
    );
    Ok(())
}

#[test]
fn application_command_targets_replace_and_remove_like_callbacks() -> Result<()> {
    let mut map = InputMap::new();
    let script_id = bind(&mut map, BindingScope::Default, 'x', "", "Script", 1)?;
    let action = BindingTarget::Command(CommandAction {
        invocation: command("editor::undo"),
        target: Some(CommandTarget::Focus),
    });
    let (command_id, removed) = map.replace_application_action(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::AfterWidget),
        action.clone(),
    )?;
    assert_eq!(removed, [(script_id, BindingTarget::Script(script(1)))]);
    assert_eq!(target(&map, "/root", 'x'), Some(action.clone()));
    assert_eq!(map.unbind(command_id)?, Some(action.clone()));
    assert_eq!(map.unbind(command_id)?, None);

    let (command_id, _) = map.replace_application_action(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::AfterWidget),
        action.clone(),
    )?;
    let (_, removed) = map.replace_application_action(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::AfterWidget),
        BindingTarget::Script(script(2)),
    )?;
    assert_eq!(removed, [(command_id, action)]);
    Ok(())
}

#[test]
fn application_snapshot_and_clear_include_commands() -> Result<()> {
    let mut map = InputMap::new();
    let action = BindingTarget::Command(CommandAction {
        invocation: command("editor::undo"),
        target: Some(CommandTarget::Exact(testing_node_id())),
    });
    let (command_id, _) = map.replace_application_action(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::BeforeWidget),
        action.clone(),
    )?;
    let snapshot = map.snapshot_application();
    let script_id = bind(&mut map, BindingScope::Default, 'y', "", "Script", 1)?;
    assert_eq!(map.targets_not_in(&snapshot), [script(1)]);
    let removed = map.clear_application();
    assert_eq!(
        removed,
        [
            (command_id, action.clone()),
            (script_id, BindingTarget::Script(script(1)))
        ]
    );
    assert!(map.bindings().is_empty());
    map.restore_application(snapshot);
    assert_eq!(map.bindings().len(), 1);
    assert_eq!(target(&map, "/root", 'x'), Some(action));
    assert_eq!(
        map.binding(command_id).unwrap().phase,
        BindingPhase::BeforeWidget
    );
    Ok(())
}

#[test]
fn framework_binding_options_preserve_explicit_phase() -> Result<()> {
    let mut map = InputMap::new();
    let mut options = options("/root/help/**/", BindingPhase::AfterWidget);
    options.scope = BindingScope::Exclusive(HELP);
    let input = InputSpec::Key('j'.into());
    let action = CommandAction {
        invocation: command("binding_list::scroll_down"),
        target: Some(CommandTarget::Focus),
    };
    let id = map.bind_framework(HELP, input, options.clone(), action.clone())?;
    assert_eq!(
        map.bind_framework(HELP, input, options.clone(), action.clone())?,
        id
    );
    map.set_modal_bindings(Some(ModalBindings::Framework(HELP)));
    let resolved = map
        .resolve_match(&Path::from("/root/help/list"), input)
        .unwrap();
    assert_eq!(resolved.phase, BindingPhase::AfterWidget);
    assert_eq!(resolved.target, BindingTarget::Command(action.clone()));
    options.phase = BindingPhase::BeforeWidget;
    assert!(map.bind_framework(HELP, input, options, action).is_err());
    assert_eq!(map.binding(id).unwrap().phase, BindingPhase::AfterWidget);
    Ok(())
}
