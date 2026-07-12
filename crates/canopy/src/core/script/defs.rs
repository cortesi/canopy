use std::collections::{BTreeMap, BTreeSet};

use ruau::{declaration, module};

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
    let mut builder = declaration::Builder::new();
    builder.add_class(declaration::Class::new("NodeId"));
    register_framework_declarations(&mut builder);
    base_api::register_declarations(&mut builder);
    output.push_str(
        &builder
            .build()
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
    let mut builder = declaration::Builder::new();
    builder.add_section("Application Commands");
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
            .build()
            .expect("command declarations are statically valid")
            .render(),
    );

    output
}

/// Build a Luau function signature for a command.
pub(crate) fn command_fn_sig(spec: &CommandSpec) -> declaration::FunctionSignature {
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

/// Return the Luau type recorded in command metadata.
pub fn command_type_to_luau(spec: &CommandTypeSpec) -> String {
    spec.luau_ty().render()
}

/// Register framework-owned record and alias declarations.
fn register_framework_declarations(builder: &mut impl FrameworkDeclarationSink) {
    builder.alias(declaration::Alias::new(
        "Point",
        declaration::Type::table([
            declaration::Field::new("x", declaration::Type::Number).doc("Horizontal position."),
            declaration::Field::new("y", declaration::Type::Number).doc("Vertical position."),
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
    builder.alias(
        declaration::Alias::new(
            "NodeInfo",
            declaration::Type::table([
                declaration::Field::new("id", declaration::Type::named("NodeId"))
                    .doc("Stable node handle for use in other API calls."),
                declaration::Field::new("name", declaration::Type::String)
                    .doc("Widget owner name used in paths and command dispatch."),
                declaration::Field::new("focused", declaration::Type::Boolean)
                    .doc("True when this node currently owns focus."),
                declaration::Field::new("on_focus_path", declaration::Type::Boolean)
                    .doc("True when this node lies on the path to the focused node."),
                declaration::Field::new("hidden", declaration::Type::Boolean)
                    .doc("True when the node's hidden flag is set."),
                declaration::Field::new("visible", declaration::Type::Boolean)
                    .doc("True when the node is visible."),
                declaration::Field::new("children", declaration::Type::named("NodeId").array())
                    .doc("Direct child nodes in tree order."),
                declaration::Field::new("rect", declaration::Type::named("Rect").optional())
                    .doc("Outer rectangle on screen, or nil for zero-sized nodes."),
                declaration::Field::new(
                    "content_rect",
                    declaration::Type::named("Rect").optional(),
                )
                .doc("Inner content rectangle after padding, or nil when zero-sized."),
                declaration::Field::new("canvas", declaration::Type::named("Size"))
                    .doc("Total scrollable canvas size in content coordinates."),
                declaration::Field::new("scroll", declaration::Type::named("Point"))
                    .doc("Current viewport origin within the canvas."),
                declaration::Field::new("accept_focus", declaration::Type::Boolean)
                    .doc("True when the widget reports that it can accept focus."),
            ]),
        )
        .doc("Summary information for a node in the widget tree."),
    );
    builder.alias(declaration::Alias::new(
        "TreeNode",
        declaration::Type::Intersection(vec![
            declaration::Type::named("NodeInfo"),
            declaration::Type::table([declaration::Field::new(
                "children",
                declaration::Type::named("TreeNode").array(),
            )
            .doc("Recursive child tree entries in tree order.")]),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "BindOptions",
        declaration::Type::table([
            declaration::Field::new("mode", declaration::Type::String.optional())
                .doc("Optional input mode. Nil or empty uses the default mode."),
            declaration::Field::new("path", declaration::Type::String.optional())
                .doc("Optional path filter such as `editor/*`."),
            declaration::Field::new("desc", declaration::Type::String.optional())
                .doc("Optional human-readable description used by discovery tooling."),
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

/// Add framework-owned aliases to a generated native module.
pub(crate) fn register_framework_types(builder: &mut module::Builder) {
    register_framework_declarations(builder);
}

/// Target supporting Canopy's framework-owned alias and class declarations.
trait FrameworkDeclarationSink {
    /// Add one alias.
    fn alias(&mut self, alias: declaration::Alias);
}

impl FrameworkDeclarationSink for declaration::Builder {
    fn alias(&mut self, alias: declaration::Alias) {
        Self::add_alias(self, alias);
    }
}

impl FrameworkDeclarationSink for module::Builder {
    fn alias(&mut self, alias: declaration::Alias) {
        Self::alias(self, alias);
    }
}

/// Register the active-binding discovery record.
fn register_binding_info(builder: &mut impl FrameworkDeclarationSink) {
    builder.alias(declaration::Alias::new(
        "BindingInfo",
        declaration::Type::table([
            declaration::Field::new("input", declaration::Type::String)
                .doc("Normalized key or mouse spec string."),
            declaration::Field::new("input_type", declaration::Type::literals(["key", "mouse"]))
                .doc("Input category."),
            declaration::Field::new("mode", declaration::Type::String)
                .doc("Input mode name. The default mode is the empty string."),
            declaration::Field::new("path", declaration::Type::String)
                .doc("Path filter string used when matching the focused path."),
            declaration::Field::new("desc", declaration::Type::String.optional())
                .doc("Optional human-readable description when available."),
            declaration::Field::new("target", declaration::Type::String)
                .doc("Human-readable binding target summary such as `root.quit()` or `luau`."),
        ]),
    ));
}

/// Register command discovery records.
fn register_command_info(builder: &mut impl FrameworkDeclarationSink) {
    builder.alias(declaration::Alias::new(
        "CommandParamInfo",
        declaration::Type::table([
            declaration::Field::new("name", declaration::Type::String)
                .doc("Parameter name used for named invocation."),
            declaration::Field::new("kind", declaration::Type::literals(["injected", "user"]))
                .doc("Whether the parameter is injected or user-supplied."),
            declaration::Field::new("rust_type", declaration::Type::String)
                .doc("Rust type name from command metadata."),
            declaration::Field::new("luau_type", declaration::Type::String)
                .doc("Luau type rendered for this parameter."),
            declaration::Field::new("doc", declaration::Type::String.optional())
                .doc("Optional parameter documentation."),
            declaration::Field::new("optional", declaration::Type::Boolean)
                .doc("True when the caller may omit the parameter."),
            declaration::Field::new("default", declaration::Type::String.optional())
                .doc("Default expression string, when one exists."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "CommandInfo",
        declaration::Type::table([
            declaration::Field::new("name", declaration::Type::String)
                .doc("Command name relative to its owner table."),
            declaration::Field::new("owner", declaration::Type::String)
                .doc("Widget owner name, or the empty string for free commands."),
            declaration::Field::new("doc", declaration::Type::String.optional())
                .doc("Optional command documentation."),
            declaration::Field::new(
                "params",
                declaration::Type::named("CommandParamInfo").array(),
            )
            .doc("Parameter metadata in declaration order."),
            declaration::Field::new("ret", declaration::Type::String)
                .doc("Luau return type rendered for this command."),
            declaration::Field::new("ret_doc", declaration::Type::String.optional())
                .doc("Optional return documentation."),
            declaration::Field::new("available", declaration::Type::Boolean)
                .doc("True when the command can resolve from the current script anchor."),
            declaration::Field::new("target", declaration::Type::named("NodeId").optional())
                .doc("Current target node, when a node command can resolve."),
        ]),
    ));
}

/// Register observation and diagnostics records.
fn register_observation_info(builder: &mut impl FrameworkDeclarationSink) {
    builder.alias(declaration::Alias::new(
        "ScreenCell",
        declaration::Type::table([
            declaration::Field::new("x", declaration::Type::Number).doc("Screen column."),
            declaration::Field::new("y", declaration::Type::Number).doc("Screen row."),
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
            declaration::Field::new("phase", declaration::Type::String).doc("Routing phase label."),
            declaration::Field::new("node", declaration::Type::named("NodeId").optional())
                .doc("Node associated with this route step."),
            declaration::Field::new("path", declaration::Type::String)
                .doc("Focused path visible to this step."),
            declaration::Field::new("detail", declaration::Type::String)
                .doc("Human-readable route detail."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "HelpBinding",
        declaration::Type::table([
            declaration::Field::new("input", declaration::Type::String)
                .doc("Normalized input spec."),
            declaration::Field::new("mode", declaration::Type::String).doc("Input mode."),
            declaration::Field::new("path", declaration::Type::String).doc("Path filter."),
            declaration::Field::new("kind", declaration::Type::literals(["pre", "post"]))
                .doc("Whether the binding is a pre-event override or post-event fallback."),
            declaration::Field::new("target", declaration::Type::String)
                .doc("Human-readable binding target summary."),
            declaration::Field::new("label", declaration::Type::String)
                .doc("Help label for display."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "HelpSnapshot",
        declaration::Type::table([
            declaration::Field::new("focus", declaration::Type::named("NodeId"))
                .doc("Current focus node."),
            declaration::Field::new("focus_path", declaration::Type::String)
                .doc("Path from root to focus."),
            declaration::Field::new("input_mode", declaration::Type::String)
                .doc("Current input mode."),
            declaration::Field::new("bindings", declaration::Type::named("HelpBinding").array())
                .doc("Bindings visible from the current focus context."),
            declaration::Field::new("commands", declaration::Type::named("CommandInfo").array())
                .doc("Commands visible from the current focus context."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "ScriptAssertionInfo",
        declaration::Type::table([
            declaration::Field::new("passed", declaration::Type::Boolean)
                .doc("Whether the assertion passed."),
            declaration::Field::new("message", declaration::Type::String).doc("Assertion message."),
        ]),
    ));
    builder.alias(declaration::Alias::new(
        "ScriptJournalEntry",
        declaration::Type::table([
            declaration::Field::new("id", declaration::Type::Number).doc("Monotonic journal id."),
            declaration::Field::new("origin", declaration::Type::String)
                .doc("Script origin such as eval, config, or startup."),
            declaration::Field::new("source", declaration::Type::String)
                .doc("Evaluated source text."),
            declaration::Field::new("ok", declaration::Type::Boolean)
                .doc("True when evaluation completed successfully."),
            declaration::Field::new("error", declaration::Type::String.optional())
                .doc("Error message when evaluation failed."),
            declaration::Field::new("logs", declaration::Type::String.array())
                .doc("Logs emitted by the script."),
            declaration::Field::new(
                "assertions",
                declaration::Type::named("ScriptAssertionInfo").array(),
            )
            .doc("Assertions emitted by the script."),
            declaration::Field::new("duration_ms", declaration::Type::Number)
                .doc("Wall-clock duration in milliseconds."),
        ]),
    ));
}

/// Register one owner command table and all command-owned declaration dependencies.
fn register_owner_declaration(
    builder: &mut declaration::Builder,
    owner: &str,
    specs: &[&'static CommandSpec],
    has_default_bindings: bool,
) {
    let mut registry = DeclRegistry::new(builder);
    register_command_deps(&mut registry, specs);
    builder.add_section(format!("Commands for widget \"{owner}\""));
    let mut fields = specs
        .iter()
        .map(|spec| {
            let mut field =
                declaration::Field::new(spec.name, declaration::Type::func(command_fn_sig(spec)));
            if let Some(doc) = command_doc(spec) {
                field = field.doc(doc);
            }
            field
        })
        .collect::<Vec<_>>();
    if has_default_bindings {
        fields.push(
            declaration::Field::new(
                "default_bindings",
                declaration::Type::func(declaration::FunctionSignature::new()),
            )
            .doc("Register this widget's default bindings."),
        );
    }
    builder.add_global(declaration::Global::new(
        luau_global_owner_name(owner),
        declaration::Type::table(fields),
    ));
}

/// Register declaration dependencies for command parameters and returns.
fn register_command_deps(registry: &mut DeclRegistry<'_>, specs: &[&'static CommandSpec]) {
    for spec in specs {
        for param in spec
            .params
            .iter()
            .filter(|param| param.kind == CommandParamKind::User)
        {
            param.ty.luau_decls(registry);
        }
        if let CommandReturnSpec::Value(ty) = spec.ret {
            ty.luau_decls(registry);
        }
    }
}

/// Add command-owned declaration dependencies to a generated owner module.
pub(crate) fn register_owner_dependencies(
    builder: &mut module::Builder,
    specs: &[&'static CommandSpec],
) {
    let mut registry = DeclRegistry::native_module(builder);
    register_command_deps(&mut registry, specs);
}

/// Compose command docs and parameter tags for a command table field.
pub(crate) fn command_doc(spec: &CommandSpec) -> Option<String> {
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
