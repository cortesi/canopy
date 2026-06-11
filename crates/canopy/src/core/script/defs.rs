use std::collections::{BTreeMap, BTreeSet};

use super::{base_api, luau_global_owner_name};
use crate::{
    FixtureInfo,
    commands::{
        CommandDispatchKind, CommandParamKind, CommandReturnSpec, CommandSet, CommandSpec,
        CommandTypeSpec,
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
    output.push_str(&base_api::render_declaration());
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
    let mut output = String::new();
    output.push_str(&format!("--- Commands for widget \"{owner}\"\n"));
    output.push_str(&format!("declare {}: {{\n", luau_global_owner_name(owner)));

    for spec in specs {
        if let Some(short) = spec.doc.short {
            output.push_str(&format!("    --- {short}\n"));
        }
        if let Some(long) = spec.doc.long {
            for line in long.lines().filter(|line| !line.trim().is_empty()) {
                if spec.doc.short.is_some_and(|short| short == line.trim()) {
                    continue;
                }
                output.push_str(&format!("    --- {}\n", line.trim()));
            }
        }
        for param in spec
            .params
            .iter()
            .filter(|param| param.kind == CommandParamKind::User)
        {
            if let Some(doc) = param.doc {
                output.push_str(&format!("    --- @param {} {doc}\n", param.name));
            }
        }
        output.push_str("    ");
        output.push_str(spec.name);
        output.push_str(": ");
        output.push_str(&render_function_type(spec));
        output.push_str(",\n");
    }

    if has_default_bindings {
        output.push_str("    --- Register this widget's default bindings.\n");
        output.push_str("    default_bindings: () -> (),\n");
    }

    output.push_str("}\n");
    output
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
    output.push_str("\n-- ===== Application Commands =====\n");
    output.push_str("-- Auto-generated from registered CommandSpecs.\n");

    for (owner, specs) in owners {
        output.push('\n');
        output.push_str(&render_owner_declaration(
            &owner,
            &specs,
            default_binding_owners.contains(&owner),
        ));
    }

    output
}

/// Render a Luau function type for a command.
fn render_function_type(spec: &CommandSpec) -> String {
    let params = spec
        .params
        .iter()
        .filter(|param| param.kind == CommandParamKind::User)
        .map(|param| format!("{}: {}", param.name, command_type_to_luau(&param.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "({params}) -> {}",
        match spec.ret {
            CommandReturnSpec::Unit => "()".to_string(),
            CommandReturnSpec::Value(ty) => command_type_to_luau(&ty).to_string(),
        }
    )
}

/// Return the Luau type recorded in command metadata.
pub fn command_type_to_luau(spec: &CommandTypeSpec) -> &'static str {
    spec.luau.unwrap_or("any")
}
