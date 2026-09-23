//! The committed API captures name public items by their public paths.

#[cfg(test)]
mod tests {
    use std::{error::Error, fs, path::Path};

    /// A capture that prints `crate::core::` exposes a private module path, so
    /// a public signature imported its type from inside `core` instead of
    /// through its public module.
    #[test]
    fn api_captures_name_no_private_core_paths() -> Result<(), Box<dyn Error>> {
        let api = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../api");
        let mut leaks = Vec::new();
        for entry in fs::read_dir(api)? {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            for (line, text) in fs::read_to_string(&path)?.lines().enumerate() {
                if text.contains("crate::core::") {
                    leaks.push(format!("{}:{}: {}", path.display(), line + 1, text.trim()));
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "import public types by their public path:\n{}",
            leaks.join("\n")
        );
        Ok(())
    }
}
