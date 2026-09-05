//! End-to-end Luau smoke-suite test for the Todo example.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use canopy_mcp::{
        AppMetadata, Error as McpError, ResetPolicy, SuiteConfig, app_factory, run_suite,
    };
    use todo::create_app;

    #[test]
    fn luau_smoke_suite_passes() -> Result<()> {
        let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
        let result = run_suite(
            app_factory(|| create_app(":memory:").map_err(McpError::app)).with_metadata(
                AppMetadata {
                    app: "todo".into(),
                    reset: ResetPolicy::Isolated,
                },
            ),
            &SuiteConfig::new(suite_dir),
        )?;
        assert!(result.success(), "{result:#?}");
        assert!(
            result
                .scripts
                .iter()
                .all(|script| script.outcome.metadata.app == "todo"
                    && script.outcome.metadata.reset == ResetPolicy::Isolated)
        );
        Ok(())
    }
}
