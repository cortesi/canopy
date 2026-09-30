//! Conformance of the bundled Luau grammar with its upstream baselines.
//!
//! Upstream records how the reference TextMate engine scopes each of its test
//! cases. This test scopes the same cases with syntect and the converted
//! grammar, and compares the two. The baselines are too large to vendor, so
//! the test reads them from the upstream checkout that `LUAU_TMLANGUAGE`
//! names, and `cargo xtask luau-grammar` runs it after each conversion.
//!
//! Two differences between the engines carry no meaning, and the comparison
//! removes them. The reference engine splits a token at each rule boundary and
//! syntect at each scope change, so neighbouring tokens with one scope stack
//! join. The reference engine may end a line's last token past the newline,
//! which never draws, so each line's tokens stop at its end; an empty line
//! keeps one unit. Positions count UTF-16 units, as the baselines do.

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use syntect::parsing::{ParseState, ScopeStack, SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};

    /// The line that separates a baseline's source from its tokens.
    const SEPARATOR: &str = "-----------------------------------\n";

    /// A token: its start and end in UTF-16 units, and its scopes.
    type Token = (usize, usize, String);

    #[test]
    #[ignore = "reads an upstream checkout; cargo xtask luau-grammar runs it"]
    fn the_luau_grammar_matches_the_upstream_baselines() {
        let checkout =
            env::var("LUAU_TMLANGUAGE").expect("LUAU_TMLANGUAGE names an upstream checkout");
        let checkout = Path::new(&checkout);
        let set = grammar();
        let mut cases = fs::read_dir(checkout.join("tests/baselines"))
            .expect("the checkout has baselines")
            .map(|entry| entry.expect("a baseline entry reads").path())
            .collect::<Vec<_>>();
        cases.sort();
        assert!(!cases.is_empty(), "the checkout has no baselines");
        let mut failures = Vec::new();
        for path in &cases {
            let baseline = fs::read_to_string(path).expect("a baseline reads");
            let mut parts = baseline.splitn(3, SEPARATOR).skip(1);
            let (Some(source), Some(tokens)) = (parts.next(), parts.next()) else {
                panic!("{} is not a baseline", path.display());
            };
            let source = source.strip_suffix('\n').unwrap_or(source);
            let lines = source.split('\n').collect::<Vec<_>>();
            let expected = clip(&lines, expected(tokens));
            let actual = clip(&lines, scope(&set, &lines));
            if let Some(line) = (0..lines.len()).find(|&i| expected.get(i) != actual.get(i)) {
                failures.push(format!(
                    "{} line {}: {:?}\n  expected {:?}\n  actual   {:?}",
                    path.display(),
                    line + 1,
                    lines[line],
                    expected.get(line),
                    actual.get(line),
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} cases differ:\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n")
        );
    }

    /// Return a set holding only the bundled Luau grammar.
    fn grammar() -> SyntaxSet {
        let text = include_str!("../syntaxes/Luau.sublime-syntax");
        let definition =
            SyntaxDefinition::load_from_str(text, true, None).expect("the grammar loads");
        let mut builder = SyntaxSetBuilder::new();
        builder.add(definition);
        builder.build()
    }

    /// Return each line's tokens from the token section of a baseline.
    ///
    /// A line begins with `>` and its source. Each token follows as a line of
    /// carets under the text it covers, then a line of its scopes.
    fn expected(section: &str) -> Vec<Vec<Token>> {
        let mut lines: Vec<Vec<Token>> = Vec::new();
        let mut rows = section.lines();
        while let Some(row) = rows.next() {
            if row.starts_with('>') {
                lines.push(Vec::new());
            } else if let Some(first) = row.find('^') {
                let count = row[first..].chars().take_while(|&c| c == '^').count();
                let scopes = rows
                    .next()
                    .expect("scopes follow the carets")
                    .trim()
                    .to_string();
                // The carets sit one column right, under the text after `>`.
                let start = first - 1;
                lines.last_mut().expect("tokens follow a line").push((
                    start,
                    start + count,
                    scopes,
                ));
            }
        }
        lines
    }

    /// Scope `lines` with syntect, and return each line's tokens.
    fn scope(set: &SyntaxSet, lines: &[&str]) -> Vec<Vec<Token>> {
        let mut state = ParseState::new(&set.syntaxes()[0]);
        let mut stack = ScopeStack::new();
        let mut out = Vec::new();
        for line in lines {
            let text = format!("{line}\n");
            let ops = state.parse_line(&text, set).expect("the line parses");
            let unit = |byte: usize| text[..byte].encode_utf16().count();
            let mut tokens = Vec::new();
            let mut start = 0;
            for (index, op) in ops {
                if index > start {
                    tokens.push((unit(start), unit(index), scopes(&stack)));
                    start = index;
                }
                stack.apply(&op).expect("the scope op applies");
            }
            tokens.push((unit(start), unit(text.len()), scopes(&stack)));
            out.push(tokens);
        }
        out
    }

    /// Return the scopes of `stack`, outermost first.
    fn scopes(stack: &ScopeStack) -> String {
        stack
            .as_slice()
            .iter()
            .map(|scope| scope.build_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Stop each line's tokens at its end, and join neighbours that share
    /// scopes.
    fn clip(lines: &[&str], tokens: Vec<Vec<Token>>) -> Vec<Vec<Token>> {
        tokens
            .into_iter()
            .zip(lines)
            .map(|(tokens, line)| {
                let end = line.encode_utf16().count().max(1);
                let mut joined: Vec<Token> = Vec::new();
                for (start, stop, scopes) in tokens {
                    let stop = stop.min(end);
                    if start >= stop {
                        continue;
                    }
                    match joined.last_mut() {
                        Some(last) if last.1 == start && last.2 == scopes => last.1 = stop,
                        _ => joined.push((start, stop, scopes)),
                    }
                }
                joined
            })
            .collect()
    }
}
