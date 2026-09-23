use super::*;
use crate::{
    commands::{CommandArgs, CommandCall, CommandId, CommandTarget},
    core::id::testing_node_id,
    error::Result,
    input::{key, mouse::Mouse},
};

const HELP: FrameworkBindingGroup = FrameworkBindingGroup::new("root.help");
const OTHER: FrameworkBindingGroup = FrameworkBindingGroup::new("other.modal");

fn script(id: u64) -> LuauFunctionId {
    LuauFunctionId::for_test(id)
}

fn command(id: &'static str) -> CommandCall {
    CommandCall {
        id: CommandId(id),
        args: CommandArgs::default(),
        target: None,
    }
}

/// Install one framework command binding.
fn framework(
    map: &mut InputMap,
    input: impl Into<InputSpec>,
    options: BindingOptions,
    command: CommandCall,
) -> Result<BindingId> {
    map.bind(input.into(), options, BindingAction::Command(command))
        .map(|(id, _)| id)
}

fn bind_framework(
    map: &mut InputMap,
    group: FrameworkBindingGroup,
    input: impl Into<InputSpec>,
    path: &str,
    description: &str,
    command: CommandCall,
) -> Result<BindingId> {
    framework(
        map,
        input,
        BindingOptions {
            path: Some(path.parse()?),
            tier: BindingTier::Framework(group),
            description: description.to_string(),
            source: None,
            phase: Some(BindingPhase::AfterWidget),
        },
        command,
    )
}

fn bind(
    map: &mut InputMap,
    tier: BindingTier,
    key: impl Into<Key>,
    path: &str,
    description: &str,
    target: u64,
) -> Result<BindingId> {
    map.bind(
        InputSpec::Key(key.into()),
        BindingOptions {
            tier,
            path: if path.is_empty() {
                None
            } else {
                Some(path.parse()?)
            },
            description: description.to_string(),
            source: Some("test:1".to_string()),
            phase: Some(BindingPhase::AfterWidget),
        },
        BindingAction::Script(script(target)),
    )
    .map(|(id, _)| id)
}

fn target(map: &InputMap, path: &str, key: impl Into<Key>) -> Option<BindingAction> {
    map.resolve_match(&Path::from(path), InputSpec::Key(key.into()))
        .map(|binding| binding.action.clone())
}

#[test]
fn normalized_keys_share_one_registry_slot() -> Result<()> {
    let mut map = InputMap::new();
    bind(
        &mut map,
        BindingTier::Default,
        key::Shift + 'a',
        "",
        "Shifted A",
        1,
    )?;
    assert_eq!(
        target(&map, "/root", 'A'),
        Some(BindingAction::Script(script(1)))
    );
    assert_eq!(
        target(&map, "/root", key::Shift + 'A'),
        Some(BindingAction::Script(script(1)))
    );
    Ok(())
}

#[test]
fn resolution_uses_global_then_newest_mode_then_default() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingTier::Default, 'a', "", "Default", 1)?;
    bind(
        &mut map,
        BindingTier::Mode("normal".to_string()),
        'a',
        "",
        "Normal",
        2,
    )?;
    bind(
        &mut map,
        BindingTier::Mode("modal".to_string()),
        'a',
        "",
        "Modal",
        3,
    )?;
    bind(&mut map, BindingTier::Global, '?', "/root/**/", "Help", 4)?;

    map.push_mode("normal");
    map.push_mode("modal");
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(3)))
    );
    assert_eq!(
        target(&map, "/root/editor", '?'),
        Some(BindingAction::Script(script(4)))
    );
    assert_eq!(map.active_modes(), vec!["modal", "normal"]);

    map.pop_mode();
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(2)))
    );
    map.pop_mode();
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(1)))
    );
    Ok(())
}

#[test]
fn path_score_then_latest_insertion_selects_the_winner() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingTier::Default, 'a', "editor", "Loose", 1)?;
    bind(
        &mut map,
        BindingTier::Default,
        'a',
        "editor/",
        "Anchored",
        2,
    )?;
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(2)))
    );

    bind(&mut map, BindingTier::Default, 'a', "editor/", "Latest", 3)?;
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(3)))
    );
    Ok(())
}

