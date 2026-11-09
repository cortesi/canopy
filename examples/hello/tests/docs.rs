//! The getting-started guide prints its code from this example.

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    /// The files of the example that the guide quotes.
    const SOURCES: [&str; 3] = ["src/lib.rs", "src/main.rs", "tests/smoke.rs"];

    /// Return the Rust blocks of a Markdown page, each as its lines.
    fn rust_blocks(page: &str) -> Vec<Vec<&str>> {
        let mut blocks = Vec::new();
        let mut lines = page.lines();
        while let Some(line) = lines.next() {
            if line.trim_end() == "```rust" {
                blocks.push(
                    lines
                        .by_ref()
                        .take_while(|line| line.trim_end() != "```")
                        .collect(),
                );
            }
        }
        blocks
    }

    /// Remove the indentation that every non-blank line of `block` shares.
    fn dedent(block: &[&str]) -> Vec<String> {
        let indent = block
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        block
            .iter()
            .map(|line| line.get(indent..).unwrap_or("").to_string())
            .collect()
    }

    /// Return whether `block` is a run of lines in `source`, at one
    /// indentation.
    fn quotes(source: &[&str], block: &[String]) -> bool {
        let Some(first) = block.first() else {
            return true;
        };
        source.windows(block.len()).any(|window| {
            let Some(indent) = window[0].strip_suffix(first.as_str()) else {
                return false;
            };
            indent.trim().is_empty()
                && window.iter().zip(block).all(|(line, expected)| {
                    if expected.is_empty() {
                        line.trim().is_empty()
                    } else {
                        line.strip_prefix(indent) == Some(expected.as_str())
                    }
                })
        })
    }

    #[test]
    fn each_rust_block_of_the_guide_is_in_the_example() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let page = fs::read_to_string(root.join("../../docs/getting-started.md")).unwrap();
        let sources: Vec<String> = SOURCES
            .iter()
            .map(|file| fs::read_to_string(root.join(file)).unwrap())
            .collect();
        let blocks = rust_blocks(&page);
        assert!(!blocks.is_empty(), "the guide has Rust blocks");
        for block in blocks {
            // The guide's suite sits at the root of an application workspace.
            let block: Vec<String> = dedent(&block)
                .into_iter()
                .map(|line| line.replace(r#"join("../../smoke")"#, r#"join("smoke")"#))
                .collect();
            let found = sources.iter().any(|source| {
                let lines: Vec<&str> = source.lines().collect();
                quotes(&lines, &block)
            });
            assert!(
                found,
                "this block of the guide is in no file of the example:\n{}",
                block.join("\n")
            );
        }
    }
}
