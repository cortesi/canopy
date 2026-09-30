//! Convert the Luau TextMate grammar into the sublime-syntax file that
//! `canopy-widgets` bundles, since syntect loads only sublime-syntax, then
//! check the result against upstream's baselines.
//!
//! A match rule becomes a match. A begin/end rule becomes a match that pushes
//! an anonymous context: the end pattern comes first, so it wins a tie as
//! TextMate's does, then the rule's patterns. `name` becomes the context's
//! meta scope, which covers the region with its begin and end, and
//! `contentName` its meta content scope, which covers only the inside. A
//! begin/while rule embeds its patterns and escapes, from any depth, at the
//! start of the first line the while pattern does not match.

use std::{fs, path::Path, process::Command};

use serde_json::{Map, Value};

use crate::cargo_env;

/// The directory of the bundled grammar, relative to the workspace root.
const SYNTAXES: &str = "crates/canopy-widgets/syntaxes";

/// Convert the grammar in an upstream `checkout`, write it and its notice into
/// the widgets crate, and run the conformance test against the checkout's
/// baselines.
pub fn run(workspace_root: &Path, checkout: &Path) -> bool {
    match write(workspace_root, checkout) {
        Ok(()) => check(workspace_root, checkout),
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

/// Convert the grammar, and write it with its source and license notice.
fn write(workspace_root: &Path, checkout: &Path) -> Result<(), String> {
    let read = |name: &str| {
        let path = checkout.join(name);
        fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))
    };
    let grammar = serde_json::from_str(&read("Luau.tmLanguage.json")?)
        .map_err(|error| format!("Luau.tmLanguage.json is not JSON: {error}"))?;
    let license = read("LICENSE.md")?;
    let commit = commit(checkout)?;
    let syntaxes = workspace_root.join(SYNTAXES);
    let write = |name: &str, text: String| {
        fs::write(syntaxes.join(name), text)
            .map_err(|error| format!("cannot write {SYNTAXES}/{name}: {error}"))
    };
    write("Luau.sublime-syntax", convert(&grammar, &commit)?)?;
    write("Luau.LICENSE.md", notice(&commit, &license))?;
    println!("wrote {SYNTAXES} from {commit}");
    Ok(())
}

/// Return the commit the checkout has out.
fn commit(checkout: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(format!("{} is not a git checkout", checkout.display()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Return the notice that records the grammar's source and carries its
/// license.
fn notice(commit: &str, license: &str) -> String {
    format!(
        "# Luau grammar\n\n\
         `Luau.sublime-syntax` is converted from\n\
         [Luau.tmLanguage](https://github.com/JohnnyMorganz/Luau.tmLanguage) at commit\n\
         `{commit}`. To update it, check out that repository\nand run:\n\n\
         ```sh\ncargo xtask luau-grammar path/to/Luau.tmLanguage\n```\n\n\
         The command also checks the grammar against the checkout's baselines.\n\n\
         The grammar carries its upstream license:\n\n---\n\n{license}"
    )
}

/// Run the conformance test against the checkout's baselines.
fn check(workspace_root: &Path, checkout: &Path) -> bool {
    let status = cargo_env::command("cargo")
        .current_dir(workspace_root)
        .args([
            "test",
            "-p",
            "canopy-widgets",
            "--test",
            "luau_grammar",
            "--",
            "--ignored",
        ])
        .env("LUAU_TMLANGUAGE", checkout)
        .status();
    match status {
        Ok(status) if status.success() => true,
        Ok(_) => {
            eprintln!("the converted grammar does not match the upstream baselines");
            false
        }
        Err(error) => {
            eprintln!("cannot run cargo: {error}");
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
        } else if let (Some(begin), Some(pattern)) = (text("begin"), text("while")) {
            // TextMate checks a while pattern at the start of each line at any
            // depth, which an escape does too.
            let captures = captures_of(rule, &["beginCaptures", "captures"]);
            out.push(format!("{indent}- match: {}", quote(begin)));
            if let Some(scope) = scope_of(&[text("name"), capture_name(captures, "0")]) {
                out.push(format!("{indent}  scope: {}", quote(&scope)));
            }
            numbered(captures, &format!("{indent}  "), out);
            out.push(format!("{indent}  embed:"));
            let start = out.len();
            if let Some(patterns) = rule.get("patterns") {
                self.rules(patterns, &format!("{indent}    "), out)?;
            }
            if out.len() == start {
                out.push(format!("{indent}    []"));
            }
            if let Some(scope) = scope_of(&[text("name"), text("contentName")]) {
                out.push(format!("{indent}  embed_scope: {}", quote(&scope)));
            }
            let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
            out.push(format!(
                "{indent}  escape: {}",
                quote(&format!("^(?!{pattern})"))
            ));
        } else if let Some(begin) = text("begin") {
            let captures = captures_of(rule, &["beginCaptures", "captures"]);
            out.push(format!("{indent}- match: {}", quote(begin)));
            // The pushed context's meta scope already covers the match that
            // pushes it, so only capture 0 scopes the match itself.
            if let Some(scope) = capture_name(captures, "0") {
                out.push(format!("{indent}  scope: {}", quote(scope)));
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
                "  string:\n    - match: \"\\\"\"\n      push:\n        \
                 - meta_scope: \"string.quoted.demo\"\n        - match: \"\\\"\"\n          pop: true\n        \
                 - match: \"\\\\\\\\.\"\n          scope: \"constant.character.escape.demo\"\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn while_rules_escape_from_any_depth() {
        let grammar = json!({
            "name": "Demo",
            "scopeName": "source.demo",
            "patterns": [{ "include": "#block" }],
            "repository": {
                "block": {
                    "name": "meta.block.demo",
                    "begin": "(>)",
                    "while": "^(?=>)",
                    "beginCaptures": { "1": { "name": "punctuation.demo" } },
                    "patterns": [{ "include": "$self" }],
                },
            },
        });
        let text = convert(&grammar, "abc").expect("the grammar converts");
        assert!(
            text.contains(
                "  block:\n    - match: \"(>)\"\n      scope: \"meta.block.demo\"\n      \
                 captures:\n        1: \"punctuation.demo\"\n      embed:\n        - include: main\n      \
                 embed_scope: \"meta.block.demo\"\n      escape: \"^(?!(?=>))\"\n"
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
