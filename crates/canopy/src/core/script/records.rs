//! Builders that turn live canopy state into script-visible records.

use std::collections::BTreeMap;

use ruau::vm::Scope;

use super::{
    ArgValue, AttrSet, Canopy, Cell, Color, CoreViewContext, NodeId, Point, RectI32, Result,
    ViewContext, commands, error, inputmap, node_list_to_arg, point_to_arg, rect_to_arg,
    size_to_arg,
};
use crate::{
    core::termbuf::TermBuf,
    input,
    input::{KeyRouteExplanation, KeyRouteStep, RouteOutcome, key::Key},
    render::cursor::{CursorMotion, CursorShape, CursorSnapshot},
    runtime::{FrameSnapshot, NodeSnapshot},
    widget::EventOutcome,
};

/// Convert a publication without consulting live widget or node state.
pub(super) fn snapshot_to_arg(frame: &FrameSnapshot) -> ArgValue {
    let size = frame.size();
    let mut rows = Vec::with_capacity(size.h as usize);
    for y in 0..size.h {
        let row = (0..size.w)
            .filter_map(|x| {
                let cell = frame.buffer.get(Point { x, y })?;
                Some(cell_to_arg(x, y, cell))
            })
            .collect();
        rows.push(ArgValue::Array(row));
    }
    ArgValue::Map(BTreeMap::from([
        ("frame_id".into(), ArgValue::UInt(frame.frame_id.0)),
        ("size".into(), size_to_arg(size)),
        (
            "focus".into(),
            frame.focus.map(ArgValue::Node).unwrap_or(ArgValue::Null),
        ),
        (
            "nodes".into(),
            ArgValue::Array(frame.nodes.iter().map(snapshot_node_to_arg).collect()),
        ),
        ("cells".into(), ArgValue::Array(rows)),
        (
            "cursors".into(),
            ArgValue::Array(frame.cursors.iter().map(cursor_to_arg).collect()),
        ),
    ]))
}

/// Convert one painted cursor.
fn cursor_to_arg(cursor: &CursorSnapshot) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("node".into(), ArgValue::Node(cursor.node)),
        ("x".into(), ArgValue::UInt(u64::from(cursor.location.x))),
        ("y".into(), ArgValue::UInt(u64::from(cursor.location.y))),
        ("role".into(), ArgValue::String(cursor.role.clone())),
        (
            "shape".into(),
            ArgValue::String(cursor_shape_name(cursor.look.shape).into()),
        ),
        ("color".into(), color_to_arg(cursor.look.color)),
        (
            "motion".into(),
            ArgValue::String(cursor_motion_name(cursor.look.motion).into()),
        ),
        ("primary".into(), ArgValue::Bool(cursor.primary)),
    ]))
}

/// Return the script name of a cursor shape.
pub(super) fn cursor_shape_name(shape: CursorShape) -> &'static str {
    match shape {
        CursorShape::Block => "block",
        CursorShape::Underline => "underline",
    }
}

/// Return the script name of a cursor motion.
pub(super) fn cursor_motion_name(motion: CursorMotion) -> &'static str {
    match motion {
        CursorMotion::Steady => "steady",
        CursorMotion::Blink { .. } => "blink",
        CursorMotion::Pulse { .. } => "pulse",
    }
}

