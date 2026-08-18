//! Builders that turn live canopy state into script-visible records.

use std::collections::BTreeMap;

use ruau::vm::Scope;

use super::{
    ArgValue, AttrSet, Canopy, Cell, Color, CommandSpec, CoreViewContext, NodeId, Point, RectI32,
    Result, ViewContext, commands, defs, error, inputmap, node_id_to_arg, node_list_to_arg,
    point_to_arg, rect_to_arg, size_to_arg, widget_access,
};

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
    let accept_focus = widget_access::accepts_focus(&canopy.core, node_id);
    Ok(BTreeMap::from([
        ("id".to_string(), node_id_to_arg(node_id)),
        ("name".to_string(), ArgValue::String(node.name.to_string())),
        (
            "focused".to_string(),
            ArgValue::Bool(root_ctx.node_is_focused(node_id)),
        ),
        (
            "on_focus_path".to_string(),
            ArgValue::Bool(root_ctx.node_is_on_focus_path(node_id)),
        ),
        ("hidden".to_string(), ArgValue::Bool(node.hidden)),
        ("visible".to_string(), ArgValue::Bool(!node.hidden)),
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

/// Convert a node into a recursive tree record.
pub(super) fn tree_node_to_arg(canopy: &Canopy, node_id: NodeId) -> Result<ArgValue> {
    let mut info = node_info_to_arg(canopy, node_id)?;
    let Some(node) = canopy.core.nodes.get(node_id) else {
        return Err(error::Error::NotFound(format!("node {node_id:?}")));
    };
    let children = node
        .children
        .iter()
        .copied()
        .map(|child_id| tree_node_to_arg(canopy, child_id))
        .collect::<Result<Vec<_>>>()?;
    info.insert("children".to_string(), ArgValue::Array(children));
    Ok(ArgValue::Map(info))
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
            "owner".to_string(),
            ArgValue::String(match binding.owner {
                inputmap::BindingOwner::Application => "application".to_string(),
                inputmap::BindingOwner::Framework(group) => format!("framework:{group}"),
            }),
        ),
        (
            "scope".to_string(),
            ArgValue::String(binding.scope.label().to_string()),
        ),
        (
            "path".to_string(),
            ArgValue::String(binding.path_filter().to_string()),
        ),
        (
            "description".to_string(),
            ArgValue::String(binding.description.clone()),
        ),
        (
            "target".to_string(),
            ArgValue::String(binding.target.label().to_string()),
        ),
    ]);
    if let Some(mode) = binding.scope.mode() {
        record.insert("mode".to_string(), ArgValue::String(mode.to_string()));
    }
    if let Some(source) = &binding.source {
        record.insert("source".to_string(), ArgValue::String(source.clone()));
    }
    ArgValue::Map(record)
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
            ArgValue::String(defs::command_type_to_luau(&param.ty)),
        ),
        ("optional".to_string(), ArgValue::Bool(param.optional)),
    ]);
    if let Some(doc) = param.doc {
        record.insert("doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let Some(default) = param.default {
        record.insert("default".to_string(), ArgValue::String(default.to_string()));
    }
    ArgValue::Map(record)
}

