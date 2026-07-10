//! End-to-end Luau smoke-suite test for the Todo example.

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;
    use canopy_mcp::{Error as McpError, SuiteConfig, run_suite};
    use todo::create_app;

    fn db_path(tag: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "todo_smoke_{}_{}.db",
            tag,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before epoch")
                .as_millis(),
        ))
    }

    #[test]
    fn luau_smoke_suite_passes() -> Result<()> {
        let db_path = db_path("suite");
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