/// Serialize owned semantic and geometry data, including expired node tokens.
fn snapshot_node_to_arg(node: &NodeSnapshot) -> ArgValue {
    let semantics = &node.semantics;
    let activation_status = semantics
        .activation_status
        .as_ref()
        .map(|status| {
            let reason = match status {
                commands::CommandStatus::Enabled => ArgValue::Null,
                commands::CommandStatus::Disabled(reason) => ArgValue::String(reason.clone()),
            };
            ArgValue::Map(BTreeMap::from([
                ("kind".into(), ArgValue::String(status.label().into())),
                ("reason".into(), reason),
            ]))
        })
        .unwrap_or(ArgValue::Null);
    let (rect, content_rect, scroll, canvas) = match node.view {
        Some(view) => (
            rect_to_arg(view.outer),
            rect_to_arg(view.content),
            point_to_arg(view.scroll),
            size_to_arg(view.canvas),
        ),
        None => (
            ArgValue::Null,
            ArgValue::Null,
            ArgValue::Null,
            ArgValue::Null,
        ),
    };
    ArgValue::Map(BTreeMap::from([
        ("id".into(), ArgValue::Node(node.id)),
        (
            "parent".into(),
            node.parent.map(ArgValue::Node).unwrap_or(ArgValue::Null),
        ),
        (
            "children".into(),
            node_list_to_arg(node.children.iter().copied()),
        ),
        ("name".into(), ArgValue::String(node.name.to_string())),
        (
            "identity".into(),
            node.identity
                .as_ref()
                .map(|identity| {
                    ArgValue::Map(BTreeMap::from([
                        ("scope".into(), ArgValue::Node(identity.scope)),
                        ("key".into(), ArgValue::String(identity.key.clone())),
                    ]))
                })
                .unwrap_or(ArgValue::Null),
        ),
        ("attached".into(), ArgValue::Bool(node.attached)),
        ("displayed".into(), ArgValue::Bool(node.displayed)),
        (
            "intersects_viewport".into(),
            ArgValue::Bool(node.intersects_viewport),
        ),
        ("rect".into(), rect),
        ("content_rect".into(), content_rect),
        ("scroll".into(), scroll),
        ("canvas".into(), canvas),
        ("focused".into(), ArgValue::Bool(node.focused)),
        (
            "semantics".into(),
            ArgValue::Map(BTreeMap::from([
                (
                    "role".into(),
                    semantics
                        .role
                        .clone()
                        .map(ArgValue::String)
                        .unwrap_or(ArgValue::Null),
                ),
                (
                    "label".into(),
                    semantics
                        .label
                        .clone()
                        .map(ArgValue::String)
                        .unwrap_or(ArgValue::Null),
                ),
                (
                    "value".into(),
                    semantics
                        .value
                        .clone()
                        .map(ArgValue::String)
                        .unwrap_or(ArgValue::Null),
                ),
                (
                    "selected".into(),
                    semantics
                        .selected
                        .map(ArgValue::Bool)
                        .unwrap_or(ArgValue::Null),
                ),
                (
                    "selected_keys".into(),
                    ArgValue::Array(semantics.selected_keys.clone()),
                ),
                (
                    "checked_keys".into(),
                    ArgValue::Array(semantics.checked_keys.clone()),
                ),
                ("activation_status".into(), activation_status),
            ])),
        ),
    ]))
}

/// Convert a node into the `NodeInfo` scripting record.
pub(super) fn node_info_to_arg(
    canopy: &Canopy,
    node_id: NodeId,
) -> Result<BTreeMap<String, ArgValue>> {
    let Some(node) = canopy.core.nodes.get(node_id) else {
        return Err(error::Error::NotFound(format!("node {node_id:?}")));
    };
    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
    let rect = if node.view.outer.w == 0 || node.view.outer.h == 0 {
        ArgValue::Null
    } else {
        rect_to_arg(node.view.outer)
    };
    let content_rect = if node.view.content.w == 0 || node.view.content.h == 0 {
        ArgValue::Null
    } else {
        rect_to_arg(node.view.content)
    };
    let accept_focus = canopy.core.accepts_focus(node_id);
    Ok(BTreeMap::from([
        ("id".to_string(), ArgValue::Node(node_id)),
        (
            "parent".to_string(),
            node.parent.map_or(ArgValue::Null, ArgValue::Node),
        ),
        ("name".to_string(), ArgValue::String(node.name.to_string())),
        (
            "identity".to_string(),
            root_ctx
                .identity(node_id)
                .map(|identity| {
                    ArgValue::Map(BTreeMap::from([
                        ("scope".to_string(), ArgValue::Node(identity.scope)),
                        ("key".to_string(), ArgValue::String(identity.key)),
                    ]))
                })
                .unwrap_or(ArgValue::Null),
        ),
        (
            "focused".to_string(),
            ArgValue::Bool(root_ctx.focused_node() == Some(node_id)),
        ),
        (
            "on_focus_path".to_string(),
            ArgValue::Bool(root_ctx.is_on_focus_path(node_id)),
        ),
        ("hidden".to_string(), ArgValue::Bool(node.hidden)),
        (
            "children".to_string(),
            node_list_to_arg(node.children.iter().copied()),
        ),
        ("rect".to_string(), rect),
        ("content_rect".to_string(), content_rect),
        ("canvas".to_string(), size_to_arg(node.canvas)),
        ("scroll".to_string(), point_to_arg(node.scroll)),
        ("accept_focus".to_string(), ArgValue::Bool(accept_focus)),
    ]))
}

