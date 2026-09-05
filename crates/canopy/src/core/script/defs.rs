use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use ruau::{declaration, module, vm::NativeModule};

use crate::{
    FixtureInfo,
    commands::{
        CommandDispatchKind, CommandParamKind, CommandReturnSpec, CommandSet, CommandSpec,
        DeclRegistry,
    },
};

/// Header comment shared by every rendered canopy API surface.
const PREAMBLE: &str = include_str!("../../../luau/preamble.d.luau");

/// Render the Luau definition file from the modules the surface installs.
///
/// Modules render in the order `prepare_finalize` installs them, so the text
/// and the audited surface never drift.
pub(super) fn render_definitions(
    modules: &[Arc<dyn NativeModule>],
    fixtures: &[FixtureInfo],
) -> String {
    let mut output = String::from(PREAMBLE);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    for module in modules {
        output.push('\n');
        output.push_str(module.declaration().render().trim_end());
        output.push('\n');
    }
    if !fixtures.is_empty() {
        output.push_str("\n-- ===== Fixtures =====\n");
        for fixture in fixtures {
            output.push_str(&format!("-- {}: {}\n", fixture.name, fixture.description));
        }
    }
    output
}

/// Group node-dispatched command specs by owner, including default-binding
/// owners.
pub(super) fn owner_command_specs(
    commands: &CommandSet,
    default_binding_owners: &BTreeSet<String>,
) -> BTreeMap<String, Vec<&'static CommandSpec>> {
    let mut owners: BTreeMap<String, Vec<&'static CommandSpec>> = BTreeMap::new();
    for (_, spec) in commands.iter() {
        let CommandDispatchKind::Node { owner } = spec.dispatch else {
            continue;
        };
        owners.entry(owner.to_string()).or_default().push(spec);
    }
    for owner in default_binding_owners {
        owners.entry(owner.clone()).or_default();
    }
    for specs in owners.values_mut() {
        specs.sort_by_key(|spec| spec.id.0);
    }
    owners
}

/// Build a Luau function signature for a command.
pub(super) fn command_fn_sig(spec: &CommandSpec) -> declaration::FunctionSignature {
    let params = spec
        .params
        .iter()
        .filter(|param| param.kind == CommandParamKind::User)
        .map(|param| declaration::Parameter::new(param.name, param.ty.luau_ty()))
        .fold(
            declaration::FunctionSignature::new(),
            declaration::FunctionSignature::param,
        );
    match spec.ret {
        CommandReturnSpec::Unit => params,
        CommandReturnSpec::Value(ty) => params.ret(ty.luau_ty()),
    }
}