/// Convert a command specification into its scripting record.
pub(super) fn command_info_to_arg(
    spec: &CommandSpec,
    resolution: Option<commands::CommandResolution>,
) -> ArgValue {
    let owner = match spec.dispatch {
        commands::CommandDispatchKind::Node { owner } => owner,
        commands::CommandDispatchKind::Free => "",
    };
    let mut record = BTreeMap::from([
        ("name".to_string(), ArgValue::String(spec.name.to_string())),
        ("owner".to_string(), ArgValue::String(owner.to_string())),
        (
            "params".to_string(),
            ArgValue::Array(spec.params.iter().map(command_param_to_arg).collect()),
        ),
        (
            "ret".to_string(),
            ArgValue::String(match spec.ret {
                commands::CommandReturnSpec::Unit => "()".to_string(),
                commands::CommandReturnSpec::Value(ty) => defs::command_type_to_luau(&ty),
            }),
        ),
        (
            "available".to_string(),
            ArgValue::Bool(resolution.is_some()),
        ),
    ]);
    if let Some(doc) = spec.doc.long {
        record.insert("doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let commands::CommandReturnSpec::Value(ty) = spec.ret
        && let Some(doc) = ty.doc
    {
        record.insert("ret_doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let Some(target) = resolution.and_then(commands::CommandResolution::target) {
        record.insert("target".to_string(), node_id_to_arg(target));
    }
    ArgValue::Map(record)
}

/// Convert the current rendered screen buffer into its scripting record.
pub(super) fn screen_to_arg(canopy: &mut Canopy) -> Result<ArgValue> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    Ok(ArgValue::Array(
        buffer
            .rows()
            .into_iter()
            .map(|row| ArgValue::Array(row.into_iter().map(ArgValue::String).collect()))
            .collect(),
    ))
}

/// Convert the current rendered screen buffer into styled cell records.
pub(super) fn screen_cells_to_arg(canopy: &mut Canopy) -> Result<ArgValue> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    let size = buffer.size();
    let mut rows = Vec::with_capacity(size.h as usize);
    for y in 0..size.h {
        let mut row = Vec::with_capacity(size.w as usize);
        for x in 0..size.w {
            let cell = buffer
                .get(Point { x, y })
                .expect("buffer coordinates should always be valid");
            row.push(cell_to_arg(x, y, cell));
        }
        rows.push(ArgValue::Array(row));
    }
    Ok(ArgValue::Array(rows))
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
    let mut names = Vec::new();
    if attrs.bold {
        names.push(ArgValue::String("bold".to_string()));
    }
    if attrs.crossedout {
        names.push(ArgValue::String("crossedout".to_string()));
    }
    if attrs.dim {
        names.push(ArgValue::String("dim".to_string()));
    }
    if attrs.italic {
        names.push(ArgValue::String("italic".to_string()));
    }
    if attrs.overline {
        names.push(ArgValue::String("overline".to_string()));
    }
    if attrs.underline {
        names.push(ArgValue::String("underline".to_string()));
    }
    ArgValue::Array(names)
}

/// Return the rendered screen text inside a signed rectangle, clipped to the screen.
pub(super) fn screen_text_for_rect(canopy: &mut Canopy, rect: RectI32) -> Result<String> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
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

/// Return the rendered screen as plain text.
pub(super) fn screen_text(canopy: &mut Canopy) -> Result<String> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    Ok(buffer.screen_text())
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
                        "phase".to_string(),
                        ArgValue::String(entry.phase.as_str().to_string()),
                    ),
                    ("path".to_string(), ArgValue::String(entry.path.clone())),
                    ("detail".to_string(), ArgValue::String(entry.detail.clone())),
                ]);
                if let Some(node) = entry.node {
                    record.insert("node".to_string(), node_id_to_arg(node));
                }
                ArgValue::Map(record)
            })
            .collect(),
    )
}

/// Convert a contextual binding snapshot to a scripting record.
pub(super) fn available_bindings_to_arg(
    canopy: &Canopy,
    requested: Option<NodeId>,
) -> Result<ArgValue> {
    let snapshot = canopy.available_bindings(requested)?;
    let bindings = snapshot
        .bindings
        .iter()
        .map(|binding| {
            let mut record = BTreeMap::from([
                ("id".to_string(), ArgValue::UInt(binding.id.as_u64())),
                (
                    "input".to_string(),
                    ArgValue::String(binding.key.to_string()),
                ),
                (
                    "description".to_string(),
                    ArgValue::String(binding.description.clone()),
                ),
                (
                    "owner".to_string(),
                    ArgValue::String(match binding.owner {
                        inputmap::BindingOwner::Application => "application".to_string(),
                        inputmap::BindingOwner::Framework(group) => {
                            format!("framework:{group}")
                        }
                    }),
                ),
                (
                    "scope".to_string(),
                    ArgValue::String(binding.scope.label().to_string()),
                ),
                (
                    "path".to_string(),
                    ArgValue::String(binding.path_filter.clone()),
                ),
                (
                    "route_path".to_string(),
                    ArgValue::String(binding.route_path.to_string()),
                ),
                (
                    "phase".to_string(),
                    ArgValue::String(
                        match binding.phase {
                            inputmap::BindingPhase::BeforeWidget => "before_widget",
                            inputmap::BindingPhase::AfterIgnore => "after_ignore",
                        }
                        .to_string(),
                    ),
                ),
            ]);
            if let Some(mode) = binding.scope.mode() {
                record.insert("mode".to_string(), ArgValue::String(mode.to_string()));
            }
            if let Some(source) = &binding.source {
                record.insert("source".to_string(), ArgValue::String(source.clone()));
            }
            ArgValue::Map(record)
        })
        .collect();
    Ok(ArgValue::Map(BTreeMap::from([
        ("focus".to_string(), node_id_to_arg(snapshot.focus)),
        (
            "focus_path".to_string(),
            ArgValue::String(snapshot.focus_path.to_string()),
        ),
        (
            "active_modes".to_string(),
            ArgValue::Array(
                snapshot
                    .active_modes
                    .iter()
                    .cloned()
                    .map(ArgValue::String)
                    .collect(),
            ),
        ),
        ("bindings".to_string(), ArgValue::Array(bindings)),
        (
            "exclusive_group".to_string(),
            snapshot.exclusive_group.map_or(ArgValue::Null, |group| {
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
                    ("origin".to_string(), ArgValue::String(entry.origin.clone())),
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