/// Convert registered fixtures into a scripting array.
pub(super) fn fixtures_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .fixture_infos()
            .into_iter()
            .map(|fixture| {
                ArgValue::Map(BTreeMap::from([
                    ("name".to_string(), ArgValue::String(fixture.name)),
                    (
                        "description".to_string(),
                        ArgValue::String(fixture.description),
                    ),
                ]))
            })
            .collect(),
    )
}

/// Label for a script-declared callback: the caller's script line when the
/// declaration site is known, so binding introspection points at the source.
pub(super) fn script_callback_label(scope: &Scope<'_>) -> String {
    match scope.caller_location(0) {
        Some(location) => format!("script:{}", location.line),
        None => "script".to_string(),
    }
}

/// Convert one binding record into its scripting record.
pub(super) fn binding_info_to_arg(binding: &inputmap::BindingRecord) -> ArgValue {
    let input_type = match binding.input {
        inputmap::InputSpec::Key(_) => "key",
        inputmap::InputSpec::Mouse(_) => "mouse",
    };
    let mut record = BTreeMap::from([
        (
            "input".to_string(),
            ArgValue::String(binding.input.to_string()),
        ),
        (
            "input_type".to_string(),
            ArgValue::String(input_type.to_string()),
        ),
        ("id".to_string(), ArgValue::UInt(binding.id.as_u64())),
        (
            "path".to_string(),
            ArgValue::String(binding.path_filter().to_string()),
        ),
        (
            "description".to_string(),
            ArgValue::String(binding.description.clone()),
        ),
        (
            "show_in_help".to_string(),
            ArgValue::Bool(binding.show_in_help),
        ),
        (
            "action".to_string(),
            ArgValue::String(binding.action.label().to_string()),
        ),
        (
            "phase".to_string(),
            ArgValue::String(binding.phase.label().to_string()),
        ),
    ]);
    insert_tier(&mut record, &binding.tier);
    if let Some(intent) = binding.action.intent() {
        record.insert("intent".to_string(), ArgValue::String(intent.to_string()));
    }
    if let inputmap::BindingAction::Command(call) = &binding.action {
        insert_command_call(&mut record, call);
    }
    if let inputmap::BindingAction::Menu(mode) = &binding.action {
        record.insert("menu".to_string(), ArgValue::String(mode.clone()));
    }
    if let Some(source) = &binding.source {
        record.insert("source".to_string(), ArgValue::String(source.clone()));
    }
    ArgValue::Map(record)
}

/// Add a binding tier, with its mode or framework group, to an observation
/// record.
fn insert_tier(record: &mut BTreeMap<String, ArgValue>, tier: &inputmap::BindingTier) {
    record.insert(
        "tier".to_string(),
        ArgValue::String(tier.label().to_string()),
    );
    match tier {
        inputmap::BindingTier::Framework(group) => {
            record.insert("group".to_string(), ArgValue::String(group.to_string()));
        }
        inputmap::BindingTier::Mode(mode) => {
            record.insert("mode".to_string(), ArgValue::String(mode.clone()));
        }
        inputmap::BindingTier::Global | inputmap::BindingTier::Default => {}
    }
}

