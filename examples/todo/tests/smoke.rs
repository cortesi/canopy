//! End-to-end Luau smoke-suite test for the Todo example.

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use anyhow::Result;
    use canopy_mcp::{Error as McpError, SuiteConfig, run_suite};
    use todo::create_app;

    #[test]
    fn luau_smoke_suite_passes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("todo_smoke.db");
        let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
        let result = run_suite(
            move || {
                if db_path.exists() {
                    fs::remove_file(&db_path)?;
                }
                create_app(db_path.to_str().expect("utf-8 db path"))
                    .map_err(|error| McpError::app_boxed(error.into_boxed_dyn_error()))
            },
            &SuiteConfig::new(suite_dir),
        )?;
        assert!(result.success(), "{result:#?}");
        Ok(())
    }
}
