//! A small shared R27 trace for native, terminal, MCP, proxy, and replay tests.

use std::collections::BTreeMap;

use crate::{
    Canopy, Context, EventOutcome, ViewContext, Widget, WidgetSemantics, command,
    commands::ArgValue,
    derive_commands,
    error::Result,
    event::{Event, key::KeyCode},
    geom::Size,
    layout::Layout,
    render::Render,
    state::NodeName,
};

/// R06/R07 argument shapes and exact targeting, R18 waits, and R19
/// observations. The result omits opaque node tokens and absolute frame
/// generations.
pub const SCRIPT: &str = r#"
local old = canopy.snapshot()
if old == nil then error("initial publication missing") end
local old_value = old.nodes[1].semantics.value
canopy.call_exact(canopy.root(), "contract::set", 6)
canopy.call_named("contract::set", { value = 7, extra = 2 })
local pending = canopy.snapshot()
if pending == nil then error("publication disappeared") end
canopy.assert(pending.frame_id == old.frame_id)
canopy.flush()
canopy.wait_for(function()
    local frame = canopy.snapshot()
    return frame ~= nil and frame.nodes[1].semantics.value == "9"
end)
local fresh = canopy.snapshot()
if fresh == nil then error("final publication missing") end
return {
    value = tonumber(fresh.nodes[1].semantics.value),
    label = fresh.nodes[1].semantics.label,
    displayed = fresh.nodes[1].displayed,
    old_retained = old.nodes[1].semantics.value == old_value,
    frame_advanced = fresh.frame_id > old.frame_id,
}
"#;

/// Shared trace widget with observable native input and command mutation.
struct Contract {
    /// Small integer encoded in the painted cell and semantic value.
    value: i64,
}

#[derive_commands]
impl Contract {
    /// Set a value with an optional additional amount.
    #[command]
    fn set(&mut self, value: i64, extra: Option<i64>) {
        self.value = value + extra.unwrap_or_default();
    }
}

impl Widget for Contract {
    fn name(&self) -> NodeName {
        NodeName::convert("contract")
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _view: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Key(key) if key.key == KeyCode::Char('x')) {
            self.value += 1;
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn render(&mut self, render: &mut Render, view: &dyn ViewContext) -> Result<()> {
        let glyph = char::from_digit(self.value.rem_euclid(10) as u32, 10).expect("decimal digit");
        render.fill("", view.view().outer_rect_local(), glyph)
    }

    fn semantics(&self, _view: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("contract".into()),
            label: Some("shared trace".into()),
            value: Some(self.value.to_string()),
            ..WidgetSemantics::default()
        })
    }
}

/// Create a finalized, unprepared fixture with a fixed 12 by 3 viewport.
pub fn app() -> Result<Canopy> {
    let mut app = Canopy::new();
    app.add_commands::<Contract>()?;
    app.replace_root(Contract { value: 0 })?;
    app.with_root_context(|ctx| ctx.set_semantic_key(ctx.root_id(), ctx.root_id(), "contract"))?;
    app.set_root_size(Size::new(12, 3))?;
    app.finalize_api()?;
    Ok(app)
}

/// Expected adapter-independent projection of the shared trace.
pub fn expected() -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("value".into(), ArgValue::Int(9)),
        ("label".into(), ArgValue::String("shared trace".into())),
        ("displayed".into(), ArgValue::Bool(true)),
        ("old_retained".into(), ArgValue::Bool(true)),
        ("frame_advanced".into(), ArgValue::Bool(true)),
    ]))
}
