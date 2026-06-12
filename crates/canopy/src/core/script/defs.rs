use std::collections::{BTreeMap, BTreeSet};

use oxau::decl;

use super::{base_api, luau_global_owner_name};
use crate::{
    FixtureInfo,
    commands::{
        CommandDispatchKind, CommandParamKind, CommandReturnSpec, CommandSet, CommandSpec,
        CommandTypeSpec, DeclRegistry,
    },
};

/// Static Luau preamble shared by every canopy app.
const PREAMBLE: &str = include_str!("../../../luau/preamble.d.luau");

/// Return the static Luau preamble declaring the base canopy API surface.
pub(crate) fn preamble() -> String {
    let mut output = String::from(PREAMBLE);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push('\n');
    let mut builder = decl::DeclBuilder::new();
    register_framework_declarations(&mut builder);
    base_api::register_declarations(&mut builder);
    output.push_str(
        &builder
            .finish()
            .expect("framework declarations are statically valid")
            .render(),
    );
    output
}

/// Group node-dispatched command specs by owner, including default-binding owners.
pub(crate) fn owner_command_specs(
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

/// Render one owner's `declare` block for its sorted command specs.
pub(crate) fn render_owner_declaration(
    owner: &str,
    specs: &[&'static CommandSpec],
    has_default_bindings: bool,
) -> String {
    let mut builder = decl::DeclBuilder::new();
    register_owner_declaration(&mut builder, owner, specs, has_default_bindings);
    builder
        .finish()
        .expect("owner command declarations are statically valid")
        .render()
}

/// Render the complete Luau definition file for the current command set.
pub fn render_definitions(
    commands: &CommandSet,
    default_binding_owners: &BTreeSet<String>,
    fixtures: &[FixtureInfo],
) -> String {
    let owners = owner_command_specs(commands, default_binding_owners);

    let mut output = preamble();
    if !fixtures.is_empty() {
        output.push_str("\n-- ===== Fixtures =====\n");
        for fixture in fixtures {
            output.push_str(&format!("-- {}: {}\n", fixture.name, fixture.description));
        }
    }
    let mut builder = decl::DeclBuilder::new();
    builder.section("Application Commands");
    for (owner, specs) in owners {
        register_owner_declaration(
            &mut builder,
            &owner,
            &specs,
            default_binding_owners.contains(&owner),
        );
    }
    output.push('\n');
    output.push_str(
        &builder
            .finish()
            .expect("command declarations are statically valid")
            .render(),
    );

    output
}

/// Build a Luau function signature for a command.
fn command_fn_sig(spec: &CommandSpec) -> decl::FnSig {
    let params = spec
        .params
        .iter()
        .filter(|param| param.kind == CommandParamKind::User)
        .map(|param| decl::Param::new(param.name, param.ty.luau_ty()))
        .fold(decl::FnSig::new(), decl::FnSig::param);
    match spec.ret {
        CommandReturnSpec::Unit => params,
        CommandReturnSpec::Value(ty) => params.ret(ty.luau_ty()),
    }
}

/// Return the Luau type recorded in command metadata.
pub fn command_type_to_luau(spec: &CommandTypeSpec) -> String {
    spec.luau_ty().render()
}

/// Register framework-owned record, class, and alias declarations.
fn register_framework_declarations(builder: &mut decl::DeclBuilder) {
    builder.class(decl::Class::new("NodeId"));
    builder.alias(decl::Alias::new(
        "Point",
        decl::Ty::table([
            decl::Field::new("x", decl::Ty::Number).doc("Horizontal position."),
            decl::Field::new("y", decl::Ty::Number).doc("Vertical position."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "Size",
        decl::Ty::table([
            decl::Field::new("w", decl::Ty::Number).doc("Width in cells."),
            decl::Field::new("h", decl::Ty::Number).doc("Height in cells."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "Rect",
        decl::Ty::table([
            decl::Field::new("x", decl::Ty::Number).doc("Left edge in cells from the origin."),
            decl::Field::new("y", decl::Ty::Number).doc("Top edge in cells from the origin."),
            decl::Field::new("w", decl::Ty::Number).doc("Width in cells."),
            decl::Field::new("h", decl::Ty::Number).doc("Height in cells."),
        ]),
    ));
    builder.alias(
        decl::Alias::new(
            "NodeInfo",
            decl::Ty::table([
                decl::Field::new("id", decl::Ty::named("NodeId"))
                    .doc("Stable node handle for use in other API calls."),
                decl::Field::new("name", decl::Ty::String)
                    .doc("Widget owner name used in paths and command dispatch."),
                decl::Field::new("focused", decl::Ty::Boolean)
                    .doc("True when this node currently owns focus."),
                decl::Field::new("on_focus_path", decl::Ty::Boolean)
                    .doc("True when this node lies on the path to the focused node."),
                decl::Field::new("hidden", decl::Ty::Boolean)
                    .doc("True when the node's hidden flag is set."),
                decl::Field::new("visible", decl::Ty::Boolean)
                    .doc("True when the node is visible."),
                decl::Field::new("children", decl::Ty::named("NodeId").array())
                    .doc("Direct child nodes in tree order."),
                decl::Field::new("rect", decl::Ty::named("Rect").optional())
                    .doc("Outer rectangle on screen, or nil for zero-sized nodes."),
                decl::Field::new("content_rect", decl::Ty::named("Rect").optional())
                    .doc("Inner content rectangle after padding, or nil when zero-sized."),
                decl::Field::new("canvas", decl::Ty::named("Size"))
                    .doc("Total scrollable canvas size in content coordinates."),
                decl::Field::new("scroll", decl::Ty::named("Point"))
                    .doc("Current viewport origin within the canvas."),
                decl::Field::new("accept_focus", decl::Ty::Boolean)
                    .doc("True when the widget reports that it can accept focus."),
            ]),
        )
        .doc("Summary information for a node in the widget tree."),
    );
    builder.alias(decl::Alias::new(
        "TreeNode",
        decl::Ty::Intersection(vec![
            decl::Ty::named("NodeInfo"),
            decl::Ty::table([
                decl::Field::new("children", decl::Ty::named("TreeNode").array())
                    .doc("Recursive child tree entries in tree order."),
            ]),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "BindOptions",
        decl::Ty::table([
            decl::Field::new("mode", decl::Ty::String.optional())
                .doc("Optional input mode. Nil or empty uses the default mode."),
            decl::Field::new("path", decl::Ty::String.optional())
                .doc("Optional path filter such as `editor/*`."),
            decl::Field::new("desc", decl::Ty::String.optional())
                .doc("Optional human-readable description used by discovery tooling."),
        ]),
    ));
    builder.alias(decl::Alias::new("MouseSpec", decl::Ty::String));
    builder.alias(decl::Alias::new(
        "FixtureInfo",
        decl::Ty::table([
            decl::Field::new("name", decl::Ty::String)
                .doc("Stable fixture name used by automation tooling."),
            decl::Field::new("description", decl::Ty::String)
                .doc("Human-readable description of the state the fixture creates."),
        ]),
    ));
    register_binding_info(builder);
    register_command_info(builder);
    register_observation_info(builder);
}

/// Register the active-binding discovery record.
fn register_binding_info(builder: &mut decl::DeclBuilder) {
    builder.alias(decl::Alias::new(
        "BindingInfo",
        decl::Ty::table([
            decl::Field::new("input", decl::Ty::String).doc("Normalized key or mouse spec string."),
            decl::Field::new("input_type", decl::Ty::literals(["key", "mouse"]))
                .doc("Input category."),
            decl::Field::new("mode", decl::Ty::String)
                .doc("Input mode name. The default mode is the empty string."),
            decl::Field::new("path", decl::Ty::String)
                .doc("Path filter string used when matching the focused path."),
            decl::Field::new("desc", decl::Ty::String.optional())
                .doc("Optional human-readable description when available."),
            decl::Field::new("target", decl::Ty::String)
                .doc("Human-readable binding target summary such as `root.quit()` or `luau`."),
        ]),
    ));
}

/// Register command discovery records.
fn register_command_info(builder: &mut decl::DeclBuilder) {
    builder.alias(decl::Alias::new(
        "CommandParamInfo",
        decl::Ty::table([
            decl::Field::new("name", decl::Ty::String)
                .doc("Parameter name used for named invocation."),
            decl::Field::new("kind", decl::Ty::literals(["injected", "user"]))
                .doc("Whether the parameter is injected or user-supplied."),
            decl::Field::new("rust_type", decl::Ty::String)
                .doc("Rust type name from command metadata."),
            decl::Field::new("luau_type", decl::Ty::String)
                .doc("Luau type rendered for this parameter."),
            decl::Field::new("doc", decl::Ty::String.optional())
                .doc("Optional parameter documentation."),
            decl::Field::new("optional", decl::Ty::Boolean)
                .doc("True when the caller may omit the parameter."),
            decl::Field::new("default", decl::Ty::String.optional())
                .doc("Default expression string, when one exists."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "CommandInfo",
        decl::Ty::table([
            decl::Field::new("name", decl::Ty::String)
                .doc("Command name relative to its owner table."),
            decl::Field::new("owner", decl::Ty::String)
                .doc("Widget owner name, or the empty string for free commands."),
            decl::Field::new("doc", decl::Ty::String.optional())
                .doc("Optional command documentation."),
            decl::Field::new("params", decl::Ty::named("CommandParamInfo").array())
                .doc("Parameter metadata in declaration order."),
            decl::Field::new("ret", decl::Ty::String)
                .doc("Luau return type rendered for this command."),
            decl::Field::new("ret_doc", decl::Ty::String.optional())
                .doc("Optional return documentation."),
            decl::Field::new("available", decl::Ty::Boolean)
                .doc("True when the command can resolve from the current script anchor."),
            decl::Field::new("target", decl::Ty::named("NodeId").optional())
                .doc("Current target node, when a node command can resolve."),
        ]),
    ));
}

/// Register observation and diagnostics records.
fn register_observation_info(builder: &mut decl::DeclBuilder) {
    builder.alias(decl::Alias::new(
        "ScreenCell",
        decl::Ty::table([
            decl::Field::new("x", decl::Ty::Number).doc("Screen column."),
            decl::Field::new("y", decl::Ty::Number).doc("Screen row."),
            decl::Field::new("text", decl::Ty::String).doc("Rendered grapheme text for this cell."),
            decl::Field::new("fg", decl::Ty::String).doc("Resolved foreground color as #rrggbb."),
            decl::Field::new("bg", decl::Ty::String).doc("Resolved background color as #rrggbb."),
            decl::Field::new("attrs", decl::Ty::String.array())
                .doc("Resolved text attributes such as bold or underline."),
            decl::Field::new("continuation", decl::Ty::Boolean)
                .doc("True when this cell continues a wide grapheme."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "RouteTraceEntry",
        decl::Ty::table([
            decl::Field::new("phase", decl::Ty::String).doc("Routing phase label."),
            decl::Field::new("node", decl::Ty::named("NodeId").optional())
                .doc("Node associated with this route step."),
            decl::Field::new("path", decl::Ty::String).doc("Focused path visible to this step."),
            decl::Field::new("detail", decl::Ty::String).doc("Human-readable route detail."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "HelpBinding",
        decl::Ty::table([
            decl::Field::new("input", decl::Ty::String).doc("Normalized input spec."),
            decl::Field::new("mode", decl::Ty::String).doc("Input mode."),
            decl::Field::new("path", decl::Ty::String).doc("Path filter."),
            decl::Field::new("kind", decl::Ty::literals(["pre", "post"]))
                .doc("Whether the binding is a pre-event override or post-event fallback."),
            decl::Field::new("target", decl::Ty::String)
                .doc("Human-readable binding target summary."),
            decl::Field::new("label", decl::Ty::String).doc("Help label for display."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "HelpSnapshot",
        decl::Ty::table([
            decl::Field::new("focus", decl::Ty::named("NodeId")).doc("Current focus node."),
            decl::Field::new("focus_path", decl::Ty::String).doc("Path from root to focus."),
            decl::Field::new("input_mode", decl::Ty::String).doc("Current input mode."),
            decl::Field::new("bindings", decl::Ty::named("HelpBinding").array())
                .doc("Bindings visible from the current focus context."),
            decl::Field::new("commands", decl::Ty::named("CommandInfo").array())
                .doc("Commands visible from the current focus context."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "ScriptAssertionInfo",
        decl::Ty::table([
            decl::Field::new("passed", decl::Ty::Boolean).doc("Whether the assertion passed."),
            decl::Field::new("message", decl::Ty::String).doc("Assertion message."),
        ]),
    ));
    builder.alias(decl::Alias::new(
        "ScriptJournalEntry",
        decl::Ty::table([
            decl::Field::new("id", decl::Ty::Number).doc("Monotonic journal id."),
            decl::Field::new("origin", decl::Ty::String)
                .doc("Script origin such as eval, config, or startup."),
            decl::Field::new("source", decl::Ty::String).doc("Evaluated source text."),
            decl::Field::new("ok", decl::Ty::Boolean)
                .doc("True when evaluation completed successfully."),
            decl::Field::new("error", decl::Ty::String.optional())
                .doc("Error message when evaluation failed."),
            decl::Field::new("logs", decl::Ty::String.array()).doc("Logs emitted by the script."),
            decl::Field::new("assertions", decl::Ty::named("ScriptAssertionInfo").array())
                .doc("Assertions emitted by the script."),
            decl::Field::new("duration_ms", decl::Ty::Number)
                .doc("Wall-clock duration in milliseconds."),
        ]),
    ));
}

/// Register one owner command table and all command-owned declaration dependencies.
fn register_owner_declaration(
    builder: &mut decl::DeclBuilder,
    owner: &str,
    specs: &[&'static CommandSpec],
    has_default_bindings: bool,
) {
    register_command_deps(builder, specs);
    builder.section(format!("Commands for widget \"{owner}\""));
    let mut fields = specs
        .iter()
        .map(|spec| {
            let mut field = decl::Field::new(spec.name, decl::Ty::func(command_fn_sig(spec)));
            if let Some(doc) = command_doc(spec) {
                field = field.doc(doc);
            }
            field
        })
        .collect::<Vec<_>>();
    if has_default_bindings {
        fields.push(
            decl::Field::new("default_bindings", decl::Ty::func(decl::FnSig::new()))
                .doc("Register this widget's default bindings."),
        );
    }
    builder.global(decl::Global::new(
        luau_global_owner_name(owner),
        decl::Ty::table(fields),
    ));
}

/// Register declaration dependencies for command parameters and returns.
fn register_command_deps(builder: &mut decl::DeclBuilder, specs: &[&'static CommandSpec]) {
    let mut registry = DeclRegistry::new(builder);
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
fn command_doc(spec: &CommandSpec) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(short) = spec.doc.short {
        lines.push(short.to_string());
    }
    if let Some(long) = spec.doc.long {
        for line in long.lines().filter(|line| !line.trim().is_empty()) {
            if spec.doc.short.is_some_and(|short| short == line.trim()) {
                continue;
            }
            lines.push(line.trim().to_string());
        }
    }
    for param in spec
        .params
        .iter()
        .filter(|param| param.kind == CommandParamKind::User)
    {
        if let Some(doc) = param.doc {
            lines.push(format!("@param {} {doc}", param.name));
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}