#[test]
fn global_bindings_require_both_path_anchors() {
    for path in ["root/**/", "/root/**", "root/**"] {
        let mut map = InputMap::new();
        assert!(bind(&mut map, BindingTier::Global, '?', path, "Help", 1).is_err());
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
fn the_framework_tier_takes_no_script_and_scripts_take_no_framework_tier() -> Result<()> {
    let mut map = InputMap::new();
    let mut options = options("/root/help/**/", BindingPhase::AfterWidget);
    options.tier = BindingTier::Framework(HELP);
    assert!(
        map.bind(
            InputSpec::Key('j'.into()),
            options.clone(),
            BindingAction::Script(script(1)),
        )
        .is_err(),
        "nothing releases a framework script target"
    );
    assert!(
        validate_application_binding(&options, None).is_err(),
        "script validation rejects the framework tier"
    );
    assert!(map.bindings().is_empty());
    Ok(())
}

#[test]
fn modal_bindings_block_all_application_tiers() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingTier::Default, 'j', "", "Application", 1)?;
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

    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &[],
    }));
    assert_eq!(
        target(&map, "/root/help/binding_list", 'j'),
        Some(BindingAction::Command(command("binding_list::scroll_down")))
    );
    assert_eq!(target(&map, "/root/help/binding_list", 'x'), None);
    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[OTHER],
        intents: &[],
    }));
    assert_eq!(target(&map, "/root/help/binding_list", 'j'), None);
    map.set_modal_bindings(None);
    assert_eq!(map.active_framework_group(), None);
    assert_eq!(
        target(&map, "/root/help/binding_list", 'j'),
        Some(BindingAction::Script(script(1)))
    );
    Ok(())
}

#[test]
fn a_transient_mode_hides_older_modes_and_the_default_tier() -> Result<()> {
    let mut map = InputMap::new();
    let default = bind(&mut map, BindingTier::Default, 'a', "", "Default", 1)?;
    bind(
        &mut map,
        BindingTier::Mode("normal".to_string()),
        'b',
        "",
        "Normal",
        2,
    )?;
    bind(
        &mut map,
        BindingTier::Mode("prefix".to_string()),
        'c',
        "",
        "Prefix",
        3,
    )?;
    bind(&mut map, BindingTier::Global, '?', "/root/**/", "Help", 4)?;

    map.push_mode("normal");
    let generation = map.mode_generation();
    map.push_transient_mode("prefix");
    assert_ne!(map.mode_generation(), generation);
    assert_eq!(map.transient_mode(), Some("prefix"));
    assert_eq!(map.mode(), "prefix");
    assert_eq!(
        target(&map, "/root/editor", 'c'),
        Some(BindingAction::Script(script(3)))
    );
    assert_eq!(
        target(&map, "/root/editor", '?'),
        Some(BindingAction::Script(script(4))),
        "global bindings still win"
    );
    assert_eq!(target(&map, "/root/editor", 'b'), None);
    assert_eq!(target(&map, "/root/editor", 'a'), None);
    assert_eq!(
        map.registry_status(default, &[Path::from("/root/editor")])
            .label(),
        "blocked by transient mode prefix"
    );

    map.pop_mode();
    assert_eq!(map.transient_mode(), None);
    assert_eq!(
        target(&map, "/root/editor", 'b'),
        Some(BindingAction::Script(script(2)))
    );
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(1)))
    );
    Ok(())
}

#[test]
fn application_mutation_cannot_remove_framework_records() -> Result<()> {
    let mut map = InputMap::new();
    let app = bind(&mut map, BindingTier::Default, 'a', "", "App", 1)?;
    let framework = bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    assert_eq!(map.unbind(app)?, Some(BindingAction::Script(script(1))));
    assert!(map.unbind(framework).is_err());
    map.clear_application();
    assert_eq!(map.bindings().len(), 1);
    assert_eq!(map.bindings()[0].id, framework);
    Ok(())
}

#[test]
fn startup_restore_preserves_framework_records_and_modal_bindings() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingTier::Default, 'a', "", "Before", 1)?;
    let snapshot = map.snapshot_application();
    bind(&mut map, BindingTier::Default, 'b', "", "Transient", 2)?;
    bind_framework(
        &mut map,
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &[],
    }));

    map.restore_application(snapshot);
    assert_eq!(map.bindings().len(), 2);
    assert_eq!(map.active_framework_group(), Some(HELP));
    Ok(())
}