/// Add a stored command call's arguments and target policy to an observation
/// record.
fn insert_command_call(record: &mut BTreeMap<String, ArgValue>, call: &commands::CommandCall) {
    record.insert(
        "command".to_string(),
        ArgValue::String(call.id.0.to_string()),
    );
    record.insert(
        "arguments".to_string(),
        match &call.args {
            commands::CommandArgs::Positional(values) => ArgValue::Array(values.clone()),
            commands::CommandArgs::Named(fields) => ArgValue::Map(fields.clone()),
        },
    );
    let (kind, node) = match call.target {
        None => ("route", None),
        Some(commands::CommandTarget::Exact(node)) => ("exact", Some(node)),
        Some(commands::CommandTarget::From(node)) => ("from", Some(node)),
        Some(commands::CommandTarget::Focus) => ("focus", None),
    };
    let mut target = BTreeMap::from([("kind".to_string(), ArgValue::String(kind.to_string()))]);
    if let Some(node) = node {
        target.insert("node".to_string(), ArgValue::Node(node));
    }
    record.insert("command_target".to_string(), ArgValue::Map(target));
}

/// Add availability and eligibility without conflating them with target
/// resolution or missing context.
///
/// An unregistered command has no availability, and reports itself as
/// unavailable.
fn insert_command_availability(
    record: &mut BTreeMap<String, ArgValue>,
    availability: Option<&commands::CommandAvailability>,
) {
    let resolution = availability.and_then(|availability| availability.resolution);
    let missing = availability.map_or(&[][..], |availability| {
        availability.missing_requirements.as_slice()
    });
    record.insert(
        "available".to_string(),
        ArgValue::Bool(resolution.is_some()),
    );
    if let Some(status) = availability.and_then(|availability| availability.status.as_ref()) {
        record.insert(
            "status".to_string(),
            ArgValue::String(status.label().to_string()),
        );
        if let commands::CommandStatus::Disabled(reason) = status {
            record.insert(
                "disabled_reason".to_string(),
                ArgValue::String(reason.clone()),
            );
        }
    }
    record.insert(
        "missing_requirements".to_string(),
        ArgValue::Array(
            missing
                .iter()
                .map(|requirement| ArgValue::String(requirement.as_str().to_string()))
                .collect(),
        ),
    );
    if let Some(resolution) = resolution {
        record.insert("target".to_string(), ArgValue::Node(resolution.target()));
    }
}

/// Convert a command parameter specification into its scripting record.
fn command_param_to_arg(param: &commands::CommandParamSpec) -> ArgValue {
    let mut record = BTreeMap::from([
        ("name".to_string(), ArgValue::String(param.name.to_string())),
        (
            "kind".to_string(),
            ArgValue::String(
                match param.kind {
                    commands::CommandParamKind::Injected => "injected",
                    commands::CommandParamKind::User => "user",
                }
                .to_string(),
            ),
        ),
        (
            "rust_type".to_string(),
            ArgValue::String(param.ty.rust.to_string()),
        ),
        (
            "luau_type".to_string(),
            ArgValue::String(param.ty.luau_ty().render()),
        ),
        ("optional".to_string(), ArgValue::Bool(param.optional)),
    ]);
    if let Some(requirement) = param.requirement.and_then(|requirement| requirement()) {
        record.insert(
            "requirement".to_string(),
            ArgValue::String(requirement.as_str().to_string()),
        );
    }
    if let Some(doc) = param.ty.doc {
        record.insert("doc".to_string(), ArgValue::String(doc.to_string()));
    }
    ArgValue::Map(record)
}

