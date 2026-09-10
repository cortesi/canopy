use canopy::{
    CanopyBuilder, Loader, RoutePhase, Widget, error::Result, event::key::Key, geom::Size,
    testing::harness::Harness,
};
use canopy_widgets::Root;

use crate::{demo_canopy, termgym, widget_editor};

fn wrapped_harness<W>(app: W, setup: fn(CanopyBuilder) -> CanopyBuilder) -> Result<Harness>
where
    W: Widget + Loader + 'static,
{
    let canopy = setup(demo_canopy().configure(W::load))
        .assemble(move |canopy| {
            Root::new().install(canopy, app)?;
            Ok(())
        })
        .build()?;
    let mut harness = Harness::from_canopy(canopy, Size::new(80, 24))?;
    harness.render()?;
    Ok(harness)
}

#[test]
fn demo_api_build_does_not_assemble_or_publish() -> Result<()> {
    let canopy = termgym::binding_setup(demo_canopy().configure(termgym::TermGym::load)).build()?;
    assert!(canopy.snapshot().is_none());
    assert!(
        canopy
            .with_root_view(|context| context.find_nodes("root/termgym"))
            .is_empty()
    );
    Ok(())
}

fn add_scroll_rows(canopy: &mut canopy::Canopy) -> Result<()> {
    let mut source = ('a'..='t')
        .map(|key| {
            format!(
                "canopy.bind(\"alt-{key}\", {{ phase = \"after_widget\", description = \"Extra help row {key}\" }}, function() end)\n"
            )
        })
        .collect::<String>();
    source.push_str(
        r#"canopy.bind("ctrl-x", { phase = "before_widget",
            description = "Acceptance sentinel",
            path = "/root/**/",
            tier = "global",
        }, function()
            canopy.set_mode("accepted")
        end)
        "#,
    );
    canopy.eval_script(&source).map(|_| ())
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
    assert!(harness.tbuf().contains_text("Keyboard shortcuts"));
    assert!(!harness.tbuf().contains_text("Context:"));
    assert!(harness.tbuf().contains_text("↑/↓ Scroll"));
    assert!(harness.tbuf().contains_text("Esc Close"));
    let frame = harness.canopy.snapshot().expect("published help");
    let panel = frame
        .nodes
        .iter()
        .find(|node| node.name == "help_panel")
        .and_then(|node| node.rect)
        .expect("visible help panel");
    let cell = |x, y| &frame.cells[(y * frame.viewport.w + x) as usize];
    let background = cell(panel.tl.x as u32, panel.tl.y as u32).style.bg;
    for y in panel.tl.y as u32..panel.tl.y as u32 + panel.h {
        for x in panel.tl.x as u32..panel.tl.x as u32 + panel.w {
            assert_eq!(
                cell(x, y).style.bg,
                background,
                "help background gap at {x},{y}"
            );
        }
    }

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
    let harness = wrapped_harness(termgym::TermGym::new(), termgym::binding_setup)?;
    prove_help_flow(harness)
}

#[test]
fn widget_editor_help_opens_over_a_consuming_editor_and_restores_input() -> Result<()> {
    let harness = wrapped_harness(
        widget_editor::WidgetEditor::new("fn main() {}\n", "rs", "test.rs"),
        widget_editor::binding_setup,
    )?;
    prove_help_flow(harness)
}