#[test]
fn identifier_exhaustion_does_not_replace_an_existing_binding() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingTier::Default, 'a', "", "Existing", 1)?;
    map.next_id = u64::MAX;
    assert!(bind(&mut map, BindingTier::Default, 'a', "", "New", 2).is_err());
    assert_eq!(
        target(&map, "/root", 'a'),
        Some(BindingAction::Script(script(1)))
    );
    Ok(())
}

#[test]
fn replacement_is_scoped_by_input_tier_and_exact_path() -> Result<()> {
    let mut map = InputMap::new();
    let old = bind(&mut map, BindingTier::Default, 'a', "editor/", "Old", 1)?;
    let mode = bind(
        &mut map,
        BindingTier::Mode("insert".to_string()),
        'a',
        "editor/",
        "Mode",
        2,
    )?;
    let new = bind(&mut map, BindingTier::Default, 'a', "editor/", "New", 3)?;

    assert!(map.binding(old).is_none());
    assert!(map.binding(mode).is_some());
    assert!(map.binding(new).is_some());
    assert_eq!(map.bindings().len(), 2);
    assert_eq!(
        target(&map, "/root/editor", 'a'),
        Some(BindingAction::Script(script(3)))
    );
    Ok(())
}

#[test]
fn diagnostics_distinguish_tier_path_insertion_route_and_framework_group_causes() -> Result<()> {
    let mut map = InputMap::new();
    let earlier_route = bind(
        &mut map,
        BindingTier::Default,
        'a',
        "/root/editor/",
        "Earlier route",
        1,
    )?;
    let later_route = bind(
        &mut map,
        BindingTier::Default,
        'a',
        "/root/",
        "Later route",
        2,
    )?;
    let insertion_loser = bind(&mut map, BindingTier::Default, 'b', "*/editor/", "First", 3)?;
    let insertion_winner = bind(&mut map, BindingTier::Default, 'b', "/root/*/", "Second", 4)?;
    let path_loser = bind(&mut map, BindingTier::Default, 'c', "*/", "Loose", 5)?;
    let path_winner = bind(
        &mut map,
        BindingTier::Default,
        'c',
        "editor/",
        "Anchored",
        6,
    )?;
    let default = bind(&mut map, BindingTier::Default, 'd', "", "Default", 7)?;
    let global = bind(&mut map, BindingTier::Global, 'd', "/root/**/", "Global", 8)?;
    let unmatched = bind(&mut map, BindingTier::Default, 'e', "other/", "Other", 9)?;
    let route = [Path::from("/root/editor"), Path::from("/root")];

    assert_eq!(
        map.registry_status(earlier_route, &route).label(),
        "effective"
    );
    assert_eq!(
        map.registry_status(later_route, &route).label(),
        "shadowed at an earlier route node"
    );
    assert_eq!(
        map.registry_status(insertion_loser, &route).label(),
        "shadowed by later insertion"
    );
    assert_eq!(
        map.registry_status(insertion_winner, &route).label(),
        "effective"
    );
    assert_eq!(
        map.registry_status(path_loser, &route).label(),
        "shadowed by a more specific path"
    );
    assert_eq!(
        map.registry_status(path_winner, &route).label(),
        "effective"
    );
    assert_eq!(
        map.registry_status(default, &route).label(),
        "shadowed by a higher-priority tier"
    );
    assert_eq!(map.registry_status(global, &route).label(), "effective");
    assert_eq!(
        map.registry_status(unmatched, &route).label(),
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
    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &[],
    }));
    assert_eq!(
        map.registry_status(earlier_route, &route).label(),
        "blocked by framework group root.help"
    );
    // Global bindings reach through every modal.
    assert_eq!(map.registry_status(global, &route).label(), "effective");
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
        tier: BindingTier::Default,
        description: "Test action".to_string(),
        source: None,
        phase: Some(phase),
    }
}

