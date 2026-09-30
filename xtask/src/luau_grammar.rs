//! Convert the Luau TextMate grammar into the sublime-syntax file that
//! `canopy-widgets` bundles, since syntect loads only sublime-syntax.
//!
//! A match rule becomes a match. A begin/end rule becomes a match that pushes
//! an anonymous context: the end pattern comes first, so it wins a tie as
//! TextMate's does, then the rule's patterns. `name` scopes the whole region
//! and `contentName` its inside. A begin/while rule ends at the start of the
//! first line the while pattern does not match.

use std::{fs, path::Path};

use serde_json::{Map, Value};

/// The converted grammar, relative to the workspace root.
const OUTPUT: &str = "crates/canopy-widgets/syntaxes/Luau.sublime-syntax";

/// Convert the grammar at `input`, taken from upstream `commit`, and write the
/// result into the widgets crate.
pub fn run(workspace_root: &Path, input: &Path, commit: &str) -> bool {
    let converted = fs::read_to_string(input)
        .map_err(|error| format!("cannot read {}: {error}", input.display()))
        .and_then(|text| {
            serde_json::from_str(&text)
                .map_err(|error| format!("{} is not JSON: {error}", input.display()))
        })
        .and_then(|grammar| convert(&grammar, commit));
    let written = converted.and_then(|text| {
        fs::write(workspace_root.join(OUTPUT), text)
            .map_err(|error| format!("cannot write {OUTPUT}: {error}"))
    });
    match written {
        Ok(()) => {
            println!("wrote {OUTPUT}");
            true
        }
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

/// Return the sublime-syntax text for a TextMate `grammar`.
fn convert(grammar: &Value, commit: &str) -> Result<String, String> {
    let own = grammar["scopeName"]
        .as_str()
        .ok_or("the grammar names no scope")?;
    let name = grammar["name"].as_str().ok_or("the grammar has no name")?;
    let converter = Converter { own };
    let mut lines = vec![
        "%YAML 1.2".to_string(),
        "---".to_string(),
        format!("# Converted from JohnnyMorganz/Luau.tmLanguage at {commit}."),
        "# Regenerate rather than edit; see the notice beside this file.".to_string(),
        format!("name: {name}"),
        format!("scope: {own}"),
        "file_extensions:".to_string(),
    ];
    let extensions = grammar["fileTypes"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for extension in extensions.iter().filter_map(Value::as_str) {
        lines.push(format!("  - {}", extension.trim_start_matches('.')));
    }
    lines.push("contexts:".to_string());
    lines.push("  main:".to_string());
    converter.rules(&grammar["patterns"], "    ", &mut lines)?;
    let repository = grammar["repository"]
        .as_object()
        .ok_or("the grammar has no repository")?;
    let mut names = repository.keys().collect::<Vec<_>>();
    names.sort();
    for name in names {
        lines.push(format!("  {name}:"));
        let start = lines.len();
        converter.rule(&repository[name], "    ", &mut lines)?;
        if lines.len() == start {
            lines.push("    []".to_string());
        }
    }
    Ok(lines.join("\n") + "\n")
}

/// Converts the rules of one grammar.
struct Converter<'a> {
    /// The grammar's own scope, which an include of itself names.
    own: &'a str,
}

impl Converter<'_> {
    /// Append each rule of a `patterns` array.
    fn rules(&self, patterns: &Value, indent: &str, out: &mut Vec<String>) -> Result<(), String> {
        for rule in patterns.as_array().map_or(&[][..], Vec::as_slice) {
            self.rule(rule, indent, out)?;
        }
        Ok(())
    }

    /// Append one rule.
    fn rule(&self, rule: &Value, indent: &str, out: &mut Vec<String>) -> Result<(), String> {
        let rule = rule.as_object().ok_or("a rule is not an object")?;
        let text = |key: &str| rule.get(key).and_then(Value::as_str);
        if let Some(reference) = text("include") {
            out.push(format!("{indent}- include: {}", self.include(reference)));
        } else if let Some(pattern) = text("match") {
            let captures = captures_of(rule, &["captures"]);
            out.push(format!("{indent}- match: {}", quote(pattern)));
            if let Some(scope) = scope_of(&[text("name"), capture_name(captures, "0")]) {
                out.push(format!("{indent}  scope: {}", quote(&scope)));
            }
            numbered(captures, &format!("{indent}  "), out);
        } else if let Some(begin) = text("begin") {
            let captures = captures_of(rule, &["beginCaptures", "captures"]);
            out.push(format!("{indent}- match: {}", quote(begin)));
            if let Some(scope) = scope_of(&[text("name"), capture_name(captures, "0")]) {
                out.push(format!("{indent}  scope: {}", quote(&scope)));
            }
            numbered(captures, &format!("{indent}  "), out);
            out.push(format!("{indent}  push:"));
            let inner = format!("{indent}    ");
            if let Some(name) = text("name") {
                out.push(format!("{inner}- meta_scope: {}", quote(name)));
            }
            if let Some(name) = text("contentName") {
                out.push(format!("{inner}- meta_content_scope: {}", quote(name)));
            }
            if let Some(end) = text("end") {
                let ends = captures_of(rule, &["endCaptures", "captures"]);
                out.push(format!("{inner}- match: {}", quote(end)));
                if let Some(scope) = capture_name(ends, "0") {
                    out.push(format!("{inner}  scope: {}", quote(scope)));
                }
                numbered(ends, &format!("{inner}  "), out);
                out.push(format!("{inner}  pop: true"));
            } else if let Some(pattern) = text("while") {
                let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
                out.push(format!(
                    "{inner}- match: {}",
                    quote(&format!("^(?!{pattern})"))
                ));
                out.push(format!("{inner}  pop: true"));
            }
            if let Some(patterns) = rule.get("patterns") {
                self.rules(patterns, &inner, out)?;
            }
        } else if let Some(patterns) = rule.get("patterns") {
            self.rules(patterns, indent, out)?;
        } else {
            return Err(format!("unhandled rule: {}", Value::Object(rule.clone())));
        }
        Ok(())
    }

    /// Return the context an include names.
    fn include(&self, reference: &str) -> String {
        if matches!(reference, "$self" | "$base") || reference == self.own {
            "main".to_string()
        } else if let Some(name) = reference.strip_prefix('#') {
            name.to_string()
        } else {
            format!("scope:{reference}")
        }
    }
}

/// Return the first of `keys` that the rule holds, as a capture map.
fn captures_of<'a>(rule: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    keys.iter()
        .find_map(|key| rule.get(*key))
        .and_then(Value::as_object)
}

