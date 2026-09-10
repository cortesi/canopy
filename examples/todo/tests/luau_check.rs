//! Repository-gate checks for tracked Todo Luau scripts.

#[cfg(test)]
mod tests {
    use std::{error::Error, fs, path::Path};

    use canopy::testing::luau::assert_typechecks;
    use canopy_mcp::{SuiteConfig, discover_scripts};

    #[test]
    fn tracked_luau_todo_scripts_typecheck() -> Result<(), Box<dyn Error>> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let scripts = discover_scripts(&SuiteConfig::new(manifest.join("smoke")))?;

        let workspace_root = manifest
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let mut canopy = todo::api_app()?;
        for path in scripts {
            let relative = path.strip_prefix(workspace_root)?.to_string_lossy();
            let source = fs::read_to_string(&path)?;
            assert_typechecks(&mut canopy, &relative, &source)?;
        }
        Ok(())
    }
}