/// Convert a command specification into its scripting record.
pub(super) fn command_info_to_arg(availability: &commands::CommandAvailability) -> ArgValue {
    let spec = availability.spec;
    let mut record = BTreeMap::from([
        ("name".to_string(), ArgValue::String(spec.name.to_string())),
        (
            "owner".to_string(),
            ArgValue::String(spec.owner.to_string()),
        ),
        (
            "params".to_string(),
            ArgValue::Array(spec.params.iter().map(command_param_to_arg).collect()),
        ),
        (
            "ret".to_string(),
            ArgValue::String(match spec.ret {
                commands::CommandReturnSpec::Unit => "()".to_string(),
                commands::CommandReturnSpec::Value(ty) => ty.luau_ty().render(),
            }),
        ),
    ]);
    insert_command_availability(&mut record, Some(availability));
    if let Some(doc) = spec.doc {
        record.insert("doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let commands::CommandReturnSpec::Value(ty) = spec.ret
        && let Some(doc) = ty.doc
    {
        record.insert("ret_doc".to_string(), ArgValue::String(doc.to_string()));
    }
    ArgValue::Map(record)
}

/// Refresh the app snapshot and borrow its rendered buffer.
fn rendered_buffer(canopy: &mut Canopy) -> Result<&TermBuf> {
    canopy.prepare()?;
    canopy
        .published_buf()
        .ok_or_else(|| error::Error::script("screen unavailable before render"))
}

/// Convert one terminal cell into a scripting record.
fn cell_to_arg(x: u32, y: u32, cell: &Cell) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("x".to_string(), ArgValue::UInt(u64::from(x))),
        ("y".to_string(), ArgValue::UInt(u64::from(y))),
        ("text".to_string(), ArgValue::String(cell.rendered_text())),
        ("fg".to_string(), color_to_arg(cell.style.fg)),
        ("bg".to_string(), color_to_arg(cell.style.bg)),
        ("attrs".to_string(), attrs_to_arg(cell.style.attrs)),
        (
            "continuation".to_string(),
            ArgValue::Bool(cell.continuation),
        ),
    ]))
}

/// Convert a color to a stable RGB string.
fn color_to_arg(color: Color) -> ArgValue {
    let (r, g, b) = color.rgb();
    ArgValue::String(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Convert text attributes to stable lowercase names.
fn attrs_to_arg(attrs: AttrSet) -> ArgValue {
    ArgValue::Array(
        attrs
            .named()
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(name, _)| ArgValue::String(name.to_owned()))
            .collect(),
    )
}

/// Return the rendered screen text inside a signed rectangle, clipped to the
/// screen.
pub(super) fn screen_text_for_rect(canopy: &mut Canopy, rect: RectI32) -> Result<String> {
    let buffer = rendered_buffer(canopy)?;
    let Some(rect) = rect.intersect_rect(buffer.rect()) else {
        return Ok(String::new());
    };
    let mut rows = Vec::with_capacity(rect.h as usize);
    for y in rect.tl.y..rect.tl.y + rect.h {
        let mut row = String::new();
        for x in rect.tl.x..rect.tl.x + rect.w {
            let cell = buffer
                .get(Point { x, y })
                .expect("buffer coordinates should always be valid");
            row.push_str(&cell.rendered_text());
        }
        rows.push(row);
    }
    Ok(rows.join("\n"))
}

/// Return the rendered screen as styled text.
pub(super) fn screen_capture(canopy: &mut Canopy) -> Result<ArgValue> {
    let capture = rendered_buffer(canopy)?.capture();
    Ok(commands::SerdeArg(capture).try_to_arg_value()?)
}

/// Return the rendered screen as plain text.
pub(super) fn screen_text(canopy: &mut Canopy) -> Result<String> {
    let buffer = rendered_buffer(canopy)?;
    Ok(buffer.screen_text())
}

/// Convert the retained notices to scripting records, oldest first.
pub(super) fn notices_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .notices()
            .iter()
            .map(|notice| {
                let mut record = BTreeMap::from([
                    (
                        "message".to_string(),
                        ArgValue::String(notice.message.clone()),
                    ),
                    (
                        "kind".to_string(),
                        ArgValue::String(notice.kind.as_str().to_string()),
                    ),
                    (
                        "source".to_string(),
                        ArgValue::String(notice.source.label().to_string()),
                    ),
                ]);
                if let Some(node) = notice.node {
                    record.insert("node".to_string(), ArgValue::Node(node));
                }
                ArgValue::Map(record)
            })
            .collect(),
    )
}

/// Convert the most recent route trace to scripting records.
pub(super) fn route_trace_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .route_trace()
            .iter()
            .map(|entry| {
                let mut record = BTreeMap::from([
                    (
                        "kind".to_string(),
                        ArgValue::String(entry.kind.label().to_string()),
                    ),
                    ("path".to_string(), ArgValue::String(entry.path.clone())),
                    ("detail".to_string(), ArgValue::String(entry.detail.clone())),
                ]);
                if let Some(node) = entry.node {
                    record.insert("node".to_string(), ArgValue::Node(node));
                }
                ArgValue::Map(record)
            })
            .collect(),
    )
}