/// Return the scope of capture `index`, if it has one.
fn capture_name<'a>(captures: Option<&'a Map<String, Value>>, index: &str) -> Option<&'a str> {
    captures?.get(index)?.get("name")?.as_str()
}

/// Join the scopes present, outermost first.
fn scope_of(names: &[Option<&str>]) -> Option<String> {
    let names = names.iter().flatten().copied().collect::<Vec<_>>();
    (!names.is_empty()).then(|| names.join(" "))
}

/// Append the numbered captures, leaving out capture 0, which scopes the whole
/// match instead.
fn numbered(captures: Option<&Map<String, Value>>, indent: &str, out: &mut Vec<String>) {
    let Some(captures) = captures else {
        return;
    };
    let mut named = captures
        .iter()
        .filter(|(index, _)| index.as_str() != "0")
        .filter_map(|(index, capture)| {
            Some((index.parse::<u32>().ok()?, capture.get("name")?.as_str()?))
        })
        .collect::<Vec<_>>();
    if named.is_empty() {
        return;
    }
    named.sort_unstable();
    out.push(format!("{indent}captures:"));
    for (index, name) in named {
        out.push(format!("{indent}  {index}: {}", quote(name)));
    }
}

/// Quote `text` as a YAML double-quoted scalar, which JSON's escapes satisfy.
fn quote(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn begin_end_rules_push_a_context_that_ends_first() {
        let grammar = json!({
            "name": "Demo",
            "scopeName": "source.demo",
            "fileTypes": ["demo"],
            "patterns": [{ "include": "#string" }],
            "repository": {
                "string": {
                    "name": "string.quoted.demo",
                    "begin": "\"",
                    "end": "\"",
                    "patterns": [{ "match": "\\\\.", "name": "constant.character.escape.demo" }],
                },
            },
        });
        let text = convert(&grammar, "abc").expect("the grammar converts");
        assert!(
            text.contains(
                "  string:\n    - match: \"\\\"\"\n      scope: \"string.quoted.demo\"\n      push:\n        \
                 - meta_scope: \"string.quoted.demo\"\n        - match: \"\\\"\"\n          pop: true\n        \
                 - match: \"\\\\\\\\.\"\n          scope: \"constant.character.escape.demo\"\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn includes_and_captures_convert() {
        let grammar = json!({
            "name": "Demo",
            "scopeName": "source.demo",
            "patterns": [{ "include": "source.demo" }, { "include": "$self" }, { "include": "source.other" }],
            "repository": {
                "call": {
                    "match": "(\\w+)(\\()",
                    "captures": { "2": { "name": "punctuation" }, "1": { "name": "entity.name.function" } },
                },
            },
        });
        let text = convert(&grammar, "abc").expect("the grammar converts");
        assert!(
            text.contains("  main:\n    - include: main\n    - include: main\n    - include: scope:source.other\n"),
            "{text}"
        );
        assert!(
            text.contains(
                "      captures:\n        1: \"entity.name.function\"\n        2: \"punctuation\"\n"
            ),
            "{text}"
        );
    }
}