#[test]
fn explicit_phase_is_independent_of_selector() -> Result<()> {
    let mut map = InputMap::new();
    let input = InputSpec::Key('x'.into());
    for path in ["editor", "editor/"] {
        for phase in [BindingPhase::BeforeWidget, BindingPhase::AfterWidget] {
            map.clear_application();
            let (id, _) = map.bind(
                input,
                options(path, phase),
                BindingAction::Script(script(1)),
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
            BindingTier::Default,
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
        map.bind(
            input,
            options("editor/", phase),
            BindingAction::Script(script(1)),
        )?;
        let resolved = map
            .resolve_match(&Path::from("/root/editor"), input)
            .unwrap();
        assert_eq!(resolved.phase, phase);
    }

    // A framework group takes the early phase on the same terms.
    let mut options = options("/root/dialog/**/", BindingPhase::BeforeWidget);
    options.tier = BindingTier::Framework(HELP);
    let id = framework(
        &mut map,
        Mouse::parse_spec("LeftDown").unwrap(),
        options,
        command("button::press"),
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
    let script_id = bind(&mut map, BindingTier::Default, 'x', "", "Script", 1)?;
    let action = BindingAction::Command(command("editor::undo").with_target(CommandTarget::Focus));
    let (command_id, removed) = map.bind(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::AfterWidget),
        action.clone(),
    )?;
    assert_eq!(removed, [(script_id, BindingAction::Script(script(1)))]);
    assert_eq!(target(&map, "/root", 'x'), Some(action.clone()));
    assert_eq!(map.unbind(command_id)?, Some(action.clone()));
    assert_eq!(map.unbind(command_id)?, None);

    let (command_id, _) = map.bind(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::AfterWidget),
        action.clone(),
    )?;
    let (_, removed) = map.bind(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::AfterWidget),
        BindingAction::Script(script(2)),
    )?;
    assert_eq!(removed, [(command_id, action)]);
    Ok(())
}

#[test]
fn application_snapshot_and_clear_include_commands() -> Result<()> {
    let mut map = InputMap::new();
    let action = BindingAction::Command(
        command("editor::undo").with_target(CommandTarget::Exact(testing_node_id())),
    );
    let (command_id, _) = map.bind(
        InputSpec::Key('x'.into()),
        options("", BindingPhase::BeforeWidget),
        action.clone(),
    )?;
    let snapshot = map.snapshot_application();
    let script_id = bind(&mut map, BindingTier::Default, 'y', "", "Script", 1)?;
    assert_eq!(map.targets_not_in(&snapshot), [script(1)]);
    let removed = map.clear_application();
    assert_eq!(
        removed,
        [
            (command_id, action.clone()),
            (script_id, BindingAction::Script(script(1)))
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
    options.tier = BindingTier::Framework(HELP);
    let input = InputSpec::Key('j'.into());
    let action = command("binding_list::scroll_down").with_target(CommandTarget::Focus);
    let id = framework(&mut map, input, options.clone(), action.clone())?;
    assert_eq!(
        framework(&mut map, input, options.clone(), action.clone())?,
        id
    );
    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &[],
    }));
    let resolved = map
        .resolve_match(&Path::from("/root/help/list"), input)
        .unwrap();
    assert_eq!(resolved.phase, BindingPhase::AfterWidget);
    assert_eq!(resolved.action, BindingAction::Command(action.clone()));
    options.phase = Some(BindingPhase::BeforeWidget);
    assert!(framework(&mut map, input, options, action).is_err());
    assert_eq!(map.binding(id).unwrap().phase, BindingPhase::AfterWidget);
    Ok(())
}

fn register_action(map: &mut InputMap, name: &str) -> Result<()> {
    map.register_intent(IntentSpec::new(name, "Test action")?)
}

fn bind_action(
    map: &mut InputMap,
    tier: BindingTier,
    key: impl Into<Key>,
    path: &str,
    name: &str,
    description: &str,
) -> Result<BindingId> {
    map.bind(
        InputSpec::Key(key.into()),
        BindingOptions {
            tier,
            path: if path.is_empty() {
                None
            } else {
                Some(path.parse()?)
            },
            description: description.to_string(),
            source: None,
            phase: None,
        },
        BindingAction::Intent(IntentName::new(name)?),
    )
    .map(|(id, _)| id)
}

