use slotmap::Key as _;

use super::*;
use crate::{
    commands::{CommandArgs, CommandId},
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

fn bind(
    map: &mut InputMap,
    scope: BindingScope,
    key: impl Into<Key>,
    path: &str,
    description: &str,
    target: u64,
) -> Result<BindingId> {
    map.replace_application_binding(
        scope,
        InputSpec::Key(key.into()),
        path,
        description,
        Some("test:1".to_string()),
        script(target),
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

    map.push_mode("normal")?;
    map.push_mode("modal")?;
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
fn phase_matches_route_semantics() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'a', "editor/", "Before", 1)?;
    bind(&mut map, BindingScope::Default, 'b', "editor", "After", 2)?;
    assert_eq!(
        map.resolve_match(&Path::from("/root/editor"), InputSpec::Key('a'.into()))
            .map(|binding| binding.phase),
        Some(BindingPhase::BeforeWidget)
    );
    assert_eq!(
        map.resolve_match(
            &Path::from("/root/editor/child"),
            InputSpec::Key('b'.into())
        )
        .map(|binding| binding.phase),
        Some(BindingPhase::AfterIgnore)
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
    let first = map.bind_framework(
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Scroll down",
        command("binding_list::scroll_down"),
    )?;
    let second = map.bind_framework(
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Scroll down",
        command("binding_list::scroll_down"),
    )?;
    assert_eq!(first, second);
    assert!(
        map.bind_framework(
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
fn newest_exclusive_frame_blocks_all_application_tiers() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'j', "", "Application", 1)?;
    map.bind_framework(
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    map.bind_framework(
        OTHER,
        InputSpec::Key('x'.into()),
        "/root/other/**/",
        "Other",
        command("other::close"),
    )?;

    let owner = NodeId::null();
    let help = map.push_exclusive_bindings(HELP, owner)?;
    assert_eq!(
        target(&map, "/root/help/binding_list", 'j'),
        Some(BindingTarget::Command(command("binding_list::scroll_down")))
    );
    assert_eq!(target(&map, "/root/help/binding_list", 'x'), None);
    let other = map.push_exclusive_bindings(OTHER, owner)?;
    assert_eq!(target(&map, "/root/help/binding_list", 'j'), None);
    map.pop_exclusive_bindings(help)?;
    assert_eq!(map.active_exclusive_group(), Some(OTHER));
    map.pop_exclusive_bindings(other)?;
    assert_eq!(
        target(&map, "/root/help/binding_list", 'j'),
        Some(BindingTarget::Script(script(1)))
    );
    Ok(())
}

#[test]
fn application_mutation_cannot_remove_framework_records() -> Result<()> {
    let mut map = InputMap::new();
    let app = bind(&mut map, BindingScope::Default, 'a', "", "App", 1)?;
    let framework = map.bind_framework(
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    assert_eq!(map.unbind(app)?, Some(script(1)));
    assert!(map.unbind(framework).is_err());
    map.clear_application();
    assert_eq!(map.bindings().len(), 1);
    assert_eq!(map.bindings()[0].id, framework);
    Ok(())
}

#[test]
fn startup_restore_preserves_framework_records_and_frames() -> Result<()> {
    let mut map = InputMap::new();
    bind(&mut map, BindingScope::Default, 'a', "", "Before", 1)?;
    let snapshot = map.snapshot_application();
    bind(&mut map, BindingScope::Default, 'b', "", "Transient", 2)?;
    map.bind_framework(
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    let token = map.push_exclusive_bindings(HELP, NodeId::null())?;

    map.restore_application(snapshot);
    assert_eq!(map.bindings().len(), 2);
    assert_eq!(map.active_exclusive_group(), Some(HELP));
    map.pop_exclusive_bindings(token)?;
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

    map.bind_framework(
        HELP,
        InputSpec::Key('j'.into()),
        "/root/help/**/",
        "Help down",
        command("binding_list::scroll_down"),
    )?;
    let token = map.push_exclusive_bindings(HELP, NodeId::null())?;
    assert_eq!(
        map.diagnostic_state(global, &route),
        "blocked by exclusive group root.help"
    );
    map.pop_exclusive_bindings(token)?;
    Ok(())
}