/// Convert one effective binding to a scripting record.
///
/// Key and mouse records carry the same fields, so one conversion serves both
/// and the two lists cannot drift apart.
fn available_binding_to_arg<I: ToString>(binding: input::AvailableBinding<I>) -> ArgValue {
    let mut record = BTreeMap::from([
        ("id".to_string(), ArgValue::UInt(binding.id.as_u64())),
        (
            "input".to_string(),
            ArgValue::String(binding.input.to_string()),
        ),
        (
            "description".to_string(),
            ArgValue::String(binding.description),
        ),
        (
            "show_in_help".to_string(),
            ArgValue::Bool(binding.show_in_help),
        ),
        ("path".to_string(), ArgValue::String(binding.path_filter)),
        (
            "route_path".to_string(),
            ArgValue::String(binding.route_path.to_string()),
        ),
        (
            "action".to_string(),
            ArgValue::String(binding.action.label().to_string()),
        ),
        (
            "phase".to_string(),
            ArgValue::String(binding.phase.label().to_string()),
        ),
    ]);
    insert_tier(&mut record, &binding.tier);
    if let Some(intent) = binding.intent {
        record.insert("intent".to_string(), ArgValue::String(intent.to_string()));
    }
    if let Some(command) = binding.command {
        let mut detail = BTreeMap::new();
        insert_command_call(&mut detail, &command.call);
        insert_command_availability(&mut detail, command.availability.as_ref());
        record.insert("command".to_string(), ArgValue::Map(detail));
    }
    if let Some(source) = binding.source {
        record.insert("source".to_string(), ArgValue::String(source));
    }
    ArgValue::Map(record)
}

/// Convert a contextual binding snapshot to a scripting record.
pub(super) fn available_bindings_to_arg(
    canopy: &Canopy,
    requested: Option<NodeId>,
) -> Result<ArgValue> {
    let snapshot = canopy.available_bindings(requested)?;
    Ok(ArgValue::Map(BTreeMap::from([
        ("focus".to_string(), ArgValue::Node(snapshot.focus)),
        (
            "focus_path".to_string(),
            ArgValue::String(snapshot.focus_path.to_string()),
        ),
        (
            "active_modes".to_string(),
            ArgValue::Array(
                snapshot
                    .active_modes
                    .into_iter()
                    .map(ArgValue::String)
                    .collect(),
            ),
        ),
        (
            "bindings".to_string(),
            ArgValue::Array(
                snapshot
                    .bindings
                    .into_iter()
                    .map(available_binding_to_arg)
                    .collect(),
            ),
        ),
        (
            "mouse_bindings".to_string(),
            ArgValue::Array(
                snapshot
                    .mouse_bindings
                    .into_iter()
                    .map(available_binding_to_arg)
                    .collect(),
            ),
        ),
        (
            "transient_mode".to_string(),
            snapshot
                .transient_mode
                .map_or(ArgValue::Null, ArgValue::String),
        ),
        (
            "framework_group".to_string(),
            snapshot.framework_group.map_or(ArgValue::Null, |group| {
                ArgValue::String(group.as_str().to_string())
            }),
        ),
    ])))
}

/// Convert the script journal to scripting records.
pub(super) fn script_journal_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .script_journal()
            .iter()
            .map(|entry| {
                ArgValue::Map(BTreeMap::from([
                    ("id".to_string(), ArgValue::UInt(entry.id)),
                    (
                        "origin".to_string(),
                        ArgValue::String(entry.origin.to_string()),
                    ),
                    ("source".to_string(), ArgValue::String(entry.source.clone())),
                    ("ok".to_string(), ArgValue::Bool(entry.ok)),
                    (
                        "error".to_string(),
                        entry
                            .error
                            .clone()
                            .map(ArgValue::String)
                            .unwrap_or(ArgValue::Null),
                    ),
                    (
                        "logs".to_string(),
                        ArgValue::Array(entry.logs.iter().cloned().map(ArgValue::String).collect()),
                    ),
                    (
                        "assertions".to_string(),
                        ArgValue::Array(
                            entry
                                .assertions
                                .iter()
                                .map(|assertion| {
                                    ArgValue::Map(BTreeMap::from([
                                        ("passed".to_string(), ArgValue::Bool(assertion.passed)),
                                        (
                                            "message".to_string(),
                                            ArgValue::String(assertion.message.clone()),
                                        ),
                                    ]))
                                })
                                .collect(),
                        ),
                    ),
                    ("duration_ms".to_string(), ArgValue::UInt(entry.duration_ms)),
                ]))
            })
            .collect(),
    )
}

