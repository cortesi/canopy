use canopy::{
    Loader, RoutePhase, Widget, error::Result, event::key::Key, geom::Size,
    testing::harness::Harness,
};
use canopy_widgets::Root;

use crate::{install_help_binding, termgym, widget_editor};

fn root_harness<W>(app: W, setup: fn(&mut canopy::Canopy) -> Result<()>) -> Result<Harness>
where
    W: Widget + Loader + 'static,
{
    let mut canopy = canopy::Canopy::new();
    Root::load(&mut canopy)?;
    W::load(&mut canopy)?;
    install_help_binding(&mut canopy)?;
    canopy.finalize_api()?;
    setup(&mut canopy)?;
    Root::install_app(&mut canopy, app)?;
    canopy.run_startup_scripts()?;
    let mut harness = Harness::from_canopy(canopy, Size::new(80, 24))?;
    harness.render()?;
    Ok(harness)
}

fn add_scroll_rows(canopy: &mut canopy::Canopy) -> Result<()> {
    let mut source = ('a'..='t')
        .map(|key| {
            format!(
                "canopy.bind(\"alt-{key}\", {{ description = \"Extra help row {key}\" }}, function() end)\n"
            )
        })
        .collect::<String>();
    source.push_str(
        r#"canopy.bind("ctrl-x", {
            description = "Acceptance sentinel",
            path = "/root/**/",
            tier = "global",
        }, function()
            canopy.set_mode("accepted")
        end)
        "#,
    );
    canopy.eval_script(&source)
}

fn prove_help_flow(mut harness: Harness) -> Result<()> {
    harness.canopy.eval_script(
        r#"
        local count = 0
        for _, binding in canopy.bindings() do
            if binding.input == "?" and binding.scope == "global"
                and binding.owner == "application" then
                count += 1
            end
        end
        canopy.assert(count == 1, "the launcher must install one global help trigger")
        "#,
    )?;
    add_scroll_rows(&mut harness.canopy)?;
    let origin = harness
        .canopy
        .with_root_view(|context| context.focused_node())
        .expect("demo should focus its consuming widget");

    harness.key('?')?;
    harness.render()?;

    let list = harness
        .find_nodes("root/help/**/binding_list")
        .into_iter()
        .next()
        .expect("help binding list");
    assert_eq!(
        harness
            .canopy
            .with_root_view(|context| context.focused_node()),
        Some(list)
    );
    assert_eq!(
        harness
            .canopy
            .available_bindings(None)?
            .exclusive_group
            .map(|group| group.as_str()),
        Some("root.help")
    );
    assert!(harness.tbuf().contains_text("Key bindings"));
    assert!(!harness.tbuf().contains_text("Context:"));
    assert!(harness.tbuf().contains_text("Up/k Down/j scroll"));
    assert!(harness.tbuf().contains_text("?/Esc close"));

    harness.key(Key::parse_spec("Down").expect("valid key"))?;
    assert!(harness.canopy.route_trace().iter().any(|entry| {
        entry.phase == RoutePhase::BindingExecution && entry.detail == "Scroll down"
    }));
    harness.key('?')?;

    assert_eq!(
        harness
            .canopy
            .with_root_view(|context| context.focused_node()),
        Some(origin)
    );
    harness.key(Key::parse_spec("ctrl-x").expect("valid key"))?;
    assert_eq!(
        harness
            .canopy
            .route_trace()
            .first()
            .and_then(|entry| entry.node),
        Some(origin)
    );
    assert!(
        harness
            .canopy
            .route_trace()
            .iter()
            .any(|entry| entry.phase == RoutePhase::Handled)
    );
    assert_eq!(harness.canopy.input_mode(), "accepted");
    Ok(())
}

#[test]
fn termgym_help_opens_over_a_consuming_terminal_and_restores_input() -> Result<()> {
    let harness = root_harness(termgym::TermGym::new(), termgym::setup_bindings)?;
    prove_help_flow(harness)
}

#[test]
fn widget_editor_help_opens_over_a_consuming_editor_and_restores_input() -> Result<()> {
    let harness = root_harness(
        widget_editor::WidgetEditor::new("fn main() {}\n", "rs", "test.rs"),
        widget_editor::setup_bindings,
    )?;
    prove_help_flow(harness)
}
