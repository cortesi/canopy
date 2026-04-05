use std::collections::BTreeMap;

use super::luau_global_owner_name;
use crate::commands::{
    CommandDispatchKind, CommandParamKind, CommandReturnSpec, CommandSet, CommandSpec,
    CommandTypeSpec,
};

/// Static Luau preamble shared by every canopy app.
const PREAMBLE: &str = include_str!("../../../luau/preamble.d.luau");

/// Render the complete Luau definition file for the current command set.
pub fn render_definitions(commands: &CommandSet) -> String {
    let mut owners: BTreeMap<&'static str, Vec<&'static CommandSpec>> = BTreeMap::new();
    for (_, spec) in commands.iter() {
        let CommandDispatchKind::Node { owner } = spec.dispatch else {
            continue;
        };
        owners.entry(owner).or_default().push(spec);
    }

    let mut output = String::from(PREAMBLE);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("\n-- ===== Application Commands =====\n");
    output.push_str("-- Auto-generated from registered CommandSpecs.\n");

    for (owner, specs) in owners {
        output.push('\n');
        output.push_str(&format!("--- Commands for widget \"{owner}\"\n"));
        output.push_str(&format!("declare {}: {{\n", luau_global_owner_name(owner)));

        let mut specs = specs;
        specs.sort_by_key(|spec| spec.id.0);
        for spec in specs {
            if let Some(short) = spec.doc.short {
                output.push_str(&format!("    --- {short}\n"));
            }
            output.push_str("    ");
            output.push_str(spec.name);
            output.push_str(": ");
            output.push_str(&render_function_type(spec));
            output.push_str(",\n");
        }

        output.push_str("}\n");
    }

    output
}

/// Render a Luau function type for a command.
fn render_function_type(spec: &CommandSpec) -> String {
    let params = spec
        .params
        .iter()
        .filter(|param| param.kind == CommandParamKind::User)
        .map(|param| format!("{}: {}", param.name, rust_type_to_luau(&param.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "({params}) -> {}",
        match spec.ret {
            CommandReturnSpec::Unit => "()".to_string(),
            CommandReturnSpec::Value(ty) => rust_type_to_luau(&ty),
        }
    )
}

/// Best-effort mapping from command type metadata to Luau types.
pub fn rust_type_to_luau(spec: &CommandTypeSpec) -> String {
    if let Some(luau) = spec.luau {
        return luau.to_string();
    }

    let ty = spec.rust.trim();
    if ty == "()" {
        return "()".to_string();
    }
    if matches!(
        ty,
        "bool" | "&bool" | "std::primitive::bool" | "core::primitive::bool"
    ) {
        return "boolean".to_string();
    }
    if matches_primitive_number(ty) {
        return "number".to_string();
    }
    if matches_string(ty) {
        return "string".to_string();
    }
    if let Some(inner) = unwrap_generic(ty, "Option") {
        return format!(
            "{}?",
            rust_type_to_luau(&CommandTypeSpec {
                rust: inner,
                luau: None,
                doc: None,
            })
        );
    }
    if let Some(inner) = unwrap_generic(ty, "Vec") {
        return format!(
            "{{{}}}",
            rust_type_to_luau(&CommandTypeSpec {
                rust: inner,
                luau: None,
                doc: None,
            })
        );
    }
    if let Some((key, value)) = unwrap_map(ty)
        && matches_string(key)
    {
        return format!(
            "{{[string]: {}}}",
            rust_type_to_luau(&CommandTypeSpec {
                rust: value,
                luau: None,
                doc: None,
            })
        );
    }
    if matches!(ty, "Direction" | "canopy::geom::Direction") {
        return "\"Up\" | \"Down\" | \"Left\" | \"Right\"".to_string();
    }
    if ty.ends_with("FocusDirection") {
        return "\"Next\" | \"Prev\" | \"Up\" | \"Down\" | \"Left\" | \"Right\"".to_string();
    }
    if ty.ends_with("ZoomDirection") {
        return "\"In\" | \"Out\"".to_string();
    }

    "any".to_string()
}

/// Return true if the Rust type is treated as a string in Luau.
fn matches_string(ty: &str) -> bool {
    matches!(
        ty.trim_start_matches('&'),
        "str" | "String" | "std::string::String" | "alloc::string::String"
    )
}

/// Return true if the Rust type is numeric.
fn matches_primitive_number(ty: &str) -> bool {
    matches!(
        ty,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "f32"
            | "f64"
            | "std::primitive::i8"
            | "std::primitive::i16"
            | "std::primitive::i32"
            | "std::primitive::i64"
            | "std::primitive::isize"
            | "std::primitive::u8"
            | "std::primitive::u16"
            | "std::primitive::u32"
            | "std::primitive::u64"
            | "std::primitive::usize"
            | "std::primitive::f32"
            | "std::primitive::f64"
    )
}

/// Extract the inner type from a single-parameter generic.
fn unwrap_generic<'a>(ty: &'a str, name: &str) -> Option<&'a str> {
    let suffix = format!("{name}<");
    let ty = ty.trim();
    if let Some(rest) = ty.strip_prefix(&suffix)
        && let Some(inner) = rest.strip_suffix('>')
    {
        return Some(inner.trim());
    }
    if let Some(rest) = ty.strip_prefix(&format!("std::option::{name}<"))
        && let Some(inner) = rest.strip_suffix('>')
    {
        return Some(inner.trim());
    }
    None
}

/// Extract key/value types from a supported map type.
fn unwrap_map(ty: &str) -> Option<(&str, &str)> {
    for prefix in [
        "BTreeMap<",
        "HashMap<",
        "std::collections::BTreeMap<",
        "std::collections::HashMap<",
    ] {
        if let Some(rest) = ty.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix('>')
            && let Some((key, value)) = split_top_level_once(inner, ',')
        {
            return Some((key.trim(), value.trim()));
        }
    }
    None
}

/// Split a generic argument list once at the top level.
fn split_top_level_once(input: &str, needle: char) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (index, ch) in input.char_indices() {
        match ch {
            '<' => depth = depth.saturating_add(1),
            '>' => depth = depth.saturating_sub(1),
            _ if ch == needle && depth == 0 => return Some((&input[..index], &input[index + 1..])),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::rust_type_to_luau;
    use crate::commands::CommandTypeSpec;

    #[test]
    fn type_mapping_covers_common_cases() {
        let string = CommandTypeSpec {
            rust: "String",
            luau: None,
            doc: None,
        };
        assert_eq!(rust_type_to_luau(&string), "string");

        let option = CommandTypeSpec {
            rust: "Option<Vec<u32>>",
            luau: None,
            doc: None,
        };
        assert_eq!(rust_type_to_luau(&option), "{number}?");

        let map = CommandTypeSpec {
            rust: "BTreeMap<String, bool>",
            luau: None,
            doc: None,
        };
        assert_eq!(rust_type_to_luau(&map), "{[string]: boolean}");
    }
}