/// Register framework-owned record and alias declarations.
pub(super) fn register_framework_declarations(builder: &mut module::Builder) {
    builder.alias(declaration::Alias::new(
        "Point",
        declaration::Type::table([
            declaration::Field::new("x", declaration::Type::Number),
            declaration::Field::new("y", declaration::Type::Number),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "Size",
        declaration::Type::table([
            declaration::Field::new("w", declaration::Type::Number).doc("Width in cells."),
            declaration::Field::new("h", declaration::Type::Number).doc("Height in cells."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "Rect",
        declaration::Type::table([
            declaration::Field::new("x", declaration::Type::Number)
                .doc("Left edge in cells from the origin."),
            declaration::Field::new("y", declaration::Type::Number)
                .doc("Top edge in cells from the origin."),
            declaration::Field::new("w", declaration::Type::Number).doc("Width in cells."),
            declaration::Field::new("h", declaration::Type::Number).doc("Height in cells."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "SemanticIdentity",
        declaration::Type::table([
            declaration::Field::new("scope", declaration::Type::named("NodeId")),
            declaration::Field::new("key", declaration::Type::String),
        ]),
    ));
    builder.alias(
        declaration::Alias::new(
            "NodeInfo",
            declaration::Type::table(node_info_fields(
                declaration::Type::named("NodeId").array(),
                "Direct child nodes in tree order.",
            )),
        )
        .doc("Summary information for a node in the widget tree."),
    );
    builder.alias(declaration::Alias::new(
        "TreeNode",
        declaration::Type::table(node_info_fields(
            declaration::Type::named("TreeNode").array(),
            "Recursive child tree entries in tree order.",
        )),
    ));
    builder.alias(declaration::Alias::new(
        "CommandTarget",
        declaration::Type::table([
            declaration::Field::new(
                "kind",
                declaration::Type::literals(["anchor", "exact", "from", "focus"]),
            ),
            declaration::Field::new("node", declaration::Type::named("NodeId").optional())
                .doc("Required for exact and from; omitted for anchor and focus."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "BindOptions",
        declaration::Type::table([
            declaration::Field::new("description", declaration::Type::String)
                .doc("Required user-facing binding description."),
            declaration::Field::new("mode", declaration::Type::String.optional())
                .doc("Optional input mode. Nil or empty uses the default mode."),
            declaration::Field::new("path", declaration::Type::String.optional())
                .doc("Optional path filter such as `editor/*`."),
            declaration::Field::new(
                "phase",
                declaration::Type::literals(["before_widget", "after_widget"]).optional(),
            )
            .doc("Explicit key dispatch phase. Mouse bindings accept only after_widget."),
            declaration::Field::new("tier", declaration::Type::literals(["global"]).optional())
                .doc("Use the global tier. A global binding cannot name a mode."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "UnbindSelector",
        declaration::Type::table([
            declaration::Field::new("mode", declaration::Type::String.optional())
                .doc("Optional named mode to match."),
            declaration::Field::new("path", declaration::Type::String.optional())
                .doc("Optional exact path filter to match."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "MouseSpec",
        declaration::Type::String,
    ));
    builder.alias(declaration::Alias::new(
        "FixtureInfo",
        declaration::Type::table([
            declaration::Field::new("name", declaration::Type::String)
                .doc("Stable fixture name used by automation tooling."),
            declaration::Field::new("description", declaration::Type::String)
                .doc("Human-readable description of the state the fixture creates."),
        ]),
    ));
    register_binding_info(builder);
    register_command_info(builder);
    register_observation_info(builder);
}

/// Register the active-binding discovery record.
fn register_binding_info(builder: &mut module::Builder) {
    builder.alias(declaration::Alias::new(
        "BindingInfo",
        declaration::Type::table([
            declaration::Field::new("id", declaration::Type::Number)
                .doc("Stable numeric binding identifier."),
            declaration::Field::new("input", declaration::Type::String)
                .doc("Normalized key or mouse spec string."),
            declaration::Field::new("input_type", declaration::Type::literals(["key", "mouse"])),
            declaration::Field::new("owner", declaration::Type::String)
                .doc("Application or framework group owner."),
            declaration::Field::new(
                "scope",
                declaration::Type::literals(["global", "mode", "default", "exclusive"]),
            ),
            declaration::Field::new("mode", declaration::Type::String.optional())
                .doc("Named input mode, when the scope is mode."),
            declaration::Field::new("path", declaration::Type::String)
                .doc("Path filter string used when matching the focused path."),
            declaration::Field::new(
                "phase",
                declaration::Type::literals(["before_widget", "after_widget"]).optional(),
            ),
            declaration::Field::new("command", declaration::Type::String.optional()),
            declaration::Field::new("arguments", declaration::Type::Any.optional()),
            declaration::Field::new(
                "command_target",
                declaration::Type::table([
                    declaration::Field::new(
                        "kind",
                        declaration::Type::literals(["route", "exact", "from", "focus"]),
                    ),
                    declaration::Field::new("node", declaration::Type::named("NodeId").optional()),
                ])
                .optional(),
            ),
            declaration::Field::new("description", declaration::Type::String)
                .doc("Required user-facing description."),
            declaration::Field::new("source", declaration::Type::String.optional())
                .doc("Diagnostic source for application bindings."),
            declaration::Field::new("target", declaration::Type::literals(["script", "command"])),
        ]),
    ));
}

/// Register command discovery records.
fn register_command_info(builder: &mut module::Builder) {
    builder.alias(declaration::Alias::new(
        "CommandParamInfo",
        declaration::Type::table([
            declaration::Field::new("name", declaration::Type::String)
                .doc("Parameter name used for named invocation."),
            declaration::Field::new("kind", declaration::Type::literals(["injected", "user"]))
                .doc("Whether the parameter is injected or user-supplied."),
            declaration::Field::new(
                "requirement",
                declaration::Type::literals(["event", "mouse", "list_row"]).optional(),
            )
            .doc("Injected context kind. Optional parameters do not require its presence."),
            declaration::Field::new("rust_type", declaration::Type::String)
                .doc("Rust type name from command metadata."),
            declaration::Field::new("luau_type", declaration::Type::String)
                .doc("Luau type rendered for this parameter."),
            declaration::Field::new("doc", declaration::Type::String.optional()),
            declaration::Field::new("optional", declaration::Type::Boolean)
                .doc("True when the caller may omit the parameter."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "CommandInfo",
        declaration::Type::table([
            declaration::Field::new("name", declaration::Type::String)
                .doc("Command name relative to its owner table."),
            declaration::Field::new("owner", declaration::Type::String)
                .doc("Widget owner name, or the empty string for free commands."),
            declaration::Field::new("doc", declaration::Type::String.optional()),
            declaration::Field::new(
                "params",
                declaration::Type::named("CommandParamInfo").array(),
            )
            .doc("Parameter metadata in declaration order."),
            declaration::Field::new("ret", declaration::Type::String)
                .doc("Luau return type rendered for this command."),
            declaration::Field::new("ret_doc", declaration::Type::String.optional()),
            declaration::Field::new("available", declaration::Type::Boolean)
                .doc("True when the command can resolve from the current script anchor."),
            declaration::Field::new("target", declaration::Type::named("NodeId").optional())
                .doc("Current target node, when a node command can resolve."),
            declaration::Field::new(
                "status",
                declaration::Type::literals(["enabled", "disabled"]).optional(),
            ),
            declaration::Field::new("disabled_reason", declaration::Type::String.optional()),
            declaration::Field::new(
                "missing_requirements",
                declaration::Type::literals(["event", "mouse", "list_row"]).array(),
            ),
        ]),
    ));
}

/// Register observation and diagnostics records.
fn register_observation_info(builder: &mut module::Builder) {
    register_snapshot_info(builder);
    builder.alias(declaration::Alias::new(
        "ScreenCell",
        declaration::Type::table([
            declaration::Field::new("x", declaration::Type::Number),
            declaration::Field::new("y", declaration::Type::Number),
            declaration::Field::new("text", declaration::Type::String)
                .doc("Rendered grapheme text for this cell."),
            declaration::Field::new("fg", declaration::Type::String)
                .doc("Resolved foreground color as #rrggbb."),
            declaration::Field::new("bg", declaration::Type::String)
                .doc("Resolved background color as #rrggbb."),
            declaration::Field::new("attrs", declaration::Type::String.array())
                .doc("Resolved text attributes such as bold or underline."),
            declaration::Field::new("continuation", declaration::Type::Boolean)
                .doc("True when this cell continues a wide grapheme."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "RouteTraceEntry",
        declaration::Type::table([
            declaration::Field::new("phase", declaration::Type::String),
            declaration::Field::new("node", declaration::Type::named("NodeId").optional()),
            declaration::Field::new("path", declaration::Type::String)
                .doc("Focused path visible to this step."),
            declaration::Field::new("detail", declaration::Type::String),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "AvailableBinding",
        declaration::Type::table([
            declaration::Field::new("id", declaration::Type::Number)
                .doc("Stable numeric binding identifier."),
            declaration::Field::new("input", declaration::Type::String).doc("Normalized key spec."),
            declaration::Field::new(
                "declared_phase",
                declaration::Type::literals(["before_widget", "after_widget"]).optional(),
            ),
            declaration::Field::new(
                "command",
                declaration::Type::table([
                    declaration::Field::new("command", declaration::Type::String),
                    declaration::Field::new("arguments", declaration::Type::Any),
                    declaration::Field::new(
                        "command_target",
                        declaration::Type::table([
                            declaration::Field::new(
                                "kind",
                                declaration::Type::literals(["route", "exact", "from", "focus"]),
                            ),
                            declaration::Field::new(
                                "node",
                                declaration::Type::named("NodeId").optional(),
                            ),
                        ]),
                    ),
                    declaration::Field::new("available", declaration::Type::Boolean),
                    declaration::Field::new(
                        "target",
                        declaration::Type::named("NodeId").optional(),
                    ),
                    declaration::Field::new(
                        "status",
                        declaration::Type::literals(["enabled", "disabled"]).optional(),
                    ),
                    declaration::Field::new(
                        "disabled_reason",
                        declaration::Type::String.optional(),
                    ),
                    declaration::Field::new(
                        "missing_requirements",
                        declaration::Type::literals(["event", "mouse", "list_row"]).array(),
                    ),
                ])
                .optional(),
            ),
            declaration::Field::new("description", declaration::Type::String),
            declaration::Field::new("owner", declaration::Type::String)
                .doc("Application or framework group owner."),
            declaration::Field::new(
                "scope",
                declaration::Type::literals(["global", "mode", "default", "exclusive"]),
            ),
            declaration::Field::new("mode", declaration::Type::String.optional())
                .doc("Named mode when the scope is mode."),
            declaration::Field::new("path", declaration::Type::String),
            declaration::Field::new("route_path", declaration::Type::String)
                .doc("Route path at which this binding wins."),
            declaration::Field::new(
                "phase",
                declaration::Type::literals(["before_widget", "after_widget"]),
            )
            .doc("Phase relative to widget input handling."),
            declaration::Field::new("source", declaration::Type::String.optional())
                .doc("Diagnostic source when available."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "BindingSnapshot",
        declaration::Type::table([
            declaration::Field::new("focus", declaration::Type::named("NodeId")),
            declaration::Field::new("focus_path", declaration::Type::String)
                .doc("Path from root to focus."),
            declaration::Field::new("active_modes", declaration::Type::String.array())
                .doc("Active modes in resolution order."),
            declaration::Field::new("exclusive_group", declaration::Type::String.optional())
                .doc("Active exclusive framework group."),
            declaration::Field::new(
                "bindings",
                declaration::Type::named("AvailableBinding").array(),
            )
            .doc("Effective key bindings for the context."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "ScriptAssertionInfo",
        declaration::Type::table([
            declaration::Field::new("passed", declaration::Type::Boolean),
            declaration::Field::new("message", declaration::Type::String),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "ScriptJournalEntry",
        declaration::Type::table([
            declaration::Field::new("id", declaration::Type::Number).doc("Monotonic journal id."),
            declaration::Field::new("origin", declaration::Type::String)
                .doc("Script origin such as eval, config, or startup."),
            declaration::Field::new("source", declaration::Type::String),
            declaration::Field::new("ok", declaration::Type::Boolean)
                .doc("True when evaluation completed successfully."),
            declaration::Field::new("error", declaration::Type::String.optional())
                .doc("Error message when evaluation failed."),
            declaration::Field::new("logs", declaration::Type::String.array()),
            declaration::Field::new(
                "assertions",
                declaration::Type::named("ScriptAssertionInfo").array(),
            ),
            declaration::Field::new("duration_ms", declaration::Type::Number)
                .doc("Wall-clock duration in milliseconds."),
        ]),
    ));
}

/// Declare detached frame records with explicit display and clipping semantics.
fn register_snapshot_info(builder: &mut module::Builder) {
    use declaration::{Alias, Field, Type};
    builder.alias(Alias::new(
        "SemanticActionStatus",
        Type::table([
            Field::new("kind", Type::literals(["enabled", "disabled"])),
            Field::new("reason", Type::String.optional()),
        ]),
    ));
    builder.alias(Alias::new(
        "WidgetSemantics",
        Type::table([
            Field::new("role", Type::String.optional()),
            Field::new("label", Type::String.optional()),
            Field::new("value", Type::String.optional()),
            Field::new("selected", Type::Boolean.optional()),
            Field::new("selected_keys", Type::Any.array()),
            Field::new(
                "action_status",
                Type::named("SemanticActionStatus").optional(),
            ),
        ]),
    ));
    builder.alias(Alias::new(
        "NodeSnapshot",
        Type::table([
            Field::new("id", Type::named("NodeId")),
            Field::new("parent", Type::named("NodeId").optional()),
            Field::new("children", Type::named("NodeId").array()),
            Field::new("name", Type::String),
            Field::new(
                "semantic_identity",
                Type::named("SemanticIdentity").optional(),
            ),
            Field::new("attached", Type::Boolean),
            Field::new("displayed", Type::Boolean),
            Field::new("intersects_viewport", Type::Boolean),
            Field::new("rect", Type::named("Rect").optional()),
            Field::new("content_rect", Type::named("Rect").optional()),
            Field::new("scroll", Type::named("Point")),
            Field::new("canvas", Type::named("Size")),
            Field::new("focused", Type::Boolean),
            Field::new("semantics", Type::named("WidgetSemantics")),
        ]),
    ));
    builder.alias(Alias::new(
        "FrameSnapshot",
        Type::table([
            Field::new("frame_id", Type::Number),
            Field::new("viewport", Type::named("Size")),
            Field::new("focus", Type::named("NodeId").optional()),
            Field::new("nodes", Type::named("NodeSnapshot").array()),
            Field::new("cells", Type::named("ScreenCell").array().array()),
        ]),
    ));
}

/// Add command-owned declaration dependencies to a generated owner module.
pub(super) fn register_owner_dependencies(
    builder: &mut module::Builder,
    specs: &[&'static CommandSpec],
) {
    let mut registry = DeclRegistry::native_module(builder);
    for spec in specs {
        for param in spec
            .params
            .iter()
            .filter(|param| param.kind == CommandParamKind::User)
        {
            param.ty.luau_decls(&mut registry);
        }
        if let CommandReturnSpec::Value(ty) = spec.ret {
            ty.luau_decls(&mut registry);
        }
    }
}

/// Compose command docs and parameter tags for a command table field.
pub(super) fn command_doc(spec: &CommandSpec) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(long) = spec.doc {
        for line in long.lines().filter(|line| !line.trim().is_empty()) {
            lines.push(line.trim().to_string());
        }
    }
    for param in spec
        .params
        .iter()
        .filter(|param| param.kind == CommandParamKind::User)
    {
        if let Some(doc) = param.ty.doc {
            lines.push(format!("@param {} {doc}", param.name));
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Shared node record fields, with the child representation selected by the
/// alias.
fn node_info_fields(
    children: declaration::Type,
    children_doc: &'static str,
) -> Vec<declaration::Field> {
    vec![
        declaration::Field::new("id", declaration::Type::named("NodeId"))
            .doc("Stable node handle for use in other API calls."),
        declaration::Field::new("name", declaration::Type::String)
            .doc("Widget owner name used in paths and command dispatch."),
        declaration::Field::new(
            "semantic_identity",
            declaration::Type::named("SemanticIdentity").optional(),
        )
        .doc("Application key and explicit arena scope, if registered."),
        declaration::Field::new("focused", declaration::Type::Boolean)
            .doc("True when this node currently owns focus."),
        declaration::Field::new("on_focus_path", declaration::Type::Boolean)
            .doc("True when this node lies on the path to the focused node."),
        declaration::Field::new("hidden", declaration::Type::Boolean)
            .doc("True when the node's hidden flag is set."),
        declaration::Field::new("visible", declaration::Type::Boolean)
            .doc("True when the node is visible."),
        declaration::Field::new("children", children).doc(children_doc),
        declaration::Field::new("rect", declaration::Type::named("Rect").optional())
            .doc("Outer rectangle on screen, or nil for zero-sized nodes."),
        declaration::Field::new("content_rect", declaration::Type::named("Rect").optional())
            .doc("Inner content rectangle after padding, or nil when zero-sized."),
        declaration::Field::new("canvas", declaration::Type::named("Size"))
            .doc("Total scrollable canvas size in content coordinates."),
        declaration::Field::new("scroll", declaration::Type::named("Point"))
            .doc("Current viewport origin within the canvas."),
        declaration::Field::new("accept_focus", declaration::Type::Boolean)
            .doc("True when the widget reports that it can accept focus."),
    ]
}