#[test]
fn intents_validate_names_phases_and_inputs() -> Result<()> {
    let mut map = InputMap::new();
    register_action(&mut map, "test.clear")?;
    register_action(&mut map, "test.clear")?;
    assert!(
        map.register_intent(IntentSpec::new("test.clear", "Other")?)
            .is_err(),
        "a conflicting description is rejected"
    );
    assert!(IntentName::new("clear").is_err(), "a name must be dotted");
    assert!(IntentName::new("").is_err(), "a name cannot be empty");
    assert!(
        IntentName::new("a..b").is_err(),
        "name segments cannot be empty"
    );

    let options = BindingOptions {
        tier: BindingTier::Default,
        path: None,
        description: "Clear".to_string(),
        source: None,
        phase: None,
    };
    assert!(
        map.bind(
            InputSpec::Key('x'.into()),
            options.clone(),
            BindingAction::Intent(IntentName::new("test.other")?),
        )
        .is_err(),
        "an unregistered action name is rejected"
    );
    let mut late = options.clone();
    late.phase = Some(BindingPhase::AfterWidget);
    assert!(
        map.bind(
            InputSpec::Key('x'.into()),
            late,
            BindingAction::Intent(IntentName::new("test.clear")?),
        )
        .is_err(),
        "an action cannot run after the widget"
    );
    let mut early = options.clone();
    early.phase = Some(BindingPhase::BeforeWidget);
    let (id, _) = map.bind(
        InputSpec::Key('x'.into()),
        early,
        BindingAction::Intent(IntentName::new("test.clear")?),
    )?;
    assert_eq!(
        map.binding(id).map(|record| record.phase),
        Some(BindingPhase::BeforeWidget),
        "an explicit before_widget matches the phase every action takes"
    );
    map.unbind(id)?;
    let (id, _) = map.bind(
        InputSpec::Key('x'.into()),
        options.clone(),
        BindingAction::Intent(IntentName::new("test.clear")?),
    )?;
    assert_eq!(
        map.binding(id).map(|record| record.phase),
        Some(BindingPhase::BeforeWidget),
        "an omitted phase stores an action as before_widget"
    );
    map.unbind(id)?;
    assert!(
        map.bind(
            InputSpec::Mouse(Mouse::parse_spec("LeftDown")?),
            options,
            BindingAction::Intent(IntentName::new("test.clear")?),
        )
        .is_err(),
        "an action accepts keys only"
    );
    Ok(())
}

#[test]
fn a_framework_action_modal_admits_only_allowlisted_actions() -> Result<()> {
    let mut map = InputMap::new();
    register_action(&mut map, "test.clear")?;
    register_action(&mut map, "test.other")?;
    bind_action(
        &mut map,
        BindingTier::Default,
        'x',
        "",
        "test.clear",
        "Allowed",
    )?;
    bind_action(
        &mut map,
        BindingTier::Default,
        'y',
        "",
        "test.other",
        "Other",
    )?;
    bind(&mut map, BindingTier::Default, 'z', "", "Callback", 1)?;
    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &["test.clear"],
    }));
    let path = Path::from("/root/help/list");
    let allowed = map.resolve_match(&path, InputSpec::Key('x'.into()));
    assert_eq!(
        allowed.map(|resolved| resolved.description.as_str()),
        Some("Allowed")
    );
    assert!(
        map.resolve_match(&path, InputSpec::Key('y'.into()))
            .is_none(),
        "an action outside the allowlist is not admitted"
    );
    assert!(
        map.resolve_match(&path, InputSpec::Key('z'.into()))
            .is_none(),
        "an application callback is not admitted"
    );
    assert!(map.candidate_keys().contains(&Key::from('x')));

    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &[],
    }));
    assert!(
        map.resolve_match(&path, InputSpec::Key('x'.into()))
            .is_none(),
        "a framework-only modal excludes application actions"
    );
    assert!(!map.candidate_keys().contains(&Key::from('x')));
    Ok(())
}

#[test]
fn a_framework_modal_suspends_the_transient_cutoff() -> Result<()> {
    let mut map = InputMap::new();
    register_action(&mut map, "test.clear")?;
    let allowed = bind_action(
        &mut map,
        BindingTier::Default,
        'x',
        "",
        "test.clear",
        "Allowed",
    )?;
    map.push_transient_mode("prefix");
    map.set_modal_bindings(Some(ModalBindings::Framework {
        groups: &[HELP],
        intents: &["test.clear"],
    }));
    let route = [Path::from("/root/help/list")];
    assert_eq!(
        map.resolve_match(&route[0], InputSpec::Key('x'.into()))
            .map(|resolved| resolved.id),
        Some(allowed),
        "a suspended transient mode leaves the default tier reachable"
    );
    assert_eq!(
        map.registry_status(allowed, &route),
        RegistryStatus::Effective
    );

    map.set_modal_bindings(None);
    assert!(
        map.resolve_match(&route[0], InputSpec::Key('x'.into()))
            .is_none(),
        "without the modal, the transient mode cuts off the default tier"
    );
    Ok(())
}