/// Convert one prospective key-route explanation to a scripting record.
pub(super) fn key_explanation_to_arg(
    canopy: &Canopy,
    requested: Option<NodeId>,
    key: Key,
) -> Result<ArgValue> {
    let explanation = canopy.explain_key(requested, key)?;
    Ok(key_route_explanation_to_arg(explanation))
}

/// Convert an owned explanation into its stable scripting shape.
fn key_route_explanation_to_arg(explanation: KeyRouteExplanation) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        (
            "key".to_string(),
            ArgValue::String(explanation.key.to_string()),
        ),
        ("focus".to_string(), ArgValue::Node(explanation.focus)),
        (
            "focus_path".to_string(),
            ArgValue::String(explanation.focus_path.to_string()),
        ),
        (
            "steps".to_string(),
            ArgValue::Array(
                explanation
                    .steps
                    .iter()
                    .map(key_route_step_to_arg)
                    .collect(),
            ),
        ),
        (
            "outcome".to_string(),
            key_route_outcome_to_arg(explanation.outcome),
        ),
    ]))
}

/// Convert one examined route step.
fn key_route_step_to_arg(step: &KeyRouteStep) -> ArgValue {
    let mut record = BTreeMap::from([
        ("node".to_string(), ArgValue::Node(step.node)),
        ("path".to_string(), ArgValue::String(step.path.to_string())),
        (
            "widget".to_string(),
            ArgValue::String(event_outcome_label(step.widget).to_string()),
        ),
    ]);
    if let Some(binding) = step.binding {
        record.insert(
            "binding".to_string(),
            ArgValue::Map(BTreeMap::from([
                ("id".to_string(), ArgValue::UInt(binding.id.as_u64())),
                (
                    "action".to_string(),
                    ArgValue::String(binding.kind.label().to_string()),
                ),
                (
                    "phase".to_string(),
                    ArgValue::String(binding.phase.label().to_string()),
                ),
            ])),
        );
    }
    ArgValue::Map(record)
}

/// Return the script label for a widget event outcome.
fn event_outcome_label(outcome: EventOutcome) -> &'static str {
    match outcome {
        EventOutcome::Handle => "handle",
        EventOutcome::Ignore => "ignore",
    }
}

/// Convert the decisive route outcome.
fn key_route_outcome_to_arg(outcome: RouteOutcome) -> ArgValue {
    let (kind, winner, widget) = match outcome {
        RouteOutcome::Binding(winner) => ("binding", Some(winner), None),
        RouteOutcome::Transient(winner) => ("transient", Some(winner), None),
        RouteOutcome::Widget { node, path } => ("widget", None, Some((node, path))),
        RouteOutcome::TransientDismiss => ("transient_dismiss", None, None),
        RouteOutcome::Unhandled => ("unhandled", None, None),
    };
    let mut record = BTreeMap::from([("kind".to_string(), ArgValue::String(kind.to_string()))]);
    if let Some(winner) = winner {
        record.extend([
            (
                "binding".to_string(),
                ArgValue::UInt(winner.binding.as_u64()),
            ),
            ("node".to_string(), ArgValue::Node(winner.node)),
            (
                "path".to_string(),
                ArgValue::String(winner.path.to_string()),
            ),
            (
                "action".to_string(),
                ArgValue::String(winner.kind.label().to_string()),
            ),
            (
                "phase".to_string(),
                ArgValue::String(winner.phase.label().to_string()),
            ),
        ]);
    }
    if let Some((node, path)) = widget {
        record.insert("node".to_string(), ArgValue::Node(node));
        record.insert("path".to_string(), ArgValue::String(path.to_string()));
    }
    ArgValue::Map(record)
}
